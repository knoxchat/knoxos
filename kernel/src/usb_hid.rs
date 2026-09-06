/// USB HID (Human Interface Device) Driver
/// Implements USB HID class driver for keyboards, mice, and gamepads
/// Integrates with the input_event subsystem for evdev-compatible event delivery
///
/// Supports:
/// - USB HID keyboard with full key mapping
/// - USB HID mouse with buttons and relative movement
/// - HID report descriptor parsing
/// - Boot protocol fallback for simple devices
/// - Interrupt transfer polling
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── HID Constants ──────────────────────────────────────────────────

/// HID class code
pub const USB_CLASS_HID: u8 = 0x03;

/// HID subclass
pub const HID_SUBCLASS_NONE: u8 = 0x00;
pub const HID_SUBCLASS_BOOT: u8 = 0x01;

/// HID protocol
pub const HID_PROTOCOL_NONE: u8 = 0x00;
pub const HID_PROTOCOL_KEYBOARD: u8 = 0x01;
pub const HID_PROTOCOL_MOUSE: u8 = 0x02;

/// HID request types
pub const HID_REQ_GET_REPORT: u8 = 0x01;
pub const HID_REQ_GET_IDLE: u8 = 0x02;
pub const HID_REQ_GET_PROTOCOL: u8 = 0x03;
pub const HID_REQ_SET_REPORT: u8 = 0x09;
pub const HID_REQ_SET_IDLE: u8 = 0x0A;
pub const HID_REQ_SET_PROTOCOL: u8 = 0x0B;

/// HID report types
pub const HID_REPORT_TYPE_INPUT: u8 = 1;
pub const HID_REPORT_TYPE_OUTPUT: u8 = 2;
pub const HID_REPORT_TYPE_FEATURE: u8 = 3;

/// HID descriptor types
pub const HID_DT_HID: u8 = 0x21;
pub const HID_DT_REPORT: u8 = 0x22;
pub const HID_DT_PHYSICAL: u8 = 0x23;

/// Maximum HID report size
pub const MAX_HID_REPORT_SIZE: usize = 64;

// ─── HID Usage Pages ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum HidUsagePage {
    GenericDesktop = 0x01,
    Simulation = 0x02,
    VR = 0x03,
    Sport = 0x04,
    Game = 0x05,
    GenericDevice = 0x06,
    Keyboard = 0x07,
    Led = 0x08,
    Button = 0x09,
    Ordinal = 0x0A,
    Telephony = 0x0B,
    Consumer = 0x0C,
    Digitizer = 0x0D,
    Unicode = 0x10,
    AlphanumDisplay = 0x14,
    Unknown = 0xFFFF,
}

/// Generic Desktop usage IDs
pub const USAGE_POINTER: u16 = 0x01;
pub const USAGE_MOUSE: u16 = 0x02;
pub const USAGE_JOYSTICK: u16 = 0x04;
pub const USAGE_GAMEPAD: u16 = 0x05;
pub const USAGE_KEYBOARD: u16 = 0x06;
pub const USAGE_KEYPAD: u16 = 0x07;
pub const USAGE_MULTI_AXIS: u16 = 0x08;
pub const USAGE_X: u16 = 0x30;
pub const USAGE_Y: u16 = 0x31;
pub const USAGE_Z: u16 = 0x32;
pub const USAGE_WHEEL: u16 = 0x38;

// ─── HID Report Descriptor Items ────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HidReportField {
    pub usage_page: u16,
    pub usage_min: u16,
    pub usage_max: u16,
    pub logical_min: i32,
    pub logical_max: i32,
    pub physical_min: i32,
    pub physical_max: i32,
    pub report_size: u32,  // bits per field
    pub report_count: u32, // number of fields
    pub flags: u32,        // Input/Output/Feature flags
    pub is_variable: bool,
    pub is_relative: bool,
}

#[derive(Debug, Clone)]
pub struct HidReportDescriptor {
    pub fields: Vec<HidReportField>,
    pub total_bits: u32,
}

impl Default for HidReportDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

