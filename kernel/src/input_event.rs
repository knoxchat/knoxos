/// Input — Unified input event subsystem (Linux evdev compatible)
///
/// Provides a Linux-compatible input event layer:
///   - evdev-compatible event structures
///   - Keyboard/mouse/touchpad device abstraction
///   - PS/2, USB-HID, virtio-input backends
///   - Key mapping and repeat handling
///   - Input event buffering and dispatch
///   - /dev/input/eventN device nodes
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Linux evdev event types ───────────────────────────────────────────

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;
pub const EV_MSC: u16 = 0x04;
pub const EV_SW: u16 = 0x05;
pub const EV_LED: u16 = 0x11;
pub const EV_SND: u16 = 0x12;
pub const EV_REP: u16 = 0x14;

// SYN codes
pub const SYN_REPORT: u16 = 0;
pub const SYN_CONFIG: u16 = 1;
pub const SYN_DROPPED: u16 = 3;

// REL codes
pub const REL_X: u16 = 0x00;
pub const REL_Y: u16 = 0x01;
pub const REL_Z: u16 = 0x02;
pub const REL_WHEEL: u16 = 0x08;
pub const REL_HWHEEL: u16 = 0x06;

// ABS codes
pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;
pub const ABS_Z: u16 = 0x02;
pub const ABS_MT_SLOT: u16 = 0x2F;
pub const ABS_MT_POSITION_X: u16 = 0x35;
pub const ABS_MT_POSITION_Y: u16 = 0x36;
pub const ABS_MT_TRACKING_ID: u16 = 0x39;

// Key state values
pub const KEY_RELEASED: i32 = 0;
pub const KEY_PRESSED: i32 = 1;
pub const KEY_REPEAT: i32 = 2;

// Common BTN codes (mouse buttons)
pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;

// ─── Input Event ───────────────────────────────────────────────────────

/// Linux-compatible input_event structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InputEvent {
    pub time_sec: u64,
    pub time_usec: u64,
    pub event_type: u16,
    pub code: u16,
    pub value: i32,
}

impl InputEvent {
    pub fn new(event_type: u16, code: u16, value: i32) -> Self {
        Self {
            time_sec: 0, // filled by dispatch
            time_usec: 0,
            event_type,
            code,
            value,
        }
    }

    pub fn syn_report() -> Self {
        Self::new(EV_SYN, SYN_REPORT, 0)
    }

    pub fn key(code: u16, value: i32) -> Self {
        Self::new(EV_KEY, code, value)
    }

    pub fn rel(code: u16, value: i32) -> Self {
        Self::new(EV_REL, code, value)
    }

    pub fn abs(code: u16, value: i32) -> Self {
        Self::new(EV_ABS, code, value)
    }
}

// ─── Input Device ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDeviceType {
    Keyboard,
    Mouse,
    Touchpad,
    Touchscreen,
    Tablet,
    Gamepad,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputBus {
    PS2,
    USB,
    VirtIO,
    I2C,
    Bluetooth,
    Other,
}

#[derive(Debug, Clone)]
pub struct InputDeviceInfo {
    pub bus: InputBus,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
    pub name: String,
    pub phys: String, // physical path (e.g., "isa0060/serio0/input0")
}

pub struct InputDevice {
    pub id: u32,
    pub dev_type: InputDeviceType,
    pub info: InputDeviceInfo,
    /// Ring buffer of pending events
    pub events: Vec<InputEvent>,
    pub max_events: usize,
    /// Listeners (PIDs that have this device open)
    pub listeners: Vec<u32>,
    pub grabbed_by: Option<u32>, // exclusive grab (EVIOCGRAB)
}

impl InputDevice {
    pub fn new(id: u32, dev_type: InputDeviceType, info: InputDeviceInfo) -> Self {
        Self {
            id,
            dev_type,
            info,
            events: Vec::new(),
            max_events: 256,
            listeners: Vec::new(),
            grabbed_by: None,
        }
    }

    /// Push an event into the device buffer
    pub fn push_event(&mut self, mut ev: InputEvent) {
        // Timestamp
        let ticks = crate::clock::uptime_seconds() * 1000;
        ev.time_sec = ticks / 1000;
        ev.time_usec = (ticks % 1000) * 1000;

        if self.events.len() >= self.max_events {
            self.events.remove(0); // drop oldest
        }
        self.events.push(ev);
    }

    /// Read and drain events
    pub fn read_events(&mut self, max: usize) -> Vec<InputEvent> {
        let n = max.min(self.events.len());
        self.events.drain(..n).collect()
    }

    pub fn has_events(&self) -> bool {
        !self.events.is_empty()
    }
}

// ─── Input Manager ─────────────────────────────────────────────────────

static NEXT_DEVICE_ID: AtomicU32 = AtomicU32::new(1);

pub struct InputManager {
    pub devices: BTreeMap<u32, InputDevice>,
    /// Key repeat settings
    pub repeat_delay_ms: u32,
    pub repeat_rate_ms: u32,
    /// Global key state (bitmap for 256 keys)
    pub key_state: [u64; 4],
}

impl Default for InputManager {
    fn default() -> Self {
        Self::new()
    }
}

impl InputManager {
    pub fn new() -> Self {
        Self {
            devices: BTreeMap::new(),
            repeat_delay_ms: 500,
            repeat_rate_ms: 33, // ~30 Hz
            key_state: [0; 4],
        }
    }

    /// Register a new input device
    pub fn register_device(&mut self, dev_type: InputDeviceType, info: InputDeviceInfo) -> u32 {
        let id = NEXT_DEVICE_ID.fetch_add(1, Ordering::Relaxed);
        let dev = InputDevice::new(id, dev_type, info);
        serial_println!(
            "[INPUT] Registered device {}: {} ({:?})",
            id,
            dev.info.name,
            dev_type
        );
        self.devices.insert(id, dev);
        id
    }

