// KnoxOS AArch64 boot entry point
//
// This module provides the assembly-level entry point for AArch64 targets.
// The bootloader (UEFI or Linux arm64 Image protocol) jumps here.
// x0 = DTB pointer, x1-x3 = reserved
//
// Boot sequence:
//   1. Park secondary cores (only BSP proceeds)
//   2. Set up initial stack
//   3. Clear BSS
//   4. Set up MMU with identity + higher-half mapping
//   5. Enable MMU (SCTLR_EL1.M | C | I)
//   6. Jump to Rust kernel_main()
//
// This file is compiled only when targeting aarch64.

/// Stack size for the boot CPU (128 KB)
const BOOT_STACK_SIZE: usize = 128 * 1024;

/// Boot stack (statically allocated, aligned to 16 bytes per AAPCS64)
#[repr(C, align(16))]
struct BootStack([u8; BOOT_STACK_SIZE]);

#[used]
#[unsafe(link_section = ".data")]
static BOOT_STACK: BootStack = BootStack([0; BOOT_STACK_SIZE]);

/// AArch64 exception vector table stub
/// Four groups of four vectors, each 0x80 bytes (32 instructions) apart:
///   - Current EL with SP_EL0 (sync, irq, fiq, serror)
///   - Current EL with SP_ELx (sync, irq, fiq, serror)
///   - Lower EL using AArch64 (sync, irq, fiq, serror)
///   - Lower EL using AArch32 (sync, irq, fiq, serror)
#[repr(C, align(2048))]
pub struct ExceptionVectorTable {
    pub vectors: [u8; 2048],
}

#[used]
#[unsafe(link_section = ".rodata")]
static EXCEPTION_VECTORS: ExceptionVectorTable = ExceptionVectorTable { vectors: [0; 2048] };

/// TCR_EL1 configuration for 48-bit VA with 4KB granules
pub const TCR_EL1_VALUE: u64 = {
    let t0sz = 16u64; // 48-bit VA for TTBR0
    let t1sz = 16u64 << 16; // 48-bit VA for TTBR1
    let tg0_4k = 0u64 << 14; // 4KB granule for TTBR0
    let tg1_4k = 2u64 << 30; // 4KB granule for TTBR1
    let sh0 = 3u64 << 12; // Inner shareable
    let sh1 = 3u64 << 28; // Inner shareable
    let orgn0 = 1u64 << 10; // Outer write-back
    let irgn0 = 1u64 << 8; // Inner write-back
    let orgn1 = 1u64 << 26; // Outer write-back
    let irgn1 = 1u64 << 24; // Inner write-back
    let ips = 5u64 << 32; // 48-bit physical address
    t0sz | t1sz | tg0_4k | tg1_4k | sh0 | sh1 | orgn0 | irgn0 | orgn1 | irgn1 | ips
};

/// MAIR_EL1 attribute indices
/// Index 0: Device-nGnRnE (0x00)
/// Index 1: Normal Write-Back cacheable (0xFF)
/// Index 2: Normal Non-Cacheable (0x44)
pub const MAIR_EL1_VALUE: u64 = 0x000000000044FF00;

/// SCTLR_EL1 bits to enable MMU
pub const SCTLR_M: u64 = 1 << 0; // MMU enable
pub const SCTLR_C: u64 = 1 << 2; // Data cache enable
pub const SCTLR_I: u64 = 1 << 12; // Instruction cache enable
pub const SCTLR_SPAN: u64 = 1 << 23; // Set PAN on EL change
pub const SCTLR_WXN: u64 = 1 << 19; // Write-execute-never

