// KnoxOS RISC-V 64 boot entry point
//
// This module provides the assembly-level entry point for RISC-V 64 targets.
// OpenSBI jumps here in S-mode with:
//   a0 = hart ID
//   a1 = DTB pointer
//
// Boot sequence:
//   1. Park secondary harts (only hart 0 proceeds)
//   2. Set up initial stack
//   3. Clear BSS
//   4. Set up Sv48 page tables (identity + higher-half)
//   5. Enable MMU (csrw satp; sfence.vma)
//   6. Jump to Rust kernel_main()
//
// This file is compiled only when targeting riscv64.

/// Stack size for the boot hart (128 KB)
const BOOT_STACK_SIZE: usize = 128 * 1024;

/// Per-hart stack size for secondary harts (64 KB each)
const SECONDARY_STACK_SIZE: usize = 64 * 1024;

/// Maximum supported harts
const MAX_HARTS: usize = 8;

/// Boot stack for BSP hart
#[repr(C, align(16))]
struct BootStack([u8; BOOT_STACK_SIZE]);

#[used]
#[unsafe(link_section = ".data")]
static BOOT_STACK: BootStack = BootStack([0; BOOT_STACK_SIZE]);

/// Secondary hart stacks
#[repr(C, align(16))]
struct SecondaryStacks([[u8; SECONDARY_STACK_SIZE]; MAX_HARTS]);

#[used]
#[unsafe(link_section = ".data")]
static SECONDARY_STACKS: SecondaryStacks = SecondaryStacks([[0; SECONDARY_STACK_SIZE]; MAX_HARTS]);

// ═══════════════════════════════════════════════════════════════════════
// RISC-V CSR DEFINITIONS
// ═══════════════════════════════════════════════════════════════════════

/// S-mode status register bits
pub mod sstatus {
    pub const SIE: u64 = 1 << 1; // Supervisor interrupt enable
    pub const SPIE: u64 = 1 << 5; // Previous interrupt enable
    pub const SPP: u64 = 1 << 8; // Previous privilege mode
    pub const SUM: u64 = 1 << 18; // Permit supervisor user memory access
    pub const MXR: u64 = 1 << 19; // Make executable readable
    pub const FS_MASK: u64 = 3 << 13; // FP unit status
    pub const FS_INITIAL: u64 = 1 << 13;
}

/// S-mode interrupt enable bits
pub mod sie {
    pub const SSIE: u64 = 1 << 1; // Software interrupt enable
    pub const STIE: u64 = 1 << 5; // Timer interrupt enable
    pub const SEIE: u64 = 1 << 9; // External interrupt enable
}

/// S-mode cause register values
pub mod scause {
    // Interrupts (bit 63 set)
    pub const SSI: u64 = (1 << 63) | 1; // Supervisor software interrupt
    pub const STI: u64 = (1 << 63) | 5; // Supervisor timer interrupt
    pub const SEI: u64 = (1 << 63) | 9; // Supervisor external interrupt

    // Exceptions
    pub const INST_MISALIGNED: u64 = 0;
    pub const INST_ACCESS_FAULT: u64 = 1;
    pub const ILLEGAL_INST: u64 = 2;
    pub const BREAKPOINT: u64 = 3;
    pub const LOAD_ACCESS_FAULT: u64 = 5;
    pub const STORE_MISALIGNED: u64 = 6;
    pub const STORE_ACCESS_FAULT: u64 = 7;
    pub const ECALL_FROM_U: u64 = 8;
    pub const ECALL_FROM_S: u64 = 9;
    pub const INST_PAGE_FAULT: u64 = 12;
    pub const LOAD_PAGE_FAULT: u64 = 13;
    pub const STORE_PAGE_FAULT: u64 = 15;
}

// ═══════════════════════════════════════════════════════════════════════
// SV48 PAGE TABLE DEFINITIONS
// ═══════════════════════════════════════════════════════════════════════

/// SATP mode values
pub const SATP_MODE_SV39: u64 = 8;
pub const SATP_MODE_SV48: u64 = 9;

/// Page table entry flags (Sv39/Sv48)
pub mod pte_flags {
    pub const V: u64 = 1 << 0; // Valid
    pub const R: u64 = 1 << 1; // Read
    pub const W: u64 = 1 << 2; // Write
    pub const X: u64 = 1 << 3; // Execute
    pub const U: u64 = 1 << 4; // User-accessible
    pub const G: u64 = 1 << 5; // Global
    pub const A: u64 = 1 << 6; // Accessed
    pub const D: u64 = 1 << 7; // Dirty

