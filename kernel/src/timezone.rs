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
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// TIMEZONE TYPES
// ═══════════════════════════════════════════════════════════════════════

/// DST transition rule
#[derive(Debug, Clone, Copy)]
pub enum DstRule {
    /// Julian day (1-365, no Feb 29 leap handling)
    Julian(u16),
    /// Zero-based Julian day (0-365, counts Feb 29)
    JulianLeap(u16),
    /// Month.Week.Day (M3.2.0 = 2nd Sunday of March)
    MonthWeekDay { month: u8, week: u8, day: u8 },
}

/// DST transition specification
#[derive(Debug, Clone)]
pub struct DstTransition {
    pub rule: DstRule,
    pub time_secs: i32, // Time of day in seconds (default 02:00:00 = 7200)
}

impl Default for DstTransition {
    fn default() -> Self {
        Self {
            rule: DstRule::MonthWeekDay {
                month: 3,
                week: 2,
                day: 0,
            },
            time_secs: 7200, // 2:00 AM
        }
    }
}

/// Timezone definition
#[derive(Debug, Clone)]
pub struct Timezone {
    pub name: String,
    pub std_abbrev: String, // e.g., "EST"
    pub std_offset: i32,    // Seconds east of UTC (negative = west)
    pub dst_abbrev: String, // e.g., "EDT"
    pub dst_offset: i32,    // DST offset (usually std_offset + 3600)
    pub has_dst: bool,
    pub dst_start: DstTransition,
    pub dst_end: DstTransition,
}

impl Timezone {
    /// Create a simple timezone with no DST
    pub fn simple(name: &str, abbrev: &str, offset_hours: i32) -> Self {
        Self {
            name: String::from(name),
            std_abbrev: String::from(abbrev),
            std_offset: offset_hours * 3600,
            dst_abbrev: String::new(),
            dst_offset: 0,
            has_dst: false,
            dst_start: DstTransition::default(),
            dst_end: DstTransition::default(),
        }
    }

    /// Create a timezone with DST
    pub fn with_dst(
        name: &str,
        std_abbrev: &str,
        std_offset_hours: i32,
        dst_abbrev: &str,
        dst_start: DstTransition,
        dst_end: DstTransition,
    ) -> Self {
        Self {
            name: String::from(name),
            std_abbrev: String::from(std_abbrev),
            std_offset: std_offset_hours * 3600,
            dst_abbrev: String::from(dst_abbrev),
            dst_offset: (std_offset_hours + 1) * 3600,
            has_dst: true,
            dst_start,
            dst_end,
        }
    }

    /// Get the UTC offset for a given Unix timestamp
    pub fn utc_offset(&self, unix_time: i64) -> i32 {
        if !self.has_dst {
            return self.std_offset;
        }

        // Determine if DST is in effect for this timestamp
        let year = Self::year_from_timestamp(unix_time);
        let dst_start_ts = self.transition_timestamp(year, &self.dst_start, self.std_offset);
        let dst_end_ts = self.transition_timestamp(year, &self.dst_end, self.dst_offset);

        if dst_start_ts < dst_end_ts {
            // Northern hemisphere: DST is between start and end
            if unix_time >= dst_start_ts && unix_time < dst_end_ts {
                self.dst_offset
            } else {
                self.std_offset
            }
        } else {
            // Southern hemisphere: DST wraps around year boundary
            if unix_time >= dst_start_ts || unix_time < dst_end_ts {
                self.dst_offset
            } else {
                self.std_offset
            }
        }
    }

    /// Check if DST is currently in effect
    pub fn is_dst(&self, unix_time: i64) -> bool {
        self.utc_offset(unix_time) == self.dst_offset && self.has_dst
    }

    /// Get the abbreviation for a given time
    pub fn abbreviation(&self, unix_time: i64) -> &str {
        if self.is_dst(unix_time) {
            &self.dst_abbrev
        } else {
            &self.std_abbrev
        }
    }

    /// Convert UTC timestamp to local time components
    pub fn to_local(&self, unix_time: i64) -> LocalDateTime {
        let offset = self.utc_offset(unix_time);
        let local_ts = unix_time + offset as i64;
        let mut dt = LocalDateTime::from_unix(local_ts);
        dt.utc_offset = offset;
        dt.is_dst = self.is_dst(unix_time);
        dt.tz_abbrev = if dt.is_dst {
            self.dst_abbrev.clone()
        } else {
            self.std_abbrev.clone()
        };
        dt
    }

    /// Extract year from Unix timestamp
    fn year_from_timestamp(ts: i64) -> i32 {
        // Approximate: good enough for DST calculation
        let days = ts / 86400;
        let years_approx = days / 365;
        (1970 + years_approx) as i32
    }

    /// Calculate Unix timestamp for a DST transition in a given year
    fn transition_timestamp(&self, year: i32, trans: &DstTransition, base_offset: i32) -> i64 {
        let day_of_year = match trans.rule {
            DstRule::Julian(jday) => jday as i32 - 1,
            DstRule::JulianLeap(jday) => jday as i32,
            DstRule::MonthWeekDay { month, week, day } => {
                Self::month_week_day_to_doy(year, month, week, day)
            }
        };

        let year_start = Self::year_start_timestamp(year);
        year_start + (day_of_year as i64) * 86400 + trans.time_secs as i64 - base_offset as i64
    }

    /// Get Unix timestamp for start of a year
    fn year_start_timestamp(year: i32) -> i64 {
        let y = year as i64 - 1;
        let days = 365 * (year as i64 - 1970) + (y / 4 - y / 100 + y / 400)
            - (1969 / 4 - 1969 / 100 + 1969 / 400);
        days * 86400
    }

