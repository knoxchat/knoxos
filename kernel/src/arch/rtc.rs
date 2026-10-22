#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;
/// RTC - CMOS Real-Time Clock driver
/// Reads the actual date/time from the CMOS RTC chip
/// Provides Linux-compatible time functions (clock_gettime, gettimeofday)
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

/// CMOS I/O ports
const CMOS_ADDRESS: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

/// CMOS register addresses
const RTC_SECONDS: u8 = 0x00;
const RTC_MINUTES: u8 = 0x02;
const RTC_HOURS: u8 = 0x04;
const RTC_DAY_OF_WEEK: u8 = 0x06;
const RTC_DAY_OF_MONTH: u8 = 0x07;
const RTC_MONTH: u8 = 0x08;
const RTC_YEAR: u8 = 0x09;
const RTC_CENTURY: u8 = 0x32;
const RTC_STATUS_A: u8 = 0x0A;
const RTC_STATUS_B: u8 = 0x0B;

/// Time structure (matches Linux struct tm)
#[derive(Debug, Clone, Copy)]
pub struct DateTime {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u16,
    pub day_of_week: u8,
}

impl DateTime {
    /// Civil date from a Unix timestamp (seconds since 1970-01-01 00:00:00 UTC).
    /// `day_of_week` is 0 = Sunday … 6 = Saturday.
    pub fn from_unix(ts: i64) -> Self {
        let mut days = ts / 86400;
        let mut time_of_day = (ts % 86400) as i32;
        if time_of_day < 0 {
            time_of_day += 86400;
            days -= 1;
        }

        let hour = (time_of_day / 3600) as u8;
        let minute = ((time_of_day % 3600) / 60) as u8;
        let second = (time_of_day % 60) as u8;
        let dow = ((days % 7 + 4) % 7) as u8;

        let z = days + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = (z - era * 146097) as u32;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe as i64 + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = if m <= 2 { y + 1 } else { y };

        DateTime {
            second,
            minute,
            hour,
            day: d as u8,
            month: m as u8,
            year: year.clamp(1970, 9999) as u16,
            day_of_week: dow,
        }
    }

    /// Convert to Unix timestamp (seconds since 1970-01-01 00:00:00 UTC)
    pub fn to_unix_timestamp(&self) -> i64 {
        let mut y = self.year as i64;
        let mut m = self.month as i64;
        if m <= 2 {
            y -= 1;
            m += 12;
        }
        // Days from epoch to year start
        let days = 365 * y + y / 4 - y / 100 + y / 400 + (153 * (m - 3) + 2) / 5 + self.day as i64
            - 719469;
        days * 86400 + self.hour as i64 * 3600 + self.minute as i64 * 60 + self.second as i64
    }
}

/// Timespec structure (matches Linux struct timespec)
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