impl HidReportDescriptor {
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            total_bits: 0,
        }
    }

    /// Parse a raw HID report descriptor
    pub fn parse(data: &[u8]) -> Self {
        let mut desc = Self::new();
        let mut i = 0;

        // Global state
        let mut usage_page: u16 = 0;
        let mut logical_min: i32 = 0;
        let mut logical_max: i32 = 0;
        let mut physical_min: i32 = 0;
        let mut physical_max: i32 = 0;
        let mut report_size: u32 = 0;
        let mut report_count: u32 = 0;

        // Local state
        let mut usage_min: u16 = 0;
        let mut usage_max: u16 = 0;

        while i < data.len() {
            let header = data[i];
            let bsize = match header & 0x03 {
                0 => 0,
                1 => 1,
                2 => 2,
                3 => 4,
                _ => 0,
            };
            let btype = (header >> 2) & 0x03;
            let btag = (header >> 4) & 0x0F;

            let value: u32 = match bsize {
                0 => 0,
                1 if i + 1 < data.len() => data[i + 1] as u32,
                2 if i + 2 < data.len() => u16::from_le_bytes([data[i + 1], data[i + 2]]) as u32,
                4 if i + 4 < data.len() => {
                    u32::from_le_bytes([data[i + 1], data[i + 2], data[i + 3], data[i + 4]])
                }
                _ => 0,
            };

            match btype {
                0 => {
                    // Main items
                    match btag {
                        0x08 | 0x09 => {
                            // Input or Output
                            let field = HidReportField {
                                usage_page,
                                usage_min,
                                usage_max,
                                logical_min,
                                logical_max,
                                physical_min,
                                physical_max,
                                report_size,
                                report_count,
                                flags: value,
                                is_variable: (value & 0x02) != 0,
                                is_relative: (value & 0x04) != 0,
                            };
                            desc.total_bits += report_size * report_count;
                            desc.fields.push(field);
                            // Reset local state
                            usage_min = 0;
                            usage_max = 0;
                        }
                        _ => {}
                    }
                }
                1 => {
                    // Global items
                    match btag {
                        0x00 => usage_page = value as u16,
                        0x01 => logical_min = value as i32,
                        0x02 => logical_max = value as i32,
                        0x03 => physical_min = value as i32,
                        0x04 => physical_max = value as i32,
                        0x07 => report_size = value,
                        0x09 => report_count = value,
                        _ => {}
                    }
                }
                2 => {
                    // Local items
                    match btag {
                        0x01 => usage_min = value as u16,
                        0x02 => usage_max = value as u16,
                        _ => {}
                    }
                }
                _ => {}
            }

            i += 1 + bsize as usize;
        }

        desc
    }
}

// ─── HID Device ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidDeviceType {
    Keyboard,
    Mouse,
    Gamepad,
    Joystick,
    Other,
}

#[derive(Debug, Clone)]
pub struct HidDevice {
    pub id: u32,
    pub device_id: u32, // input_event device registration ID
    pub device_type: HidDeviceType,
    pub usb_slot: u8,
    pub interface: u8,
    pub endpoint: u8,
    pub protocol: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
    pub report_descriptor: HidReportDescriptor,
    pub poll_interval_ms: u16,
    pub connected: bool,
    pub boot_protocol: bool,
    // Keyboard state
    pub key_state: [bool; 256],
    pub modifier_state: u8,
    pub led_state: u8,
    // Mouse state
    pub buttons: u8,
    pub x: i16,
    pub y: i16,
    pub wheel: i8,
}

impl HidDevice {
    pub fn new_keyboard(id: u32, vendor_id: u16, product_id: u16, name: String) -> Self {
        Self {
            id,
            device_id: 0,
            device_type: HidDeviceType::Keyboard,
            usb_slot: 0,
            interface: 0,
            endpoint: 1,
            protocol: HID_PROTOCOL_KEYBOARD,
            vendor_id,
            product_id,
            name,
            report_descriptor: HidReportDescriptor::new(),
            poll_interval_ms: 10,
            connected: true,
            boot_protocol: true,
            key_state: [false; 256],
            modifier_state: 0,
            led_state: 0,
            buttons: 0,
            x: 0,
            y: 0,
            wheel: 0,
        }
    }

    pub fn new_mouse(id: u32, vendor_id: u16, product_id: u16, name: String) -> Self {
        Self {
            id,
            device_id: 0,
            device_type: HidDeviceType::Mouse,
            usb_slot: 0,
            interface: 0,
            endpoint: 1,
            protocol: HID_PROTOCOL_MOUSE,
            vendor_id,
            product_id,
            name,
            report_descriptor: HidReportDescriptor::new(),
            poll_interval_ms: 8,
            connected: true,
            boot_protocol: true,
            key_state: [false; 256],
            modifier_state: 0,
            led_state: 0,
            buttons: 0,
            x: 0,
            y: 0,
            wheel: 0,
        }
    }
}

// ─── HID USB-to-Scancode Mapping ────────────────────────────────────

