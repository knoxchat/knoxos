use alloc::format;
/// Architecture Abstraction Layer — Multi-architecture support
/// Provides a unified interface for x86_64, ARM64 (AArch64), and RISC-V 64
/// This module enables KnoxOS to be ported to additional architectures.
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, Ordering};

// ─── Architecture Enum ──────────────────────────────────────────────

/// Supported CPU architectures
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Architecture {
    X86_64 = 0,
    Aarch64 = 1,
    Riscv64 = 2,
}

impl Architecture {
    pub fn name(&self) -> &'static str {
        match self {
            Architecture::X86_64 => "x86_64",
            Architecture::Aarch64 => "aarch64",
            Architecture::Riscv64 => "riscv64",
        }
    }

    pub fn page_size(&self) -> usize {
        match self {
            Architecture::X86_64 => 4096,
            Architecture::Aarch64 => 4096, // 4K default (also supports 16K, 64K)
            Architecture::Riscv64 => 4096,
        }
    }

    pub fn pointer_width(&self) -> u8 {
        64
    }

    pub fn endianness(&self) -> Endianness {
        match self {
            Architecture::X86_64 => Endianness::Little,
            Architecture::Aarch64 => Endianness::Little, // AArch64 default
            Architecture::Riscv64 => Endianness::Little,
        }
    }

    pub fn max_cpus(&self) -> usize {
        match self {
            Architecture::X86_64 => 256,
            Architecture::Aarch64 => 256,
            Architecture::Riscv64 => 128,
        }
    }

    pub fn interrupt_controller(&self) -> &'static str {
        match self {
            Architecture::X86_64 => "APIC",
            Architecture::Aarch64 => "GIC",
            Architecture::Riscv64 => "PLIC/CLINT",
        }
    }

    pub fn timer_source(&self) -> &'static str {
        match self {
            Architecture::X86_64 => "TSC/HPET/PIT",
            Architecture::Aarch64 => "Generic Timer (CNTPCT_EL0)",
            Architecture::Riscv64 => "CLINT mtime",
        }
    }

    pub fn syscall_convention(&self) -> SyscallConvention {
        match self {
            Architecture::X86_64 => SyscallConvention {
                instruction: "syscall",
                number_reg: "rax",
                arg_regs: &["rdi", "rsi", "rdx", "r10", "r8", "r9"],
                return_reg: "rax",
            },
            Architecture::Aarch64 => SyscallConvention {
                instruction: "svc #0",
                number_reg: "x8",
                arg_regs: &["x0", "x1", "x2", "x3", "x4", "x5"],
                return_reg: "x0",
            },
            Architecture::Riscv64 => SyscallConvention {
                instruction: "ecall",
                number_reg: "a7",
                arg_regs: &["a0", "a1", "a2", "a3", "a4", "a5"],
                return_reg: "a0",
            },
        }
    }
}

/// Byte ordering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    Little,
    Big,
}

/// System call convention for an architecture
#[derive(Debug, Clone, Copy)]
pub struct SyscallConvention {
    pub instruction: &'static str,
    pub number_reg: &'static str,
    pub arg_regs: &'static [&'static str],
    pub return_reg: &'static str,
}

// ─── Current Architecture ───────────────────────────────────────────

static CURRENT_ARCH: AtomicU8 = AtomicU8::new(0); // x86_64 default

pub fn current_arch() -> Architecture {
    match CURRENT_ARCH.load(Ordering::Relaxed) {
        0 => Architecture::X86_64,
        1 => Architecture::Aarch64,
        2 => Architecture::Riscv64,
        _ => Architecture::X86_64,
    }
}

// ─── ARM64 (AArch64) Definitions ─────────────────────────────────

