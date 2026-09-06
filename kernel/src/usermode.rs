#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::VirtAddr;
#[cfg(target_arch = "x86_64")]
use crate::arch_compat::structures::paging::VirtAddr;
/// User Mode - Ring 3 transition and user-space execution
/// Provides user-space execution with syscall/sysret instruction support
use crate::serial_println;

/// User-space code segment selector (GDT index 4, RPL 3)
pub const USER_CODE_SEGMENT: u16 = 0x23; // (4 << 3) | 3
/// User-space data segment selector (GDT index 3, RPL 3)
pub const USER_DATA_SEGMENT: u16 = 0x1B; // (3 << 3) | 3
/// Kernel code segment selector (GDT entry 1, RPL 0)
pub const KERNEL_CODE_SEGMENT: u16 = 0x08;
/// Kernel data segment selector (GDT entry 2, RPL 0)
pub const KERNEL_DATA_SEGMENT: u16 = 0x10;

/// User-space stack top
pub const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_0000;
/// User-space stack size (8MB)
pub const USER_STACK_SIZE: u64 = 8 * 1024 * 1024;
/// User-space heap start
pub const USER_HEAP_START: u64 = 0x0000_0000_4000_0000;
/// User-space program load address
pub const USER_PROGRAM_BASE: u64 = 0x0000_0000_0040_0000;

/// MSR addresses for syscall/sysret
const MSR_STAR: u32 = 0xC000_0081;
const MSR_LSTAR: u32 = 0xC000_0082;
const MSR_CSTAR: u32 = 0xC000_0083; // Not used in 64-bit mode
const MSR_SFMASK: u32 = 0xC000_0084;
const MSR_EFER: u32 = 0xC000_0080;
const MSR_GS_BASE: u32 = 0xC000_0101;
const MSR_KERNEL_GS_BASE: u32 = 0xC000_0102;

/// Per-CPU GS scratch used by `syscall_entry` (`swapgs` then gs:[0]/gs:[8]).
#[repr(C)]
struct SyscallGs {
    user_rsp: u64,
    kernel_rsp: u64,
}

static mut SYSCALL_GS: SyscallGs = SyscallGs {
    user_rsp: 0,
    kernel_rsp: 0,
};

/// EFER bits
const EFER_SCE: u64 = 1 << 0; // System Call Extensions enable

/// Initialize syscall/sysret mechanism
pub fn init_syscall() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // Enable SCE (System Call Extensions) in EFER MSR
        let efer = rdmsr(MSR_EFER);
        wrmsr(MSR_EFER, efer | EFER_SCE);

        // Set up STAR MSR:
        // Bits 47:32 = kernel CS/SS base (CS = STAR[47:32], SS = STAR[47:32]+8)
        // Bits 63:48 = user CS/SS base (CS = STAR[63:48]+16, SS = STAR[63:48]+8)
        // sysret sets: CS = STAR[63:48]+16 | RPL3,  SS = STAR[63:48]+8 | RPL3
        // With user_base = 0x10:  CS = 0x10+16 = 0x20 | 3 = 0x23 (user code, GDT index 4)
        //                         SS = 0x10+8  = 0x18 | 3 = 0x1B (user data, GDT index 3)
        // Kernel: CS = 0x08, SS = 0x10
        let kernel_base: u64 = 0x08; // Kernel CS = 0x08, SS = 0x10
        let user_base: u64 = 0x10; // User CS = 0x10+16=0x20 (0x23 with RPL3), SS = 0x10+8=0x18 (0x1B with RPL3)
        let star = (user_base << 48) | (kernel_base << 32);
        wrmsr(MSR_STAR, star);

        // Set LSTAR to syscall entry point
        wrmsr(MSR_LSTAR, syscall_entry as *const () as u64);

        // Set SFMASK - flags to clear on syscall entry
        // Clear IF (interrupt flag) and TF (trap flag) on syscall entry
        wrmsr(MSR_SFMASK, 0x200 | 0x100); // IF | TF

        // Kernel GS: syscall_entry does swapgs then gs:[0]=user RSP, gs:[8]=kernel RSP.
        // In kernel, GS_BASE points at SYSCALL_GS. KERNEL_GS_BASE is the user GS (0).
        // jump_to_user_mode swapgs's before iretq so syscall's swapgs is correct.
        SYSCALL_GS.kernel_rsp = crate::gdt::privilege_stack_top();
        wrmsr(MSR_GS_BASE, core::ptr::addr_of!(SYSCALL_GS) as u64);
        wrmsr(MSR_KERNEL_GS_BASE, 0);
    }

    serial_println!("[KnoxOS] syscall/sysret initialized for user-mode transitions");
    serial_println!("[KnoxOS]   KERNEL_GS_BASE programmed (syscall swapgs-safe)");
}

/// Read a Model-Specific Register
unsafe fn rdmsr(msr: u32) -> u64 {
    let (mut low, mut high): (u32, u32) = (0, 0);
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nomem, nostack)
    );
    ((high as u64) << 32) | (low as u64)
}

/// Write a Model-Specific Register
unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
        options(nomem, nostack)
    );
}

