/// Context Switching - Hardware CPU state save/restore for preemptive multitasking
///
/// Kernel-to-kernel switches restore RIP by jumping to a saved continuation
/// (or a thread entry point). User-mode contexts restore via `iretq` when CS.RPL==3.
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::process::Pid;
use crate::serial_println;

/// Idle kernel thread PID (HLT loop)
pub const IDLE_PID: Pid = 0;
/// In-kernel desktop / executor PID
pub const DESKTOP_PID: Pid = 2;

/// CPU register state for context switching.
/// `fxsave_area` is 16-byte aligned (FXSAVE requirement).
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct CpuContext {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
    pub cs: u64,
    pub ss: u64,
    pub ds: u64,
    pub es: u64,
    pub fs: u64,
    pub gs: u64,
    pub cr3: u64,
    /// Padding so `fxsave_area` starts at offset 208 (16-byte aligned).
    _fxsave_pad: u64,
    pub fxsave_area: [u8; 512],
    pub fpu_initialized: bool,
}

const _: () = {
    assert!(core::mem::offset_of!(CpuContext, rax) == 0);
    assert!(core::mem::offset_of!(CpuContext, rsp) == 7 * 8);
    assert!(core::mem::offset_of!(CpuContext, rip) == 16 * 8);
    assert!(core::mem::offset_of!(CpuContext, rflags) == 17 * 8);
    assert!(core::mem::offset_of!(CpuContext, cs) == 18 * 8);
    assert!(core::mem::offset_of!(CpuContext, ss) == 19 * 8);
    assert!(core::mem::offset_of!(CpuContext, cr3) == 24 * 8);
    assert!(core::mem::offset_of!(CpuContext, fxsave_area) == 208);
    assert!(core::mem::offset_of!(CpuContext, fxsave_area) % 16 == 0);
    assert!(core::mem::offset_of!(CpuContext, fpu_initialized) == 720);
};

impl Default for CpuContext {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuContext {
    /// Create a new empty context
    pub const fn new() -> Self {
        Self {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            rsp: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rip: 0,
            rflags: 0x202, // IF (Interrupt Flag) set
            cs: 0x08,      // Kernel code segment
            ss: 0x10,      // Kernel data segment
            ds: 0x10,
            es: 0x10,
            fs: 0,
            gs: 0,
            cr3: 0,
            _fxsave_pad: 0,
            fxsave_area: [0; 512],
            fpu_initialized: false,
        }
    }

    /// Create a kernel thread context with given entry point and stack
    pub fn new_kernel_thread(entry_point: u64, stack_top: u64) -> Self {
        let mut ctx = Self::new();
        ctx.rip = entry_point;
        // SysV: at function entry RSP % 16 == 8 (as if `call` pushed a return address).
        let aligned = stack_top & !0xF;
        ctx.rsp = aligned.saturating_sub(8);
        ctx.rbp = ctx.rsp;
        ctx.cs = 0x08;
        ctx.ss = 0x10;
        ctx.rflags = 0x202;
        ctx
    }

    /// Create a user-mode context with given entry point and stack
    pub fn new_user_thread(entry_point: u64, stack_top: u64) -> Self {
        let mut ctx = Self::new();
        ctx.rip = entry_point;
        ctx.rsp = stack_top;
        ctx.rbp = stack_top;
        ctx.cs = 0x23; // User code segment (ring 3)
        ctx.ss = 0x1B; // User data segment (ring 3)
        ctx.ds = 0x1B;
        ctx.es = 0x1B;
        ctx.rflags = 0x202;
        ctx
    }

    pub fn is_runnable(&self) -> bool {
        self.rip != 0 && self.rsp != 0
    }
}

/// Per-process context storage
pub struct ProcessContext {
    /// CPU state first so it inherits 16-byte alignment from `CpuContext`
    pub context: CpuContext,
    pub pid: Pid,
    /// Kernel stack for this process (used during syscalls)
    pub kernel_stack_top: u64,
    /// Kernel stack allocation
    pub kernel_stack: Vec<u8>,
    /// Whether this process is in user mode
    pub in_user_mode: bool,
}

impl ProcessContext {
    /// Allocate a new process context with kernel stack
    pub fn new(pid: Pid) -> Self {
        const KERNEL_STACK_SIZE: usize = 32 * 1024;
        let kernel_stack = alloc::vec![0u8; KERNEL_STACK_SIZE];
        let stack_top = kernel_stack.as_ptr() as u64 + KERNEL_STACK_SIZE as u64;

        let mut context = CpuContext::new();
        // Keep RSP valid so a buggy switch never loads RSP=0.
        // RIP stays 0 until the thread is given an entry point or a save occurs.
        let aligned = stack_top & !0xF;
        context.rsp = aligned.saturating_sub(8);
        context.rbp = context.rsp;

        Self {
            context,
            pid,
            kernel_stack_top: stack_top,
            kernel_stack,
            in_user_mode: false,
        }
    }