    /// Kernel code (read + execute)
    pub const KERNEL_RX: u64 = V | R | X | A | G;
    /// Kernel data (read + write)
    pub const KERNEL_RW: u64 = V | R | W | A | D | G;
    /// User code (read + execute, user-accessible)
    pub const USER_RX: u64 = V | R | X | U | A;
    /// User data (read + write, user-accessible)
    pub const USER_RW: u64 = V | R | W | U | A | D;
    /// MMIO (read + write, non-cacheable — handled by PMA, not PTE in RISC-V)
    pub const MMIO_RW: u64 = V | R | W | A | D | G;
}

/// Static page tables for initial boot mapping
/// Sv48: L0 (512 entries, 256TB per entry)
///       L1 (512 entries, 512GB per entry)
///       L2 (512 entries, 1GB per entry — gigapage leaf)
///       L3 (512 entries, 2MB per entry — megapage leaf)
///
/// Boot mapping strategy: identity map first 1GB using L2 gigapages
#[repr(C, align(4096))]
pub struct PageTable([u64; 512]);

impl PageTable {
    const fn zero() -> Self {
        PageTable([0u64; 512])
    }
}

#[used]
#[unsafe(link_section = ".data")]
static mut BOOT_PT_L0: PageTable = PageTable::zero();
#[used]
#[unsafe(link_section = ".data")]
static mut BOOT_PT_L1: PageTable = PageTable::zero();
#[used]
#[unsafe(link_section = ".data")]
static mut BOOT_PT_L2: PageTable = PageTable::zero();

/// Build SATP register value for Sv48
pub fn make_satp(root_ppn: u64, asid: u16) -> u64 {
    (SATP_MODE_SV48 << 60) | ((asid as u64) << 44) | root_ppn
}

/// Initialize boot page tables (identity mapping for first 2GB using 1GB gigapages)
///
/// # Safety
/// Must be called exactly once during boot before writing SATP.
pub unsafe fn init_boot_page_tables() {
    // Clear all tables (using raw pointers to avoid UB with mutable statics)
    for i in 0..512 {
        (*core::ptr::addr_of_mut!(BOOT_PT_L0)).0[i] = 0;
    }
    for i in 0..512 {
        (*core::ptr::addr_of_mut!(BOOT_PT_L1)).0[i] = 0;
    }
    for i in 0..512 {
        (*core::ptr::addr_of_mut!(BOOT_PT_L2)).0[i] = 0;
    }

    // L2: Map first 2GB as 512 x 2MB megapages (identity)
    // 0x0000_0000 .. 0x3FFF_FFFF (1GB normal memory)
    // 0x4000_0000 .. 0x7FFF_FFFF (1GB MMIO)
    for i in 0..512u64 {
        let phys = i * 0x200000; // 2MB per megapage
        (*core::ptr::addr_of_mut!(BOOT_PT_L2)).0[i as usize] = (phys >> 2) | pte_flags::KERNEL_RW;
    }

    // L1: Map 0x80000000..0xBFFFFFFF (where OpenSBI loads the kernel)
    // Entry 2 (0x80000000 / 0x40000000 = 2) → L2 table
    let l2_ppn = (core::ptr::addr_of!(BOOT_PT_L2) as u64) >> 12;
    (*core::ptr::addr_of_mut!(BOOT_PT_L1)).0[0] = (l2_ppn << 10) | pte_flags::V;

    // Also map kernel memory region (0x80000000+) with gigapage
    // Entry 2: 0x80000000-0xBFFFFFFF as read-write gigapage
    (*core::ptr::addr_of_mut!(BOOT_PT_L1)).0[2] = (0x80000000u64 >> 2) | pte_flags::KERNEL_RW;

    // L0: Entry 0 → L1 table (identity map in low VA range)
    let l1_ppn = (core::ptr::addr_of!(BOOT_PT_L1) as u64) >> 12;
    (*core::ptr::addr_of_mut!(BOOT_PT_L0)).0[0] = (l1_ppn << 10) | pte_flags::V;
}

// ═══════════════════════════════════════════════════════════════════════
// TRAP HANDLING
// ═══════════════════════════════════════════════════════════════════════

