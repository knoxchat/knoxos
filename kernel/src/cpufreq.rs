/// CPU Frequency Scaling — CPU P-state and frequency management
///
/// Provides dynamic CPU frequency management:
///   - P-state enumeration and switching
///   - Governor policies (performance, powersave, ondemand, conservative)
///   - Frequency scaling based on CPU load
///   - Per-core frequency control
///   - Thermal throttling integration
///   - Energy-performance preference (EPP) via MSR
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// CONSTANTS & MSRs
// ═══════════════════════════════════════════════════════════════════════

/// MSR addresses for frequency management
const MSR_IA32_PERF_STATUS: u32 = 0x198;
const MSR_IA32_PERF_CTL: u32 = 0x199;
const MSR_IA32_MISC_ENABLE: u32 = 0x1A0;
const MSR_IA32_ENERGY_PERF_BIAS: u32 = 0x1B0;
const MSR_IA32_HWP_REQUEST: u32 = 0x774;
const MSR_IA32_HWP_CAPABILITIES: u32 = 0x771;

// ═══════════════════════════════════════════════════════════════════════
// GOVERNOR TYPES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Governor {
    /// Always maximum frequency
    Performance,
    /// Always minimum frequency
    Powersave,
    /// Scale based on load (aggressive upscaling)
    Ondemand,
    /// Scale based on load (gradual changes)
    Conservative,
    /// Hardware-managed (Intel HWP / AMD CPPC)
    Schedutil,
}