    /// Create with a specific entry point (for kernel threads)
    pub fn new_kernel_thread(pid: Pid, entry_point: u64) -> Self {
        let mut pc = Self::new(pid);
        pc.context = CpuContext::new_kernel_thread(entry_point, pc.kernel_stack_top);
        pc
    }
}

/// Boxed so pointers into `CpuContext` stay valid if the Vec reallocates.
lazy_static::lazy_static! {
    pub static ref PROCESS_CONTEXTS: Mutex<Vec<Box<ProcessContext>>> = Mutex::new(Vec::new());
}

/// The currently running process PID
static CURRENT_PID: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Completed RIP-restoring switches (for observability / tests)
static SWITCH_COUNT: AtomicU64 = AtomicU64::new(0);

/// Save the current CPU state into the given context (GPRs, RFLAGS, CR3, FXSAVE).
/// Does **not** capture RIP; use `switch_context` to save a continuation.
///
/// # Safety
/// Must be called in a context where registers are meaningful.
pub unsafe fn save_context(ctx: &mut CpuContext) {
    let ctx_ptr = ctx as *mut CpuContext as u64;
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "mov [{ctx} + 0*8], rax",
        "mov [{ctx} + 1*8], rbx",
        "mov [{ctx} + 2*8], rcx",
        "mov [{ctx} + 3*8], rdx",
        "mov [{ctx} + 4*8], rsi",
        "mov [{ctx} + 5*8], rdi",
        "mov [{ctx} + 6*8], rbp",
        "mov [{ctx} + 7*8], rsp",
        "mov [{ctx} + 8*8], r8",
        "mov [{ctx} + 9*8], r9",
        "mov [{ctx} + 10*8], r10",
        "mov [{ctx} + 11*8], r11",
        "mov [{ctx} + 12*8], r12",
        "mov [{ctx} + 13*8], r13",
        "mov [{ctx} + 14*8], r14",
        "mov [{ctx} + 15*8], r15",
        "pushfq",
        "pop rax",
        "mov [{ctx} + 17*8], rax",
        "mov rax, cr3",
        "mov [{ctx} + 24*8], rax",
        "fxsave64 [{ctx} + 208]",
        "mov byte ptr [{ctx} + 720], 1",
        ctx = in(reg) ctx_ptr,
        out("rax") _,
        options(nostack)
    );
    let _ = ctx_ptr;
}

/// Restore CPU state from the given context, including RIP (does not return
/// unless the restored continuation is this function's caller).
///
/// # Safety
/// Completely replaces the current CPU state.
pub unsafe fn restore_context(ctx: &CpuContext) {
    let mut dummy = CpuContext::new();
    switch_context(&mut dummy, ctx);
}

/// Save `old`'s continuation and restore `new` (GPRs, RIP, CS/SS, RFLAGS, FXSAVE, CR3).
///
/// # Safety
/// Interrupts should be disabled or the caller must ensure the contexts are
/// not freed. The `PROCESS_CONTEXTS` lock must **not** be held.
pub unsafe fn switch_context(old: &mut CpuContext, new: &CpuContext) {
    if core::ptr::eq(old, new) {
        return;
    }
    #[cfg(target_arch = "x86_64")]
    {
        SWITCH_COUNT.fetch_add(1, Ordering::Relaxed);
        switch_context_inner(old as *mut CpuContext, new as *const CpuContext);
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (old, new);
    }
}