pub mod aarch64 {
    /// AArch64 system registers
    #[derive(Debug, Clone, Copy)]
    #[allow(non_camel_case_types)]
    pub enum SystemRegister {
        // General purpose
        MPIDR_EL1, // Multiprocessor Affinity Register
        MIDR_EL1,  // Main ID Register
        SCTLR_EL1, // System Control Register
        TCR_EL1,   // Translation Control Register
        TTBR0_EL1, // Translation Table Base Register 0
        TTBR1_EL1, // Translation Table Base Register 1
        MAIR_EL1,  // Memory Attribute Indirection Register
        VBAR_EL1,  // Vector Base Address Register
        DAIF,      // Interrupt mask bits
        CurrentEL, // Current Exception Level
        SP_EL0,    // Stack Pointer EL0
        SP_EL1,    // Stack Pointer EL1
        ELR_EL1,   // Exception Link Register
        SPSR_EL1,  // Saved Program Status Register
        FAR_EL1,   // Fault Address Register
        ESR_EL1,   // Exception Syndrome Register
        // Timer
        CNTPCT_EL0,    // Physical counter
        CNTP_CTL_EL0,  // Physical timer control
        CNTP_TVAL_EL0, // Physical timer value
        CNTFRQ_EL0,    // Counter frequency
        // GIC (Generic Interrupt Controller)
        ICC_IAR1_EL1,  // Interrupt Acknowledge
        ICC_EOIR1_EL1, // End of Interrupt
        ICC_PMR_EL1,   // Priority Mask
        ICC_SRE_EL1,   // System Register Enable
    }

    /// AArch64 exception levels
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ExceptionLevel {
        EL0 = 0, // User mode
        EL1 = 1, // Kernel mode
        EL2 = 2, // Hypervisor
        EL3 = 3, // Secure monitor
    }

    /// AArch64 page table format (4KB granule)
    #[derive(Debug, Clone, Copy)]
    pub struct PageTableEntry(pub u64);

    impl PageTableEntry {
        pub const VALID: u64 = 1 << 0;
        pub const TABLE: u64 = 1 << 1;
        pub const ATTR_INDEX_MASK: u64 = 0x7 << 2;
        pub const NS: u64 = 1 << 5; // Non-secure
        pub const AP_RW_EL1: u64 = 0 << 6;
        pub const AP_RW_ALL: u64 = 1 << 6;
        pub const AP_RO_EL1: u64 = 2 << 6;
        pub const AP_RO_ALL: u64 = 3 << 6;
        pub const SH_INNER: u64 = 3 << 8;
        pub const AF: u64 = 1 << 10; // Access flag
        pub const NG: u64 = 1 << 11; // Not global
        pub const PXN: u64 = 1 << 53; // Privileged execute never
        pub const UXN: u64 = 1 << 54; // Unprivileged execute never

        pub fn is_valid(&self) -> bool {
            self.0 & Self::VALID != 0
        }
        pub fn is_table(&self) -> bool {
            self.0 & Self::TABLE != 0
        }
        pub fn output_addr(&self) -> u64 {
            self.0 & 0x0000_FFFF_FFFF_F000
        }
    }

    /// GIC (Generic Interrupt Controller) v3 registers
    pub struct GicDistributor {
        pub base: u64,
    }

    pub struct GicRedistributor {
        pub base: u64,
    }

    pub struct GicCpuInterface;

    /// AArch64 CPU context for context switching
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct Aarch64Context {
        pub x: [u64; 31],       // x0-x30 general purpose registers
        pub sp: u64,            // Stack pointer
        pub pc: u64,            // Program counter (ELR_EL1)
        pub pstate: u64,        // SPSR_EL1
        pub ttbr0: u64,         // Page table base
        pub tpidr_el0: u64,     // Thread pointer (TLS)
        pub fpsimd: [u128; 32], // NEON/FP registers
        pub fpcr: u32,          // FP control register
        pub fpsr: u32,          // FP status register
    }

    impl Default for Aarch64Context {
        fn default() -> Self {
            Self {
                x: [0; 31],
                sp: 0,
                pc: 0,
                pstate: 0x3c5, // EL1h, DAIF masked
                ttbr0: 0,
                tpidr_el0: 0,
                fpsimd: [0; 32],
                fpcr: 0,
                fpsr: 0,
            }
        }
    }

    /// Device Tree Blob parser stub
    pub struct DeviceTree {
        pub base: u64,
        pub size: usize,
    }

    impl DeviceTree {
        pub const DTB_MAGIC: u32 = 0xd00dfeed;

