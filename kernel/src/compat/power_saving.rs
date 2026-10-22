use crate::serial_println;
/// Power Saving & Idle Management
///
/// CPU frequency scaling, screen dimming, suspend/hibernate,
/// idle detection, battery monitoring.
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowerProfile {
    Performance,
    Balanced,
    PowerSaver,
}

#[derive(Debug, Clone, Copy)]
pub enum SuspendState {
    S0Running,
    S1Standby,
    S3Suspend,
    S4Hibernate,
    S5Off,
}

pub struct PowerManager {
    pub profile: PowerProfile,
    pub screen_dim_secs: u32,
    pub screen_off_secs: u32,
    pub suspend_secs: u32,
    pub idle_since: u64,
    pub screen_brightness: u8, // 0-100
    pub cpu_freq_mhz: u32,
    pub cpu_max_mhz: u32,
    pub battery_percent: Option<u8>,
    pub on_ac_power: bool,
}

lazy_static::lazy_static! {
    static ref POWER: Mutex<PowerManager> = Mutex::new(PowerManager {
        profile: PowerProfile::Balanced,
        screen_dim_secs: 120,
        screen_off_secs: 300,
        suspend_secs: 600,
        idle_since: 0,
        screen_brightness: 100,
        cpu_freq_mhz: 3000,
        cpu_max_mhz: 5000,
        battery_percent: None,
        on_ac_power: true,
    });
}

impl PowerManager {
    pub fn set_profile(&mut self, profile: PowerProfile) {
        self.profile = profile;
        match profile {
            PowerProfile::Performance => self.cpu_freq_mhz = self.cpu_max_mhz,
            PowerProfile::Balanced => self.cpu_freq_mhz = self.cpu_max_mhz * 3 / 4,
            PowerProfile::PowerSaver => self.cpu_freq_mhz = self.cpu_max_mhz / 2,
        }
        serial_println!(
            "[POWER] Profile: {:?} (CPU {}MHz)",
            profile,
            self.cpu_freq_mhz
        );
    }

    pub fn on_idle_tick(&mut self, now_secs: u64) {
        let idle = now_secs - self.idle_since;
        if idle >= self.screen_dim_secs as u64 && self.screen_brightness > 30 {
            self.screen_brightness = 30;
            serial_println!("[POWER] Screen dimmed");
        }
        if idle >= self.screen_off_secs as u64 && self.screen_brightness > 0 {
            self.screen_brightness = 0;
            serial_println!("[POWER] Screen off");
        }
    }

    pub fn on_user_activity(&mut self, now_secs: u64) {
        self.idle_since = now_secs;
        self.screen_brightness = 100;
    }

    pub fn set_brightness(&mut self, pct: u8) {
        self.screen_brightness = pct.min(100);
    }

    pub fn update_battery(&mut self, percent: u8, ac: bool) {
        self.battery_percent = Some(percent);
        self.on_ac_power = ac;
        if percent <= 5 && !ac {
            serial_println!("[POWER] CRITICAL: Battery {}%, hibernating", percent);
        }
    }
}

pub fn init() {
    serial_println!("[POWER] Power saving manager initialized");
}
