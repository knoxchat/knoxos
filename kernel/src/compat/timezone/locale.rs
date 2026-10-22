//! POSIX locale categories and locale-aware number/currency formatting.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

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

    pub(crate) fn default_time_format() -> TimeFormat {
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
