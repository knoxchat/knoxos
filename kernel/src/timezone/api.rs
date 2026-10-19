//! Global timezone/locale state and the public subsystem API.

use alloc::collections::BTreeMap;
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

/// Set the system timezone by IANA name (e.g., "America/New_York")
pub fn set_timezone(name: &str) -> bool {
    let mut state = TZ_STATE.lock();
    if let Some(ref mut s) = *state {
        if let Some(posix_tz) = s.tz_database.get(name) {
            if let Some(tz) = parse_posix_tz(posix_tz) {
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