/// AArch64 page table entry flags (stage 1, 4KB granule)
pub mod pte_flags {
    pub const VALID: u64 = 1 << 0;
    pub const TABLE: u64 = 1 << 1; // For L0-L2 table descriptors
    pub const PAGE: u64 = 1 << 1; // For L3 page descriptors
    pub const AF: u64 = 1 << 10; // Access flag
    pub const SH_INNER: u64 = 3 << 8; // Inner shareable
    pub const AP_RW_EL1: u64 = 0 << 6; // Read-write at EL1
    pub const AP_RO_EL1: u64 = 2 << 6; // Read-only at EL1
    pub const AP_RW_EL0: u64 = 1 << 6; // Read-write at EL0
    pub const UXN: u64 = 1 << 54; // Unprivileged execute never
    pub const PXN: u64 = 1 << 53; // Privileged execute never
    pub const ATTR_DEVICE: u64 = 0 << 2; // MAIR index 0
    pub const ATTR_NORMAL: u64 = 1 << 2; // MAIR index 1
    pub const ATTR_NC: u64 = 2 << 2; // MAIR index 2

    /// Kernel code (read-only, executable)
    pub const KERNEL_RX: u64 = VALID | PAGE | AF | SH_INNER | AP_RO_EL1 | UXN | ATTR_NORMAL;
    /// Kernel data (read-write, not executable)
    pub const KERNEL_RW: u64 = VALID | PAGE | AF | SH_INNER | AP_RW_EL1 | UXN | PXN | ATTR_NORMAL;
    /// Device MMIO (read-write, non-cacheable, not executable)
    pub const DEVICE_RW: u64 = VALID | PAGE | AF | AP_RW_EL1 | UXN | PXN | ATTR_DEVICE;
    /// User code (read-only, executable at EL0)
    pub const USER_RX: u64 =
        VALID | PAGE | AF | SH_INNER | AP_RO_EL1 | AP_RW_EL0 | PXN | ATTR_NORMAL;
    /// User data (read-write at EL0, not executable)
    pub const USER_RW: u64 = VALID | PAGE | AF | SH_INNER | AP_RW_EL0 | UXN | PXN | ATTR_NORMAL;
}

/// Static page tables for initial boot mapping (identity + higher-half)
/// 4 levels: L0 (PGD) → L1 (PUD) → L2 (PMD) → L3 (PTE)
/// We use 2MB block mappings at L2 for the first 1GB identity map
#[repr(C, align(4096))]
pub struct PageTable([u64; 512]);

impl PageTable {
    const fn zero() -> Self {
        PageTable([0u64; 512])
    }
}

#[used]
#[unsafe(link_section = ".data")]
static mut BOOT_PGD_LOW: PageTable = PageTable::zero();
#[used]
#[unsafe(link_section = ".data")]
static mut BOOT_PGD_HIGH: PageTable = PageTable::zero();
#[used]
#[unsafe(link_section = ".data")]
static mut BOOT_PUD: PageTable = PageTable::zero();
#[used]
#[unsafe(link_section = ".data")]
static mut BOOT_PMD: PageTable = PageTable::zero();

