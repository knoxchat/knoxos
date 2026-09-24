//! Wacom tablet (pressure/tilt) probe and poll placeholders.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// Wacom Tablet (pressure/tilt) Driver
// ═══════════════════════════════════════════════════════════════════════

/// Tablet pen event
#[derive(Debug, Clone, Copy)]
pub struct TabletEvent {
    pub x: u32,
    pub y: u32,
    pub pressure: u16, // 0-8192
    pub tilt_x: i16,   // -90..90 degrees
    pub tilt_y: i16,   // -90..90 degrees
    pub button: u8,    // Pen buttons bitmask
    pub in_range: bool,
    pub touching: bool,
    pub eraser: bool,
}

/// Wacom tablet device
#[derive(Debug, Clone)]
pub struct WacomTablet {
    pub device_id: u8,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub max_pressure: u16,
    pub has_tilt: bool,
}

lazy_static::lazy_static! {
    static ref WACOM_TABLETS: Mutex<Vec<WacomTablet>> = Mutex::new(Vec::new());
}

/// Probe Wacom tablet (USB HID with Wacom vendor ID 0x056A)
pub fn wacom_probe(device_id: u8, name: &str) -> bool {
    let mut tablets = WACOM_TABLETS.lock();
    tablets.push(WacomTablet {
        device_id,
        name: String::from(name),
        width: 21600, // Standard Intuos resolution
        height: 13500,
        max_pressure: 8192,
        has_tilt: true,
    });
    serial_println!("[Wacom] Tablet '{}' probed (8192 levels)", name);
    true
}

/// Get latest tablet event (from HID reports)
pub fn wacom_poll(_tablet_idx: usize) -> Option<TabletEvent> {
    Some(TabletEvent {
        x: 0,
        y: 0,
        pressure: 0,
        tilt_x: 0,
        tilt_y: 0,
        button: 0,
        in_range: false,
        touching: false,
        eraser: false,
    })
}