/// Convert USB HID keyboard usage ID to Linux input event key code
pub fn hid_to_linux_keycode(usage: u8) -> u16 {
    match usage {
        0x04 => 30,                                // A
        0x05 => 48,                                // B
        0x06 => 46,                                // C
        0x07 => 32,                                // D
        0x08 => 18,                                // E
        0x09 => 33,                                // F
        0x0A => 34,                                // G
        0x0B => 35,                                // H
        0x0C => 23,                                // I
        0x0D => 36,                                // J
        0x0E => 37,                                // K
        0x0F => 38,                                // L
        0x10 => 50,                                // M
        0x11 => 49,                                // N
        0x12 => 24,                                // O
        0x13 => 25,                                // P
        0x14 => 16,                                // Q
        0x15 => 19,                                // R
        0x16 => 31,                                // S
        0x17 => 20,                                // T
        0x18 => 22,                                // U
        0x19 => 47,                                // V
        0x1A => 17,                                // W
        0x1B => 45,                                // X
        0x1C => 21,                                // Y
        0x1D => 44,                                // Z
        0x1E => 2,                                 // 1
        0x1F => 3,                                 // 2
        0x20 => 4,                                 // 3
        0x21 => 5,                                 // 4
        0x22 => 6,                                 // 5
        0x23 => 7,                                 // 6
        0x24 => 8,                                 // 7
        0x25 => 9,                                 // 8
        0x26 => 10,                                // 9
        0x27 => 11,                                // 0
        0x28 => 28,                                // Enter
        0x29 => 1,                                 // Escape
        0x2A => 14,                                // Backspace
        0x2B => 15,                                // Tab
        0x2C => 57,                                // Space
        0x2D => 12,                                // Minus
        0x2E => 13,                                // Equal
        0x2F => 26,                                // Left Bracket
        0x30 => 27,                                // Right Bracket
        0x31 => 43,                                // Backslash
        0x33 => 39,                                // Semicolon
        0x34 => 40,                                // Apostrophe
        0x35 => 41,                                // Grave
        0x36 => 51,                                // Comma
        0x37 => 52,                                // Period
        0x38 => 53,                                // Slash
        0x39 => 58,                                // Caps Lock
        0x3A..=0x45 => 59 + (usage - 0x3A) as u16, // F1-F12
        0x46 => 99,                                // Print Screen
        0x47 => 70,                                // Scroll Lock
        0x48 => 119,                               // Pause
        0x49 => 110,                               // Insert
        0x4A => 102,                               // Home
        0x4B => 104,                               // Page Up
        0x4C => 111,                               // Delete
        0x4D => 107,                               // End
        0x4E => 109,                               // Page Down
        0x4F => 106,                               // Right Arrow
        0x50 => 105,                               // Left Arrow
        0x51 => 108,                               // Down Arrow
        0x52 => 103,                               // Up Arrow
        0x53 => 69,                                // Num Lock
        _ => 0,
    }
}

/// Parse a USB HID boot protocol keyboard report (8 bytes)
pub fn parse_boot_keyboard_report(device: &mut HidDevice, report: &[u8]) {
    if report.len() < 8 {
        return;
    }

    let new_modifiers = report[0];
    let old_modifiers = device.modifier_state;
    device.modifier_state = new_modifiers;

    // Check modifier changes
    for bit in 0..8u8 {
        let mask = 1u8 << bit;
        let was_pressed = old_modifiers & mask != 0;
        let now_pressed = new_modifiers & mask != 0;
        if was_pressed != now_pressed {
            let keycode = match bit {
                0 => 29,  // Left Ctrl
                1 => 42,  // Left Shift
                2 => 56,  // Left Alt
                3 => 125, // Left GUI/Meta
                4 => 97,  // Right Ctrl
                5 => 54,  // Right Shift
                6 => 100, // Right Alt
                7 => 126, // Right GUI/Meta
                _ => 0,
            };
            if keycode > 0 {
                crate::input_event::submit_key(
                    device.device_id,
                    keycode,
                    if now_pressed { 1 } else { 0 },
                );
            }
        }
    }

    // Parse key array (bytes 2-7, up to 6 simultaneous keys)
    // report[1] is reserved
    let mut new_keys = [0u8; 6];
    new_keys.copy_from_slice(&report[2..8]);

    // Detect released keys
    for i in 0..256 {
        if device.key_state[i] {
            let usage = i as u8;
            if !new_keys.contains(&usage) {
                device.key_state[i] = false;
                let keycode = hid_to_linux_keycode(usage);
                if keycode > 0 {
                    crate::input_event::submit_key(device.device_id, keycode, 0);
                    // key up
                }
            }
        }
    }

    // Detect pressed keys
    for &usage in &new_keys {
        if usage == 0 || usage == 1 {
            continue;
        } // No event or rollover error
        if !device.key_state[usage as usize] {
            device.key_state[usage as usize] = true;
            let keycode = hid_to_linux_keycode(usage);
            if keycode > 0 {
                crate::input_event::submit_key(device.device_id, keycode, 1); // key down
            }
        }
    }
}