/// Timeval structure (matches Linux struct timeval)
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Timeval {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

/// Timezone structure (matches Linux struct timezone)
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Timezone {
    pub tz_minuteswest: i32,
    pub tz_dsttime: i32,
}

/// Linux clock IDs
pub const CLOCK_REALTIME: u32 = 0;
pub const CLOCK_MONOTONIC: u32 = 1;
pub const CLOCK_PROCESS_CPUTIME_ID: u32 = 2;
pub const CLOCK_THREAD_CPUTIME_ID: u32 = 3;
pub const CLOCK_MONOTONIC_RAW: u32 = 4;
pub const CLOCK_REALTIME_COARSE: u32 = 5;
pub const CLOCK_MONOTONIC_COARSE: u32 = 6;
pub const CLOCK_BOOTTIME: u32 = 7;

/// Boot time in Unix epoch seconds
static BOOT_TIME: Mutex<i64> = Mutex::new(0);

/// Monotonic tick counter (from PIT, ~18.2 Hz)
static MONOTONIC_TICKS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Read a CMOS register
unsafe fn read_cmos(register: u8) -> u8 {
    let mut addr_port = Port::<u8>::new(CMOS_ADDRESS);
    let mut data_port = Port::<u8>::new(CMOS_DATA);

    // Disable NMI (bit 7) and select register
    addr_port.write(0x80 | register);
    data_port.read()
}

/// Check if CMOS is currently updating
unsafe fn cmos_is_updating() -> bool {
    let mut addr_port = Port::<u8>::new(CMOS_ADDRESS);
    let mut data_port = Port::<u8>::new(CMOS_DATA);
    addr_port.write(0x80 | RTC_STATUS_A);
    (data_port.read() & 0x80) != 0
}

/// Convert BCD to binary
fn bcd_to_bin(bcd: u8) -> u8 {
    (bcd & 0x0F) + ((bcd >> 4) * 10)
}

/// Read the current date/time from CMOS RTC
pub fn read_rtc() -> DateTime {
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    unsafe {
        // Wait for update to complete
        while cmos_is_updating() {}

        let mut second = read_cmos(RTC_SECONDS);
        let mut minute = read_cmos(RTC_MINUTES);
        let mut hour = read_cmos(RTC_HOURS);
        let day_of_week = read_cmos(RTC_DAY_OF_WEEK);
        let mut day = read_cmos(RTC_DAY_OF_MONTH);
        let mut month = read_cmos(RTC_MONTH);
        let mut year = read_cmos(RTC_YEAR) as u16;

        // Read again to ensure consistency
        while cmos_is_updating() {}
        let second2 = read_cmos(RTC_SECONDS);
        let minute2 = read_cmos(RTC_MINUTES);
        let hour2 = read_cmos(RTC_HOURS);
        let day2 = read_cmos(RTC_DAY_OF_MONTH);
        let month2 = read_cmos(RTC_MONTH);
        let year2 = read_cmos(RTC_YEAR) as u16;

        // If values changed, read once more
        if second != second2
            || minute != minute2
            || hour != hour2
            || day != day2
            || month != month2
            || year != year2
        {
            while cmos_is_updating() {}
            second = read_cmos(RTC_SECONDS);
            minute = read_cmos(RTC_MINUTES);
            hour = read_cmos(RTC_HOURS);
            day = read_cmos(RTC_DAY_OF_MONTH);
            month = read_cmos(RTC_MONTH);
            year = read_cmos(RTC_YEAR) as u16;
        }

        // Check format from status register B
        let status_b = read_cmos(RTC_STATUS_B);
        let is_bcd = (status_b & 0x04) == 0;
        let is_24h = (status_b & 0x02) != 0;

        if is_bcd {
            second = bcd_to_bin(second);
            minute = bcd_to_bin(minute);
            hour = bcd_to_bin(hour & 0x7F) | (hour & 0x80);
            day = bcd_to_bin(day);
            month = bcd_to_bin(month);
            year = bcd_to_bin(year as u8) as u16;
        }

        if !is_24h && (hour & 0x80) != 0 {
            hour = ((hour & 0x7F) + 12) % 24;
        }

        // Try to read century register
        let century = read_cmos(RTC_CENTURY);
        let century_val = if is_bcd { bcd_to_bin(century) } else { century } as u16;

        if (19..=21).contains(&century_val) {
            year += century_val * 100;
        } else {
            year += 2000; // Default to 21st century
        }

        DateTime {
            second,
            minute,
            hour,
            day,
            month,
            day_of_week,
            year,
        }
    }
}

/// Convert a DateTime to Unix epoch seconds
pub fn datetime_to_unix(dt: &DateTime) -> i64 {
    let mut year = dt.year as i64;
    let mut month = dt.month as i64;

    // Adjust for months <= February
    if month <= 2 {
        year -= 1;
        month += 12;
    }

    // Days from years
    let days = 365 * year + year / 4 - year / 100 + year / 400;
    // Days from months (March-based, shifted)
    let days = days + (153 * (month - 3) + 2) / 5;
    // Add day of month
    let days = days + dt.day as i64;
    // Subtract Unix epoch offset (days from year 0 to 1970-01-01)
    let days = days - 719469;

    days * 86400 + dt.hour as i64 * 3600 + dt.minute as i64 * 60 + dt.second as i64
}

/// Get current Unix timestamp
pub fn unix_time() -> i64 {
    let dt = read_rtc();
    datetime_to_unix(&dt)
}

/// Wall-clock local time for UI (RTC is UTC; timezone offset applied).
pub fn local_now() -> DateTime {
    let utc = unix_time();
    if crate::timezone::is_ready() {
        let local = crate::timezone::to_local_time(utc);
        DateTime {
            second: local.second,
            minute: local.minute,
            hour: local.hour,
            day: local.day,
            month: local.month,
            year: local.year.clamp(1970, 9999) as u16,
            day_of_week: local.day_of_week,
        }
    } else {
        DateTime::from_unix(utc)
    }
}

/// Get time since boot in nanoseconds
pub fn monotonic_ns() -> i64 {
    let ticks = MONOTONIC_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    // PIT runs at ~1.193182 MHz, IRQ0 fires at ~18.2 Hz
    // Each tick ≈ 54.925 ms = 54925373 ns
    (ticks as i64) * 54_925_373
}

/// Update monotonic counter (called from timer interrupt)
pub fn tick() {
    MONOTONIC_TICKS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    crate::cron::timer_irq_tick();
}

/// Linux clock_gettime implementation
pub fn clock_gettime(clock_id: u32) -> Timespec {
    match clock_id {
        CLOCK_REALTIME | CLOCK_REALTIME_COARSE => {
            let secs = unix_time();
            Timespec {
                tv_sec: secs,
                tv_nsec: 0, // RTC only has 1-second resolution
            }
        }
        CLOCK_MONOTONIC | CLOCK_MONOTONIC_RAW | CLOCK_MONOTONIC_COARSE | CLOCK_BOOTTIME => {
            let ns = monotonic_ns();
            Timespec {
                tv_sec: ns / 1_000_000_000,
                tv_nsec: ns % 1_000_000_000,
            }
        }
        CLOCK_PROCESS_CPUTIME_ID | CLOCK_THREAD_CPUTIME_ID => {
            // Same as monotonic for now
            let ns = monotonic_ns();
            Timespec {
                tv_sec: ns / 1_000_000_000,
                tv_nsec: ns % 1_000_000_000,
            }
        }
        _ => Timespec::default(),
    }
}

/// Linux gettimeofday implementation
pub fn gettimeofday() -> (Timeval, Timezone) {
    let secs = unix_time();
    (
        Timeval {
            tv_sec: secs,
            tv_usec: 0,
        },
        Timezone {
            tz_minuteswest: if crate::timezone::is_ready() {
                -(crate::timezone::get_utc_offset(secs) / 60)
            } else {
                0
            },
            tz_dsttime: 0,
        },
    )
}

/// Get uptime in seconds
pub fn uptime_seconds() -> u64 {
    let ns = monotonic_ns();
    (ns / 1_000_000_000) as u64
}

pub const GATE_UEFI_MARKER: &str = "GATE_UEFI time";

lazy_static::lazy_static! {
    static ref UEFI_VARS: Mutex<BTreeMap<(String, [u8; 16]), Vec<u8>>> =
        Mutex::new(BTreeMap::new());
}

static UEFI_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// UEFI `GetTime` — CMOS RTC, not a hardcoded calendar.
pub fn uefi_get_time() -> Option<(u16, u8, u8, u8, u8, u8)> {
    if !UEFI_AVAILABLE.load(Ordering::SeqCst) {
        return None;
    }
    let dt = read_rtc();
    Some((dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second))
}

/// UEFI `SetVariable` — in-kernel variable store.
pub fn uefi_set_variable(name: &str, guid: &[u8; 16], data: &[u8]) -> bool {
    if !UEFI_AVAILABLE.load(Ordering::SeqCst) {
        return false;
    }
    UEFI_VARS
        .lock()
        .insert((String::from(name), *guid), data.to_vec());
    true
}

/// UEFI `GetVariable`.
pub fn uefi_get_variable(name: &str, guid: &[u8; 16]) -> Option<Vec<u8>> {
    if !UEFI_AVAILABLE.load(Ordering::SeqCst) {
        return None;
    }
    UEFI_VARS.lock().get(&(String::from(name), *guid)).cloned()
}

fn uefi_self_test() -> bool {
    let guid = [0x11u8; 16];
    let wrote = uefi_set_variable("KnoxGate", &guid, b"live");
    let roundtrip = uefi_get_variable("KnoxGate", &guid).as_deref() == Some(&b"live"[..]);
    let time = uefi_get_time();
    let dt = read_rtc();
    let time_ok = time == Some((dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second));
    if wrote && roundtrip && time_ok {
        crate::serial_println!(
            "[uefi] {} (RTC {}-{:02}-{:02})",
            GATE_UEFI_MARKER,
            dt.year,
            dt.month,
            dt.day
        );
        true
    } else {
        crate::serial_println!(
            "[uefi] GATE_UEFI FAILED: write={} roundtrip={} time={:?}",
            wrote,
            roundtrip,
            time
        );
        false
    }
}

/// Initialize the RTC subsystem
pub fn init() {
    let dt = read_rtc();
    let boot_secs = datetime_to_unix(&dt);
    *BOOT_TIME.lock() = boot_secs;

    crate::serial_println!(
        "[KnoxOS] RTC initialized: {}-{:02}-{:02} {:02}:{:02}:{:02} UTC (epoch: {})",
        dt.year,
        dt.month,
        dt.day,
        dt.hour,
        dt.minute,
        dt.second,
        boot_secs
    );

    UEFI_AVAILABLE.store(true, Ordering::SeqCst);
    let _ = uefi_self_test();
}