        pub fn validate(base: u64) -> bool {
            // Check magic number (big-endian)
            let ptr = base as *const u32;
            let magic = unsafe { core::ptr::read_volatile(ptr) };
            u32::from_be(magic) == Self::DTB_MAGIC
        }
    }
}

// ─── RISC-V 64 Definitions ─────────────────────────────────────────

pub mod riscv64 {
    /// RISC-V CSRs (Control and Status Registers)
    #[derive(Debug, Clone, Copy)]
    #[allow(non_camel_case_types)]
    pub enum Csr {
        // Machine mode
        MSTATUS,  // Machine Status
        MISA,     // Machine ISA
        MIE,      // Machine Interrupt Enable
        MTVEC,    // Machine Trap Vector
        MSCRATCH, // Machine Scratch
        MEPC,     // Machine Exception PC
        MCAUSE,   // Machine Cause
        MTVAL,    // Machine Trap Value
        MIP,      // Machine Interrupt Pending
        MHARTID,  // Hart ID
        // Supervisor mode
        SSTATUS,  // Supervisor Status
        SIE,      // Supervisor Interrupt Enable
        STVEC,    // Supervisor Trap Vector
        SSCRATCH, // Supervisor Scratch
        SEPC,     // Supervisor Exception PC
        SCAUSE,   // Supervisor Cause
        STVAL,    // Supervisor Trap Value
        SIP,      // Supervisor Interrupt Pending
        SATP,     // Supervisor Address Translation and Protection
        // Counter
        CYCLE,   // Cycle counter
        TIME,    // Timer
        INSTRET, // Instruction retired counter
    }

    /// RISC-V privilege modes
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PrivilegeMode {
        User = 0,
        Supervisor = 1,
        Hypervisor = 2, // H extension
        Machine = 3,
    }

    /// Sv48 page table entry (4-level, 48-bit virtual)
    #[derive(Debug, Clone, Copy)]
    pub struct Sv48Pte(pub u64);

    impl Sv48Pte {
        pub const V: u64 = 1 << 0; // Valid
        pub const R: u64 = 1 << 1; // Read
        pub const W: u64 = 1 << 2; // Write
        pub const X: u64 = 1 << 3; // Execute
        pub const U: u64 = 1 << 4; // User
        pub const G: u64 = 1 << 5; // Global
        pub const A: u64 = 1 << 6; // Accessed
        pub const D: u64 = 1 << 7; // Dirty

        pub fn is_valid(&self) -> bool {
            self.0 & Self::V != 0
        }
        pub fn is_leaf(&self) -> bool {
            self.0 & (Self::R | Self::W | Self::X) != 0
        }
        pub fn ppn(&self) -> u64 {
            (self.0 >> 10) & 0x0FFF_FFFF_FFFF
        }
    }

    /// PLIC (Platform-Level Interrupt Controller) layout
    pub struct Plic {
        pub base: u64,
    }

    impl Plic {
        pub const PRIORITY_OFFSET: u64 = 0x0000;
        pub const PENDING_OFFSET: u64 = 0x1000;
        pub const ENABLE_OFFSET: u64 = 0x2000;
        pub const THRESHOLD_OFFSET: u64 = 0x200000;
        pub const CLAIM_OFFSET: u64 = 0x200004;
    }

    /// CLINT (Core Local Interruptor) for timer
    pub struct Clint {
        pub base: u64,
    }

    impl Clint {
        pub const MSIP_OFFSET: u64 = 0x0;
        pub const MTIMECMP_OFFSET: u64 = 0x4000;
        pub const MTIME_OFFSET: u64 = 0xBFF8;
    }

    /// RISC-V CPU context for context switching
    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub struct Riscv64Context {
        pub x: [u64; 32],  // x0-x31 general purpose registers
        pub pc: u64,       // Program counter (sepc)
        pub sstatus: u64,  // Supervisor status
        pub satp: u64,     // Page table base
        pub tp: u64,       // Thread pointer (x4)
        pub fp: [u64; 32], // f0-f31 floating point registers
        pub fcsr: u32,     // FP control and status
    }

