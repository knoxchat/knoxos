use crate::serial_println;
/// Parental Controls
///
/// User account restrictions for child accounts: content filtering,
/// time limits, application whitelisting, and activity reporting.
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Time restriction (allowed hours)
#[derive(Debug, Clone)]
pub struct TimeRestriction {
    pub weekday_start_hour: u8,
    pub weekday_end_hour: u8,
    pub weekend_start_hour: u8,
    pub weekend_end_hour: u8,
    pub max_daily_minutes: u16,
}

impl Default for TimeRestriction {
    fn default() -> Self {
        Self {
            weekday_start_hour: 8,
            weekday_end_hour: 20,
            weekend_start_hour: 9,
            weekend_end_hour: 21,
            max_daily_minutes: 120,
        }
    }
}

/// Content filter level
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContentFilter {
    Off,
    Low,
    Medium,
    High,
}

/// Parental control profile for a user
#[derive(Debug, Clone)]
pub struct ParentalProfile {
    pub username: String,
    pub enabled: bool,
    pub time: TimeRestriction,
    pub content_filter: ContentFilter,
    pub allowed_apps: BTreeSet<String>,
    pub blocked_apps: BTreeSet<String>,
    pub blocked_websites: Vec<String>,
    pub allow_app_install: bool,
    pub allow_settings_change: bool,
    pub activity_log: bool,
    pub minutes_used_today: u16,
}

lazy_static::lazy_static! {
    static ref PROFILES: Mutex<Vec<ParentalProfile>> = Mutex::new(Vec::new());
}

impl ParentalProfile {
    pub fn new(username: &str) -> Self {
        Self {
            username: String::from(username),
            enabled: true,
            time: TimeRestriction::default(),
            content_filter: ContentFilter::Medium,
            allowed_apps: BTreeSet::new(),
            blocked_apps: BTreeSet::new(),
            blocked_websites: Vec::new(),
            allow_app_install: false,
            allow_settings_change: false,
            activity_log: true,
            minutes_used_today: 0,
        }
    }

    /// Check if computer use is allowed now
    pub fn is_time_allowed(&self, hour: u8, is_weekend: bool) -> bool {
        if !self.enabled {
            return true;
        }
        let (start, end) = if is_weekend {
            (self.time.weekend_start_hour, self.time.weekend_end_hour)
        } else {
            (self.time.weekday_start_hour, self.time.weekday_end_hour)
        };
        if hour < start || hour >= end {
            return false;
        }
        if self.minutes_used_today >= self.time.max_daily_minutes {
            return false;
        }
        true
    }

    /// Check if an application is allowed
    pub fn is_app_allowed(&self, app_id: &str) -> bool {
        if !self.enabled {
            return true;
        }
        if self.blocked_apps.contains(app_id) {
            return false;
        }
        if !self.allowed_apps.is_empty() && !self.allowed_apps.contains(app_id) {
            return false;
        }
        true
    }

    /// Check if a URL is blocked
    pub fn is_url_blocked(&self, url: &str) -> bool {
        if !self.enabled {
            return false;
        }
        let url_lower = url.to_lowercase();
        self.blocked_websites
            .iter()
            .any(|blocked| url_lower.contains(blocked.as_str()))
    }

    /// Record a minute of usage
    pub fn tick_minute(&mut self) {
        self.minutes_used_today = self.minutes_used_today.saturating_add(1);
    }

    /// Reset daily counter
    pub fn reset_daily(&mut self) {
        self.minutes_used_today = 0;
    }
}

pub fn set_profile(profile: ParentalProfile) {
    let mut profiles = PROFILES.lock();
    if let Some(p) = profiles.iter_mut().find(|p| p.username == profile.username) {
        *p = profile;
    } else {
        profiles.push(profile);
    }
}

pub fn check_access(
    username: &str,
    app_id: &str,
    hour: u8,
    is_weekend: bool,
) -> Result<(), &'static str> {
    let profiles = PROFILES.lock();
    if let Some(profile) = profiles.iter().find(|p| p.username == username) {
        if !profile.is_time_allowed(hour, is_weekend) {
            return Err("Time limit reached");
        }
        if !profile.is_app_allowed(app_id) {
            return Err("Application blocked");
        }
    }
    Ok(())
}

pub fn init() {
    serial_println!("[PARENTAL] Parental controls loaded");
}
