//! Global timezone/locale state and the public subsystem API.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

use super::database::{builtin_locales, builtin_timezones};
use super::locale::Locale;
use super::posix::parse_posix_tz;
use super::types::{LocalDateTime, Timezone};

struct TimezoneState {
    current_tz: Timezone,
    tz_database: BTreeMap<String, String>, // name -> POSIX TZ string
}

struct LocaleState {
    current_locale: Locale,
    available_locales: Vec<Locale>,
}

static TZ_STATE: Mutex<Option<TimezoneState>> = Mutex::new(None);
static LOCALE_STATE: Mutex<Option<LocaleState>> = Mutex::new(None);
static INITIALIZED: AtomicBool = AtomicBool::new(false);

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the timezone and locale subsystem
pub fn init() {
    let mut tz_db = BTreeMap::new();
    for (name, posix_tz) in builtin_timezones() {
        tz_db.insert(String::from(name), String::from(posix_tz));
    }

    let default_tz = Timezone::simple("UTC", "UTC", 0);

    *TZ_STATE.lock() = Some(TimezoneState {
        current_tz: default_tz,
        tz_database: tz_db,
    });

    let available = builtin_locales();
    let default_locale = Locale::c_locale();

    *LOCALE_STATE.lock() = Some(LocaleState {
        current_locale: default_locale,
        available_locales: available,
    });

    INITIALIZED.store(true, Ordering::SeqCst);
    serial_println!("[TZ/Locale] Timezone & locale subsystem initialized (UTC, C locale)");
}

/// True after [`init`].
pub fn is_ready() -> bool {
    INITIALIZED.load(Ordering::SeqCst)
}

/// Parse `+0800`, `+08:00`, or `UTC+8` into minutes east of UTC.
pub fn parse_offset_minutes(spec: &str) -> Option<i32> {
    let s = spec.trim();
    let s = s
        .strip_prefix("UTC")
        .or_else(|| s.strip_prefix("GMT"))
        .unwrap_or(s)
        .trim();
    if s.is_empty() || s == "Z" || s == "0" {
        return Some(0);
    }
    let (sign, rest) = if let Some(r) = s.strip_prefix('+') {
        (1i32, r)
    } else if let Some(r) = s.strip_prefix('-') {
        (-1i32, r)
    } else if s.bytes().all(|b| b.is_ascii_digit() || b == b':') {
        (1i32, s)
    } else {
        return None;
    };
    let rest = rest.trim();
    let (hours, mins) = if rest.len() >= 5 && rest.as_bytes().get(2) == Some(&b':') {
        let h = rest[..2].parse::<i32>().ok()?;
        let m = rest[3..5].parse::<i32>().ok()?;
        (h, m)
    } else if rest.len() >= 4
        && rest.as_bytes().get(2) != Some(&b':')
        && rest.chars().all(|c| c.is_ascii_digit())
    {
        let h = rest[..2].parse::<i32>().ok()?;
        let m = rest[2..4].parse::<i32>().ok()?;
        (h, m)
    } else {
        let h = rest.parse::<i32>().ok()?;
        (h, 0)
    };
    if hours.abs() > 14 || mins >= 60 {
        return None;
    }
    Some(sign * (hours * 60 + mins))
}

fn posix_from_offset_minutes(minutes: i32) -> String {
    let sign = if minutes >= 0 { '-' } else { '+' };
    let abs = minutes.unsigned_abs();
    let h = abs / 60;
    let m = abs % 60;
    if m == 0 {
        format!("UTC{}{}", sign, h)
    } else {
        format!("UTC{}{}:{:02}", sign, h, m)
    }
}

fn offset_to_iana(minutes: i32) -> &'static str {
    match minutes {
        0 => "UTC",
        60 => "Europe/Berlin",
        180 => "Europe/Moscow",
        330 => "Asia/Kolkata",
        480 => "Asia/Shanghai",
        540 => "Asia/Tokyo",
        600 => "Australia/Sydney",
        -300 => "America/New_York",
        -360 => "America/Chicago",
        -420 => "America/Denver",
        -480 => "America/Los_Angeles",
        _ => "UTC",
    }
}

fn apply_spec(spec: &str) -> bool {
    let spec = spec.trim();
    if spec.is_empty() {
        return false;
    }
    if spec.contains('/') {
        if set_timezone(spec) {
            persist_etc_timezone(spec);
            return true;
        }
    }
    if let Some(mins) = parse_offset_minutes(spec) {
        let iana = offset_to_iana(mins);
        if set_timezone(iana) {
            persist_etc_timezone(iana);
            return true;
        }
        let posix = posix_from_offset_minutes(mins);
        if set_timezone(&posix) {
            persist_etc_timezone(&posix);
            return true;
        }
    }
    false
}