/// Naked switch: rdi = old, rsi = new (SysV).
///
/// Saves RIP as the resume label inside this function so the original caller
/// continues after `switch_context` when the thread is restored. A brand-new
/// kernel thread has RIP = entry and is entered with `jmp`.
#[unsafe(naked)]
#[cfg(target_arch = "x86_64")]
unsafe extern "C" fn switch_context_inner(old: *mut CpuContext, new: *const CpuContext) {
    core::arch::naked_asm!(
        // Save RFLAGS before CLI so IF is preserved for the outgoing thread.
        "pushfq",
        "pop rax",
        "mov [rdi + 17*8], rax",
        "cli",
        "mov [rdi + 0*8], rax",
        "mov [rdi + 1*8], rbx",
        "mov [rdi + 2*8], rcx",
        "mov [rdi + 3*8], rdx",
        "mov [rdi + 4*8], rsi",
        "mov [rdi + 5*8], rdi",
        "mov [rdi + 6*8], rbp",
        "mov [rdi + 7*8], rsp",
        "mov [rdi + 8*8], r8",
        "mov [rdi + 9*8], r9",
        "mov [rdi + 10*8], r10",
        "mov [rdi + 11*8], r11",
        "mov [rdi + 12*8], r12",
        "mov [rdi + 13*8], r13",
        "mov [rdi + 14*8], r14",
        "mov [rdi + 15*8], r15",
        "lea rax, [rip + .Lctx_resume]",
        "mov [rdi + 16*8], rax",
        "mov ax, cs",
        "mov [rdi + 18*8], rax",
        "mov ax, ss",
        "mov [rdi + 19*8], rax",
        "mov rax, cr3",
        "mov [rdi + 24*8], rax",
        "fxsave64 [rdi + 208]",
        "mov byte ptr [rdi + 720], 1",
        // CR3 if the incoming thread has a distinct page table.
        "mov rax, [rsi + 24*8]",
        "test rax, rax",
        "jz 2f",
        "mov rcx, cr3",
        "cmp rax, rcx",
        "je 2f",
        "mov cr3, rax",
        "2:",
        "cmp byte ptr [rsi + 720], 0",
        "je 3f",
        "fxrstor64 [rsi + 208]",
        "3:",
        // User CS.RPL == 3 → iretq. Kernel → load RSP and jmp RIP.
        "mov rax, [rsi + 18*8]",
        "and rax, 3",
        "jnz 4f",
        "mov rax, [rsi + 17*8]",
        "push rax",
        "popfq",
        "mov rbx, [rsi + 1*8]",
        "mov rcx, [rsi + 2*8]",
        "mov rdx, [rsi + 3*8]",
        "mov rbp, [rsi + 6*8]",
        "mov r8,  [rsi + 8*8]",
        "mov r9,  [rsi + 9*8]",
        "mov r10, [rsi + 10*8]",
        "mov r11, [rsi + 11*8]",
        "mov r12, [rsi + 12*8]",
        "mov r13, [rsi + 13*8]",
        "mov r14, [rsi + 14*8]",
        "mov r15, [rsi + 15*8]",
        "mov rax, [rsi + 16*8]",
        "mov rdi, [rsi + 5*8]",
        "mov rsp, [rsi + 7*8]",
        "mov rsi, [rsi + 4*8]",
        "jmp rax",
        // iretq frame: SS, RSP, RFLAGS, CS, RIP
        "4:",
        "mov rax, [rsi + 19*8]",
        "push rax",
        "mov rax, [rsi + 7*8]",
        "push rax",
        "mov rax, [rsi + 17*8]",
        "or rax, 0x200",
        "push rax",
        "mov rax, [rsi + 18*8]",
        "push rax",
        "mov rax, [rsi + 16*8]",
        "push rax",
        "mov rax, [rsi + 21*8]",
        "mov es, ax",
        "mov rax, [rsi + 20*8]",
        "mov ds, ax",
        "mov rbx, [rsi + 1*8]",
        "mov rcx, [rsi + 2*8]",
        "mov rdx, [rsi + 3*8]",
        "mov rbp, [rsi + 6*8]",
        "mov r8,  [rsi + 8*8]",
        "mov r9,  [rsi + 9*8]",
        "mov r10, [rsi + 10*8]",
        "mov r11, [rsi + 11*8]",
        "mov r12, [rsi + 12*8]",
        "mov r13, [rsi + 13*8]",
        "mov r14, [rsi + 14*8]",
        "mov r15, [rsi + 15*8]",
        "mov rdi, [rsi + 5*8]",
        "mov rax, [rsi + 0*8]",
        "mov rsi, [rsi + 4*8]",
        // Mirror jump_to_user_mode: syscall's swapgs expects user GS here.
        "swapgs",
        "iretq",
        ".Lctx_resume:",
        "ret",
    );
}

