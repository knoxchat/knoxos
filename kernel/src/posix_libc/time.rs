// Time functions (time.h / sys/time.h)
use core::sync::atomic::{AtomicU64, Ordering};

/// struct timespec
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

/// struct timeval
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timeval {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

/// struct timezone
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Timezone {
    pub tz_minuteswest: i32,
    pub tz_dsttime: i32,
}

/// struct tm (broken-down time)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Tm {
    pub tm_sec: i32,
    pub tm_min: i32,
    pub tm_hour: i32,
    pub tm_mday: i32,
    pub tm_mon: i32,
    pub tm_year: i32,
    pub tm_wday: i32,
    pub tm_yday: i32,
    pub tm_isdst: i32,
    pub tm_gmtoff: i64,
    pub tm_zone: *const u8,
}

pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_MONOTONIC: i32 = 1;
pub const CLOCK_PROCESS_CPUTIME_ID: i32 = 2;
pub const CLOCK_THREAD_CPUTIME_ID: i32 = 3;
pub const CLOCK_MONOTONIC_RAW: i32 = 4;
pub const CLOCK_REALTIME_COARSE: i32 = 5;
pub const CLOCK_MONOTONIC_COARSE: i32 = 6;
pub const CLOCK_BOOTTIME: i32 = 7;

/// Kernel boot TSC for monotonic time
pub(super) static BOOT_TSC: AtomicU64 = AtomicU64::new(0);

/// clock_gettime — get time from a clock
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clock_gettime(clock_id: i32, tp: *mut Timespec) -> i32 {
    if tp.is_null() {
        return -1;
    }

    let ticks = crate::clock::get_ticks();
    let seconds = ticks / 1000;
    let millis = ticks % 1000;

    match clock_id {
        CLOCK_REALTIME | CLOCK_REALTIME_COARSE => {
            // Use RTC for wall clock time
            let rtc = crate::rtc::read_rtc();
            (*tp).tv_sec = rtc.to_unix_timestamp();
            (*tp).tv_nsec = (millis * 1_000_000) as i64;
        }
        CLOCK_MONOTONIC | CLOCK_MONOTONIC_RAW | CLOCK_MONOTONIC_COARSE | CLOCK_BOOTTIME => {
            (*tp).tv_sec = seconds as i64;
            (*tp).tv_nsec = (millis * 1_000_000) as i64;
        }
        _ => {
            (*tp).tv_sec = seconds as i64;
            (*tp).tv_nsec = (millis * 1_000_000) as i64;
        }
    }

    0
}

/// gettimeofday — get time of day
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gettimeofday(tv: *mut Timeval, tz: *mut Timezone) -> i32 {
    if !tv.is_null() {
        let ticks = crate::clock::get_ticks();
        let rtc = crate::rtc::read_rtc();
        (*tv).tv_sec = rtc.to_unix_timestamp();
        (*tv).tv_usec = ((ticks % 1000) * 1000) as i64;
    }
    if !tz.is_null() {
        (*tz).tz_minuteswest = 0;
        (*tz).tz_dsttime = 0;
    }
    0
}

/// time — get time in seconds
#[unsafe(no_mangle)]
pub unsafe extern "C" fn time(t: *mut i64) -> i64 {
    let rtc = crate::rtc::read_rtc();
    let secs = rtc.to_unix_timestamp();
    if !t.is_null() {
        *t = secs;
    }
    secs
}

/// nanosleep — high-resolution sleep
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nanosleep(req: *const Timespec, rem: *mut Timespec) -> i32 {
    if req.is_null() {
        return -1;
    }
    let total_ms = (*req).tv_sec * 1000 + (*req).tv_nsec / 1_000_000;
    // Yield for approximate time
    let start = crate::clock::get_ticks();
    while (crate::clock::get_ticks() - start) < total_ms as u64 {
        crate::scheduler::yield_now();
    }
    if !rem.is_null() {
        (*rem).tv_sec = 0;
        (*rem).tv_nsec = 0;
    }
    0
}

/// usleep — sleep for microseconds
#[unsafe(no_mangle)]
pub unsafe extern "C" fn usleep(usec: u32) -> i32 {
    let ts = Timespec {
        tv_sec: (usec / 1_000_000) as i64,
        tv_nsec: ((usec % 1_000_000) * 1000) as i64,
    };
    nanosleep(&ts, core::ptr::null_mut())
}

/// sleep — sleep for seconds
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sleep(seconds: u32) -> u32 {
    let ts = Timespec {
        tv_sec: seconds as i64,
        tv_nsec: 0,
    };
    nanosleep(&ts, core::ptr::null_mut());
    0
}