    /// RISC-V ISA extensions
    #[derive(Debug, Clone, Copy)]
    pub struct IsaExtensions {
        pub i: bool, // Base integer
        pub m: bool, // Multiply/divide
        pub a: bool, // Atomics
        pub f: bool, // Single-precision float
        pub d: bool, // Double-precision float
        pub c: bool, // Compressed instructions
        pub v: bool, // Vector
        pub h: bool, // Hypervisor
        pub s: bool, // Supervisor mode
    }

    impl Default for IsaExtensions {
        fn default() -> Self {
            Self {
                i: true,
                m: true,
                a: true,
                f: true,
                d: true,
                c: true,
                v: false,
                h: false,
                s: true,
            }
        }
    }
}

// ─── Cross-Architecture Target Spec ─────────────────────────────────

/// Target specification for cross-compilation
pub struct TargetSpec {
    pub arch: Architecture,
    pub triple: &'static str,
    pub features: &'static str,
    pub linker_flavor: &'static str,
    pub data_layout: &'static str,
}

pub fn target_specs() -> [TargetSpec; 3] {
    [
        TargetSpec {
            arch: Architecture::X86_64,
            triple: "x86_64-unknown-knoxos",
            features: "-mmx,-sse,-sse2,+soft-float",
            linker_flavor: "ld.lld",
            data_layout: "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128",
        },
        TargetSpec {
            arch: Architecture::Aarch64,
            triple: "aarch64-unknown-knoxos",
            features: "+strict-align,+neon,+fp-armv8",
            linker_flavor: "ld.lld",
            data_layout: "e-m:e-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128",
        },
        TargetSpec {
            arch: Architecture::Riscv64,
            triple: "riscv64gc-unknown-knoxos",
            features: "+m,+a,+f,+d,+c",
            linker_flavor: "ld.lld",
            data_layout: "e-m:e-p:64:64-i64:64-i128:128-n32:64-S128",
        },
    ]
}

// ─── ELF Machine Types ──────────────────────────────────────────────

/// Map architecture to ELF machine type
pub fn elf_machine(arch: Architecture) -> u16 {
    match arch {
        Architecture::X86_64 => 0x3E,  // EM_X86_64
        Architecture::Aarch64 => 0xB7, // EM_AARCH64
        Architecture::Riscv64 => 0xF3, // EM_RISCV
    }
}

/// ELF relocation types per architecture
pub fn elf_reloc_types(arch: Architecture) -> &'static [(&'static str, u32)] {
    match arch {
        Architecture::X86_64 => &[
            ("R_X86_64_NONE", 0),
            ("R_X86_64_64", 1),
            ("R_X86_64_PC32", 2),
            ("R_X86_64_GLOB_DAT", 6),
            ("R_X86_64_JUMP_SLOT", 7),
            ("R_X86_64_RELATIVE", 8),
        ],
        Architecture::Aarch64 => &[
            ("R_AARCH64_NONE", 0),
            ("R_AARCH64_ABS64", 257),
            ("R_AARCH64_GLOB_DAT", 1025),
            ("R_AARCH64_JUMP_SLOT", 1026),
            ("R_AARCH64_RELATIVE", 1027),
            ("R_AARCH64_CALL26", 283),
        ],
        Architecture::Riscv64 => &[
            ("R_RISCV_NONE", 0),
            ("R_RISCV_64", 2),
            ("R_RISCV_RELATIVE", 3),
            ("R_RISCV_JAL", 17),
            ("R_RISCV_CALL", 18),
            ("R_RISCV_GOT_HI20", 20),
        ],
    }
}

// ─── Initialization ─────────────────────────────────────────────────

pub fn init() {
    // Detect and set current architecture
    // On x86_64, this is always x86_64
    CURRENT_ARCH.store(0, Ordering::Relaxed);

    crate::serial_println!("[KnoxOS] Architecture abstraction layer initialized");
    crate::serial_println!(
        "[KnoxOS]   Current: {} ({})",
        current_arch().name(),
        current_arch().interrupt_controller()
    );
    crate::serial_println!("[KnoxOS]   Supported targets: x86_64, aarch64, riscv64");
    crate::serial_println!("[KnoxOS]   Page size: {} bytes", current_arch().page_size());
    crate::serial_println!("[KnoxOS]   Max CPUs: {}", current_arch().max_cpus());
}
