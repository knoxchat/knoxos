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

/// Generic 8-byte HID gamepad report:
/// `[buttons, hat, lx, ly, rx, ry, lt, rt]` with 0x80 stick center.
pub fn parse_hid_report(report: &[u8]) -> Option<GamepadState> {
    if report.len() < 8 {
        return None;
    }
    let buttons = report[0] as u32;
    let lx = (report[2] as i16 - 128) * 256;
    let ly = (report[3] as i16 - 128) * 256;
    let rx = (report[4] as i16 - 128) * 256;
    let ry = (report[5] as i16 - 128) * 256;
    Some(GamepadState {
        connected: true,
        vendor_id: 0,
        product_id: 0,
        name: String::from("HID Gamepad"),
        buttons,
        left_x: lx,
        left_y: ly,
        right_x: rx,
        right_y: ry,
        left_trigger: report[6],
        right_trigger: report[7],
        dpad: report[1],
    })
}

/// Apply a HID report to a connected gamepad slot.
pub fn gamepad_handle_report(index: usize, report: &[u8]) -> bool {
    let parsed = match parse_hid_report(report) {
        Some(p) => p,
        None => return false,
    };
    let mut gamepads = GAMEPADS.lock();
    if let Some(slot) = gamepads.get_mut(index) {
        slot.buttons = parsed.buttons;
        slot.left_x = parsed.left_x;
        slot.left_y = parsed.left_y;
        slot.right_x = parsed.right_x;
        slot.right_y = parsed.right_y;
        slot.left_trigger = parsed.left_trigger;
        slot.right_trigger = parsed.right_trigger;
        slot.dpad = parsed.dpad;
        true
    } else {
        false
    }
}

pub const GATE_USB_GAMEPAD_MARKER: &str = "GATE_USB gamepad";

pub fn gamepad_self_test() -> bool {
    let _ = gamepad_probe(1, 0x045E, 0x028E);
    // A button + left stick right + LT half-pressed
    let report = [BTN_A as u8, 8, 255, 128, 128, 128, 128, 0];
    if !gamepad_handle_report(0, &report) {
        serial_println!("[USB-Gamepad] GATE_USB FAILED: handle");
        return false;
    }
    let Some(st) = gamepad_poll(0) else {
        serial_println!("[USB-Gamepad] GATE_USB FAILED: poll");
        return false;
    };
    let ok = st.buttons & BTN_A != 0 && st.left_x > 10000 && st.left_trigger == 128;
    if ok {
        serial_println!(
            "[USB-Gamepad] {} (A + LX={} LT={})",
            GATE_USB_GAMEPAD_MARKER,
            st.left_x,
            st.left_trigger
        );
    } else {
        serial_println!(
            "[USB-Gamepad] GATE_USB FAILED buttons={:#x} lx={} lt={}",
            st.buttons,
            st.left_x,
            st.left_trigger
        );
    }
    ok
}