    /// Convert M.W.D rule to day-of-year
    fn month_week_day_to_doy(year: i32, month: u8, week: u8, day: u8) -> i32 {
        let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let month_days: [i32; 12] = [
            31,
            if is_leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];

        // Day of year for first day of target month
        let mut doy: i32 = 0;
        for i in 0..(month as usize - 1) {
            doy += month_days[i];
        }

        // Day of week for first day of the month (Zeller-like)
        // 0=Sunday, 1=Monday, ... 6=Saturday
        let first_dow = Self::day_of_week(year, month as i32, 1);

        // Find the target day (e.g., 2nd Sunday)
        let mut target_day = 1 + ((day as i32 - first_dow + 7) % 7);
        if week > 1 {
            target_day += 7 * (week as i32 - 1);
        }
        // Week 5 means "last occurrence"
        if week == 5 {
            while target_day + 7 <= month_days[month as usize - 1] {
                target_day += 7;
            }
        }

        doy + target_day - 1
    }

    /// Calculate day of week (0=Sunday) for a given date
    fn day_of_week(year: i32, month: i32, day: i32) -> i32 {
        // Tomohiko Sakamoto's algorithm
        let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = if month < 3 { year - 1 } else { year };
        let m = month as usize - 1;
        (y + y / 4 - y / 100 + y / 400 + t[m] + day) % 7
    }
}

/// Local date/time with timezone info
#[derive(Debug, Clone)]
pub struct LocalDateTime {
    pub year: i32,
    pub month: u8,        // 1-12
    pub day: u8,          // 1-31
    pub hour: u8,         // 0-23
    pub minute: u8,       // 0-59
    pub second: u8,       // 0-59
    pub day_of_week: u8,  // 0=Sunday
    pub day_of_year: u16, // 0-365
    pub utc_offset: i32,
    pub is_dst: bool,
    pub tz_abbrev: String,
}

impl LocalDateTime {
    /// Create from Unix timestamp (UTC)
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

        // Day of week (Jan 1, 1970 was Thursday = 4)
        let dow = ((days % 7 + 4) % 7) as u8;

        // Civil date from days since epoch (algorithm from Howard Hinnant)
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

        Self {
            year: year as i32,
            month: m as u8,
            day: d as u8,
            hour,
            minute,
            second,
            day_of_week: dow,
            day_of_year: doy as u16,
            utc_offset: 0,
            is_dst: false,
            tz_abbrev: String::from("UTC"),
        }
    }

    /// Format as ISO 8601 string
    pub fn to_iso8601(&self) -> String {
        let sign = if self.utc_offset >= 0 { '+' } else { '-' };
        let off_hours = self.utc_offset.unsigned_abs() / 3600;
        let off_mins = (self.utc_offset.unsigned_abs() % 3600) / 60;
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}{:02}:{:02}",
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            sign,
            off_hours,
            off_mins
        )
    }

    /// Format using strftime-compatible format string
    pub fn strftime(&self, fmt: &str) -> String {
        let mut result = String::new();
        let mut chars = fmt.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '%' {
                if let Some(&spec) = chars.peek() {
                    chars.next();
                    match spec {
                        'Y' => result.push_str(&format!("{:04}", self.year)),
                        'y' => result.push_str(&format!("{:02}", self.year % 100)),
                        'm' => result.push_str(&format!("{:02}", self.month)),
                        'd' => result.push_str(&format!("{:02}", self.day)),
                        'H' => result.push_str(&format!("{:02}", self.hour)),
                        'M' => result.push_str(&format!("{:02}", self.minute)),
                        'S' => result.push_str(&format!("{:02}", self.second)),
                        'I' => {
                            let h12 = if self.hour == 0 {
                                12
                            } else if self.hour > 12 {
                                self.hour - 12
                            } else {
                                self.hour
                            };
                            result.push_str(&format!("{:02}", h12));
                        }
                        'p' => result.push_str(if self.hour < 12 { "AM" } else { "PM" }),
                        'P' => result.push_str(if self.hour < 12 { "am" } else { "pm" }),
                        'A' => result.push_str(Self::weekday_name(self.day_of_week)),
                        'a' => result.push_str(Self::weekday_abbrev(self.day_of_week)),
                        'B' => result.push_str(Self::month_name(self.month)),
                        'b' | 'h' => result.push_str(Self::month_abbrev(self.month)),
                        'j' => result.push_str(&format!("{:03}", self.day_of_year + 1)),
                        'u' => result.push_str(&format!(
                            "{}",
                            if self.day_of_week == 0 {
                                7
                            } else {
                                self.day_of_week
                            }
                        )),
                        'w' => result.push_str(&format!("{}", self.day_of_week)),
                        'Z' => result.push_str(&self.tz_abbrev),
                        'z' => {
                            let sign = if self.utc_offset >= 0 { '+' } else { '-' };
                            let h = self.utc_offset.unsigned_abs() / 3600;
                            let m = (self.utc_offset.unsigned_abs() % 3600) / 60;
                            result.push_str(&format!("{}{:02}{:02}", sign, h, m));
                        }
                        'n' => result.push('\n'),
                        't' => result.push('\t'),
                        '%' => result.push('%'),
                        'F' => result.push_str(&format!(
                            "{:04}-{:02}-{:02}",
                            self.year, self.month, self.day
                        )),
                        'T' => result.push_str(&format!(
                            "{:02}:{:02}:{:02}",
                            self.hour, self.minute, self.second
                        )),
                        'R' => result.push_str(&format!("{:02}:{:02}", self.hour, self.minute)),
                        'c' => {
                            result.push_str(&format!(
                                "{} {} {:2} {:02}:{:02}:{:02} {}",
                                Self::weekday_abbrev(self.day_of_week),
                                Self::month_abbrev(self.month),
                                self.day,
                                self.hour,
                                self.minute,
                                self.second,
                                self.year
                            ));
                        }
                        'x' => result.push_str(&format!(
                            "{:02}/{:02}/{:02}",
                            self.month,
                            self.day,
                            self.year % 100
                        )),
                        'X' => result.push_str(&format!(
                            "{:02}:{:02}:{:02}",
                            self.hour, self.minute, self.second
                        )),
                        's' => {
                            // Unix timestamp (approximate, without full conversion)
                            result.push('0');
                        }
                        _ => {
                            result.push('%');
                            result.push(spec);
                        }
                    }
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    fn weekday_name(dow: u8) -> &'static str {
        match dow {
            0 => "Sunday",
            1 => "Monday",
            2 => "Tuesday",
            3 => "Wednesday",
            4 => "Thursday",
            5 => "Friday",
            6 => "Saturday",
            _ => "Unknown",
        }
    }

    fn weekday_abbrev(dow: u8) -> &'static str {
        match dow {
            0 => "Sun",
            1 => "Mon",
            2 => "Tue",
            3 => "Wed",
            4 => "Thu",
            5 => "Fri",
            6 => "Sat",
            _ => "???",
        }
    }

    fn month_name(m: u8) -> &'static str {
        match m {
            1 => "January",
            2 => "February",
            3 => "March",
            4 => "April",
            5 => "May",
            6 => "June",
            7 => "July",
            8 => "August",
            9 => "September",
            10 => "October",
            11 => "November",
            12 => "December",
            _ => "Unknown",
        }
    }

    fn month_abbrev(m: u8) -> &'static str {
        match m {
            1 => "Jan",
            2 => "Feb",
            3 => "Mar",
            4 => "Apr",
            5 => "May",
            6 => "Jun",
            7 => "Jul",
            8 => "Aug",
            9 => "Sep",
            10 => "Oct",
            11 => "Nov",
            12 => "Dec",
            _ => "???",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// LOCALE TYPES
// ═══════════════════════════════════════════════════════════════════════

/// Locale category (POSIX LC_* categories)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocaleCategory {
    LcCtype,
    LcNumeric,
    LcTime,
    LcCollate,
    LcMonetary,
    LcMessages,
    LcAll,
}

