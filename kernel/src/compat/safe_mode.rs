use crate::serial_println;
/// Safe Mode Boot
///
/// Minimal boot with only essential drivers, no GUI compositor,
/// basic text console for troubleshooting.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BootMode {
    Normal,
    Safe,
    Emergency,
    Recovery,
}

pub struct SafeMode {
    pub mode: BootMode,
    pub disabled_modules: Vec<String>,
    pub minimal_drivers_only: bool,
    pub skip_gui: bool,
    pub skip_network: bool,
    pub read_only_root: bool,
}

lazy_static::lazy_static! {
    static ref STATE: Mutex<SafeMode> = Mutex::new(SafeMode {
        mode: BootMode::Normal,
        disabled_modules: Vec::new(),
        minimal_drivers_only: false,
        skip_gui: false,
        skip_network: false,
        read_only_root: false,
    });
}

/// Parse kernel command line for safe mode flags
pub fn parse_cmdline(cmdline: &str) {
    let mut state = STATE.lock();
    if cmdline.contains("safe") || cmdline.contains("safemode") {
        state.mode = BootMode::Safe;
        state.minimal_drivers_only = true;
        state.skip_gui = true;
        serial_println!("[SAFEMODE] Safe mode activated");
    }
    if cmdline.contains("emergency") {
        state.mode = BootMode::Emergency;
        state.minimal_drivers_only = true;
        state.skip_gui = true;
        state.skip_network = true;
        state.read_only_root = true;
        serial_println!("[SAFEMODE] Emergency mode activated");
    }
    if cmdline.contains("recovery") {
        state.mode = BootMode::Recovery;
        state.skip_gui = false; // Recovery uses basic GUI
        serial_println!("[SAFEMODE] Recovery mode activated");
    }
}

pub fn is_safe_mode() -> bool {
    STATE.lock().mode == BootMode::Safe
}
pub fn is_emergency() -> bool {
    STATE.lock().mode == BootMode::Emergency
}
pub fn should_skip_gui() -> bool {
    STATE.lock().skip_gui
}
pub fn should_skip_network() -> bool {
    STATE.lock().skip_network
}
pub fn current_mode() -> BootMode {
    STATE.lock().mode
}

pub fn init() {
    serial_println!("[SAFEMODE] Safe mode boot handler initialized");
}
