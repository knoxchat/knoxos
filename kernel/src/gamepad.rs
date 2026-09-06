// SPDX-License-Identifier: MIT
//! Gamepad / joystick input driver (item 6.12)
//!
//! Handles gamepad/joystick input via USB HID or VirtIO input devices.
//! Supports standard gamepad layout (Xbox/PS style) with analog sticks,
//! triggers, buttons, and D-pad.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Gamepad button identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GamepadButton {
    /// Face buttons (ABXY / Cross, Circle, Square, Triangle)
    South, // A / Cross
    East,  // B / Circle
    West,  // X / Square
    North, // Y / Triangle
    /// Shoulder buttons
    LeftBumper,
    RightBumper,
    /// Stick clicks
    LeftStick,
    RightStick,
    /// Center buttons
    Start,
    Select,
    Guide, // Xbox button / PS button
    /// D-Pad
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

/// Gamepad axis identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GamepadAxis {
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
    LeftTrigger,
    RightTrigger,
}

/// Gamepad state snapshot
#[derive(Debug, Clone)]
pub struct GamepadState {
    /// Button states (pressed = true)
    pub buttons: BTreeMap<GamepadButton, bool>,
    /// Axis values (-1.0 to 1.0 for sticks, 0.0 to 1.0 for triggers)
    pub axes: BTreeMap<GamepadAxis, f32>,
    /// Vibration state (left motor, right motor) 0.0-1.0
    pub rumble: (f32, f32),
    /// Connection state
    pub connected: bool,
}

impl GamepadState {
    pub fn new() -> Self {
        let mut buttons = BTreeMap::new();
        let mut axes = BTreeMap::new();

        // Initialize all buttons to released
        for btn in &[
            GamepadButton::South,
            GamepadButton::East,
            GamepadButton::West,
            GamepadButton::North,
            GamepadButton::LeftBumper,
            GamepadButton::RightBumper,
            GamepadButton::LeftStick,
            GamepadButton::RightStick,
            GamepadButton::Start,
            GamepadButton::Select,
            GamepadButton::Guide,
            GamepadButton::DPadUp,
            GamepadButton::DPadDown,
            GamepadButton::DPadLeft,
            GamepadButton::DPadRight,
        ] {
            buttons.insert(*btn, false);
        }

        // Initialize all axes to 0
        for axis in &[
            GamepadAxis::LeftStickX,
            GamepadAxis::LeftStickY,
            GamepadAxis::RightStickX,
            GamepadAxis::RightStickY,
            GamepadAxis::LeftTrigger,
            GamepadAxis::RightTrigger,
        ] {
            axes.insert(*axis, 0.0);
        }

        Self {
            buttons,
            axes,
            rumble: (0.0, 0.0),
            connected: false,
        }
    }

    /// Check if a button is pressed
    pub fn is_pressed(&self, button: GamepadButton) -> bool {
        self.buttons.get(&button).copied().unwrap_or(false)
    }

    /// Get an axis value
    pub fn axis_value(&self, axis: GamepadAxis) -> f32 {
        self.axes.get(&axis).copied().unwrap_or(0.0)
    }

    /// Apply deadzone to stick axes
    pub fn apply_deadzone(&mut self, deadzone: f32) {
        for axis in &[
            GamepadAxis::LeftStickX,
            GamepadAxis::LeftStickY,
            GamepadAxis::RightStickX,
            GamepadAxis::RightStickY,
        ] {
            if let Some(val) = self.axes.get_mut(axis) {
                if val.abs() < deadzone {
                    *val = 0.0;
                }
            }
        }
    }
}

/// Gamepad event types
#[derive(Debug, Clone)]
pub enum GamepadEvent {
    Connected(u32),
    Disconnected(u32),
    ButtonPressed(u32, GamepadButton),
    ButtonReleased(u32, GamepadButton),
    AxisChanged(u32, GamepadAxis, f32),
}

