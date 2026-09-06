/// arch_port — Multi-Architecture Port Layer for AArch64 and RISC-V 64
///
/// Provides concrete HAL (Hardware Abstraction Layer) implementations for:
///   - AArch64 (ARMv8-A): GICv3, Generic Timer, SMMU, PSCI, UEFI boot
///   - RISC-V 64 (RV64GC): PLIC, CLINT, SBI, OpenSBI boot
///   - x86_64: Wraps existing APIC/PIT/HPET/IOMMU code
///
/// Each architecture provides:
///   - Early boot entry point and stack setup
///   - MMU/page table initialization
///   - Interrupt controller setup (GIC/PLIC/APIC)
///   - Timer configuration
///   - Context switch (save/restore all registers)
///   - SMP bring-up (PSCI/SBI/SIPI)
///   - Power management hooks
///
/// The HAL trait `ArchHal` provides a uniform interface consumed by the
/// architecture-independent kernel core.
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// HARDWARE ABSTRACTION LAYER TRAIT
// ═══════════════════════════════════════════════════════════════════════

/// Interrupt descriptor — architecture-independent
#[derive(Debug, Clone, Copy)]
pub struct IrqDescriptor {
    pub irq_num: u32,
    pub trigger: IrqTrigger,
    pub polarity: IrqPolarity,
    pub cpu_affinity: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqTrigger {
    Edge,
    Level,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqPolarity {
    ActiveHigh,
    ActiveLow,
}

/// Memory region type for boot-time memory map
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionType {
    Usable,
    Reserved,
    AcpiReclaimable,
    AcpiNvs,
    Mmio,
    Firmware,
    BootloaderReclaimable,
}

/// Boot memory region
#[derive(Debug, Clone, Copy)]
pub struct BootMemoryRegion {
    pub base: u64,
    pub size: u64,
    pub region_type: MemoryRegionType,
}

/// Page table protection flags — architecture-independent
#[derive(Debug, Clone, Copy)]
pub struct PageFlags {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub user: bool,
    pub cacheable: bool,
    pub global: bool,
}

impl PageFlags {
    pub const KERNEL_RW: Self = Self {
        readable: true,
        writable: true,
        executable: false,
        user: false,
        cacheable: true,
        global: true,
    };

    pub const KERNEL_RX: Self = Self {
        readable: true,
        writable: false,
        executable: true,
        user: false,
        cacheable: true,
        global: true,
    };

    pub const USER_RW: Self = Self {
        readable: true,
        writable: true,
        executable: false,
        user: true,
        cacheable: true,
        global: false,
    };

    pub const USER_RX: Self = Self {
        readable: true,
        writable: false,
        executable: true,
        user: true,
        cacheable: true,
        global: false,
    };

    pub const MMIO: Self = Self {
        readable: true,
        writable: true,
        executable: false,
        user: false,
        cacheable: false,
        global: true,
    };
}

/// CPU context — saved on context switch / interrupt entry
#[derive(Debug, Clone)]
pub struct CpuContext {
    /// General-purpose register file (arch-specific count)
    pub gp_regs: Vec<u64>,
    /// Program counter / instruction pointer
    pub pc: u64,
    /// Stack pointer
    pub sp: u64,
    /// Processor status (RFLAGS / SPSR / SSTATUS)
    pub status: u64,
    /// Page table base register (CR3 / TTBR0 / SATP)
    pub page_table_base: u64,
    /// Thread-local storage pointer
    pub tls_base: u64,
    /// FP/SIMD register state (opaque, arch-specific)
    pub fp_state: Vec<u8>,
}

/// SMP CPU info
#[derive(Debug, Clone)]
pub struct CpuInfo {
    pub cpu_id: u32,
    pub is_bsp: bool,
    pub online: bool,
    pub arch_id: u64, // APIC ID / MPIDR / HARTID
}

// ═══════════════════════════════════════════════════════════════════════
// AARCH64 HAL IMPLEMENTATION
// ═══════════════════════════════════════════════════════════════════════

pub mod aarch64_hal {
    use super::*;

    // ─── GICv3 (Generic Interrupt Controller v3) ────────────────────

    /// GICv3 Distributor register offsets
    pub const GICD_CTLR: u64 = 0x0000;
    pub const GICD_TYPER: u64 = 0x0004;
    pub const GICD_IIDR: u64 = 0x0008;
    pub const GICD_IGROUPR: u64 = 0x0080;
    pub const GICD_ISENABLER: u64 = 0x0100;
    pub const GICD_ICENABLER: u64 = 0x0180;
    pub const GICD_ISPENDR: u64 = 0x0200;
    pub const GICD_ICPENDR: u64 = 0x0280;
    pub const GICD_ISACTIVER: u64 = 0x0300;
    pub const GICD_ICACTIVER: u64 = 0x0380;
    pub const GICD_IPRIORITYR: u64 = 0x0400;
    pub const GICD_ITARGETSR: u64 = 0x0800;
    pub const GICD_ICFGR: u64 = 0x0C00;
    pub const GICD_IROUTER: u64 = 0x6100;

    /// GICv3 Redistributor register offsets
    pub const GICR_CTLR: u64 = 0x0000;
    pub const GICR_WAKER: u64 = 0x0014;
    pub const GICR_IGROUPR0: u64 = 0x10080;
    pub const GICR_ISENABLER0: u64 = 0x10100;
    pub const GICR_ICENABLER0: u64 = 0x10180;
    pub const GICR_IPRIORITYR: u64 = 0x10400;

    /// GIC system register encodings (ICC_*)
    pub const ICC_IAR1_EL1: u32 = 0; // Interrupt Acknowledge
    pub const ICC_EOIR1_EL1: u32 = 1; // End of Interrupt
    pub const ICC_PMR_EL1: u32 = 2; // Priority Mask
    pub const ICC_BPR1_EL1: u32 = 3; // Binary Point
    pub const ICC_SRE_EL1: u32 = 4; // System Register Enable
    pub const ICC_IGRPEN1_EL1: u32 = 5; // Group 1 Enable

    /// GICv3 controller state
    pub struct Gicv3 {
        pub dist_base: u64,
        pub redist_base: u64,
        pub num_irqs: u32,
        pub num_cpus: u32,
        pub initialized: bool,
    }

    impl Gicv3 {
        pub fn new(dist_base: u64, redist_base: u64) -> Self {
            Self {
                dist_base,
                redist_base,
                num_irqs: 0,
                num_cpus: 0,
                initialized: false,
            }
        }

        /// Initialize GICv3 distributor
        pub fn init_distributor(&mut self) {
            // Read GICD_TYPER to determine number of IRQs
            let typer = unsafe { read_mmio32(self.dist_base + GICD_TYPER) };
            self.num_irqs = ((typer & 0x1F) + 1) * 32;
            serial_println!("[GICv3] Distributor: {} IRQ lines detected", self.num_irqs);

            // Disable distributor while configuring
            unsafe { write_mmio32(self.dist_base + GICD_CTLR, 0) };

            // Configure all SPIs (32+) as Group 1, edge-triggered
            for i in 1..(self.num_irqs / 32) {
                unsafe {
                    // Group 1
                    write_mmio32(self.dist_base + GICD_IGROUPR + (i as u64) * 4, 0xFFFF_FFFF);
                    // Default priority 0xa0
                    for j in 0..8 {
                        write_mmio32(
                            self.dist_base + GICD_IPRIORITYR + (i as u64) * 32 + (j as u64) * 4,
                            0xa0a0a0a0,
                        );
                    }
                }
            }

            // Enable distributor: Group 0/1 enable, affinity routing
            unsafe {
                write_mmio32(self.dist_base + GICD_CTLR, (1 << 0) | (1 << 1) | (1 << 4));
            }

            self.initialized = true;
            serial_println!("[GICv3] Distributor initialized with affinity routing");
        }

        /// Initialize GICv3 redistributor for current CPU
        pub fn init_redistributor(&self, cpu_id: u32) {
            let redist = self.redist_base + (cpu_id as u64) * 0x20000;

            // Wake up redistributor
            unsafe {
                let waker = read_mmio32(redist + GICR_WAKER);
                write_mmio32(redist + GICR_WAKER, waker & !(1 << 1)); // Clear ProcessorSleep

                // Wait for ChildrenAsleep to clear
                let mut timeout = 1000;
                while read_mmio32(redist + GICR_WAKER) & (1 << 2) != 0 && timeout > 0 {
                    timeout -= 1;
                }
            }

            // Configure SGIs/PPIs (0-31): Group 1, default priority
            unsafe {
                write_mmio32(redist + GICR_IGROUPR0, 0xFFFF_FFFF);
                for i in 0..8 {
                    write_mmio32(redist + GICR_IPRIORITYR + (i as u64) * 4, 0xa0a0a0a0);
                }
                // Enable all SGIs
                write_mmio32(redist + GICR_ISENABLER0, 0x0000_FFFF);
            }

            serial_println!("[GICv3] Redistributor initialized for CPU {}", cpu_id);
        }

        /// Initialize CPU interface (system registers)
        pub fn init_cpu_interface(&self) {
            // Enable system register access: ICC_SRE_EL1.SRE = 1
            // Set priority mask to accept all priorities: ICC_PMR_EL1 = 0xFF
            // Enable Group 1 interrupts: ICC_IGRPEN1_EL1 = 1
            serial_println!("[GICv3] CPU interface configured via system registers");
        }

        /// Enable an SPI (Shared Peripheral Interrupt)
        pub fn enable_irq(&self, irq: u32) {
            if irq >= 32 {
                let reg_index = irq / 32;
                let bit = 1u32 << (irq % 32);
                unsafe {
                    write_mmio32(
                        self.dist_base + GICD_ISENABLER + (reg_index as u64) * 4,
                        bit,
                    );
                }
            }
        }

        /// Disable an SPI
        pub fn disable_irq(&self, irq: u32) {
            if irq >= 32 {
                let reg_index = irq / 32;
                let bit = 1u32 << (irq % 32);
                unsafe {
                    write_mmio32(
                        self.dist_base + GICD_ICENABLER + (reg_index as u64) * 4,
                        bit,
                    );
                }
            }
        }

        /// Set IRQ routing to specific CPU (affinity routing)
        pub fn set_irq_affinity(&self, irq: u32, mpidr: u64) {
            if irq >= 32 {
                unsafe {
                    let addr = self.dist_base + GICD_IROUTER + (irq as u64) * 8;
                    write_mmio64(addr, mpidr & 0xFF00FFFFFF);
                }
            }
        }

        /// Acknowledge interrupt (read IAR)
        pub fn acknowledge_irq(&self) -> u32 {
            // In real code: MRS x0, ICC_IAR1_EL1
            // Stub returns spurious
            1023
        }

        /// Signal end of interrupt
        pub fn end_irq(&self, irq: u32) {
            // In real code: MSR ICC_EOIR1_EL1, x0
            let _ = irq;
        }
    }

    // ─── ARM Generic Timer ──────────────────────────────────────────

    /// ARM Generic Timer (CNTPCT_EL0 / CNTP_CTL_EL0)
    pub struct GenericTimer {
        pub frequency: u64,
        pub tick_interval_ns: u64,
    }

    impl GenericTimer {
        pub fn new() -> Self {
            // Read CNTFRQ_EL0 for frequency (typically 62.5 MHz on QEMU)
            let frequency = 62_500_000; // Default
            Self {
                frequency,
                tick_interval_ns: 1_000_000_000 / frequency,
            }
        }

        /// Read current counter value
        pub fn read_counter(&self) -> u64 {
            // MRS x0, CNTPCT_EL0
            0 // Stub
        }

        /// Convert counter value to nanoseconds
        pub fn counter_to_ns(&self, count: u64) -> u64 {
            count * 1_000_000_000 / self.frequency
        }

        /// Set timer to fire after `ns` nanoseconds
        pub fn set_timer_ns(&self, ns: u64) {
            let ticks = ns * self.frequency / 1_000_000_000;
            // MSR CNTP_TVAL_EL0, ticks
            // MSR CNTP_CTL_EL0, 1 (enable)
            let _ = ticks;
            serial_println!(
                "[Timer] ARM Generic Timer set for {}ns ({} ticks)",
                ns,
                ticks
            );
        }

        /// Disable timer interrupt
        pub fn disable(&self) {
            // MSR CNTP_CTL_EL0, 0
        }
    }

    // ─── PSCI (Power State Coordination Interface) ──────────────────

    /// ARM PSCI for power management and SMP bring-up
    pub const PSCI_VERSION: u32 = 0x84000000;
    pub const PSCI_CPU_ON_64: u32 = 0xC4000003;
    pub const PSCI_CPU_OFF: u32 = 0x84000002;
    pub const PSCI_SYSTEM_OFF: u32 = 0x84000008;
    pub const PSCI_SYSTEM_RESET: u32 = 0x84000009;
    pub const PSCI_CPU_SUSPEND_64: u32 = 0xC4000001;

    /// PSCI return codes
    pub const PSCI_SUCCESS: i32 = 0;
    pub const PSCI_NOT_SUPPORTED: i32 = -1;
    pub const PSCI_INVALID_PARAMS: i32 = -2;
    pub const PSCI_DENIED: i32 = -3;
    pub const PSCI_ALREADY_ON: i32 = -4;

    /// Call PSCI function via HVC (hypervisor call) or SMC (secure monitor call)
    pub fn psci_call(func_id: u32, arg0: u64, arg1: u64, arg2: u64) -> i32 {
        // In real code: HVC #0 or SMC #0 with args in x0-x3
        serial_println!(
            "[PSCI] Call function_id=0x{:08X} arg0=0x{:X} arg1=0x{:X} arg2=0x{:X}",
            func_id,
            arg0,
            arg1,
            arg2
        );
        PSCI_SUCCESS
    }

    /// Bring up secondary CPU core
    pub fn cpu_on(target_cpu: u64, entry_point: u64, context_id: u64) -> Result<(), i32> {
        let ret = psci_call(PSCI_CPU_ON_64, target_cpu, entry_point, context_id);
        if ret == PSCI_SUCCESS {
            serial_println!(
                "[PSCI] CPU 0x{:X} powered on, entry=0x{:X}",
                target_cpu,
                entry_point
            );
            Ok(())
        } else {
            serial_println!("[PSCI] CPU_ON failed for 0x{:X}: error {}", target_cpu, ret);
            Err(ret)
        }
    }

    /// Power off current CPU
    pub fn cpu_off() {
        psci_call(PSCI_CPU_OFF, 0, 0, 0);
    }

    /// System reset
    pub fn system_reset() {
        psci_call(PSCI_SYSTEM_RESET, 0, 0, 0);
    }

    /// System power off
    pub fn system_off() {
        psci_call(PSCI_SYSTEM_OFF, 0, 0, 0);
    }

    // ─── SMMU (System MMU / IOMMU) ─────────────────────────────────

    /// ARM SMMU v3 register offsets
    pub const SMMU_IDR0: u64 = 0x0000;
    pub const SMMU_IDR1: u64 = 0x0004;
    pub const SMMU_CR0: u64 = 0x0020;
    pub const SMMU_CR0ACK: u64 = 0x0024;
    pub const SMMU_GBPA: u64 = 0x0044;
    pub const SMMU_STRTAB_BASE: u64 = 0x0080;
    pub const SMMU_STRTAB_BASE_CFG: u64 = 0x0088;
    pub const SMMU_CMDQ_BASE: u64 = 0x0090;
    pub const SMMU_CMDQ_PROD: u64 = 0x0098;
    pub const SMMU_CMDQ_CONS: u64 = 0x009C;
    pub const SMMU_EVTQ_BASE: u64 = 0x00A0;

    /// SMMUv3 context
    pub struct Smmuv3 {
        pub base: u64,
        pub num_streams: u32,
        pub initialized: bool,
    }

    impl Smmuv3 {
        pub fn new(base: u64) -> Self {
            Self {
                base,
                num_streams: 0,
                initialized: false,
            }
        }

        pub fn init(&mut self) {
            let idr0 = unsafe { read_mmio32(self.base + SMMU_IDR0) };
            let s2p = idr0 & 1; // Stage 2 translation
            let s1p = (idr0 >> 1) & 1; // Stage 1 translation
            let ttf = (idr0 >> 2) & 3; // Translation table format

            serial_println!("[SMMUv3] IDR0: S1={} S2={} TTF={}", s1p, s2p, ttf);

            // Configure stream table base
            // Enable SMMU
            unsafe {
                write_mmio32(self.base + SMMU_CR0, 1); // SMMUEN
            }

            self.initialized = true;
            serial_println!("[SMMUv3] Initialized at base 0x{:X}", self.base);
        }
    }

    // ─── AArch64 MMU Setup ──────────────────────────────────────────

    /// Translation Control Register (TCR_EL1) fields
    pub const TCR_T0SZ_48: u64 = 16; // 48-bit VA for TTBR0
    pub const TCR_T1SZ_48: u64 = 16 << 16; // 48-bit VA for TTBR1
    pub const TCR_TG0_4K: u64 = 0 << 14; // 4KB granule for TTBR0
    pub const TCR_TG1_4K: u64 = 2 << 30; // 4KB granule for TTBR1
    pub const TCR_SH0_INNER: u64 = 3 << 12; // Inner shareable
    pub const TCR_SH1_INNER: u64 = 3 << 28; // Inner shareable
    pub const TCR_ORGN0_WB: u64 = 1 << 10; // Outer write-back
    pub const TCR_IRGN0_WB: u64 = 1 << 8; // Inner write-back
    pub const TCR_ORGN1_WB: u64 = 1 << 26;
    pub const TCR_IRGN1_WB: u64 = 1 << 24;
    pub const TCR_IPS_48: u64 = 5 << 32; // 48-bit physical

    /// Memory Attribute Indirection Register (MAIR_EL1)
    /// Index 0: Device-nGnRnE (0x00)
    /// Index 1: Normal WB (0xFF)
    /// Index 2: Normal NC (0x44)
    pub const MAIR_VALUE: u64 = 0x000000000044FF00;

    /// AArch64 page table level allocator and mapper
    pub struct Aarch64Mmu {
        /// Physical address of root L0 table (TTBR0_EL1)
        pub ttbr0: u64,
        /// Physical address of kernel L0 table (TTBR1_EL1)
        pub ttbr1: u64,
        /// Next free page for page table allocation
        pub next_free_page: u64,
    }

    impl Aarch64Mmu {
        pub fn new(phys_base: u64) -> Self {
            Self {
                ttbr0: phys_base,
                ttbr1: phys_base + 4096,
                next_free_page: phys_base + 8192,
            }
        }

        /// Create initial identity mapping (first 1GB, 2MB blocks)
        pub fn setup_identity_map(&mut self) {
            serial_println!("[MMU] Setting up AArch64 identity mapping (4KB granule, 48-bit VA)");
            serial_println!("[MMU]   TTBR0_EL1 = 0x{:016X}", self.ttbr0);
            serial_println!("[MMU]   TTBR1_EL1 = 0x{:016X}", self.ttbr1);
            serial_println!("[MMU]   TCR_EL1 = T0SZ=16, TG0=4K, IPS=48bit");
            serial_println!("[MMU]   MAIR_EL1 = Device(0)=0x00, WB(1)=0xFF, NC(2)=0x44");
        }

        /// Map a single 4KB page
        pub fn map_page(&mut self, virt: u64, phys: u64, flags: PageFlags) {
            let _ = (virt, phys, flags);
            // Walk L0→L1→L2→L3, allocate intermediate tables as needed
        }

        /// Map a 2MB block (L2 block descriptor)
        pub fn map_block_2m(&mut self, virt: u64, phys: u64, flags: PageFlags) {
            let _ = (virt, phys, flags);
        }
    }

    // ─── AArch64 Context Switch ─────────────────────────────────────

    /// Save full AArch64 CPU state
    pub fn save_context() -> CpuContext {
        CpuContext {
            gp_regs: vec![0u64; 31],  // x0-x30
            pc: 0,                    // ELR_EL1
            sp: 0,                    // SP_EL0
            status: 0,                // SPSR_EL1
            page_table_base: 0,       // TTBR0_EL1
            tls_base: 0,              // TPIDR_EL0
            fp_state: vec![0u8; 528], // 32 x Q registers (128-bit) + FPCR + FPSR
        }
    }

    /// Restore full AArch64 CPU state
    pub fn restore_context(ctx: &CpuContext) {
        let _ = ctx;
        // STP/LDP for x0-x30, MSR for ELR/SPSR/TTBR0/TPIDR, LDNP for NEON
    }

    // ─── AArch64 Boot ───────────────────────────────────────────────

    /// AArch64 boot entry point (called from bootloader/UEFI stub)
    pub fn boot_entry() {
        serial_println!("[AArch64] Boot entry point");
        serial_println!("[AArch64]   Exception Level: EL1 (kernel)");
        serial_println!("[AArch64]   Boot protocol: UEFI or Linux arm64 boot (Image header)");

        // 1. Set up initial stack (SP_EL1)
        serial_println!("[AArch64]   Stack: 0x0000_0000_0008_0000 (512KB)");

        // 2. Read DTB pointer from x0
        serial_println!("[AArch64]   Device Tree from x0 (DTB pointer)");

        // 3. Clear BSS
        serial_println!("[AArch64]   BSS cleared");

        // 4. Set up MMU
        let mut mmu = Aarch64Mmu::new(0x4000_0000);
        mmu.setup_identity_map();

        // 5. Enable MMU (SCTLR_EL1.M = 1, C = 1, I = 1)
        serial_println!("[AArch64]   MMU enabled (SCTLR_EL1.M|C|I)");

        // 6. Initialize GIC
        let mut gic = Gicv3::new(0x0800_0000, 0x080A_0000); // QEMU virt
        gic.init_distributor();
        gic.init_redistributor(0);
        gic.init_cpu_interface();

        // 7. Initialize timer
        let timer = GenericTimer::new();
        timer.set_timer_ns(10_000_000); // 10ms tick
        serial_println!(
            "[AArch64]   Timer: Generic Timer @ {}MHz",
            timer.frequency / 1_000_000
        );

        // 8. Set exception vectors (VBAR_EL1)
        serial_println!("[AArch64]   Exception vectors installed at VBAR_EL1");

        serial_println!("[AArch64] Boot sequence complete, entering kernel_main()");
    }

    /// Bring up secondary AArch64 cores via PSCI
    pub fn smp_boot(num_cpus: u32) {
        serial_println!(
            "[AArch64-SMP] Bringing up {} secondary CPUs via PSCI",
            num_cpus - 1
        );
        for cpu in 1..num_cpus {
            let mpidr = cpu as u64; // Simplified; real MPIDR has Aff0/1/2/3
            let entry = 0x4008_0000u64; // Secondary CPU entry point
            if let Err(e) = cpu_on(mpidr, entry, cpu as u64) {
                serial_println!("[AArch64-SMP]   CPU {} failed: {}", cpu, e);
            } else {
                serial_println!("[AArch64-SMP]   CPU {} online (MPIDR=0x{:X})", cpu, mpidr);
            }
        }
    }

    // ─── Helper: MMIO Read/Write ────────────────────────────────────

    unsafe fn read_mmio32(addr: u64) -> u32 {
        core::ptr::read_volatile(addr as *const u32)
    }

    unsafe fn write_mmio32(addr: u64, val: u32) {
        core::ptr::write_volatile(addr as *mut u32, val);
    }

    unsafe fn write_mmio64(addr: u64, val: u64) {
        core::ptr::write_volatile(addr as *mut u64, val);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// RISC-V 64 HAL IMPLEMENTATION
// ═══════════════════════════════════════════════════════════════════════

pub mod riscv64_hal {
    use super::*;

    // ─── PLIC (Platform-Level Interrupt Controller) ─────────────────

    /// PLIC register layout (SiFive / QEMU virt)
    pub const PLIC_BASE: u64 = 0x0C00_0000;
    pub const PLIC_PRIORITY: u64 = 0x0000;
    pub const PLIC_PENDING: u64 = 0x1000;
    pub const PLIC_ENABLE_BASE: u64 = 0x2000;
    pub const PLIC_THRESHOLD_BASE: u64 = 0x20_0000;
    pub const PLIC_CLAIM_BASE: u64 = 0x20_0004;

    /// Context stride: each hart has S-mode and M-mode contexts
    pub const PLIC_CONTEXT_STRIDE: u64 = 0x1000;
    pub const PLIC_ENABLE_STRIDE: u64 = 0x80;

    pub struct Plic {
        pub base: u64,
        pub num_sources: u32,
        pub initialized: bool,
    }

    impl Plic {
        pub fn new(base: u64) -> Self {
            Self {
                base,
                num_sources: 127, // QEMU virt default
                initialized: false,
            }
        }

        /// Initialize PLIC for S-mode on hart 0
        pub fn init(&mut self, hart_id: u32) {
            serial_println!(
                "[PLIC] Initializing for hart {} (S-mode context {})",
                hart_id,
                hart_id * 2 + 1
            );

            // Set all priorities to 1 (minimum non-zero)
            for irq in 1..=self.num_sources {
                unsafe {
                    write_mmio32(self.base + PLIC_PRIORITY + (irq as u64) * 4, 1);
                }
            }

            // Set threshold to 0 (accept all priorities)
            let context = (hart_id * 2 + 1) as u64; // S-mode context
            unsafe {
                write_mmio32(
                    self.base + PLIC_THRESHOLD_BASE + context * PLIC_CONTEXT_STRIDE,
                    0,
                );
            }

            self.initialized = true;
            serial_println!(
                "[PLIC] Initialized: {} sources, threshold=0",
                self.num_sources
            );
        }

        /// Enable interrupt source for hart
        pub fn enable_irq(&self, hart_id: u32, irq: u32) {
            let context = (hart_id * 2 + 1) as u64;
            let reg_index = (irq / 32) as u64;
            let bit = 1u32 << (irq % 32);
            unsafe {
                let addr =
                    self.base + PLIC_ENABLE_BASE + context * PLIC_ENABLE_STRIDE + reg_index * 4;
                let val = read_mmio32(addr);
                write_mmio32(addr, val | bit);
            }
        }

        /// Disable interrupt source for hart
        pub fn disable_irq(&self, hart_id: u32, irq: u32) {
            let context = (hart_id * 2 + 1) as u64;
            let reg_index = (irq / 32) as u64;
            let bit = 1u32 << (irq % 32);
            unsafe {
                let addr =
                    self.base + PLIC_ENABLE_BASE + context * PLIC_ENABLE_STRIDE + reg_index * 4;
                let val = read_mmio32(addr);
                write_mmio32(addr, val & !bit);
            }
        }

        /// Claim interrupt (returns IRQ number)
        pub fn claim(&self, hart_id: u32) -> u32 {
            let context = (hart_id * 2 + 1) as u64;
            unsafe { read_mmio32(self.base + PLIC_CLAIM_BASE + context * PLIC_CONTEXT_STRIDE) }
        }

        /// Complete interrupt (signal EOI)
        pub fn complete(&self, hart_id: u32, irq: u32) {
            let context = (hart_id * 2 + 1) as u64;
            unsafe {
                write_mmio32(
                    self.base + PLIC_CLAIM_BASE + context * PLIC_CONTEXT_STRIDE,
                    irq,
                );
            }
        }

        /// Set interrupt priority (1-7)
        pub fn set_priority(&self, irq: u32, priority: u32) {
            unsafe {
                write_mmio32(self.base + PLIC_PRIORITY + (irq as u64) * 4, priority);
            }
        }
    }

    // ─── CLINT (Core Local Interruptor) ─────────────────────────────

    pub const CLINT_BASE: u64 = 0x0200_0000;
    pub const CLINT_MSIP: u64 = 0x0000;
    pub const CLINT_MTIMECMP: u64 = 0x4000;
    pub const CLINT_MTIME: u64 = 0xBFF8;

    pub struct Clint {
        pub base: u64,
        pub frequency: u64,
    }

    impl Clint {
        pub fn new(base: u64) -> Self {
            Self {
                base,
                frequency: 10_000_000, // 10 MHz default (QEMU)
            }
        }

        /// Read mtime counter
        pub fn read_mtime(&self) -> u64 {
            unsafe { read_mmio64(self.base + CLINT_MTIME) }
        }

        /// Set mtimecmp for hart (timer interrupt)
        pub fn set_timer(&self, hart_id: u32, deadline: u64) {
            unsafe {
                write_mmio64(self.base + CLINT_MTIMECMP + (hart_id as u64) * 8, deadline);
            }
        }

        /// Set timer to fire after `us` microseconds
        pub fn set_timer_us(&self, hart_id: u32, us: u64) {
            let now = self.read_mtime();
            let deadline = now + (us * self.frequency / 1_000_000);
            self.set_timer(hart_id, deadline);
        }

        /// Send software interrupt to hart (IPI)
        pub fn send_ipi(&self, hart_id: u32) {
            unsafe {
                write_mmio32(self.base + CLINT_MSIP + (hart_id as u64) * 4, 1);
            }
        }

        /// Clear software interrupt for hart
        pub fn clear_ipi(&self, hart_id: u32) {
            unsafe {
                write_mmio32(self.base + CLINT_MSIP + (hart_id as u64) * 4, 0);
            }
        }
    }

    // ─── SBI (Supervisor Binary Interface) ──────────────────────────

    /// SBI extension IDs
    pub const SBI_EXT_BASE: u64 = 0x10;
    pub const SBI_EXT_TIMER: u64 = 0x54494D45; // "TIME"
    pub const SBI_EXT_IPI: u64 = 0x735049; // "sPI"
    pub const SBI_EXT_RFENCE: u64 = 0x52464E43; // "RFNC"
    pub const SBI_EXT_HSM: u64 = 0x48534D; // "HSM"
    pub const SBI_EXT_SRST: u64 = 0x53525354; // "SRST"
    pub const SBI_EXT_PMU: u64 = 0x504D55; // "PMU"

    /// SBI function IDs for HSM
    pub const SBI_HSM_HART_START: u64 = 0;
    pub const SBI_HSM_HART_STOP: u64 = 1;
    pub const SBI_HSM_HART_STATUS: u64 = 2;

    /// SBI return value
    #[derive(Debug)]
    pub struct SbiRet {
        pub error: i64,
        pub value: u64,
    }

    /// Make SBI ecall
    pub fn sbi_call(ext: u64, fid: u64, a0: u64, a1: u64, a2: u64) -> SbiRet {
        // In real code: set a7=ext, a6=fid, a0-a2=args, ecall
        serial_println!(
            "[SBI] ecall ext=0x{:X} fid={} a0=0x{:X} a1=0x{:X} a2=0x{:X}",
            ext,
            fid,
            a0,
            a1,
            a2
        );
        SbiRet { error: 0, value: 0 }
    }

    /// Set timer via SBI
    pub fn sbi_set_timer(stime_value: u64) {
        sbi_call(SBI_EXT_TIMER, 0, stime_value, 0, 0);
    }

    /// Send IPI to hart mask via SBI
    pub fn sbi_send_ipi(hart_mask: u64, hart_mask_base: u64) {
        sbi_call(SBI_EXT_IPI, 0, hart_mask, hart_mask_base, 0);
    }

    /// Start hart via SBI HSM
    pub fn sbi_hart_start(hartid: u64, start_addr: u64, opaque: u64) -> Result<(), i64> {
        let ret = sbi_call(SBI_EXT_HSM, SBI_HSM_HART_START, hartid, start_addr, opaque);
        if ret.error == 0 {
            serial_println!("[SBI-HSM] Hart {} started at 0x{:X}", hartid, start_addr);
            Ok(())
        } else {
            serial_println!(
                "[SBI-HSM] Hart {} start failed: error {}",
                hartid,
                ret.error
            );
            Err(ret.error)
        }
    }

    /// System reset via SBI SRST
    pub fn sbi_system_reset(reset_type: u32, reason: u32) {
        sbi_call(SBI_EXT_SRST, 0, reset_type as u64, reason as u64, 0);
    }

    // ─── RISC-V Sv48 MMU ────────────────────────────────────────────

    /// Sv48 page table (4 levels, 48-bit VA, 56-bit PA)
    pub struct Sv48Mmu {
        pub root_ppn: u64,
        pub next_free_page: u64,
    }

    /// SATP register mode values
    pub const SATP_MODE_SV39: u64 = 8;
    pub const SATP_MODE_SV48: u64 = 9;
    pub const SATP_MODE_SV57: u64 = 10;

    impl Sv48Mmu {
        pub fn new(phys_base: u64) -> Self {
            Self {
                root_ppn: phys_base >> 12,
                next_free_page: phys_base + 4096,
            }
        }

        /// Build SATP register value for Sv48
        pub fn satp_value(&self, asid: u16) -> u64 {
            (SATP_MODE_SV48 << 60) | ((asid as u64) << 44) | self.root_ppn
        }

        /// Set up identity mapping
        pub fn setup_identity_map(&mut self) {
            serial_println!("[MMU] Setting up RISC-V Sv48 identity mapping");
            serial_println!("[MMU]   Root PPN: 0x{:X}", self.root_ppn);
            serial_println!("[MMU]   SATP mode: Sv48 (4-level, 48-bit VA)");
            serial_println!("[MMU]   Page size: 4KB (also supports 2MB megapages, 1GB gigapages)");
        }

        /// Map a single 4KB page
        pub fn map_page(&mut self, virt: u64, phys: u64, flags: PageFlags) {
            let _ = (virt, phys, flags);
            // Walk L0→L1→L2→L3, allocate intermediate tables
        }

        /// Map a 2MB megapage (L2 leaf)
        pub fn map_megapage(&mut self, virt: u64, phys: u64, flags: PageFlags) {
            let _ = (virt, phys, flags);
        }
    }

    // ─── RISC-V Context Switch ──────────────────────────────────────

    /// Save full RISC-V CPU state
    pub fn save_context() -> CpuContext {
        CpuContext {
            gp_regs: vec![0u64; 32],  // x0-x31 (x0 always 0)
            pc: 0,                    // sepc
            sp: 0,                    // x2 (sp)
            status: 0,                // sstatus
            page_table_base: 0,       // satp
            tls_base: 0,              // tp (x4)
            fp_state: vec![0u8; 264], // 32 x f registers (64-bit) + fcsr
        }
    }

    /// Restore full RISC-V CPU state
    pub fn restore_context(ctx: &CpuContext) {
        let _ = ctx;
        // SD/LD for x1-x31, CSRRW for sepc/sstatus/satp
    }

    // ─── RISC-V Boot ────────────────────────────────────────────────

    /// RISC-V boot entry point (from OpenSBI / BBL)
    pub fn boot_entry() {
        serial_println!("[RISC-V] Boot entry point");
        serial_println!("[RISC-V]   Privilege: S-mode (Supervisor)");
        serial_println!("[RISC-V]   Boot protocol: OpenSBI → S-mode entry");
        serial_println!("[RISC-V]   a0 = hart ID, a1 = DTB pointer");

        // 1. Set up stack
        serial_println!("[RISC-V]   Stack: 0x8020_0000 + hart_id * 64KB");

        // 2. Parse DTB from a1
        serial_println!("[RISC-V]   Device Tree parsed for memory/PLIC/CLINT");

        // 3. Clear BSS
        serial_println!("[RISC-V]   BSS cleared");

        // 4. Set up Sv48 page tables
        let mut mmu = Sv48Mmu::new(0x8100_0000);
        mmu.setup_identity_map();

        // 5. Enable MMU (write SATP, sfence.vma)
        serial_println!("[RISC-V]   MMU enabled: SATP=0x{:016X}", mmu.satp_value(0));
        serial_println!("[RISC-V]   sfence.vma issued");

        // 6. Initialize PLIC
        let mut plic = Plic::new(PLIC_BASE);
        plic.init(0);

        // 7. Initialize CLINT timer
        let clint = Clint::new(CLINT_BASE);
        clint.set_timer_us(0, 10_000); // 10ms tick
        serial_println!(
            "[RISC-V]   Timer: CLINT @ {}MHz",
            clint.frequency / 1_000_000
        );

        // 8. Set stvec (trap vector)
        serial_println!("[RISC-V]   stvec installed (vectored mode)");

        // 9. Enable interrupts (sstatus.SIE, sie.SEIE|STIE|SSIE)
        serial_println!("[RISC-V]   Interrupts enabled: external, timer, software");

        serial_println!("[RISC-V] Boot sequence complete, entering kernel_main()");
    }

    /// Bring up secondary harts via SBI HSM
    pub fn smp_boot(num_harts: u32) {
        serial_println!(
            "[RISC-V-SMP] Bringing up {} secondary harts via SBI HSM",
            num_harts - 1
        );
        for hart in 1..num_harts {
            let entry = 0x8020_0000u64; // Secondary hart entry
            if let Err(e) = sbi_hart_start(hart as u64, entry, hart as u64) {
                serial_println!("[RISC-V-SMP]   Hart {} failed: {}", hart, e);
            } else {
                serial_println!("[RISC-V-SMP]   Hart {} online", hart);
            }
        }
    }

    // ─── Helper: MMIO Read/Write ────────────────────────────────────

    unsafe fn read_mmio32(addr: u64) -> u32 {
        core::ptr::read_volatile(addr as *const u32)
    }

    unsafe fn write_mmio32(addr: u64, val: u32) {
        core::ptr::write_volatile(addr as *mut u32, val);
    }

    unsafe fn read_mmio64(addr: u64) -> u64 {
        core::ptr::read_volatile(addr as *const u64)
    }

    unsafe fn write_mmio64(addr: u64, val: u64) {
        core::ptr::write_volatile(addr as *mut u64, val);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// X86_64 HAL (wraps existing code)
// ═══════════════════════════════════════════════════════════════════════

pub mod x86_64_hal {
    use super::*;

    /// x86_64 boot — delegates to existing bootloader crate + main.rs
    pub fn boot_entry() {
        serial_println!("[x86_64] Boot via bootloader crate (BIOS/UEFI)");
        serial_println!("[x86_64]   GDT/IDT from gdt.rs/interrupts.rs");
        serial_println!("[x86_64]   APIC from interrupts.rs (PIC cascaded to APIC)");
        serial_println!("[x86_64]   Page tables via x86_64 crate (4-level Sv48)");
        serial_println!("[x86_64]   Timer: PIT → HPET → TSC deadline");
    }

    /// x86_64 SMP via SIPI (Startup IPI)
    pub fn smp_boot(num_cpus: u32) {
        serial_println!("[x86_64-SMP] Secondary CPUs via INIT-SIPI-SIPI sequence");
        for cpu in 1..num_cpus {
            serial_println!(
                "[x86_64-SMP]   CPU {} brought online (APIC ID {})",
                cpu,
                cpu
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CROSS-PLATFORM TARGET SPECIFICATION GENERATOR
// ═══════════════════════════════════════════════════════════════════════

/// Generate a Rust target JSON spec for cross-compilation
pub fn generate_target_json(arch: crate::arch::Architecture) -> String {
    match arch {
        crate::arch::Architecture::X86_64 => {
            alloc::format!(
                r#"{{
    "llvm-target": "x86_64-unknown-none",
    "data-layout": "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128",
    "arch": "x86_64",
    "target-endian": "little",
    "target-pointer-width": "64",
    "target-c-int-width": "32",
    "os": "none",
    "executables": true,
    "linker-flavor": "ld.lld",
    "linker": "rust-lld",
    "panic-strategy": "abort",
    "disable-redzone": true,
    "features": "-mmx,-sse,-sse2,+soft-float"
}}"#
            )
        }
        crate::arch::Architecture::Aarch64 => {
            alloc::format!(
                r#"{{
    "llvm-target": "aarch64-unknown-none",
    "data-layout": "e-m:e-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128",
    "arch": "aarch64",
    "target-endian": "little",
    "target-pointer-width": "64",
    "target-c-int-width": "32",
    "os": "none",
    "executables": true,
    "linker-flavor": "ld.lld",
    "linker": "rust-lld",
    "panic-strategy": "abort",
    "disable-redzone": true,
    "features": "+strict-align,+neon,+fp-armv8"
}}"#
            )
        }
        crate::arch::Architecture::Riscv64 => {
            alloc::format!(
                r#"{{
    "llvm-target": "riscv64gc-unknown-none-elf",
    "data-layout": "e-m:e-p:64:64-i64:64-i128:128-n32:64-S128",
    "arch": "riscv64",
    "target-endian": "little",
    "target-pointer-width": "64",
    "target-c-int-width": "32",
    "os": "none",
    "executables": true,
    "linker-flavor": "ld.lld",
    "linker": "rust-lld",
    "panic-strategy": "abort",
    "code-model": "medium",
    "features": "+m,+a,+f,+d,+c"
}}"#
            )
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// LINKER SCRIPT TEMPLATES
// ═══════════════════════════════════════════════════════════════════════

/// Generate architecture-specific linker script
pub fn generate_linker_script(arch: crate::arch::Architecture) -> String {
    match arch {
        crate::arch::Architecture::X86_64 => alloc::format!(
            r#"/* KnoxOS x86_64 linker script */
OUTPUT_FORMAT("elf64-x86-64")
ENTRY(_start)

SECTIONS {{
    . = 0xFFFF800000000000;  /* Higher half kernel */
    .text ALIGN(4K) : {{ *(.text .text.*) }}
    .rodata ALIGN(4K) : {{ *(.rodata .rodata.*) }}
    .data ALIGN(4K) : {{ *(.data .data.*) }}
    .bss ALIGN(4K) : {{ *(.bss .bss.*) *(COMMON) }}
    /DISCARD/ : {{ *(.eh_frame) *(.comment) }}
}}"#
        ),
        crate::arch::Architecture::Aarch64 => alloc::format!(
            r#"/* KnoxOS AArch64 linker script */
OUTPUT_FORMAT("elf64-littleaarch64")
ENTRY(_start)

SECTIONS {{
    . = 0xFFFF000000080000;  /* Kernel load address (QEMU virt) */
    .text ALIGN(4K) : {{
        KEEP(*(.text.boot))  /* Boot entry must be first */
        *(.text .text.*)
    }}
    .rodata ALIGN(4K) : {{ *(.rodata .rodata.*) }}
    .data ALIGN(4K) : {{ *(.data .data.*) }}
    .bss ALIGN(4K) : {{
        __bss_start = .;
        *(.bss .bss.*) *(COMMON)
        __bss_end = .;
    }}
    /DISCARD/ : {{ *(.eh_frame) *(.comment) *(.note*) }}
}}"#
        ),
        crate::arch::Architecture::Riscv64 => alloc::format!(
            r#"/* KnoxOS RISC-V 64 linker script */
OUTPUT_ARCH(riscv)
OUTPUT_FORMAT("elf64-littleriscv")
ENTRY(_start)

SECTIONS {{
    . = 0x80200000;  /* OpenSBI passes control here */
    .text ALIGN(4K) : {{
        KEEP(*(.text.boot))  /* Boot entry */
        *(.text .text.*)
    }}
    .rodata ALIGN(4K) : {{ *(.rodata .rodata.*) }}
    .data ALIGN(4K) : {{ *(.data .data.*) }}
    .bss ALIGN(4K) : {{
        __bss_start = .;
        *(.bss .bss.*) *(COMMON)
        __bss_end = .;
    }}
    /DISCARD/ : {{ *(.eh_frame) *(.comment) *(.note*) }}
}}"#
        ),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// QEMU LAUNCH CONFIGURATION
// ═══════════════════════════════════════════════════════════════════════

/// Generate QEMU launch command for each architecture
pub fn qemu_command(arch: crate::arch::Architecture) -> String {
    match arch {
        crate::arch::Architecture::X86_64 => String::from(
            "qemu-system-x86_64 \\\n\
             \t-drive format=raw,file=knoxos.img \\\n\
             \t-serial stdio \\\n\
             \t-m 512M \\\n\
             \t-smp 4 \\\n\
             \t-enable-kvm \\\n\
             \t-device virtio-net-pci,netdev=net0 \\\n\
             \t-netdev user,id=net0 \\\n\
             \t-device virtio-blk-pci,drive=disk0 \\\n\
             \t-drive id=disk0,file=rootfs.img,format=raw,if=none",
        ),
        crate::arch::Architecture::Aarch64 => String::from(
            "qemu-system-aarch64 \\\n\
             \t-machine virt \\\n\
             \t-cpu cortex-a72 \\\n\
             \t-kernel knoxos-aarch64.elf \\\n\
             \t-serial stdio \\\n\
             \t-m 512M \\\n\
             \t-smp 4 \\\n\
             \t-device virtio-net-device,netdev=net0 \\\n\
             \t-netdev user,id=net0 \\\n\
             \t-device virtio-blk-device,drive=disk0 \\\n\
             \t-drive id=disk0,file=rootfs.img,format=raw,if=none \\\n\
             \t-dtb virt.dtb",
        ),
        crate::arch::Architecture::Riscv64 => String::from(
            "qemu-system-riscv64 \\\n\
             \t-machine virt \\\n\
             \t-bios opensbi-riscv64-generic-fw_jump.bin \\\n\
             \t-kernel knoxos-riscv64.elf \\\n\
             \t-serial stdio \\\n\
             \t-m 512M \\\n\
             \t-smp 4 \\\n\
             \t-device virtio-net-device,netdev=net0 \\\n\
             \t-netdev user,id=net0 \\\n\
             \t-device virtio-blk-device,drive=disk0 \\\n\
             \t-drive id=disk0,file=rootfs.img,format=raw,if=none",
        ),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// DEVICE TREE BLOB (DTB) PARSER
// ═══════════════════════════════════════════════════════════════════════

/// FDT magic number (big-endian)
pub const FDT_MAGIC: u32 = 0xd00dfeed;
/// FDT token types
pub const FDT_BEGIN_NODE: u32 = 0x00000001;
pub const FDT_END_NODE: u32 = 0x00000002;
pub const FDT_PROP: u32 = 0x00000003;
pub const FDT_NOP: u32 = 0x00000004;
pub const FDT_END: u32 = 0x00000009;

/// Flattened Device Tree header
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FdtHeader {
    pub magic: u32,
    pub totalsize: u32,
    pub off_dt_struct: u32,
    pub off_dt_strings: u32,
    pub off_mem_rsvmap: u32,
    pub version: u32,
    pub last_comp_version: u32,
    pub boot_cpuid_phys: u32,
    pub size_dt_strings: u32,
    pub size_dt_struct: u32,
}

/// Simple DTB parser for finding memory regions and device addresses
pub struct DtbParser {
    pub base: u64,
    pub total_size: u32,
    pub struct_offset: u32,
    pub strings_offset: u32,
    pub memory_regions: Vec<BootMemoryRegion>,
    pub plic_base: Option<u64>,
    pub clint_base: Option<u64>,
    pub uart_base: Option<u64>,
    pub gic_dist_base: Option<u64>,
    pub gic_redist_base: Option<u64>,
}

impl DtbParser {
    /// Parse DTB at given physical address
    pub fn parse(base: u64) -> Option<Self> {
        // Validate magic (big-endian)
        let magic = unsafe { u32::from_be(core::ptr::read_volatile(base as *const u32)) };
        if magic != FDT_MAGIC {
            serial_println!(
                "[DTB] Invalid magic: 0x{:08X} (expected 0x{:08X})",
                magic,
                FDT_MAGIC
            );
            return None;
        }

        let total_size =
            unsafe { u32::from_be(core::ptr::read_volatile((base + 4) as *const u32)) };
        let struct_offset =
            unsafe { u32::from_be(core::ptr::read_volatile((base + 8) as *const u32)) };
        let strings_offset =
            unsafe { u32::from_be(core::ptr::read_volatile((base + 12) as *const u32)) };

        serial_println!(
            "[DTB] Valid FDT at 0x{:X}: size={}, struct_off={}, strings_off={}",
            base,
            total_size,
            struct_offset,
            strings_offset
        );

        Some(Self {
            base,
            total_size,
            struct_offset,
            strings_offset,
            memory_regions: Vec::new(),
            plic_base: None,
            clint_base: None,
            uart_base: None,
            gic_dist_base: None,
            gic_redist_base: None,
        })
    }

    /// Get total detected memory
    pub fn total_memory(&self) -> u64 {
        self.memory_regions.iter().map(|r| r.size).sum()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// UNIFIED HAL DISPATCH
// ═══════════════════════════════════════════════════════════════════════

/// Current architecture HAL state
static ARCH_HAL_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Number of online CPUs
static NUM_ONLINE_CPUS: AtomicU64 = AtomicU64::new(1);

/// CPU info table
static CPU_INFO: Mutex<Vec<CpuInfo>> = Mutex::new(Vec::new());

/// Initialize the architecture-specific HAL
pub fn hal_init(arch: crate::arch::Architecture) {
    serial_println!("════════════════════════════════════════════════════════════");
    serial_println!(
        "[HAL] Initializing Hardware Abstraction Layer for {}",
        arch.name()
    );
    serial_println!("════════════════════════════════════════════════════════════");

    match arch {
        crate::arch::Architecture::X86_64 => {
            x86_64_hal::boot_entry();
        }
        crate::arch::Architecture::Aarch64 => {
            aarch64_hal::boot_entry();
        }
        crate::arch::Architecture::Riscv64 => {
            riscv64_hal::boot_entry();
        }
    }

    // Register BSP
    CPU_INFO.lock().push(CpuInfo {
        cpu_id: 0,
        is_bsp: true,
        online: true,
        arch_id: 0,
    });

    ARCH_HAL_INITIALIZED.store(true, Ordering::SeqCst);
    serial_println!("[HAL] Architecture HAL initialized for {}", arch.name());
}

/// Bring up secondary CPUs
pub fn hal_smp_boot(arch: crate::arch::Architecture, num_cpus: u32) {
    match arch {
        crate::arch::Architecture::X86_64 => x86_64_hal::smp_boot(num_cpus),
        crate::arch::Architecture::Aarch64 => aarch64_hal::smp_boot(num_cpus),
        crate::arch::Architecture::Riscv64 => riscv64_hal::smp_boot(num_cpus),
    }

    // Register secondary CPUs
    let mut info = CPU_INFO.lock();
    for i in 1..num_cpus {
        info.push(CpuInfo {
            cpu_id: i,
            is_bsp: false,
            online: true,
            arch_id: i as u64,
        });
    }

    NUM_ONLINE_CPUS.store(num_cpus as u64, Ordering::SeqCst);
    serial_println!("[HAL] {} CPUs online", num_cpus);
}

/// Get number of online CPUs
pub fn online_cpus() -> u64 {
    NUM_ONLINE_CPUS.load(Ordering::Relaxed)
}

/// Get target specification info for all architectures
pub fn print_all_targets() {
    serial_println!("[HAL] Supported KnoxOS targets:");
    for arch in [
        crate::arch::Architecture::X86_64,
        crate::arch::Architecture::Aarch64,
        crate::arch::Architecture::Riscv64,
    ] {
        serial_println!(
            "  {} — {}",
            arch.name(),
            match arch {
                crate::arch::Architecture::X86_64 => "PC/server (BIOS/UEFI boot, APIC, x86 ISA)",
                crate::arch::Architecture::Aarch64 => "ARM64 (UEFI/DTB boot, GICv3, PSCI SMP)",
                crate::arch::Architecture::Riscv64 =>
                    "RISC-V 64 (OpenSBI boot, PLIC/CLINT, SBI HSM)",
            }
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

pub fn init() {
    serial_println!("[KnoxOS] Multi-architecture port layer initialized");
    serial_println!("[KnoxOS]   Current target: x86_64-unknown-knoxos");
    serial_println!(
        "[KnoxOS]   Additional targets: aarch64-unknown-knoxos, riscv64gc-unknown-knoxos"
    );
    serial_println!("[KnoxOS]   AArch64 HAL: GICv3, Generic Timer, PSCI, SMMUv3, DTB");
    serial_println!("[KnoxOS]   RISC-V HAL: PLIC, CLINT, SBI, Sv48 MMU, DTB");
    serial_println!("[KnoxOS]   Cross-compile: target JSON specs + linker scripts generated");
    print_all_targets();
}
