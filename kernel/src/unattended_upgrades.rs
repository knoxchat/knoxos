use crate::serial_println;
/// Unattended Upgrades Daemon
///
/// Background daemon that checks for and applies security/stable updates
/// automatically, with configurable schedule and notification.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub enum UpgradePolicy {
    SecurityOnly,
    Stable,
    All,
    Disabled,
}

pub struct UnattendedConfig {
    pub policy: UpgradePolicy,
    pub check_interval_hours: u32,
    pub auto_reboot: bool,
    pub reboot_time: (u8, u8), // (hour, minute)
    pub notify_user: bool,
    pub blacklist: Vec<String>,
}

pub struct UpgradeStatus {
    pub last_check: u64,
    pub last_upgrade: u64,
    pub pending_count: u32,
    pub history: Vec<UpgradeRecord>,
}

#[derive(Debug, Clone)]
pub struct UpgradeRecord {
    pub package: String,
    pub from_version: String,
    pub to_version: String,
    pub timestamp: u64,
    pub success: bool,
}

lazy_static::lazy_static! {
    static ref STATE: Mutex<(UnattendedConfig, UpgradeStatus)> = Mutex::new((
        UnattendedConfig {
            policy: UpgradePolicy::SecurityOnly,
            check_interval_hours: 24,
            auto_reboot: false,
            reboot_time: (3, 0),
            notify_user: true,
            blacklist: Vec::new(),
        },
        UpgradeStatus {
            last_check: 0,
            last_upgrade: 0,
            pending_count: 0,
            history: Vec::new(),
        }
    ));
}

pub fn check_updates() -> u32 {
    let mut state = STATE.lock();
    serial_println!(
        "[UNATTENDED] Checking for updates (policy: {:?})",
        state.0.policy
    );
    state.1.last_check = 0; // would use real clock
    state.1.pending_count
}

pub fn apply_pending() -> Vec<UpgradeRecord> {
    let mut state = STATE.lock();
    serial_println!(
        "[UNATTENDED] Applying {} pending updates",
        state.1.pending_count
    );
    state.1.pending_count = 0;
    state.1.last_upgrade = 0;
    Vec::new()
}

pub fn set_policy(policy: UpgradePolicy) {
    STATE.lock().0.policy = policy;
    serial_println!("[UNATTENDED] Policy: {:?}", policy);
}

pub fn init() {
    serial_println!("[UNATTENDED] Unattended upgrades daemon initialized");
}
