//! DST rules, timezone definitions, and local date/time conversion.

use alloc::format;
use alloc::string::String;

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
