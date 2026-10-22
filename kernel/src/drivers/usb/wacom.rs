//! Wacom HID pen report: in-range, tip, X/Y, pressure, tilt.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Tablet pen event
#[derive(Debug, Clone, Copy, Default)]
pub struct TabletEvent {
    pub x: u32,
    pub y: u32,
    pub pressure: u16,
    pub tilt_x: i16,
    pub tilt_y: i16,
    pub button: u8,
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
    last: TabletEvent,
}

lazy_static::lazy_static! {
    static ref WACOM_TABLETS: Mutex<Vec<WacomTablet>> = Mutex::new(Vec::new());
}

/// Probe Wacom tablet (USB HID vendor 0x056A)
pub fn wacom_probe(device_id: u8, name: &str) -> bool {
    let mut tablets = WACOM_TABLETS.lock();
    tablets.push(WacomTablet {
        device_id,
        name: String::from(name),
        width: 21600,
        height: 13500,
        max_pressure: 8192,
        has_tilt: true,
        last: TabletEvent::default(),
    });
    serial_println!("[Wacom] Tablet '{}' probed (8192 levels)", name);
    true
}

/// Intuos-style 10-byte HID report:
/// `[id, status, x_lo, x_hi, y_lo, y_hi, p_lo, p_hi, tilt_x, tilt_y]`
/// status: bit0 in-range, bit1 tip, bit2 eraser, bits 3–4 barrel.
pub fn parse_wacom_report(report: &[u8]) -> Option<TabletEvent> {
    if report.len() < 10 {
        return None;
    }
    let status = report[1];
    Some(TabletEvent {
        x: u16::from_le_bytes([report[2], report[3]]) as u32,
        y: u16::from_le_bytes([report[4], report[5]]) as u32,
        pressure: u16::from_le_bytes([report[6], report[7]]),
        tilt_x: report[8] as i8 as i16,
        tilt_y: report[9] as i8 as i16,
        button: (status >> 3) & 0x03,
        in_range: status & 0x01 != 0,
        touching: status & 0x02 != 0,
        eraser: status & 0x04 != 0,
    })
}

pub fn wacom_handle_report(tablet_idx: usize, report: &[u8]) -> bool {
    let parsed = match parse_wacom_report(report) {
        Some(p) => p,
        None => return false,
    };
    let mut tablets = WACOM_TABLETS.lock();
    if let Some(slot) = tablets.get_mut(tablet_idx) {
        slot.last = parsed;
        true
    } else {
        false
    }
}

pub fn wacom_poll(tablet_idx: usize) -> Option<TabletEvent> {
    WACOM_TABLETS.lock().get(tablet_idx).map(|t| t.last)
}

pub const GATE_USB_WACOM_MARKER: &str = "GATE_USB wacom";

pub fn wacom_self_test() -> bool {
    let _ = wacom_probe(1, "Intuos");
    // in-range + tip, x=1000, y=2000, pressure=4096, tilt +10/-5
    let report = [
        0x02u8,
        0x03,
        0xE8,
        0x03,
        0xD0,
        0x07,
        0x00,
        0x10,
        10u8,
        (-5i8) as u8,
    ];
    if !wacom_handle_report(0, &report) {
        serial_println!("[Wacom] GATE_USB FAILED: handle");
        return false;
    }
    let Some(ev) = wacom_poll(0) else {
        serial_println!("[Wacom] GATE_USB FAILED: poll");
        return false;
    };
    let ok = ev.in_range
        && ev.touching
        && ev.x == 1000
        && ev.y == 2000
        && ev.pressure == 4096
        && ev.tilt_x == 10
        && ev.tilt_y == -5;
    if ok {
        serial_println!(
            "[Wacom] {} (x={} y={} p={})",
            GATE_USB_WACOM_MARKER,
            ev.x,
            ev.y,
            ev.pressure
        );
    } else {
        serial_println!(
            "[Wacom] GATE_USB FAILED x={} y={} p={} tx={} ty={}",
            ev.x,
            ev.y,
            ev.pressure,
            ev.tilt_x,
            ev.tilt_y
        );
    }
    ok
}