/// Syscall entry point - called by the `syscall` instruction
///
/// When `syscall` executes:
/// - RCX = return RIP
/// - R11 = return RFLAGS
/// - RIP = LSTAR value (this function)
/// - CS = kernel CS, SS = kernel SS
///
/// Syscall ABI (Linux x86_64):
/// - RAX = syscall number
/// - RDI = arg1, RSI = arg2, RDX = arg3
/// - R10 = arg4, R8 = arg5, R9 = arg6
/// - RAX = return value
#[unsafe(naked)]
#[cfg(target_arch = "x86_64")]
unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // Save user registers on kernel stack
        "swapgs",                   // Switch to kernel GS base
        "mov gs:[0x0], rsp",        // Save user RSP
        "mov rsp, gs:[0x8]",        // Load kernel RSP

        // Build a trap frame
        "push rcx",                 // Save user RIP (syscall puts it in RCX)
        "push r11",                 // Save user RFLAGS (syscall puts it in R11)
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // r10 holds arg4 from userspace (syscall ABI uses r10 instead of rcx,
        // because the CPU clobbers rcx with the return RIP).
        // The C ABI register shuffle below will place r10 into the correct
        // register for the handler call.

        // Call the Rust syscall handler
        // On entry from userspace via syscall:
        //   rax=syscall number, rdi=arg1, rsi=arg2, rdx=arg3, r10=arg4, r8=arg5, r9=arg6
        // C ABI needs: rdi=number, rsi=arg1, rdx=arg2, rcx=arg3, r8=arg4, r9=arg5
        // 7th arg (arg6) goes on the stack.
        //
        // Stack alignment: after 8 trap-frame pushes (64 bytes), RSP is 16-aligned.
        // We need RSP 16-aligned at the `call` site with arg6 as the stack param.
        //
        // Layout before call (growing downward):
        //   [RSP + 8]  = saved rax (syscall number) — will be read into rdi
        //   [RSP + 0]  = arg6 (user r9) — 7th C param at [RSP] before call
        //                → becomes [RSP+8] after call pushes return addr ✓
        //
        // After push rax + push r9: RSP -= 16, still aligned ✓
        "push rax",                 // Save syscall number (RSP -= 8, misaligned)
        "push r9",                  // 7th C arg = user arg6 (RSP -= 8, aligned again)
        // Rearrange registers into C ABI positions.
        // Order matters — each source register is read before it's overwritten.
        "mov r9, r8",               // 6th C arg = user arg5 (r8)
        "mov r8, r10",              // 5th C arg = user arg4 (r10)
        "mov rcx, rdx",             // 4th C arg = user arg3 (rdx)
        "mov rdx, rsi",             // 3rd C arg = user arg2 (rsi)
        "mov rsi, rdi",             // 2nd C arg = user arg1 (rdi)
        "mov rdi, [rsp + 8]",       // 1st C arg = syscall number (saved rax)
        "call {handler}",

        // Return value is in RAX
        "add rsp, 16",              // Pop arg6 + saved syscall number

        // Restore callee-saved registers
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",

        // Prepare for sysretq
        "pop r11",                  // Restore RFLAGS
        "pop rcx",                  // Restore RIP

        "mov rsp, gs:[0x0]",        // Restore user RSP
        "swapgs",                   // Switch back to user GS

        "sysretq",                  // Return to user mode

        handler = sym syscall_handler_wrapper,
    );
}

/// Wrapper that calls the actual syscall dispatcher
extern "C" fn syscall_handler_wrapper(
    number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    crate::syscall::handle_syscall(number, arg1, arg2, arg3, arg4, arg5, arg6)
}

/// Jump to user mode - execute a program at the given entry point
/// # Safety
/// The entry point and stack must be properly mapped in user-space page tables
pub unsafe fn jump_to_user_mode(entry_point: u64, user_stack: u64) {
    serial_println!(
        "[KnoxOS] Entering user mode: entry={:#x} stack={:#x}",
        entry_point,
        user_stack
    );

    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        // Set up for iretq to user mode
        "cli",                              // Disable interrupts

        // Push SS (user data segment)
        "push {user_ss}",
        // Push RSP (user stack pointer)
        "push {user_rsp}",
        // Push RFLAGS (with IF enabled)
        "push 0x202",
        // Push CS (user code segment)
        "push {user_cs}",
        // Push RIP (user entry point)
        "push {entry}",

        // DS/ES to user data. Do not load GS — that would wipe IA32_GS_BASE.
        "mov ax, {user_ds:x}",
        "mov ds, ax",
        "mov es, ax",

        // Swap to user GS (0); KERNEL_GS_BASE keeps SYSCALL_GS for swapgs on syscall.
        "swapgs",
        "iretq",

        entry = in(reg) entry_point,
        user_rsp = in(reg) user_stack,
        user_cs = in(reg) USER_CODE_SEGMENT as u64,
        user_ss = in(reg) USER_DATA_SEGMENT as u64,
        user_ds = in(reg) USER_DATA_SEGMENT,
        options(noreturn)
    );
}

/// Check if we're currently in user mode
pub fn is_user_mode() -> bool {
    let mut cs: u16 = 0;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "mov {:x}, cs",
            out(reg) cs,
            options(nomem, nostack)
        );
    }
    (cs & 3) == 3 // RPL = 3 means user mode
}

/// Initialize user mode support
pub fn init() {
    init_syscall();
    serial_println!("[KnoxOS] User mode (Ring 3) support initialized");
    serial_println!("[KnoxOS]   User code segment: {:#06x}", USER_CODE_SEGMENT);
    serial_println!("[KnoxOS]   User data segment: {:#06x}", USER_DATA_SEGMENT);
    serial_println!("[KnoxOS]   User stack top: {:#x}", USER_STACK_TOP);
}
