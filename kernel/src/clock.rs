/// clock — POSIX clocks and high-resolution timers
/// Linux-compatible clock_gettime, clock_settime, clock_getres, timers
///
/// Clock IDs: REALTIME, MONOTONIC, PROCESS_CPUTIME_ID, THREAD_CPUTIME_ID,
///            MONOTONIC_RAW, REALTIME_COARSE, MONOTONIC_COARSE, BOOTTIME
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// POSIX clock IDs (matching Linux clockid_t)
pub const CLOCK_REALTIME: u32 = 0;
pub const CLOCK_MONOTONIC: u32 = 1;
pub const CLOCK_PROCESS_CPUTIME_ID: u32 = 2;
pub const CLOCK_THREAD_CPUTIME_ID: u32 = 3;
pub const CLOCK_MONOTONIC_RAW: u32 = 4;
pub const CLOCK_REALTIME_COARSE: u32 = 5;
pub const CLOCK_MONOTONIC_COARSE: u32 = 6;
pub const CLOCK_BOOTTIME: u32 = 7;
pub const CLOCK_REALTIME_ALARM: u32 = 8;
pub const CLOCK_BOOTTIME_ALARM: u32 = 9;
pub const CLOCK_TAI: u32 = 11;

/// Timespec structure
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

impl Timespec {
    pub fn new(sec: i64, nsec: i64) -> Self {
        Timespec {
            tv_sec: sec,
            tv_nsec: nsec,
        }
    }

    pub fn zero() -> Self {
        Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        }
    }

    pub fn to_ns(&self) -> i64 {
        self.tv_sec * 1_000_000_000 + self.tv_nsec
    }

    pub fn from_ns(ns: i64) -> Self {
        Timespec {
            tv_sec: ns / 1_000_000_000,
            tv_nsec: ns % 1_000_000_000,
        }
    }

    pub fn add(&self, other: &Timespec) -> Timespec {
        let ns = self.to_ns() + other.to_ns();
        Self::from_ns(ns)
    }
}

/// itimerspec for interval timers
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct ItimerSpec {
    pub it_interval: Timespec, // Timer interval
    pub it_value: Timespec,    // Initial expiration
}

/// Clock state
struct ClockState {
    /// Boot timestamp (seconds since epoch from RTC)
    boot_time_secs: i64,
    /// TSC frequency estimate (Hz)
    tsc_freq: u64,
    /// TSC value at boot
    boot_tsc: u64,
    /// Monotonic offset (for clock_settime adjustments)
    realtime_offset_ns: i64,
}

lazy_static::lazy_static! {
    static ref CLOCK_STATE: Mutex<ClockState> = Mutex::new(ClockState {
        boot_time_secs: 0,
        tsc_freq: 2_000_000_000, // Default 2 GHz estimate
        boot_tsc: 0,
        realtime_offset_ns: 0,
    });
}

/// Get current TSC value
fn read_tsc() -> u64 {
    crate::arch_compat::read_tsc()
}

/// Convert TSC ticks to nanoseconds
fn tsc_to_ns(ticks: u64) -> i64 {
    let state = CLOCK_STATE.lock();
    if state.tsc_freq == 0 {
        return 0;
    }
    // ticks * 1_000_000_000 / freq, avoiding overflow
    let secs = ticks / state.tsc_freq;
    let remainder = ticks % state.tsc_freq;
    (secs as i64) * 1_000_000_000 + (remainder as i64 * 1_000_000_000 / state.tsc_freq as i64)
}

/// Get monotonic time (nanoseconds since boot)
pub fn monotonic_ns() -> i64 {
    let state = CLOCK_STATE.lock();
    let ticks = read_tsc().wrapping_sub(state.boot_tsc);
    let freq = state.tsc_freq;
    drop(state);

    if freq == 0 {
        return 0;
    }

    let secs = ticks / freq;
    let remainder = ticks % freq;
    (secs as i64) * 1_000_000_000 + (remainder as i64 * 1_000_000_000 / freq as i64)
}

/// Get realtime (nanoseconds since epoch)
pub fn realtime_ns() -> i64 {
    let state = CLOCK_STATE.lock();
    let mono = monotonic_ns();
    state.boot_time_secs * 1_000_000_000 + mono + state.realtime_offset_ns
}

/// clock_gettime implementation
pub fn clock_gettime(clock_id: u32) -> Result<Timespec, i32> {
    match clock_id {
        CLOCK_REALTIME | CLOCK_REALTIME_COARSE | CLOCK_REALTIME_ALARM => {
            let ns = realtime_ns();
            Ok(Timespec::from_ns(ns))
        }
        CLOCK_MONOTONIC
        | CLOCK_MONOTONIC_RAW
        | CLOCK_MONOTONIC_COARSE
        | CLOCK_BOOTTIME
        | CLOCK_BOOTTIME_ALARM => {
            let ns = monotonic_ns();
            Ok(Timespec::from_ns(ns))
        }
        CLOCK_PROCESS_CPUTIME_ID | CLOCK_THREAD_CPUTIME_ID => {
            // Return monotonic time as approximation
            let ns = monotonic_ns();
            Ok(Timespec::from_ns(ns))
        }
        CLOCK_TAI => {
            // TAI = UTC + leap seconds (approximate as UTC)
            let ns = realtime_ns();
            Ok(Timespec::from_ns(ns))
        }
        _ => Err(-22), // EINVAL
    }
}

