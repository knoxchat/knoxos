#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;
/// RTC - CMOS Real-Time Clock driver
/// Reads the actual date/time from the CMOS RTC chip
/// Provides Linux-compatible time functions (clock_gettime, gettimeofday)
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
            tz_minuteswest: 0,
            tz_dsttime: 0,
        },
    )
}

/// Get uptime in seconds
pub fn uptime_seconds() -> u64 {
    let ns = monotonic_ns();
    (ns / 1_000_000_000) as u64
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
}