/// Initialize the boot page tables with identity mapping (first 1GB via 2MB blocks)
///
/// # Safety
/// Must be called exactly once during boot before enabling MMU.
pub unsafe fn init_boot_page_tables() {
    // Clear all tables (using raw pointers to avoid UB with mutable statics)
    for i in 0..512 {
        (*core::ptr::addr_of_mut!(BOOT_PGD_LOW)).0[i] = 0;
    }
    for i in 0..512 {
        (*core::ptr::addr_of_mut!(BOOT_PGD_HIGH)).0[i] = 0;
    }
    for i in 0..512 {
        (*core::ptr::addr_of_mut!(BOOT_PUD)).0[i] = 0;
    }
    for i in 0..512 {
        (*core::ptr::addr_of_mut!(BOOT_PMD)).0[i] = 0;
    }

    // L2 (PMD): Map first 1GB as 512 x 2MB blocks (identity)
    for i in 0..512u64 {
        let phys = i * 0x200000; // 2MB per block
        let flags = if phys < 0x4000_0000 {
            // Normal memory for first 1GB
            pte_flags::VALID | pte_flags::AF | pte_flags::SH_INNER | pte_flags::ATTR_NORMAL
        } else {
            // Device memory above 1GB
            pte_flags::VALID | pte_flags::AF | pte_flags::ATTR_DEVICE
        };
        (*core::ptr::addr_of_mut!(BOOT_PMD)).0[i as usize] = phys | flags;
    }

    // L1 (PUD): Entry 0 points to PMD table
    let pmd_addr = core::ptr::addr_of!(BOOT_PMD) as u64;
    (*core::ptr::addr_of_mut!(BOOT_PUD)).0[0] = pmd_addr | pte_flags::VALID | pte_flags::TABLE;

    // L0 (PGD low): Entry 0 points to PUD — identity map at VA 0x0
    let pud_addr = core::ptr::addr_of!(BOOT_PUD) as u64;
    (*core::ptr::addr_of_mut!(BOOT_PGD_LOW)).0[0] = pud_addr | pte_flags::VALID | pte_flags::TABLE;

    // L0 (PGD high): Entry 256 points to same PUD — higher-half at 0xFFFF_0000_0000_0000
    (*core::ptr::addr_of_mut!(BOOT_PGD_HIGH)).0[256] =
        pud_addr | pte_flags::VALID | pte_flags::TABLE;
}

/// PSCI function codes for power management
pub mod psci {
    pub const VERSION: u32 = 0x84000000;
    pub const CPU_ON_64: u32 = 0xC4000003;
    pub const CPU_OFF: u32 = 0x84000002;
    pub const SYSTEM_OFF: u32 = 0x84000008;
    pub const SYSTEM_RESET: u32 = 0x84000009;
    pub const CPU_SUSPEND_64: u32 = 0xC4000001;
}

/// Secondary CPU entry context
#[repr(C)]
pub struct SecondaryCpuContext {
    pub stack_top: u64,
    pub page_table: u64,
    pub kernel_entry: u64,
    pub cpu_id: u64,
}

/// Platform initialization routine called after MMU is enabled
pub fn platform_init(dtb_ptr: u64) {
    // Parse DTB for hardware discovery
    if let Some(dtb) = crate::arch_port::DtbParser::parse(dtb_ptr) {
        crate::serial_println!(
            "[AArch64] DTB parsed: {} bytes, memory={}MB",
            dtb.total_size,
            dtb.total_memory() / (1024 * 1024)
        );
    }

    // Initialize GICv3
    let mut gic = crate::arch_port::aarch64_hal::Gicv3::new(0x0800_0000, 0x080A_0000);
    gic.init_distributor();
    gic.init_redistributor(0);
    gic.init_cpu_interface();

    // Initialize generic timer (10ms tick)
    let timer = crate::arch_port::aarch64_hal::GenericTimer::new();
    timer.set_timer_ns(10_000_000);

    // Initialize SMMUv3 if present
    let mut smmu = crate::arch_port::aarch64_hal::Smmuv3::new(0x0900_0000);
    smmu.init();

    crate::serial_println!("[AArch64] Platform initialization complete");
}

/// Perform SMP bringup for secondary cores
pub fn smp_init(num_cpus: u32) {
    crate::arch_port::aarch64_hal::smp_boot(num_cpus);
}

pub fn init() {
    crate::serial_println!("[AArch64-Boot] Boot module initialized");
    crate::serial_println!("[AArch64-Boot]   Page tables: 4-level (L0-L3), 4KB granule, 48-bit VA");
    crate::serial_println!("[AArch64-Boot]   TCR_EL1 = 0x{:016X}", TCR_EL1_VALUE);
    crate::serial_println!("[AArch64-Boot]   MAIR_EL1 = 0x{:016X}", MAIR_EL1_VALUE);
    crate::serial_println!("[AArch64-Boot]   Targets: Raspberry Pi 4/5, Apple M1/M2/M3, QEMU virt");
}