/// Create a context for a new process
pub fn create_process_context(pid: Pid) {
    let pc = ProcessContext::new(pid);
    PROCESS_CONTEXTS.lock().push(Box::new(pc));
}

/// Create a kernel thread context
pub fn create_kernel_thread_context(pid: Pid, entry_point: u64) {
    let pc = ProcessContext::new_kernel_thread(pid, entry_point);
    PROCESS_CONTEXTS.lock().push(Box::new(pc));
}

/// Create a user-mode process context with entry point, stack, and CR3
pub fn create_user_process_context(pid: Pid, entry_point: u64, user_stack: u64, cr3: u64) {
    let mut pc = ProcessContext::new(pid);
    pc.context = CpuContext::new_user_thread(entry_point, user_stack);
    pc.context.cr3 = cr3;
    pc.in_user_mode = true;
    serial_println!(
        "[context] User process context PID={} entry={:#x} stack={:#x} cr3={:#x}",
        pid,
        entry_point,
        user_stack,
        cr3
    );
    PROCESS_CONTEXTS.lock().push(Box::new(pc));
}

/// Remove a process context
pub fn destroy_process_context(pid: Pid) {
    PROCESS_CONTEXTS.lock().retain(|pc| pc.pid != pid);
}

/// Get current process PID
pub fn current_pid() -> u32 {
    CURRENT_PID.load(Ordering::Relaxed)
}

/// Set current process PID
pub fn set_current_pid(pid: u32) {
    CURRENT_PID.store(pid, Ordering::Relaxed);
}

pub fn switch_count() -> u64 {
    SWITCH_COUNT.load(Ordering::Relaxed)
}

pub fn has_runnable_context(pid: Pid) -> bool {
    PROCESS_CONTEXTS
        .lock()
        .iter()
        .find(|pc| pc.pid == pid)
        .is_some_and(|pc| pc.context.is_runnable())
}

/// Clear RIP so this PID cannot be dispatched until a real save or entry is set.
pub fn invalidate_rip(pid: Pid) {
    let mut contexts = PROCESS_CONTEXTS.lock();
    if let Some(pc) = contexts.iter_mut().find(|pc| pc.pid == pid) {
        pc.context.rip = 0;
    }
}

/// Called from timer interrupt to potentially switch processes
pub fn schedule_tick() {
    let mut scheduler = crate::scheduler::SCHEDULER.lock();
    if scheduler.tick() {
        if let Some(next_pid) = scheduler.schedule() {
            drop(scheduler);
            if next_pid != current_pid() && has_runnable_context(next_pid) {
                unsafe {
                    switch_to(next_pid);
                }
            }
        }
    }
}

/// Switch to a specific process by PID.
///
/// # Safety
/// Must be called with the scheduler / `PROCESS_CONTEXTS` lock **not** held.
pub unsafe fn switch_to(next_pid: Pid) {
    let current = current_pid();
    if next_pid == current {
        return;
    }

    let (old_ptr, new_ptr) = {
        let mut contexts = PROCESS_CONTEXTS.lock();
        let old_i = match contexts.iter().position(|pc| pc.pid == current) {
            Some(i) => i,
            None => return,
        };
        let new_i = match contexts.iter().position(|pc| pc.pid == next_pid) {
            Some(i) => i,
            None => return,
        };
        if !contexts[new_i].context.is_runnable() {
            return;
        }
        (
            &mut contexts[old_i].context as *mut CpuContext,
            &contexts[new_i].context as *const CpuContext,
        )
    };

    set_current_pid(next_pid);
    switch_context(&mut *old_ptr, &*new_ptr);
}

/// Cooperative yield to the idle thread (HLT). Returns when idle switches back.
pub fn yield_to_idle() {
    if current_pid() == IDLE_PID {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("sti; hlt", options(nomem, nostack));
        }
        return;
    }
    if !has_runnable_context(IDLE_PID) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("sti; hlt", options(nomem, nostack));
        }
        return;
    }
    unsafe {
        switch_to(IDLE_PID);
    }
}

extern "C" fn idle_loop() -> ! {
    loop {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("sti; hlt", options(nomem, nostack));
        }
        // After an interrupt, resume the desktop executor if it has a saved RIP.
        if has_runnable_context(DESKTOP_PID) {
            unsafe {
                switch_to(DESKTOP_PID);
            }
        }
    }
}