/// clock_settime implementation (only CLOCK_REALTIME can be set)
pub fn clock_settime(clock_id: u32, tp: &Timespec) -> Result<(), i32> {
    match clock_id {
        CLOCK_REALTIME => {
            let uid = crate::users::get_current_uid();
            if uid != 0 {
                return Err(-1); // EPERM
            }

            let current_realtime = realtime_ns();
            let target_ns = tp.to_ns();
            let mut state = CLOCK_STATE.lock();
            state.realtime_offset_ns += target_ns - current_realtime;
            Ok(())
        }
        CLOCK_MONOTONIC | CLOCK_MONOTONIC_RAW | CLOCK_BOOTTIME => {
            Err(-22) // EINVAL — monotonic clocks can't be set
        }
        _ => Err(-22),
    }
}

/// clock_getres implementation
pub fn clock_getres(clock_id: u32) -> Result<Timespec, i32> {
    match clock_id {
        CLOCK_REALTIME
        | CLOCK_MONOTONIC
        | CLOCK_MONOTONIC_RAW
        | CLOCK_PROCESS_CPUTIME_ID
        | CLOCK_THREAD_CPUTIME_ID
        | CLOCK_BOOTTIME
        | CLOCK_TAI => {
            // 1 nanosecond resolution (TSC-based)
            Ok(Timespec::new(0, 1))
        }
        CLOCK_REALTIME_COARSE | CLOCK_MONOTONIC_COARSE => {
            // ~1ms resolution for coarse clocks
            Ok(Timespec::new(0, 1_000_000))
        }
        _ => Err(-22),
    }
}

/// Calibrate TSC frequency using PIT
pub fn calibrate_tsc() {
    // Simple calibration: measure TSC ticks over a short PIT delay
    // We use PIT channel 2 for a ~10ms delay

    // For now, estimate from CPUID if available
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    let freq = if let Some(tsc_info) = cpuid.get_tsc_info() {
        let crystal_freq = tsc_info.tsc_frequency().unwrap_or(0);
        if crystal_freq > 0 {
            crystal_freq
        } else {
            2_000_000_000 // Default 2 GHz
        }
    } else {
        2_000_000_000
    };

    let mut state = CLOCK_STATE.lock();
    state.tsc_freq = freq;
    state.boot_tsc = read_tsc();

    serial_println!("[KnoxOS] TSC frequency: {} MHz", freq / 1_000_000);
}

/// Set boot time from RTC
pub fn set_boot_time(epoch_secs: i64) {
    CLOCK_STATE.lock().boot_time_secs = epoch_secs;
}

/// nanosleep implementation
pub fn nanosleep(req: &Timespec) -> Result<Timespec, i32> {
    if req.tv_sec < 0 || req.tv_nsec < 0 || req.tv_nsec >= 1_000_000_000 {
        return Err(-22); // EINVAL
    }

    let target_ns = monotonic_ns() + req.to_ns();

    // Busy-wait with HLT for low power
    while monotonic_ns() < target_ns {
        crate::arch_compat::instructions::interrupts::hlt();
    }

    Ok(Timespec::zero()) // No remaining time
}

/// clock_nanosleep implementation
pub fn clock_nanosleep(clock_id: u32, flags: i32, req: &Timespec) -> Result<Timespec, i32> {
    let timer_abstime = 1;

    if flags & timer_abstime != 0 {
        // Absolute time
        let now = clock_gettime(clock_id)?;
        let delay_ns = req.to_ns() - now.to_ns();
        if delay_ns <= 0 {
            return Ok(Timespec::zero());
        }
        nanosleep(&Timespec::from_ns(delay_ns))
    } else {
        nanosleep(req)
    }
}

/// Get uptime in seconds
pub fn uptime_seconds() -> u64 {
    (monotonic_ns() / 1_000_000_000) as u64
}

/// Get timer ticks (delegates to interrupts::get_ticks)
pub fn get_ticks() -> u64 {
    crate::interrupts::get_ticks()
}

pub fn init() {
    // Calibrate TSC
    calibrate_tsc();

    // Set boot time from RTC
    let epoch_secs = crate::rtc::unix_time();
    set_boot_time(epoch_secs);

    serial_println!(
        "[KnoxOS] POSIX clocks initialized (boot time: {} epoch)",
        epoch_secs
    );
}