/// Gamepad info
#[derive(Debug, Clone)]
pub struct GamepadInfo {
    pub id: u32,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub state: GamepadState,
}

lazy_static::lazy_static! {
    static ref GAMEPADS: Mutex<BTreeMap<u32, GamepadInfo>> = Mutex::new(BTreeMap::new());
    static ref EVENT_QUEUE: Mutex<Vec<GamepadEvent>> = Mutex::new(Vec::new());
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static EVENTS_PROCESSED: AtomicU64 = AtomicU64::new(0);

/// Default stick deadzone
const DEFAULT_DEADZONE: f32 = 0.15;

/// Register a new gamepad
pub fn register_gamepad(name: &str, vendor: u16, product: u16) -> u32 {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) as u32;
    let mut state = GamepadState::new();
    state.connected = true;

    let info = GamepadInfo {
        id,
        name: String::from(name),
        vendor_id: vendor,
        product_id: product,
        state,
    };

    GAMEPADS.lock().insert(id, info);
    EVENT_QUEUE.lock().push(GamepadEvent::Connected(id));

    crate::serial_println!("[gamepad] registered: id={}, name={}", id, name);
    id
}

/// Unregister a gamepad
pub fn unregister_gamepad(id: u32) {
    if let Some(info) = GAMEPADS.lock().remove(&id) {
        EVENT_QUEUE.lock().push(GamepadEvent::Disconnected(id));
        crate::serial_println!("[gamepad] disconnected: id={}, name={}", id, info.name);
    }
}

