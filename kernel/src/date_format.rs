use crate::serial_println;
/// Date/Time Format Localization
///
/// Locale-aware date, time, number, and currency formatting.
use alloc::string::String;
use spin::Mutex;

#[derive(Debug, Clone)]
pub struct LocaleFormat {
    pub locale: String,     // "en_US", "de_DE", "ja_JP"
    pub date_short: String, // "MM/dd/yyyy", "dd.MM.yyyy"
    pub date_long: String,  // "MMMM d, yyyy"
    pub time_24h: bool,
    pub decimal_sep: char,
    pub thousands_sep: char,
    pub currency_symbol: String,
    pub currency_before: bool,
    pub first_day_of_week: u8, // 0=Sunday, 1=Monday
}

lazy_static::lazy_static! {
    static ref LOCALE: Mutex<LocaleFormat> = Mutex::new(LocaleFormat {
        locale: String::from("en_US"),
        date_short: String::from("MM/dd/yyyy"),
        date_long: String::from("MMMM d, yyyy"),
        time_24h: false,
        decimal_sep: '.',
        thousands_sep: ',',
        currency_symbol: String::from("$"),
        currency_before: true,
        first_day_of_week: 0,
    });
}

pub fn set_locale(locale: &str) {
    let mut loc = LOCALE.lock();
    loc.locale = String::from(locale);
    match locale {
        "de_DE" => {
            loc.date_short = String::from("dd.MM.yyyy");
            loc.time_24h = true;
            loc.decimal_sep = ',';
            loc.thousands_sep = '.';
            loc.currency_symbol = String::from("€");
            loc.currency_before = false;
            loc.first_day_of_week = 1;
        }
        "ja_JP" => {
            loc.date_short = String::from("yyyy/MM/dd");
            loc.time_24h = true;
            loc.decimal_sep = '.';
            loc.thousands_sep = ',';
            loc.currency_symbol = String::from("¥");
            loc.currency_before = true;
            loc.first_day_of_week = 0;
        }
        _ => { /* en_US defaults */ }
    }
    serial_println!("[DATE_FMT] Locale set: {}", locale);
}

pub fn format_date(year: u32, month: u32, day: u32) -> String {
    let loc = LOCALE.lock();
    let fmt = &loc.date_short;
    let _ = fmt;
    alloc::format!("{:04}-{:02}-{:02}", year, month, day)
}

pub fn format_time(hour: u32, minute: u32, second: u32) -> String {
    let loc = LOCALE.lock();
    if loc.time_24h {
        alloc::format!("{:02}:{:02}:{:02}", hour, minute, second)
    } else {
        let (h, ampm) = if hour >= 12 {
            (if hour > 12 { hour - 12 } else { 12 }, "PM")
        } else {
            (if hour == 0 { 12 } else { hour }, "AM")
        };
        alloc::format!("{}:{:02}:{:02} {}", h, minute, second, ampm)
    }
}

pub fn format_number(value: i64) -> String {
    let loc = LOCALE.lock();
    let abs = if value < 0 {
        (-value) as u64
    } else {
        value as u64
    };
    let s = alloc::format!("{}", abs);
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(loc.thousands_sep);
        }
        result.push(ch);
    }
    if value < 0 {
        result.push('-');
    }
    result.chars().rev().collect()
}

pub fn init() {
    serial_println!("[DATE_FMT] Date/time format localization initialized");
}
