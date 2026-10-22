//! POSIX TZ string parsing (e.g. "EST5EDT,M3.2.0,M11.1.0").

use alloc::string::String;
use alloc::vec::Vec;

use super::types::{DstRule, DstTransition, Timezone};

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
        return Some(Timezone::simple(tz_string, &std_abbrev, -std_offset_hours));
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