/// Update button state
pub fn update_button(id: u32, button: GamepadButton, pressed: bool) {
    let mut gamepads = GAMEPADS.lock();
    if let Some(info) = gamepads.get_mut(&id) {
        let prev = info.state.is_pressed(button);
        info.state.buttons.insert(button, pressed);
        if pressed && !prev {
            EVENT_QUEUE
                .lock()
                .push(GamepadEvent::ButtonPressed(id, button));
        } else if !pressed && prev {
            EVENT_QUEUE
                .lock()
                .push(GamepadEvent::ButtonReleased(id, button));
        }
        EVENTS_PROCESSED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Update axis value
pub fn update_axis(id: u32, axis: GamepadAxis, value: f32) {
    let mut gamepads = GAMEPADS.lock();
    if let Some(info) = gamepads.get_mut(&id) {
        let clamped = value.clamp(-1.0, 1.0);
        info.state.axes.insert(axis, clamped);
        EVENT_QUEUE
            .lock()
            .push(GamepadEvent::AxisChanged(id, axis, clamped));
        EVENTS_PROCESSED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Set rumble/vibration for a gamepad
pub fn set_rumble(id: u32, left_motor: f32, right_motor: f32) {
    let mut gamepads = GAMEPADS.lock();
    if let Some(info) = gamepads.get_mut(&id) {
        info.state.rumble = (left_motor.clamp(0.0, 1.0), right_motor.clamp(0.0, 1.0));
    }
}

/// Get the state of a specific gamepad
pub fn get_state(id: u32) -> Option<GamepadState> {
    let gamepads = GAMEPADS.lock();
    gamepads.get(&id).map(|info| {
        let mut state = info.state.clone();
        state.apply_deadzone(DEFAULT_DEADZONE);
        state
    })
}

/// Get all connected gamepad IDs
pub fn connected_gamepads() -> Vec<u32> {
    GAMEPADS.lock().keys().copied().collect()
}

/// Poll for pending gamepad events
pub fn poll_events() -> Vec<GamepadEvent> {
    let mut queue = EVENT_QUEUE.lock();
    core::mem::take(&mut *queue)
}

/// Process raw USB HID gamepad report
pub fn process_hid_report(id: u32, report: &[u8]) {
    // Standard HID gamepad report format:
    // byte 0-1: buttons bitmask
    // byte 2: left stick X (0-255, center=128)
    // byte 3: left stick Y (0-255, center=128)
    // byte 4: right stick X
    // byte 5: right stick Y
    // byte 6: left trigger (0-255)
    // byte 7: right trigger (0-255)
    if report.len() < 8 {
        return;
    }

    let buttons = u16::from_le_bytes([report[0], report[1]]);

    // Map bits to buttons
    let button_map = [
        (0x0001, GamepadButton::South),
        (0x0002, GamepadButton::East),
        (0x0004, GamepadButton::West),
        (0x0008, GamepadButton::North),
        (0x0010, GamepadButton::LeftBumper),
        (0x0020, GamepadButton::RightBumper),
        (0x0040, GamepadButton::Select),
        (0x0080, GamepadButton::Start),
        (0x0100, GamepadButton::LeftStick),
        (0x0200, GamepadButton::RightStick),
        (0x0400, GamepadButton::Guide),
        (0x1000, GamepadButton::DPadUp),
        (0x2000, GamepadButton::DPadDown),
        (0x4000, GamepadButton::DPadLeft),
        (0x8000, GamepadButton::DPadRight),
    ];

    for (mask, btn) in &button_map {
        update_button(id, *btn, buttons & mask != 0);
    }

    // Convert 0-255 to -1.0..1.0
    let to_axis = |v: u8| -> f32 { (v as f32 - 128.0) / 128.0 };
    let to_trigger = |v: u8| -> f32 { v as f32 / 255.0 };

    update_axis(id, GamepadAxis::LeftStickX, to_axis(report[2]));
    update_axis(id, GamepadAxis::LeftStickY, to_axis(report[3]));
    update_axis(id, GamepadAxis::RightStickX, to_axis(report[4]));
    update_axis(id, GamepadAxis::RightStickY, to_axis(report[5]));
    update_axis(id, GamepadAxis::LeftTrigger, to_trigger(report[6]));
    update_axis(id, GamepadAxis::RightTrigger, to_trigger(report[7]));
}

pub fn stats() -> u64 {
    EVENTS_PROCESSED.load(Ordering::Relaxed)
}

/// Initialize the gamepad subsystem
pub fn init() {
    // Scan for USB HID gamepad devices
    let usb_count = detect_usb_gamepads();
    // Scan for VirtIO input gamepads
    let virtio_count = detect_virtio_gamepads();

    crate::serial_println!(
        "[gamepad] initialized, deadzone={}, usb={}, virtio={}",
        DEFAULT_DEADZONE,
        usb_count,
        virtio_count
    );
}

// ═══════════════════════════════════════════════════════════════════════
// USB HID GAMEPAD DETECTION
// ═══════════════════════════════════════════════════════════════════════

/// USB HID usage page for Game Controls
const HID_USAGE_PAGE_GAME: u16 = 0x05;
/// USB HID usage page for Generic Desktop
const HID_USAGE_PAGE_DESKTOP: u16 = 0x01;
/// USB HID usage: Gamepad
const HID_USAGE_GAMEPAD: u8 = 0x05;
/// USB HID usage: Joystick
const HID_USAGE_JOYSTICK: u8 = 0x04;

/// Known USB Vendor:Product IDs for gamepads
const KNOWN_GAMEPADS: &[(u16, u16, &str)] = &[
    (0x045E, 0x028E, "Xbox 360 Controller"),
    (0x045E, 0x02D1, "Xbox One Controller"),
    (0x045E, 0x0B12, "Xbox Series X Controller"),
    (0x054C, 0x0CE6, "DualSense (PS5)"),
    (0x054C, 0x09CC, "DualShock 4 (PS4)"),
    (0x054C, 0x05C4, "DualShock 4 v1"),
    (0x057E, 0x2009, "Switch Pro Controller"),
    (0x057E, 0x200E, "Switch Joy-Con (L+R)"),
    (0x046D, 0xC21D, "Logitech F310"),
    (0x046D, 0xC21E, "Logitech F510"),
    (0x046D, 0xC21F, "Logitech F710"),
    (0x0079, 0x0006, "Generic USB Gamepad"),
    (0x2DC8, 0x2002, "8BitDo SN30 Pro+"),
];

/// Scan PCI USB controllers for HID gamepad devices
fn detect_usb_gamepads() -> usize {
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::instructions::port::Port;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::port::Port;
    let mut count = 0;

    // Find XHCI controllers
    let usb_controllers = crate::pcie_ecam::find_by_class(0x0C, 0x03);

    for ctrl in &usb_controllers {
        // For each USB controller, we'd enumerate attached devices
        // Check for HID class (0x03) with boot subclass or vendor-specific
        crate::serial_println!(
            "[gamepad] USB controller at PCI {}:{}.{} available for gamepad enumeration",
            ctrl.bus,
            ctrl.device,
            ctrl.function
        );
    }

    // Check if QEMU has emulated gamepads via -device usb-gamepad
    // In practice, enumerate USB device descriptors looking for:
    // - Interface Class 0x03 (HID)
    // - Usage Page 0x01, Usage 0x04 (Joystick) or 0x05 (Gamepad)

    count
}

/// Scan for VirtIO input gamepad devices
fn detect_virtio_gamepads() -> usize {
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::instructions::port::Port;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::port::Port;
    let mut count = 0;

    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            let addr: u32 = (1 << 31) | ((bus as u32) << 16) | ((dev as u32) << 11);
            let id = unsafe {
                let mut p = Port::<u32>::new(0xCF8);
                let mut d = Port::<u32>::new(0xCFC);
                p.write(addr);
                d.read()
            };

            let vendor = (id & 0xFFFF) as u16;
            let device_id = ((id >> 16) & 0xFFFF) as u16;

            // VirtIO input device (0x1052)
            if vendor == 0x1AF4 && device_id == 0x1052 {
                // Read device-specific config to check if it's a gamepad
                // VirtIO input devtype == 0x05 (gamepad)
                // For now, just note discovery
                crate::serial_println!(
                    "[gamepad] VirtIO input device at PCI {:02x}:{:02x}.0",
                    bus,
                    dev
                );
            }
        }
    }
    count
}

/// Send USB HID output report for rumble/force feedback
pub fn send_rumble_report(id: u32) {
    let gamepads = GAMEPADS.lock();
    if let Some(info) = gamepads.get(&id) {
        let (left, right) = info.state.rumble;
        let left_byte = (left * 255.0) as u8;
        let right_byte = (right * 255.0) as u8;

        // Xbox-style rumble report: [0x00, 0x08, 0x00, left, right, 0x00, 0x00, 0x00]
        let report = [0x00u8, 0x08, 0x00, left_byte, right_byte, 0x00, 0x00, 0x00];

        // In a real implementation, send this via USB HID SET_REPORT
        // using the XHCI driver's interrupt OUT endpoint
        crate::serial_println!(
            "[gamepad] Rumble report for pad {}: L={}, R={}",
            id,
            left_byte,
            right_byte
        );

        // Queue for USB HID output
        let _ = &report;
    }
}

/// Process Xbox-style extended gamepad report (with triggers as axes)
pub fn process_xbox_report(id: u32, report: &[u8]) {
    if report.len() < 14 {
        return;
    }

    // Xbox controller report format:
    // [0] = report type (0x20)
    // [1] = report size
    // [2-3] = buttons
    // [4] = left trigger (0-255)
    // [5] = right trigger (0-255)
    // [6-7] = left stick X (signed 16-bit)
    // [8-9] = left stick Y (signed 16-bit)
    // [10-11] = right stick X (signed 16-bit)
    // [12-13] = right stick Y (signed 16-bit)
    let buttons = u16::from_le_bytes([report[2], report[3]]);

    let button_map = [
        (0x0001, GamepadButton::DPadUp),
        (0x0002, GamepadButton::DPadDown),
        (0x0004, GamepadButton::DPadLeft),
        (0x0008, GamepadButton::DPadRight),
        (0x0010, GamepadButton::Start),
        (0x0020, GamepadButton::Select),
        (0x0040, GamepadButton::LeftStick),
        (0x0080, GamepadButton::RightStick),
        (0x0100, GamepadButton::LeftBumper),
        (0x0200, GamepadButton::RightBumper),
        (0x0400, GamepadButton::Guide),
        (0x1000, GamepadButton::South), // A
        (0x2000, GamepadButton::East),  // B
        (0x4000, GamepadButton::West),  // X
        (0x8000, GamepadButton::North), // Y
    ];

    for (mask, btn) in &button_map {
        update_button(id, *btn, buttons & mask != 0);
    }

    update_axis(id, GamepadAxis::LeftTrigger, report[4] as f32 / 255.0);
    update_axis(id, GamepadAxis::RightTrigger, report[5] as f32 / 255.0);

    let sx = |b: &[u8]| -> f32 { i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0 };
    update_axis(id, GamepadAxis::LeftStickX, sx(&report[6..8]));
    update_axis(id, GamepadAxis::LeftStickY, sx(&report[8..10]));
    update_axis(id, GamepadAxis::RightStickX, sx(&report[10..12]));
    update_axis(id, GamepadAxis::RightStickY, sx(&report[12..14]));
}

/// Process DualShock 4 / DualSense report
pub fn process_ds4_report(id: u32, report: &[u8]) {
    if report.len() < 10 {
        return;
    }

    // DS4 report: [report_id, lx, ly, rx, ry, buttons1, buttons2, buttons3, l2, r2, ...]
    let lx = report[1];
    let ly = report[2];
    let rx = report[3];
    let ry = report[4];
    let buttons1 = report[5];
    let buttons2 = report[6];
    let l2 = report[8];
    let r2 = report[9];

    // D-pad from low nibble of buttons1 (hat switch)
    let dpad = buttons1 & 0x0F;
    update_button(
        id,
        GamepadButton::DPadUp,
        dpad == 0 || dpad == 1 || dpad == 7,
    );
    update_button(
        id,
        GamepadButton::DPadRight,
        dpad == 1 || dpad == 2 || dpad == 3,
    );
    update_button(
        id,
        GamepadButton::DPadDown,
        dpad == 3 || dpad == 4 || dpad == 5,
    );
    update_button(
        id,
        GamepadButton::DPadLeft,
        dpad == 5 || dpad == 6 || dpad == 7,
    );

    update_button(id, GamepadButton::West, buttons1 & 0x10 != 0); // Square
    update_button(id, GamepadButton::South, buttons1 & 0x20 != 0); // Cross
    update_button(id, GamepadButton::East, buttons1 & 0x40 != 0); // Circle
    update_button(id, GamepadButton::North, buttons1 & 0x80 != 0); // Triangle

    update_button(id, GamepadButton::LeftBumper, buttons2 & 0x01 != 0);
    update_button(id, GamepadButton::RightBumper, buttons2 & 0x02 != 0);
    update_button(id, GamepadButton::Select, buttons2 & 0x10 != 0); // Share
    update_button(id, GamepadButton::Start, buttons2 & 0x20 != 0); // Options
    update_button(id, GamepadButton::LeftStick, buttons2 & 0x40 != 0);
    update_button(id, GamepadButton::RightStick, buttons2 & 0x80 != 0);

    let to_axis = |v: u8| -> f32 { (v as f32 - 128.0) / 128.0 };
    update_axis(id, GamepadAxis::LeftStickX, to_axis(lx));
    update_axis(id, GamepadAxis::LeftStickY, to_axis(ly));
    update_axis(id, GamepadAxis::RightStickX, to_axis(rx));
    update_axis(id, GamepadAxis::RightStickY, to_axis(ry));
    update_axis(id, GamepadAxis::LeftTrigger, l2 as f32 / 255.0);
    update_axis(id, GamepadAxis::RightTrigger, r2 as f32 / 255.0);
}