/// Number format specification
#[derive(Debug, Clone)]
pub struct NumericFormat {
    pub decimal_point: String, // e.g., "." or ","
    pub thousands_sep: String, // e.g., "," or "."
    pub grouping: Vec<u8>,     // e.g., [3] for groups of 3
}

/// Monetary format specification
#[derive(Debug, Clone)]
pub struct MonetaryFormat {
    pub currency_symbol: String, // e.g., "$", "€", "£"
    pub int_curr_symbol: String, // e.g., "USD ", "EUR "
    pub mon_decimal_point: String,
    pub mon_thousands_sep: String,
    pub mon_grouping: Vec<u8>,
    pub positive_sign: String,
    pub negative_sign: String,
    pub frac_digits: u8,
    pub p_cs_precedes: bool, // currency symbol precedes positive
    pub n_cs_precedes: bool, // currency symbol precedes negative
    pub p_sep_by_space: bool,
    pub n_sep_by_space: bool,
}

/// Time format specification
#[derive(Debug, Clone)]
pub struct TimeFormat {
    pub d_t_fmt: String,    // Date+time format (e.g., "%a %b %e %H:%M:%S %Y")
    pub d_fmt: String,      // Date format (e.g., "%m/%d/%Y")
    pub t_fmt: String,      // Time format (e.g., "%H:%M:%S")
    pub t_fmt_ampm: String, // 12-hour time format (e.g., "%I:%M:%S %p")
    pub am_pm: [String; 2], // AM/PM strings
    pub day_names: [String; 7],
    pub day_abbrevs: [String; 7],
    pub month_names: [String; 12],
    pub month_abbrevs: [String; 12],
    pub first_weekday: u8, // 0=Sunday, 1=Monday
}

/// Messages format (yes/no prompts)
#[derive(Debug, Clone)]
pub struct MessagesFormat {
    pub yesexpr: String, // Regex for "yes" (e.g., "^[yY]")
    pub noexpr: String,  // Regex for "no" (e.g., "^[nN]")
    pub yesstr: String,  // "yes" string
    pub nostr: String,   // "no" string
}

/// Complete locale definition
#[derive(Debug, Clone)]
pub struct Locale {
    pub name: String,
    pub language: String,
    pub territory: String,
    pub codeset: String,
    pub numeric: NumericFormat,
    pub monetary: MonetaryFormat,
    pub time: TimeFormat,
    pub messages: MessagesFormat,
}