static TEST_A: AtomicU64 = AtomicU64::new(0);
static TEST_B: AtomicU64 = AtomicU64::new(0);

const TEST_PID_A: Pid = 100;
const TEST_PID_B: Pid = 101;

extern "C" fn kthread_a() -> ! {
    TEST_A.fetch_add(1, Ordering::SeqCst);
    unsafe {
        switch_to(TEST_PID_B);
    }
    TEST_A.fetch_add(1, Ordering::SeqCst);
    unsafe {
        switch_to(DESKTOP_PID);
    }
    loop {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}

extern "C" fn kthread_b() -> ! {
    TEST_B.fetch_add(1, Ordering::SeqCst);
    unsafe {
        switch_to(TEST_PID_A);
    }
    loop {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}

/// Switch two kernel threads with different RIP, then return to the caller.
fn self_test_rip_switch() {
    TEST_A.store(0, Ordering::SeqCst);
    TEST_B.store(0, Ordering::SeqCst);
    create_kernel_thread_context(TEST_PID_A, kthread_a as *const () as u64);
    create_kernel_thread_context(TEST_PID_B, kthread_b as *const () as u64);

    let before = switch_count();
    unsafe {
        switch_to(TEST_PID_A);
    }
    let a = TEST_A.load(Ordering::SeqCst);
    let b = TEST_B.load(Ordering::SeqCst);
    destroy_process_context(TEST_PID_A);
    destroy_process_context(TEST_PID_B);
    invalidate_rip(DESKTOP_PID);
    set_current_pid(DESKTOP_PID);

    if a >= 2 && b >= 1 && switch_count() > before {
        serial_println!(
            "[context] RIP switch self-test passed (A={} B={} switches={})",
            a,
            b,
            switch_count()
        );
    } else {
        serial_println!(
            "[context] RIP switch self-test FAILED (A={} B={} switches={})",
            a,
            b,
            switch_count()
        );
    }
}

/// Initialize context switching
pub fn init() {
    create_kernel_thread_context(IDLE_PID, idle_loop as *const () as u64);
    create_process_context(1); // init (no entry until userspace)
    create_process_context(DESKTOP_PID);
    set_current_pid(DESKTOP_PID);
    serial_println!("[KnoxOS] Context switching initialized");
    serial_println!("[KnoxOS]   Idle thread PID={} (HLT)", IDLE_PID);
    self_test_rip_switch();
}

/// Get a copy of the user context for a process (for signal delivery)
pub fn get_user_context(pid: Pid) -> Option<CpuContext> {
    let contexts = PROCESS_CONTEXTS.lock();
    contexts
        .iter()
        .find(|pc| pc.pid == pid)
        .map(|pc| pc.context)
}

/// Set the user context RIP, RSP, and first argument (RDI) for signal delivery
pub fn set_user_context(pid: Pid, rip: u64, rsp: u64, rdi: u64) {
    let mut contexts = PROCESS_CONTEXTS.lock();
    if let Some(pc) = contexts.iter_mut().find(|pc| pc.pid == pid) {
        pc.context.rip = rip;
        pc.context.rsp = rsp;
        pc.context.rdi = rdi;
    }
}

/// Restore full user context from a signal frame (sigreturn)
pub fn restore_user_context(pid: Pid, frame: &crate::signals::SignalFrame) {
    let mut contexts = PROCESS_CONTEXTS.lock();
    if let Some(pc) = contexts.iter_mut().find(|pc| pc.pid == pid) {
        pc.context.rip = frame.saved_rip;
        pc.context.rsp = frame.saved_rsp;
        pc.context.rflags = frame.saved_rflags;
        pc.context.rax = frame.saved_rax;
        pc.context.rbx = frame.saved_rbx;
        pc.context.rcx = frame.saved_rcx;
        pc.context.rdx = frame.saved_rdx;
        pc.context.rsi = frame.saved_rsi;
        pc.context.rdi = frame.saved_rdi;
        pc.context.rbp = frame.saved_rbp;
        pc.context.r8 = frame.saved_r8;
        pc.context.r9 = frame.saved_r9;
        pc.context.r10 = frame.saved_r10;
        pc.context.r11 = frame.saved_r11;
        pc.context.r12 = frame.saved_r12;
        pc.context.r13 = frame.saved_r13;
        pc.context.r14 = frame.saved_r14;
        pc.context.r15 = frame.saved_r15;
    }
}
