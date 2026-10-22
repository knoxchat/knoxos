use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use spin::Mutex;

use super::regs::IOAPIC_BASE;

// ─── Per-CPU Data ───────────────────────────────────────────────────────

/// Maximum number of CPUs supported
pub const MAX_CPUS: usize = 16;

const _: () = assert!(MAX_CPUS == crate::gdt::MAX_CPUS);

/// Per-CPU data structure
#[derive(Debug)]
pub struct PerCpuData {
    /// APIC ID
    pub apic_id: u32,
    /// CPU index (0-based)
    pub cpu_index: u32,
    /// Is this the BSP (Bootstrap Processor)?
    pub is_bsp: bool,
    /// Is this CPU online?
    pub online: bool,
    /// Current process PID on this CPU
    pub current_pid: u32,
    /// Number of context switches on this CPU
    pub context_switches: u64,
    /// Number of timer interrupts
    pub timer_ticks: u64,
    /// CPU idle ticks
    pub idle_ticks: u64,
}

impl Default for PerCpuData {
    fn default() -> Self {
        Self::new()
    }
}

impl PerCpuData {
    pub const fn new() -> Self {
        Self {
            apic_id: 0,
            cpu_index: 0,
            is_bsp: false,
            online: false,
            current_pid: 0,
            context_switches: 0,
            timer_ticks: 0,
            idle_ticks: 0,
        }
    }
}

// ─── Global State ───────────────────────────────────────────────────────

/// Number of CPUs detected
pub(crate) static NUM_CPUS: AtomicU32 = AtomicU32::new(1);
/// Number of CPUs that have started
pub(crate) static CPUS_STARTED: AtomicU32 = AtomicU32::new(1);
/// Is APIC available and initialized?
pub(crate) static APIC_AVAILABLE: AtomicBool = AtomicBool::new(false);
/// Physical memory offset for MMIO access
pub(crate) static PHYS_OFFSET: AtomicU64 = AtomicU64::new(0);
/// I/O APIC MMIO physical base (MADT when present, else 0xFEC00000)
pub(crate) static IOAPIC_PHYS: AtomicU64 = AtomicU64::new(IOAPIC_BASE);
/// I/O APIC GSI base from MADT
pub(crate) static IOAPIC_GSI_BASE: AtomicU32 = AtomicU32::new(0);
/// APIC timer frequency (ticks per second)
pub(crate) static TIMER_FREQUENCY: AtomicU32 = AtomicU32::new(0);

lazy_static::lazy_static! {
    /// Per-CPU data array
    pub static ref CPU_DATA: Mutex<[PerCpuData; MAX_CPUS]> = {
        const INIT: PerCpuData = PerCpuData::new();
        Mutex::new([INIT; MAX_CPUS])
    };
}

/// Is APIC available?
pub fn is_available() -> bool {
    APIC_AVAILABLE.load(Ordering::Relaxed)
}

/// Get number of CPUs
pub fn num_cpus() -> u32 {
    NUM_CPUS.load(Ordering::Relaxed)
}

/// Get number of online CPUs
pub fn online_cpus() -> u32 {
    CPUS_STARTED.load(Ordering::Relaxed)
}

/// Get CPU statistics
pub fn cpu_stats() -> Vec<(u32, u32, bool, u64, u64)> {
    let cpus = CPU_DATA.lock();
    let mut stats = Vec::new();
    for cpu in cpus.iter() {
        if cpu.online {
            stats.push((
                cpu.cpu_index,
                cpu.apic_id,
                cpu.is_bsp,
                cpu.timer_ticks,
                cpu.context_switches,
            ));
        }
    }
    stats
}