fn persist_etc_timezone(name: &str) {
    crate::vfs::ensure_directory("/etc");
    let mut body = String::from(name);
    body.push('\n');
    let _ = crate::vfs::write_file_dispatch("/etc/timezone", body.as_bytes());
}

/// Detect host timezone from fw_cfg, cmdline, or `/etc/timezone` (like a desktop OS).
pub fn detect_host() {
    if !INITIALIZED.load(Ordering::SeqCst) {
        init();
    }
    if let Some(spec) = crate::fw_cfg::read_file("opt/knoxos.tz") {
        if apply_spec(&spec) {
            serial_println!("[TZ] Auto-detected host timezone from firmware: {}", spec);
            return;
        }
    }
    if let Some(spec) = crate::cmdline::get("timezone") {
        if apply_spec(&spec) {
            serial_println!("[TZ] Timezone from cmdline: {}", spec);
            return;
        }
    }
    if let Some(data) = crate::vfs::read_file_dispatch("/etc/timezone") {
        if let Ok(text) = core::str::from_utf8(&data) {
            if apply_spec(text) {
                serial_println!("[TZ] Timezone from /etc/timezone: {}", text.trim());
            }
        }
    }
}

/// Set the system timezone by IANA name (e.g., "America/New_York")
pub fn set_timezone(name: &str) -> bool {
    let mut state = TZ_STATE.lock();
    if let Some(ref mut s) = *state {
        if let Some(posix_tz) = s.tz_database.get(name) {
            if let Some(mut tz) = parse_posix_tz(posix_tz) {
                tz.name = String::from(name);
                serial_println!("[TZ] Set timezone to {} ({})", name, tz.std_abbrev);
                s.current_tz = tz;
                return true;
            }
        }
        // Try direct POSIX TZ string
        if let Some(tz) = parse_posix_tz(name) {
            s.current_tz = tz;
            return true;
        }
    }
    false
}

/// Get the current timezone name
pub fn get_timezone_name() -> String {
    let state = TZ_STATE.lock();
    if let Some(ref s) = *state {
        s.current_tz.name.clone()
    } else {
        String::from("UTC")
    }
}

/// Convert UTC Unix timestamp to local time
pub fn to_local_time(unix_time: i64) -> LocalDateTime {
    let state = TZ_STATE.lock();
    if let Some(ref s) = *state {
        s.current_tz.to_local(unix_time)
    } else {
        LocalDateTime::from_unix(unix_time)
    }
}

/// Get UTC offset in seconds for the current timezone at a given time
pub fn get_utc_offset(unix_time: i64) -> i32 {
    let state = TZ_STATE.lock();
    if let Some(ref s) = *state {
        s.current_tz.utc_offset(unix_time)
    } else {
        0
    }
}

/// List all available timezone names
pub fn list_timezones() -> Vec<String> {
    let state = TZ_STATE.lock();
    if let Some(ref s) = *state {
        s.tz_database.keys().cloned().collect()
    } else {
        Vec::new()
    }
}

/// Set the system locale by name (e.g., "en_US.UTF-8")
pub fn set_locale(name: &str) -> bool {
    let mut state = LOCALE_STATE.lock();
    if let Some(ref mut s) = *state {
        for locale in &s.available_locales {
            if locale.name == name {
                serial_println!("[Locale] Set locale to {}", name);
                s.current_locale = locale.clone();
                return true;
            }
        }
    }
    false
}

/// Get the current locale name
pub fn get_locale_name() -> String {
    let state = LOCALE_STATE.lock();
    if let Some(ref s) = *state {
        s.current_locale.name.clone()
    } else {
        String::from("C")
    }
}

/// Get a clone of the current locale
pub fn get_current_locale() -> Locale {
    let state = LOCALE_STATE.lock();
    if let Some(ref s) = *state {
        s.current_locale.clone()
    } else {
        Locale::c_locale()
    }
}

/// List all available locale names
pub fn list_locales() -> Vec<String> {
    let state = LOCALE_STATE.lock();
    if let Some(ref s) = *state {
        s.available_locales.iter().map(|l| l.name.clone()).collect()
    } else {
        vec![String::from("C")]
    }
}

/// Format a Unix timestamp as a string using the current locale and timezone
pub fn format_datetime(unix_time: i64, fmt: &str) -> String {
    let local = to_local_time(unix_time);
    local.strftime(fmt)
}

/// Format a number using the current locale
pub fn format_number(value: f64) -> String {
    get_current_locale().format_number(value)
}

/// Format a currency value using the current locale
pub fn format_currency(value: f64) -> String {
    get_current_locale().format_currency(value)
}