impl Governor {
    pub fn name(&self) -> &'static str {
        match self {
            Governor::Performance => "performance",
            Governor::Powersave => "powersave",
            Governor::Ondemand => "ondemand",
            Governor::Conservative => "conservative",
            Governor::Schedutil => "schedutil",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "performance" => Some(Governor::Performance),
            "powersave" => Some(Governor::Powersave),
            "ondemand" => Some(Governor::Ondemand),
            "conservative" => Some(Governor::Conservative),
            "schedutil" => Some(Governor::Schedutil),
            _ => None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// P-STATE INFO
// ═══════════════════════════════════════════════════════════════════════

/// Frequency in MHz
pub type FreqMHz = u32;

#[derive(Debug, Clone, Copy)]
pub struct PState {
    pub ratio: u8, // CPU ratio/multiplier
    pub freq_mhz: FreqMHz,
    pub voltage_mv: u16, // Core voltage in millivolts
}

#[derive(Debug, Clone)]
pub struct FreqInfo {
    pub min_freq: FreqMHz,
    pub max_freq: FreqMHz,
    pub base_freq: FreqMHz,
    pub turbo_freq: FreqMHz,
    pub current_freq: FreqMHz,
    pub bus_freq: FreqMHz, // Base clock (usually 100 MHz)
    pub hwp_supported: bool,
    pub turbo_enabled: bool,
}

impl FreqInfo {
    pub fn new() -> Self {
        Self {
            min_freq: 800,
            max_freq: 3600,
            base_freq: 2400,
            turbo_freq: 4200,
            current_freq: 2400,
            bus_freq: 100,
            hwp_supported: false,
            turbo_enabled: true,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CPUFREQ STATE
// ═══════════════════════════════════════════════════════════════════════

static CURRENT_GOVERNOR: AtomicU8 = AtomicU8::new(2); // Ondemand by default
static TARGET_FREQ: AtomicU64 = AtomicU64::new(0);
static SCALING_ENABLED: AtomicBool = AtomicBool::new(false);

lazy_static::lazy_static! {
    static ref FREQ_INFO: Mutex<FreqInfo> = Mutex::new(FreqInfo::new());
}

fn governor_from_u8(v: u8) -> Governor {
    match v {
        0 => Governor::Performance,
        1 => Governor::Powersave,
        2 => Governor::Ondemand,
        3 => Governor::Conservative,
        4 => Governor::Schedutil,
        _ => Governor::Ondemand,
    }
}

fn governor_to_u8(g: Governor) -> u8 {
    match g {
        Governor::Performance => 0,
        Governor::Powersave => 1,
        Governor::Ondemand => 2,
        Governor::Conservative => 3,
        Governor::Schedutil => 4,
    }
}

/// Read MSR (safe wrapper)
unsafe fn read_msr(msr: u32) -> u64 {
    let (mut low, mut high): (u32, u32) = (0, 0);
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
    );
    ((high as u64) << 32) | (low as u64)
}

/// Write MSR (safe wrapper)
unsafe fn write_msr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// FREQUENCY DETECTION
// ═══════════════════════════════════════════════════════════════════════

/// Detect CPU frequency capabilities using CPUID
pub fn detect_frequencies() {
    let mut info = FREQ_INFO.lock();

    // Try to read from CPUID leaf 0x16 (Processor Frequency Information)
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();

    if let Some(freq_info) = cpuid.get_processor_frequency_info() {
        let base = freq_info.processor_base_frequency();
        let max = freq_info.processor_max_frequency();
        let bus = freq_info.bus_frequency();

        if base > 0 {
            info.base_freq = base as u32;
        }
        if max > 0 {
            info.turbo_freq = max as u32;
            info.max_freq = max as u32;
        }
        if bus > 0 {
            info.bus_freq = bus as u32;
        }
    }

    // Check HWP support (CPUID.06H:EAX[bit 7])
    if let Some(thermal) = cpuid.get_thermal_power_info() {
        info.hwp_supported = thermal.has_hwp();
    }

    // Read current frequency from MSR (may #GP on some QEMU CPU models)
    // Skip raw MSR reads — rely on CPUID data which is always safe
    if info.base_freq > 0 {
        info.current_freq = info.base_freq;
    }

    serial_println!(
        "[CpuFreq] Base={}MHz Max={}MHz Turbo={}MHz Bus={}MHz HWP={}",
        info.base_freq,
        info.max_freq,
        info.turbo_freq,
        info.bus_freq,
        info.hwp_supported
    );
}

// ═══════════════════════════════════════════════════════════════════════
// GOVERNOR LOGIC
// ═══════════════════════════════════════════════════════════════════════

/// Set the active governor
pub fn set_governor(gov: Governor) {
    CURRENT_GOVERNOR.store(governor_to_u8(gov), Ordering::Relaxed);
    serial_println!("[CpuFreq] Governor set to: {}", gov.name());

    // Apply immediate changes for static governors
    match gov {
        Governor::Performance => set_frequency_max(),
        Governor::Powersave => set_frequency_min(),
        _ => {}
    }
}

/// Get current governor
pub fn current_governor() -> Governor {
    governor_from_u8(CURRENT_GOVERNOR.load(Ordering::Relaxed))
}

/// Set CPU to maximum frequency
fn set_frequency_max() {
    let info = FREQ_INFO.lock();
    let target = if info.turbo_enabled {
        info.turbo_freq
    } else {
        info.max_freq
    };
    drop(info);
    set_frequency(target);
}

/// Set CPU to minimum frequency
fn set_frequency_min() {
    let info = FREQ_INFO.lock();
    let target = info.min_freq;
    drop(info);
    set_frequency(target);
}

/// Set a specific CPU frequency (in MHz)
pub fn set_frequency(freq_mhz: FreqMHz) {
    let info = FREQ_INFO.lock();
    let bus = info.bus_freq;
    let min = info.min_freq;
    let max = if info.turbo_enabled {
        info.turbo_freq
    } else {
        info.max_freq
    };
    drop(info);

    let clamped = freq_mhz.max(min).min(max);
    let ratio = clamped / bus.max(1);

    TARGET_FREQ.store(clamped as u64, Ordering::Relaxed);

    // Write to PERF_CTL MSR
    unsafe {
        let value = (ratio as u64) << 8;
        write_msr(MSR_IA32_PERF_CTL, value);
    }

    // Update stored current frequency
    FREQ_INFO.lock().current_freq = clamped;
}

/// Called periodically to adjust frequency based on load
pub fn tick(cpu_load_percent: u8) {
    if !SCALING_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let gov = current_governor();
    let info = FREQ_INFO.lock();
    let min = info.min_freq;
    let max = if info.turbo_enabled {
        info.turbo_freq
    } else {
        info.max_freq
    };
    let current = info.current_freq;
    let base = info.base_freq;
    drop(info);

    let target = match gov {
        Governor::Performance => max,
        Governor::Powersave => min,
        Governor::Ondemand => {
            // Jump to max when load > 80%, else scale down
            if cpu_load_percent > 80 {
                max
            } else if cpu_load_percent < 20 {
                min
            } else {
                let range = max - min;
                min + (range * cpu_load_percent as u32 / 100)
            }
        }
        Governor::Conservative => {
            // Gradual step changes (±5% per tick)
            let step = (max - min) / 20; // 5% of range
            if cpu_load_percent > 75 {
                (current + step).min(max)
            } else if cpu_load_percent < 25 {
                if current > step + min {
                    current - step
                } else {
                    min
                }
            } else {
                current
            }
        }
        Governor::Schedutil => {
            // HWP: let hardware manage
            if FREQ_INFO.lock().hwp_supported {
                // Set HWP request for desired performance level
                unsafe {
                    let perf = cpu_load_percent as u64;
                    let hwp_req = perf | (perf << 8) | (0xFF_u64 << 16);
                    write_msr(MSR_IA32_HWP_REQUEST, hwp_req);
                }
                return;
            }
            // Fallback to ondemand behavior
            let range = max - min;
            min + (range * cpu_load_percent as u32 / 100)
        }
    };

    if target != current {
        set_frequency(target);
    }
}

/// Get current frequency info
pub fn get_freq_info() -> FreqInfo {
    FREQ_INFO.lock().clone()
}

/// Get current frequency in MHz
pub fn current_freq() -> FreqMHz {
    FREQ_INFO.lock().current_freq
}

/// Enable/disable scaling
pub fn set_scaling_enabled(enabled: bool) {
    SCALING_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Initialize CPU frequency scaling
pub fn init() {
    detect_frequencies();
    SCALING_ENABLED.store(true, Ordering::Relaxed);
    serial_println!("[KnoxOS] CPU frequency scaling initialized");
}