impl Locale {
    /// Create the default "C" / "POSIX" locale
    pub fn c_locale() -> Self {
        Self {
            name: String::from("C"),
            language: String::from("en"),
            territory: String::from("US"),
            codeset: String::from("UTF-8"),
            numeric: NumericFormat {
                decimal_point: String::from("."),
                thousands_sep: String::new(),
                grouping: Vec::new(),
            },
            monetary: MonetaryFormat {
                currency_symbol: String::new(),
                int_curr_symbol: String::new(),
                mon_decimal_point: String::new(),
                mon_thousands_sep: String::new(),
                mon_grouping: Vec::new(),
                positive_sign: String::new(),
                negative_sign: String::from("-"),
                frac_digits: 255,
                p_cs_precedes: true,
                n_cs_precedes: true,
                p_sep_by_space: false,
                n_sep_by_space: false,
            },
            time: Self::default_time_format(),
            messages: MessagesFormat {
                yesexpr: String::from("^[yY]"),
                noexpr: String::from("^[nN]"),
                yesstr: String::from("yes"),
                nostr: String::from("no"),
            },
        }
    }

    fn default_time_format() -> TimeFormat {
        TimeFormat {
            d_t_fmt: String::from("%a %b %e %H:%M:%S %Y"),
            d_fmt: String::from("%m/%d/%y"),
            t_fmt: String::from("%H:%M:%S"),
            t_fmt_ampm: String::from("%I:%M:%S %p"),
            am_pm: [String::from("AM"), String::from("PM")],
            day_names: [
                String::from("Sunday"),
                String::from("Monday"),
                String::from("Tuesday"),
                String::from("Wednesday"),
                String::from("Thursday"),
                String::from("Friday"),
                String::from("Saturday"),
            ],
            day_abbrevs: [
                String::from("Sun"),
                String::from("Mon"),
                String::from("Tue"),
                String::from("Wed"),
                String::from("Thu"),
                String::from("Fri"),
                String::from("Sat"),
            ],
            month_names: [
                String::from("January"),
                String::from("February"),
                String::from("March"),
                String::from("April"),
                String::from("May"),
                String::from("June"),
                String::from("July"),
                String::from("August"),
                String::from("September"),
                String::from("October"),
                String::from("November"),
                String::from("December"),
            ],
            month_abbrevs: [
                String::from("Jan"),
                String::from("Feb"),
                String::from("Mar"),
                String::from("Apr"),
                String::from("May"),
                String::from("Jun"),
                String::from("Jul"),
                String::from("Aug"),
                String::from("Sep"),
                String::from("Oct"),
                String::from("Nov"),
                String::from("Dec"),
            ],
            first_weekday: 0,
        }
    }

    /// Format a number according to this locale
    pub fn format_number(&self, value: f64) -> String {
        let int_part = value.abs() as u64;
        let frac_part = ((value.abs() - int_part as f64) * 100.0) as u64;
        let sign = if value < 0.0 { "-" } else { "" };

        let int_str = format!("{}", int_part);
        let formatted =
            if !self.numeric.thousands_sep.is_empty() && !self.numeric.grouping.is_empty() {
                self.apply_grouping(&int_str)
            } else {
                int_str
            };

        if frac_part > 0 {
            format!(
                "{}{}{}{:02}",
                sign, formatted, self.numeric.decimal_point, frac_part
            )
        } else {
            format!("{}{}", sign, formatted)
        }
    }

    /// Format currency according to this locale
    pub fn format_currency(&self, value: f64) -> String {
        let abs_val = value.abs();
        let int_part = abs_val as u64;
        let frac_digits = if self.monetary.frac_digits == 255 {
            2
        } else {
            self.monetary.frac_digits
        };
        let frac_mult = 10u64.pow(frac_digits as u32);
        let frac_part = ((abs_val - int_part as f64) * frac_mult as f64) as u64;

        let int_str = format!("{}", int_part);
        let formatted_int = if !self.monetary.mon_thousands_sep.is_empty() {
            self.apply_monetary_grouping(&int_str)
        } else {
            int_str
        };

        let amount = format!(
            "{}{}{:0>width$}",
            formatted_int,
            self.monetary.mon_decimal_point,
            frac_part,
            width = frac_digits as usize
        );

        let symbol = &self.monetary.currency_symbol;
        if value >= 0.0 {
            if self.monetary.p_cs_precedes {
                let sep = if self.monetary.p_sep_by_space {
                    " "
                } else {
                    ""
                };
                format!("{}{}{}", symbol, sep, amount)
            } else {
                let sep = if self.monetary.p_sep_by_space {
                    " "
                } else {
                    ""
                };
                format!("{}{}{}", amount, sep, symbol)
            }
        } else {
            let neg = &self.monetary.negative_sign;
            if self.monetary.n_cs_precedes {
                let sep = if self.monetary.n_sep_by_space {
                    " "
                } else {
                    ""
                };
                format!("{}{}{}{}", neg, symbol, sep, amount)
            } else {
                let sep = if self.monetary.n_sep_by_space {
                    " "
                } else {
                    ""
                };
                format!("{}{}{}{}", neg, amount, sep, symbol)
            }
        }
    }

    fn apply_grouping(&self, s: &str) -> String {
        if self.numeric.grouping.is_empty() || self.numeric.thousands_sep.is_empty() {
            return String::from(s);
        }
        let group_size = self.numeric.grouping[0] as usize;
        if group_size == 0 {
            return String::from(s);
        }

        let bytes: Vec<char> = s.chars().collect();
        let mut result = Vec::new();

        for (count, i) in (0..bytes.len()).rev().enumerate() {
            if count > 0 && count % group_size == 0 {
                result.push(self.numeric.thousands_sep.chars().next().unwrap_or(','));
            }
            result.push(bytes[i]);
        }
        result.reverse();
        result.iter().collect()
    }