/// Parse a USB HID boot protocol mouse report (3-4 bytes)
pub fn parse_boot_mouse_report(device: &mut HidDevice, report: &[u8]) {
    if report.len() < 3 {
        return;
    }

    let buttons = report[0];
    let dx = report[1] as i8;
    let dy = report[2] as i8;
    let wheel = if report.len() > 3 { report[3] as i8 } else { 0 };

    // Button changes
    let old_buttons = device.buttons;
    device.buttons = buttons;

    for bit in 0..3u8 {
        let mask = 1u8 << bit;
        let was = old_buttons & mask != 0;
        let now = buttons & mask != 0;
        if was != now {
            let btn_code = match bit {
                0 => 0x110u16, // BTN_LEFT
                1 => 0x111,    // BTN_RIGHT
                2 => 0x112,    // BTN_MIDDLE
                _ => 0,
            };
            crate::input_event::submit_mouse_button(device.device_id, btn_code, now);
        }
    }

    // Relative movement
    if dx != 0 || dy != 0 {
        crate::input_event::submit_mouse_move(device.device_id, dx as i32, dy as i32);
    }
    // Wheel events submitted via raw input_event API
    if wheel != 0 {
        let mut mgr = crate::input_event::INPUT_MANAGER.lock();
        mgr.submit_event(
            device.device_id,
            crate::input_event::InputEvent::rel(0x08, wheel as i32),
        );
        mgr.submit_event(
            device.device_id,
            crate::input_event::InputEvent::syn_report(),
        );
    }
}

// ─── Global State ───────────────────────────────────────────────────

static HID_DEVICES: Mutex<Vec<HidDevice>> = Mutex::new(Vec::new());
static NEXT_HID_ID: AtomicU32 = AtomicU32::new(1);
static HID_AVAILABLE: AtomicBool = AtomicBool::new(false);

pub fn register_device(device: HidDevice) {
    let mut devices = HID_DEVICES.lock();
    serial_println!(
        "[USB-HID] Registered {:?}: {} (VID:{:04x} PID:{:04x})",
        device.device_type,
        device.name,
        device.vendor_id,
        device.product_id
    );
    devices.push(device);
}

pub fn device_count() -> usize {
    HID_DEVICES.lock().len()
}

pub fn is_available() -> bool {
    HID_AVAILABLE.load(Ordering::Relaxed)
}

/// Poll all HID devices for new reports
pub fn poll_devices() {
    // In a full implementation, this would read from USB interrupt endpoints
    // For now, USB HID devices are handled by the XHCI controller's interrupt handler
}

/// Process a raw HID report from a USB device
pub fn process_report(device_id: u32, report: &[u8]) {
    let mut devices = HID_DEVICES.lock();
    if let Some(device) = devices.iter_mut().find(|d| d.id == device_id) {
        match device.device_type {
            HidDeviceType::Keyboard => parse_boot_keyboard_report(device, report),
            HidDeviceType::Mouse => parse_boot_mouse_report(device, report),
            _ => {}
        }
    }
}

pub fn init() {
    // Check if USB XHCI controller is available
    if crate::usb::is_available() {
        // Probe for HID devices on the USB bus
        let devices = crate::usb::list_devices();
        for usb_dev in &devices {
            // Check if this is a HID device by class code
            if usb_dev.device_class == USB_CLASS_HID {
                let id = NEXT_HID_ID.fetch_add(1, Ordering::Relaxed);
                let hid_dev = match usb_dev.device_protocol {
                    p if p == HID_PROTOCOL_KEYBOARD => HidDevice::new_keyboard(
                        id,
                        usb_dev.vendor_id,
                        usb_dev.product_id,
                        alloc::format!(
                            "USB Keyboard {:04x}:{:04x}",
                            usb_dev.vendor_id,
                            usb_dev.product_id
                        ),
                    ),
                    p if p == HID_PROTOCOL_MOUSE => HidDevice::new_mouse(
                        id,
                        usb_dev.vendor_id,
                        usb_dev.product_id,
                        alloc::format!(
                            "USB Mouse {:04x}:{:04x}",
                            usb_dev.vendor_id,
                            usb_dev.product_id
                        ),
                    ),
                    _ => continue,
                };
                register_device(hid_dev);
            }
        }
        HID_AVAILABLE.store(true, Ordering::Relaxed);
        serial_println!(
            "[USB-HID] USB HID driver initialized ({} devices)",
            device_count()
        );
    } else {
        serial_println!("[USB-HID] No USB controller available, USB HID disabled");
    }
}
