//! USB gamepad / joystick state tracking.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// USB Gamepad / Joystick Driver
// ═══════════════════════════════════════════════════════════════════════

/// Gamepad button state
#[derive(Debug, Clone, Default)]
pub struct GamepadState {
    pub connected: bool,
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
    pub buttons: u32,      // Bitmask of pressed buttons
    pub left_x: i16,       // Left stick X (-32768..32767)
    pub left_y: i16,       // Left stick Y
    pub right_x: i16,      // Right stick X
    pub right_y: i16,      // Right stick Y
    pub left_trigger: u8,  // Left trigger (0..255)
    pub right_trigger: u8, // Right trigger
    pub dpad: u8,          // D-pad hat switch
}

/// Button constants
pub const BTN_A: u32 = 1 << 0;
pub const BTN_B: u32 = 1 << 1;
pub const BTN_X: u32 = 1 << 2;
pub const BTN_Y: u32 = 1 << 3;
pub const BTN_LB: u32 = 1 << 4;
pub const BTN_RB: u32 = 1 << 5;
pub const BTN_START: u32 = 1 << 6;
pub const BTN_SELECT: u32 = 1 << 7;
pub const BTN_LSTICK: u32 = 1 << 8;
pub const BTN_RSTICK: u32 = 1 << 9;

lazy_static::lazy_static! {
    static ref GAMEPADS: Mutex<Vec<GamepadState>> = Mutex::new(Vec::new());
}

/// Initialize gamepad — probe USB HID devices for gamepad usage page
pub fn gamepad_probe(device_id: u8, vendor_id: u16, product_id: u16) -> bool {
    let mut gamepads = GAMEPADS.lock();
    let name = match (vendor_id, product_id) {
        (0x045E, _) => String::from("Xbox Controller"),
        (0x054C, _) => String::from("PlayStation Controller"),
        (0x057E, _) => String::from("Nintendo Controller"),
        _ => alloc::format!("USB Gamepad {:04x}:{:04x}", vendor_id, product_id),
    };
    gamepads.push(GamepadState {
        connected: true,
        vendor_id,
        product_id,
        name,
        ..Default::default()
    });
    serial_println!(
        "[USB-Gamepad] Probed gamepad {:04x}:{:04x}",
        vendor_id,
        product_id
    );
    true
}

/// Poll gamepad for latest state
pub fn gamepad_poll(index: usize) -> Option<GamepadState> {
    GAMEPADS.lock().get(index).cloned()
}

/// Get number of connected gamepads
pub fn gamepad_count() -> usize {
    GAMEPADS.lock().iter().filter(|g| g.connected).count()
}