    /// Unregister device
    pub fn unregister_device(&mut self, id: u32) {
        if self.devices.remove(&id).is_some() {
            serial_println!("[INPUT] Unregistered device {}", id);
        }
    }

    /// Submit an event to a device
    pub fn submit_event(&mut self, device_id: u32, ev: InputEvent) {
        // Update global key state
        if ev.event_type == EV_KEY && ev.code < 256 {
            let idx = (ev.code / 64) as usize;
            let bit = ev.code % 64;
            if ev.value != KEY_RELEASED {
                self.key_state[idx] |= 1 << bit;
            } else {
                self.key_state[idx] &= !(1 << bit);
            }
        }

        if let Some(dev) = self.devices.get_mut(&device_id) {
            dev.push_event(ev);
        }
    }

    /// Submit a batch of events (keyboard: key + syn, mouse: rel_x + rel_y + syn)
    pub fn submit_events(&mut self, device_id: u32, events: &[InputEvent]) {
        for ev in events {
            self.submit_event(device_id, *ev);
        }
    }

    /// Check if a key is currently pressed
    pub fn is_key_pressed(&self, code: u16) -> bool {
        if code >= 256 {
            return false;
        }
        let idx = (code / 64) as usize;
        let bit = code % 64;
        self.key_state[idx] & (1 << bit) != 0
    }

    /// Get list of devices
    pub fn list_devices(&self) -> Vec<(u32, InputDeviceType, String)> {
        self.devices
            .iter()
            .map(|(&id, dev)| (id, dev.dev_type, dev.info.name.clone()))
            .collect()
    }

    /// Add a listener to a device
    pub fn add_listener(&mut self, device_id: u32, pid: u32) {
        if let Some(dev) = self.devices.get_mut(&device_id) {
            if !dev.listeners.contains(&pid) {
                dev.listeners.push(pid);
            }
        }
    }

    /// Remove listener
    pub fn remove_listener(&mut self, device_id: u32, pid: u32) {
        if let Some(dev) = self.devices.get_mut(&device_id) {
            dev.listeners.retain(|&p| p != pid);
        }
    }

    /// EVIOCGRAB — exclusive grab
    pub fn grab_device(&mut self, device_id: u32, pid: u32) -> bool {
        if let Some(dev) = self.devices.get_mut(&device_id) {
            if dev.grabbed_by.is_some() {
                return false;
            }
            dev.grabbed_by = Some(pid);
            true
        } else {
            false
        }
    }

    pub fn ungrab_device(&mut self, device_id: u32, pid: u32) {
        if let Some(dev) = self.devices.get_mut(&device_id) {
            if dev.grabbed_by == Some(pid) {
                dev.grabbed_by = None;
            }
        }
    }
}

// ─── Global state ──────────────────────────────────────────────────────

lazy_static::lazy_static! {
    pub static ref INPUT_MANAGER: Mutex<InputManager> = Mutex::new(InputManager::new());
}

static INPUT_READY: AtomicBool = AtomicBool::new(false);

// ─── Convenience API ───────────────────────────────────────────────────

pub fn register_device(dev_type: InputDeviceType, info: InputDeviceInfo) -> u32 {
    INPUT_MANAGER.lock().register_device(dev_type, info)
}

pub fn submit_key(device_id: u32, code: u16, value: i32) {
    let mut mgr = INPUT_MANAGER.lock();
    mgr.submit_event(device_id, InputEvent::key(code, value));
    mgr.submit_event(device_id, InputEvent::syn_report());
}

pub fn submit_mouse_move(device_id: u32, dx: i32, dy: i32) {
    let mut mgr = INPUT_MANAGER.lock();
    mgr.submit_event(device_id, InputEvent::rel(REL_X, dx));
    mgr.submit_event(device_id, InputEvent::rel(REL_Y, dy));
    mgr.submit_event(device_id, InputEvent::syn_report());
}

pub fn submit_mouse_button(device_id: u32, button: u16, pressed: bool) {
    let mut mgr = INPUT_MANAGER.lock();
    mgr.submit_event(
        device_id,
        InputEvent::key(button, if pressed { 1 } else { 0 }),
    );
    mgr.submit_event(device_id, InputEvent::syn_report());
}

pub fn is_ready() -> bool {
    INPUT_READY.load(Ordering::Relaxed)
}

/// Initialize the input subsystem
pub fn init() {
    let mut mgr = INPUT_MANAGER.lock();

    // Register built-in PS/2 keyboard
    let kb_info = InputDeviceInfo {
        bus: InputBus::PS2,
        vendor: 0x0001,
        product: 0x0001,
        version: 0x0001,
        name: String::from("AT Translated Set 2 keyboard"),
        phys: String::from("isa0060/serio0/input0"),
    };
    let _kb_id = mgr.register_device(InputDeviceType::Keyboard, kb_info);

    // Register built-in PS/2 mouse
    let mouse_info = InputDeviceInfo {
        bus: InputBus::PS2,
        vendor: 0x0002,
        product: 0x0001,
        version: 0x0001,
        name: String::from("PS/2 Generic Mouse"),
        phys: String::from("isa0060/serio1/input0"),
    };
    let _mouse_id = mgr.register_device(InputDeviceType::Mouse, mouse_info);

    drop(mgr);
    INPUT_READY.store(true, Ordering::Relaxed);
    serial_println!("[INPUT] Input event subsystem initialized (evdev-compatible)");
}