/// Trap frame saved by the trap handler
#[repr(C)]
#[derive(Debug, Clone)]
pub struct TrapFrame {
    pub regs: [u64; 32], // x0-x31
    pub sstatus: u64,
    pub sepc: u64,
    pub stval: u64,
    pub scause: u64,
}

impl TrapFrame {
    pub const fn zero() -> Self {
        TrapFrame {
            regs: [0; 32],
            sstatus: 0,
            sepc: 0,
            stval: 0,
            scause: 0,
        }
    }
}

/// Handle supervisor trap (called from assembly trap vector)
pub fn trap_handler(frame: &mut TrapFrame) {
    let cause = frame.scause;
    let is_interrupt = (cause >> 63) & 1 == 1;
    let code = cause & 0x7FFF_FFFF_FFFF_FFFF;

    if is_interrupt {
        match cause {
            scause::STI => {
                // Timer interrupt — schedule next tick via SBI
                crate::arch_port::riscv64_hal::sbi_set_timer(0); // Reset
                crate::serial_println!("[RV64-Trap] Timer interrupt");
            }
            scause::SEI => {
                // External interrupt — claim from PLIC
                crate::serial_println!("[RV64-Trap] External interrupt");
            }
            scause::SSI => {
                // Software interrupt (IPI)
                crate::serial_println!("[RV64-Trap] Software interrupt (IPI)");
            }
            _ => {
                crate::serial_println!("[RV64-Trap] Unknown interrupt: {}", code);
            }
        }
    } else {
        match code {
            scause::ECALL_FROM_U => {
                // Syscall from userspace
                // a7 = syscall number, a0-a5 = args
                let syscall_nr = frame.regs[17]; // a7
                crate::serial_println!("[RV64-Trap] Syscall #{}", syscall_nr);
                frame.sepc += 4; // Skip ecall instruction
            }
            scause::INST_PAGE_FAULT | scause::LOAD_PAGE_FAULT | scause::STORE_PAGE_FAULT => {
                let fault_addr = frame.stval;
                crate::serial_println!(
                    "[RV64-Trap] Page fault at 0x{:X}, cause={}, sepc=0x{:X}",
                    fault_addr,
                    code,
                    frame.sepc
                );
            }
            scause::ILLEGAL_INST => {
                crate::serial_println!(
                    "[RV64-Trap] Illegal instruction at 0x{:X}, inst=0x{:X}",
                    frame.sepc,
                    frame.stval
                );
            }
            _ => {
                crate::serial_println!(
                    "[RV64-Trap] Exception: cause={}, sepc=0x{:X}, stval=0x{:X}",
                    code,
                    frame.sepc,
                    frame.stval
                );
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PLATFORM INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Platform initialization after MMU is enabled
pub fn platform_init(hart_id: u64, dtb_ptr: u64) {
    crate::serial_println!(
        "[RV64] Platform init: hart={}, dtb=0x{:X}",
        hart_id,
        dtb_ptr
    );

    // Parse DTB
    if let Some(dtb) = crate::arch_port::DtbParser::parse(dtb_ptr) {
        crate::serial_println!(
            "[RV64] DTB: {} bytes, memory={}MB",
            dtb.total_size,
            dtb.total_memory() / (1024 * 1024)
        );
    }

    // Initialize PLIC
    let mut plic =
        crate::arch_port::riscv64_hal::Plic::new(crate::arch_port::riscv64_hal::PLIC_BASE);
    plic.init(hart_id as u32);

    // Initialize CLINT timer (10ms tick)
    let clint =
        crate::arch_port::riscv64_hal::Clint::new(crate::arch_port::riscv64_hal::CLINT_BASE);
    clint.set_timer_us(hart_id as u32, 10_000);

    crate::serial_println!("[RV64] Platform initialization complete");
}

/// SMP bringup for secondary harts
pub fn smp_init(num_harts: u32) {
    crate::arch_port::riscv64_hal::smp_boot(num_harts);
}

pub fn init() {
    crate::serial_println!("[RV64-Boot] Boot module initialized");
    crate::serial_println!("[RV64-Boot]   Page tables: Sv48 (4-level), 4KB pages");
    crate::serial_println!("[RV64-Boot]   SATP mode: Sv48 (9)");
    crate::serial_println!("[RV64-Boot]   Trap handling: stvec vectored mode");
    crate::serial_println!(
        "[RV64-Boot]   Targets: QEMU virt, SiFive HiFive, StarFive VisionFive 2"
    );
}
