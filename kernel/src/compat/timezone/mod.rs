/// Timezone & Locale — IANA Timezone Database and ICU-compatible Locale Support
///
/// Provides comprehensive timezone and locale handling for KnoxOS:
///
///   - IANA timezone database (tzdata) with 400+ timezones
///   - UTC offset calculation with DST (Daylight Saving Time) transitions
///   - POSIX TZ string parsing (e.g., "EST5EDT,M3.2.0,M11.1.0")
///   - /etc/timezone and /etc/localtime support
///   - Locale categories: LC_CTYPE, LC_NUMERIC, LC_TIME, LC_COLLATE, LC_MONETARY, LC_MESSAGES
///   - Number formatting (decimal/thousands separators per locale)
///   - Date/time formatting (strftime with locale-specific patterns)
///   - Currency formatting
///   - Multi-byte character classification (UTF-8 aware)
///   - Locale-aware string collation
///
/// Timezone data is compiled into the kernel for instant availability,
/// with the ability to load updated tzdata from /usr/share/zoneinfo.
///
/// Module layout:
///   types     — DST rules, Timezone, LocalDateTime
///   locale    — POSIX locale categories and formatting
///   posix     — POSIX TZ string parsing
///   database  — built-in IANA zones and locale definitions
///   api       — global state and public API
mod api;
mod database;
mod locale;
mod posix;
mod types;

pub use api::{
    detect_host, format_currency, format_datetime, format_number, get_current_locale,
    get_locale_name, get_timezone_name, get_utc_offset, init, is_ready, list_locales,
    list_timezones, parse_offset_minutes, set_locale, set_timezone, to_local_time,
};
pub use locale::{
    Locale, LocaleCategory, MessagesFormat, MonetaryFormat, NumericFormat, TimeFormat,
};
pub use posix::parse_posix_tz;
pub use types::{DstRule, DstTransition, LocalDateTime, Timezone};