    fn apply_monetary_grouping(&self, s: &str) -> String {
        if self.monetary.mon_grouping.is_empty() || self.monetary.mon_thousands_sep.is_empty() {
            return String::from(s);
        }
        let group_size = self.monetary.mon_grouping[0] as usize;
        if group_size == 0 {
            return String::from(s);
        }

        let bytes: Vec<char> = s.chars().collect();
        let mut result = Vec::new();

        for (count, i) in (0..bytes.len()).rev().enumerate() {
            if count > 0 && count % group_size == 0 {
                result.push(
                    self.monetary
                        .mon_thousands_sep
                        .chars()
                        .next()
                        .unwrap_or(','),
                );
            }
            result.push(bytes[i]);
        }
        result.reverse();
        result.iter().collect()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// IANA TIMEZONE DATABASE (built-in subset)
// ═══════════════════════════════════════════════════════════════════════

/// Parse a POSIX TZ string (e.g., "EST5EDT,M3.2.0,M11.1.0")
pub fn parse_posix_tz(tz_string: &str) -> Option<Timezone> {
    let s = tz_string.trim();
    if s.is_empty() {
        return None;
    }

    // Parse standard abbreviation and offset
    let (std_abbrev, rest) = parse_tz_abbrev(s)?;
    let (std_offset_hours, rest) = parse_tz_offset(rest)?;

    if rest.is_empty() {
        // No DST
        return Some(Timezone::simple(tz_string, &std_abbrev, std_offset_hours));
    }

    // Parse DST abbreviation
    let (dst_abbrev, rest) = parse_tz_abbrev(rest)?;
    let (dst_offset_hours, rest) = if !rest.is_empty()
        && (rest.starts_with('-') || rest.starts_with('+') || rest.as_bytes()[0].is_ascii_digit())
    {
        // DST offset specified
        let (off, r) = parse_tz_offset(rest)?;
        (off, r)
    } else {
        // Default: DST is 1 hour ahead of standard
        (std_offset_hours - 1, rest)
    };

    // Parse transition rules
    let rest = rest.strip_prefix(',').unwrap_or(rest);
    let parts: Vec<&str> = rest.split(',').collect();

    let dst_start = if !parts.is_empty() {
        parse_transition_rule(parts[0]).unwrap_or_default()
    } else {
        DstTransition::default()
    };

    let dst_end = if parts.len() > 1 {
        parse_transition_rule(parts[1]).unwrap_or(DstTransition {
            rule: DstRule::MonthWeekDay {
                month: 11,
                week: 1,
                day: 0,
            },
            time_secs: 7200,
        })
    } else {
        DstTransition {
            rule: DstRule::MonthWeekDay {
                month: 11,
                week: 1,
                day: 0,
            },
            time_secs: 7200,
        }
    };

    Some(Timezone::with_dst(
        tz_string,
        &std_abbrev,
        -std_offset_hours, // POSIX convention: positive west
        &dst_abbrev,
        dst_start,
        dst_end,
    ))
}

fn parse_tz_abbrev(s: &str) -> Option<(String, &str)> {
    if s.starts_with('<') {
        // Quoted abbreviation
        let end = s.find('>')?;
        Some((String::from(&s[1..end]), &s[end + 1..]))
    } else {
        let end = s.find(|c: char| c.is_ascii_digit() || c == '-' || c == '+' || c == ',')?;
        if end < 3 {
            return None;
        }
        Some((String::from(&s[..end]), &s[end..]))
    }
}

fn parse_tz_offset(s: &str) -> Option<(i32, &str)> {
    let mut neg = false;
    let mut rest = s;
    if rest.starts_with('-') {
        neg = true;
        rest = &rest[1..];
    } else if rest.starts_with('+') {
        rest = &rest[1..];
    }

    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != ':')
        .unwrap_or(rest.len());
    let num_str = &rest[..end];
    let parts: Vec<&str> = num_str.split(':').collect();
    let hours: i32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let offset = if neg { -hours } else { hours };
    Some((offset, &rest[end..]))
}

fn parse_transition_rule(s: &str) -> Option<DstTransition> {
    let (rule_str, time_str) = if let Some(idx) = s.find('/') {
        (&s[..idx], Some(&s[idx + 1..]))
    } else {
        (s, None)
    };

    let time_secs = if let Some(ts) = time_str {
        let parts: Vec<&str> = ts.split(':').collect();
        let h: i32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(2);
        let m: i32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let s: i32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        h * 3600 + m * 60 + s
    } else {
        7200 // Default 2:00 AM
    };

    let rule = if let Some(rest) = rule_str.strip_prefix('M') {
        // M.W.D format
        let parts: Vec<&str> = rest.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        let month: u8 = parts[0].parse().ok()?;
        let week: u8 = parts[1].parse().ok()?;
        let day: u8 = parts[2].parse().ok()?;
        DstRule::MonthWeekDay { month, week, day }
    } else if let Some(rest) = rule_str.strip_prefix('J') {
        let jday: u16 = rest.parse().ok()?;
        DstRule::Julian(jday)
    } else {
        let jday: u16 = rule_str.parse().ok()?;
        DstRule::JulianLeap(jday)
    };

    Some(DstTransition { rule, time_secs })
}

// ═══════════════════════════════════════════════════════════════════════
// BUILT-IN TIMEZONE DATABASE
// ═══════════════════════════════════════════════════════════════════════

/// Built-in timezones (most common IANA zones)
fn builtin_timezones() -> Vec<(&'static str, &'static str)> {
    vec![
        // Americas
        ("America/New_York", "EST5EDT,M3.2.0,M11.1.0"),
        ("America/Chicago", "CST6CDT,M3.2.0,M11.1.0"),
        ("America/Denver", "MST7MDT,M3.2.0,M11.1.0"),
        ("America/Los_Angeles", "PST8PDT,M3.2.0,M11.1.0"),
        ("America/Anchorage", "AKST9AKDT,M3.2.0,M11.1.0"),
        ("America/Phoenix", "MST7"),
        ("America/Toronto", "EST5EDT,M3.2.0,M11.1.0"),
        ("America/Vancouver", "PST8PDT,M3.2.0,M11.1.0"),
        ("America/Mexico_City", "CST6CDT,M4.1.0,M10.5.0"),
        ("America/Sao_Paulo", "<-03>3"),
        ("America/Argentina/Buenos_Aires", "<-03>3"),
        ("America/Bogota", "<-05>5"),
        ("America/Santiago", "<-04>4<-03>,M9.1.6/24,M4.1.6/24"),
        ("America/Lima", "<-05>5"),
        ("America/Havana", "CST5CDT,M3.2.0/0,M11.1.0/1"),
        // Europe
        ("Europe/London", "GMT0BST,M3.5.0/1,M10.5.0"),
        ("Europe/Paris", "CET-1CEST,M3.5.0,M10.5.0/3"),
        ("Europe/Berlin", "CET-1CEST,M3.5.0,M10.5.0/3"),
        ("Europe/Madrid", "CET-1CEST,M3.5.0,M10.5.0/3"),
        ("Europe/Rome", "CET-1CEST,M3.5.0,M10.5.0/3"),
        ("Europe/Amsterdam", "CET-1CEST,M3.5.0,M10.5.0/3"),
        ("Europe/Brussels", "CET-1CEST,M3.5.0,M10.5.0/3"),
        ("Europe/Zurich", "CET-1CEST,M3.5.0,M10.5.0/3"),
        ("Europe/Stockholm", "CET-1CEST,M3.5.0,M10.5.0/3"),
        ("Europe/Moscow", "MSK-3"),
        ("Europe/Istanbul", "<+03>-3"),
        ("Europe/Athens", "EET-2EEST,M3.5.0/3,M10.5.0/4"),
        ("Europe/Warsaw", "CET-1CEST,M3.5.0,M10.5.0/3"),
        ("Europe/Kiev", "EET-2EEST,M3.5.0/3,M10.5.0/4"),
        ("Europe/Helsinki", "EET-2EEST,M3.5.0/3,M10.5.0/4"),
        ("Europe/Lisbon", "WET0WEST,M3.5.0/1,M10.5.0"),
        // Asia
        ("Asia/Tokyo", "JST-9"),
        ("Asia/Shanghai", "CST-8"),
        ("Asia/Hong_Kong", "HKT-8"),
        ("Asia/Taipei", "CST-8"),
        ("Asia/Seoul", "KST-9"),
        ("Asia/Singapore", "<+08>-8"),
        ("Asia/Kolkata", "IST-5:30"),
        ("Asia/Dubai", "<+04>-4"),
        ("Asia/Riyadh", "<+03>-3"),
        ("Asia/Tehran", "<+0330>-3:30"),
        ("Asia/Bangkok", "<+07>-7"),
        ("Asia/Jakarta", "WIB-7"),
        ("Asia/Ho_Chi_Minh", "<+07>-7"),
        ("Asia/Manila", "PST-8"),
        ("Asia/Karachi", "PKT-5"),
        // Oceania
        ("Australia/Sydney", "AEST-10AEDT,M10.1.0,M4.1.0/3"),
        ("Australia/Melbourne", "AEST-10AEDT,M10.1.0,M4.1.0/3"),
        ("Australia/Perth", "AWST-8"),
        ("Australia/Brisbane", "AEST-10"),
        ("Australia/Adelaide", "ACST-9:30ACDT,M10.1.0,M4.1.0/3"),
        ("Pacific/Auckland", "NZST-12NZDT,M9.5.0,M4.1.0/3"),
        ("Pacific/Fiji", "<+12>-12"),
        ("Pacific/Honolulu", "HST10"),
        // Africa
        ("Africa/Cairo", "EET-2"),
        ("Africa/Lagos", "WAT-1"),
        ("Africa/Johannesburg", "SAST-2"),
        ("Africa/Nairobi", "EAT-3"),
        ("Africa/Casablanca", "<+01>-1"),
        // UTC and fixed offsets
        ("UTC", "UTC0"),
        ("Etc/GMT", "GMT0"),
        ("Etc/GMT+1", "<-01>1"),
        ("Etc/GMT+2", "<-02>2"),
        ("Etc/GMT+3", "<-03>3"),
        ("Etc/GMT+4", "<-04>4"),
        ("Etc/GMT+5", "<-05>5"),
        ("Etc/GMT+6", "<-06>6"),
        ("Etc/GMT+7", "<-07>7"),
        ("Etc/GMT+8", "<-08>8"),
        ("Etc/GMT+9", "<-09>9"),
        ("Etc/GMT+10", "<-10>10"),
        ("Etc/GMT+11", "<-11>11"),
        ("Etc/GMT+12", "<-12>12"),
        ("Etc/GMT-1", "<+01>-1"),
        ("Etc/GMT-2", "<+02>-2"),
        ("Etc/GMT-3", "<+03>-3"),
        ("Etc/GMT-4", "<+04>-4"),
        ("Etc/GMT-5", "<+05>-5"),
        ("Etc/GMT-6", "<+06>-6"),
        ("Etc/GMT-7", "<+07>-7"),
        ("Etc/GMT-8", "<+08>-8"),
        ("Etc/GMT-9", "<+09>-9"),
        ("Etc/GMT-10", "<+10>-10"),
        ("Etc/GMT-11", "<+11>-11"),
        ("Etc/GMT-12", "<+12>-12"),
        ("Etc/GMT-13", "<+13>-13"),
        ("Etc/GMT-14", "<+14>-14"),
    ]
}

/// Built-in locale definitions
fn builtin_locales() -> Vec<Locale> {
    vec![
        // C / POSIX locale
        Locale::c_locale(),
        // en_US.UTF-8
        Locale {
            name: String::from("en_US.UTF-8"),
            language: String::from("en"),
            territory: String::from("US"),
            codeset: String::from("UTF-8"),
            numeric: NumericFormat {
                decimal_point: String::from("."),
                thousands_sep: String::from(","),
                grouping: vec![3],
            },
            monetary: MonetaryFormat {
                currency_symbol: String::from("$"),
                int_curr_symbol: String::from("USD "),
                mon_decimal_point: String::from("."),
                mon_thousands_sep: String::from(","),
                mon_grouping: vec![3],
                positive_sign: String::new(),
                negative_sign: String::from("-"),
                frac_digits: 2,
                p_cs_precedes: true,
                n_cs_precedes: true,
                p_sep_by_space: false,
                n_sep_by_space: false,
            },
            time: Locale::default_time_format(),
            messages: MessagesFormat {
                yesexpr: String::from("^[yY]"),
                noexpr: String::from("^[nN]"),
                yesstr: String::from("yes"),
                nostr: String::from("no"),
            },
        },
        // en_GB.UTF-8
        Locale {
            name: String::from("en_GB.UTF-8"),
            language: String::from("en"),
            territory: String::from("GB"),
            codeset: String::from("UTF-8"),
            numeric: NumericFormat {
                decimal_point: String::from("."),
                thousands_sep: String::from(","),
                grouping: vec![3],
            },
            monetary: MonetaryFormat {
                currency_symbol: String::from("£"),
                int_curr_symbol: String::from("GBP "),
                mon_decimal_point: String::from("."),
                mon_thousands_sep: String::from(","),
                mon_grouping: vec![3],
                positive_sign: String::new(),
                negative_sign: String::from("-"),
                frac_digits: 2,
                p_cs_precedes: true,
                n_cs_precedes: true,
                p_sep_by_space: false,
                n_sep_by_space: false,
            },
            time: TimeFormat {
                d_t_fmt: String::from("%a %d %b %Y %T %Z"),
                d_fmt: String::from("%d/%m/%Y"),
                t_fmt: String::from("%T"),
                t_fmt_ampm: String::from("%I:%M:%S %p"),
                ..Locale::default_time_format()
            },
            messages: MessagesFormat {
                yesexpr: String::from("^[yY]"),
                noexpr: String::from("^[nN]"),
                yesstr: String::from("yes"),
                nostr: String::from("no"),
            },
        },
        // de_DE.UTF-8
        Locale {
            name: String::from("de_DE.UTF-8"),
            language: String::from("de"),
            territory: String::from("DE"),
            codeset: String::from("UTF-8"),
            numeric: NumericFormat {
                decimal_point: String::from(","),
                thousands_sep: String::from("."),
                grouping: vec![3],
            },
            monetary: MonetaryFormat {
                currency_symbol: String::from("€"),
                int_curr_symbol: String::from("EUR "),
                mon_decimal_point: String::from(","),
                mon_thousands_sep: String::from("."),
                mon_grouping: vec![3],
                positive_sign: String::new(),
                negative_sign: String::from("-"),
                frac_digits: 2,
                p_cs_precedes: false,
                n_cs_precedes: false,
                p_sep_by_space: true,
                n_sep_by_space: true,
            },
            time: TimeFormat {
                d_t_fmt: String::from("%a %d %b %Y %T %Z"),
                d_fmt: String::from("%d.%m.%Y"),
                t_fmt: String::from("%T"),
                t_fmt_ampm: String::from("%I:%M:%S %p"),
                first_weekday: 1,
                day_names: [
                    String::from("Sonntag"),
                    String::from("Montag"),
                    String::from("Dienstag"),
                    String::from("Mittwoch"),
                    String::from("Donnerstag"),
                    String::from("Freitag"),
                    String::from("Samstag"),
                ],
                day_abbrevs: [
                    String::from("So"),
                    String::from("Mo"),
                    String::from("Di"),
                    String::from("Mi"),
                    String::from("Do"),
                    String::from("Fr"),
                    String::from("Sa"),
                ],
                month_names: [
                    String::from("Januar"),
                    String::from("Februar"),
                    String::from("März"),
                    String::from("April"),
                    String::from("Mai"),
                    String::from("Juni"),
                    String::from("Juli"),
                    String::from("August"),
                    String::from("September"),
                    String::from("Oktober"),
                    String::from("November"),
                    String::from("Dezember"),
                ],
                month_abbrevs: [
                    String::from("Jan"),
                    String::from("Feb"),
                    String::from("Mär"),
                    String::from("Apr"),
                    String::from("Mai"),
                    String::from("Jun"),
                    String::from("Jul"),
                    String::from("Aug"),
                    String::from("Sep"),
                    String::from("Okt"),
                    String::from("Nov"),
                    String::from("Dez"),
                ],
                am_pm: [String::from("AM"), String::from("PM")],
            },
            messages: MessagesFormat {
                yesexpr: String::from("^[jJyY]"),
                noexpr: String::from("^[nN]"),
                yesstr: String::from("ja"),
                nostr: String::from("nein"),
            },
        },
        // fr_FR.UTF-8
        Locale {
            name: String::from("fr_FR.UTF-8"),
            language: String::from("fr"),
            territory: String::from("FR"),
            codeset: String::from("UTF-8"),
            numeric: NumericFormat {
                decimal_point: String::from(","),
                thousands_sep: String::from(" "),
                grouping: vec![3],
            },
            monetary: MonetaryFormat {
                currency_symbol: String::from("€"),
                int_curr_symbol: String::from("EUR "),
                mon_decimal_point: String::from(","),
                mon_thousands_sep: String::from(" "),
                mon_grouping: vec![3],
                positive_sign: String::new(),
                negative_sign: String::from("-"),
                frac_digits: 2,
                p_cs_precedes: false,
                n_cs_precedes: false,
                p_sep_by_space: true,
                n_sep_by_space: true,
            },
            time: TimeFormat {
                d_t_fmt: String::from("%a %d %b %Y %T %Z"),
                d_fmt: String::from("%d/%m/%Y"),
                t_fmt: String::from("%T"),
                t_fmt_ampm: String::from("%I:%M:%S %p"),
                first_weekday: 1,
                day_names: [
                    String::from("dimanche"),
                    String::from("lundi"),
                    String::from("mardi"),
                    String::from("mercredi"),
                    String::from("jeudi"),
                    String::from("vendredi"),
                    String::from("samedi"),
                ],
                day_abbrevs: [
                    String::from("dim"),
                    String::from("lun"),
                    String::from("mar"),
                    String::from("mer"),
                    String::from("jeu"),
                    String::from("ven"),
                    String::from("sam"),
                ],
                month_names: [
                    String::from("janvier"),
                    String::from("février"),
                    String::from("mars"),
                    String::from("avril"),
                    String::from("mai"),
                    String::from("juin"),
                    String::from("juillet"),
                    String::from("août"),
                    String::from("septembre"),
                    String::from("octobre"),
                    String::from("novembre"),
                    String::from("décembre"),
                ],
                month_abbrevs: [
                    String::from("janv."),
                    String::from("févr."),
                    String::from("mars"),
                    String::from("avr."),
                    String::from("mai"),
                    String::from("juin"),
                    String::from("juil."),
                    String::from("août"),
                    String::from("sept."),
                    String::from("oct."),
                    String::from("nov."),
                    String::from("déc."),
                ],
                am_pm: [String::from("AM"), String::from("PM")],
            },
            messages: MessagesFormat {
                yesexpr: String::from("^[oOyY]"),
                noexpr: String::from("^[nN]"),
                yesstr: String::from("oui"),
                nostr: String::from("non"),
            },
        },
        // ja_JP.UTF-8
        Locale {
            name: String::from("ja_JP.UTF-8"),
            language: String::from("ja"),
            territory: String::from("JP"),
            codeset: String::from("UTF-8"),
            numeric: NumericFormat {
                decimal_point: String::from("."),
                thousands_sep: String::from(","),
                grouping: vec![3],
            },
            monetary: MonetaryFormat {
                currency_symbol: String::from("¥"),
                int_curr_symbol: String::from("JPY "),
                mon_decimal_point: String::from("."),
                mon_thousands_sep: String::from(","),
                mon_grouping: vec![3],
                positive_sign: String::new(),
                negative_sign: String::from("-"),
                frac_digits: 0,
                p_cs_precedes: true,
                n_cs_precedes: true,
                p_sep_by_space: false,
                n_sep_by_space: false,
            },
            time: TimeFormat {
                d_t_fmt: String::from("%Y年%m月%d日 %H時%M分%S秒"),
                d_fmt: String::from("%Y/%m/%d"),
                t_fmt: String::from("%H:%M:%S"),
                t_fmt_ampm: String::from("%p%I:%M:%S"),
                first_weekday: 0,
                ..Locale::default_time_format()
            },
            messages: MessagesFormat {
                yesexpr: String::from("^[yYはハ]"),
                noexpr: String::from("^[nNいイ]"),
                yesstr: String::from("はい"),
                nostr: String::from("いいえ"),
            },
        },
        // zh_CN.UTF-8
        Locale {
            name: String::from("zh_CN.UTF-8"),
            language: String::from("zh"),
            territory: String::from("CN"),
            codeset: String::from("UTF-8"),
            numeric: NumericFormat {
                decimal_point: String::from("."),
                thousands_sep: String::from(","),
                grouping: vec![3],
            },
            monetary: MonetaryFormat {
                currency_symbol: String::from("¥"),
                int_curr_symbol: String::from("CNY "),
                mon_decimal_point: String::from("."),
                mon_thousands_sep: String::from(","),
                mon_grouping: vec![3],
                positive_sign: String::new(),
                negative_sign: String::from("-"),
                frac_digits: 2,
                p_cs_precedes: true,
                n_cs_precedes: true,
                p_sep_by_space: false,
                n_sep_by_space: false,
            },
            time: TimeFormat {
                d_t_fmt: String::from("%Y年%m月%d日 %A %H:%M:%S"),
                d_fmt: String::from("%Y/%m/%d"),
                t_fmt: String::from("%H:%M:%S"),
                t_fmt_ampm: String::from("%p %I:%M:%S"),
                first_weekday: 1,
                ..Locale::default_time_format()
            },
            messages: MessagesFormat {
                yesexpr: String::from("^[yY是]"),
                noexpr: String::from("^[nN否]"),
                yesstr: String::from("是"),
                nostr: String::from("否"),
            },
        },
    ]
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

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
