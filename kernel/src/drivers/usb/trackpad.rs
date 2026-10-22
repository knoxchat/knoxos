//! Trackpad HID multitouch report parse and two-finger gesture recognition.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Trackpad gesture type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackpadGesture {
    None,
    TwoFingerScroll,
    TwoFingerPinch,
    ThreeFingerSwipe,
    FourFingerSwipe,
    TwoFingerRotate,
    Tap,
    DoubleTap,
    TwoFingerTap,
    ThreeFingerTap,
}

/// Trackpad touch point
#[derive(Debug, Clone, Copy, Default)]
pub struct TouchPoint {
    pub id: u8,
    pub x: u16,
    pub y: u16,
    pub pressure: u8,
    pub width: u8,
    pub height: u8,
}

/// Trackpad device
#[derive(Debug, Clone)]
pub struct TrackpadDevice {
    pub device_id: u8,
    pub name: String,
    pub max_fingers: u8,
    pub width: u16,
    pub height: u16,
    pub multitouch: bool,
}

lazy_static::lazy_static! {
    static ref TRACKPADS: Mutex<Vec<TrackpadDevice>> = Mutex::new(Vec::new());
    static ref LAST_POINTS: Mutex<Vec<TouchPoint>> = Mutex::new(Vec::new());
}

/// Probe trackpad device (USB or I2C HID)
pub fn trackpad_probe(device_id: u8, name: &str, max_fingers: u8) -> bool {
    let mut trackpads = TRACKPADS.lock();
    trackpads.push(TrackpadDevice {
        device_id,
        name: String::from(name),
        max_fingers,
        width: 4096,
        height: 2048,
        multitouch: max_fingers > 1,
    });
    serial_println!(
        "[Trackpad] '{}' probed ({}-finger multitouch)",
        name,
        max_fingers
    );
    true
}

/// HID multitouch: `[count, (id, tip, x_lo, x_hi, y_lo, y_hi) * count]`.
pub fn parse_mt_report(report: &[u8]) -> Option<Vec<TouchPoint>> {
    if report.is_empty() {
        return None;
    }
    let count = report[0] as usize;
    if count == 0 || count > 5 {
        return None;
    }
    let need = 1 + count * 6;
    if report.len() < need {
        return None;
    }
    let mut pts = Vec::with_capacity(count);
    for i in 0..count {
        let o = 1 + i * 6;
        let tip = report[o + 1] & 1 != 0;
        pts.push(TouchPoint {
            id: report[o],
            x: u16::from_le_bytes([report[o + 2], report[o + 3]]),
            y: u16::from_le_bytes([report[o + 4], report[o + 5]]),
            pressure: if tip { 128 } else { 0 },
            width: 0,
            height: 0,
        });
    }
    Some(pts)
}

fn dist(a: &TouchPoint, b: &TouchPoint) -> i32 {
    let dx = a.x as i32 - b.x as i32;
    let dy = a.y as i32 - b.y as i32;
    dx * dx + dy * dy
}

/// Recognize gesture from contact deltas, not just finger count.
pub fn trackpad_recognize_gesture(
    points: &[TouchPoint],
    prev_points: &[TouchPoint],
) -> TrackpadGesture {
    match points.len() {
        0 => TrackpadGesture::None,
        1 => TrackpadGesture::Tap,
        2 => {
            if prev_points.len() != 2 {
                return TrackpadGesture::TwoFingerScroll;
            }
            let dy0 = points[0].y as i32 - prev_points[0].y as i32;
            let dy1 = points[1].y as i32 - prev_points[1].y as i32;
            let dx0 = points[0].x as i32 - prev_points[0].x as i32;
            let dx1 = points[1].x as i32 - prev_points[1].x as i32;
            let mean_dy = (dy0 + dy1) / 2;
            let mean_dx = (dx0 + dx1) / 2;
            let d_now = dist(&points[0], &points[1]);
            let d_prev = dist(&prev_points[0], &prev_points[1]);
            if (d_now - d_prev).unsigned_abs() > 6400 {
                TrackpadGesture::TwoFingerPinch
            } else if mean_dy.abs() > 8 || mean_dx.abs() > 8 {
                TrackpadGesture::TwoFingerScroll
            } else {
                TrackpadGesture::TwoFingerTap
            }
        }
        3 => TrackpadGesture::ThreeFingerSwipe,
        4 => TrackpadGesture::FourFingerSwipe,
        _ => TrackpadGesture::None,
    }
}

pub fn trackpad_handle_report(report: &[u8]) -> Option<TrackpadGesture> {
    let points = parse_mt_report(report)?;
    let mut prev = LAST_POINTS.lock();
    let gesture = trackpad_recognize_gesture(&points, &prev);
    *prev = points;
    Some(gesture)
}

pub const GATE_USB_TRACKPAD_MARKER: &str = "GATE_USB trackpad";

pub fn trackpad_self_test() -> bool {
    let _ = trackpad_probe(1, "Synaptics", 4);
    // Two contacts, Y drops by 100 → two-finger scroll.
    let prev = [
        2u8, 0, 1, 0x00, 0x04, 0xE8, 0x03, 1, 1, 0x00, 0x08, 0xE8, 0x03,
    ];
    let now = [
        2u8, 0, 1, 0x00, 0x04, 0x84, 0x03, 1, 1, 0x00, 0x08, 0x84, 0x03,
    ];
    if trackpad_handle_report(&prev) != Some(TrackpadGesture::TwoFingerScroll) {
        serial_println!("[Trackpad] GATE_USB FAILED: first report");
        return false;
    }
    match trackpad_handle_report(&now) {
        Some(TrackpadGesture::TwoFingerScroll) => {
            serial_println!(
                "[Trackpad] {} (2-finger HID ΔY scroll)",
                GATE_USB_TRACKPAD_MARKER
            );
            true
        }
        other => {
            serial_println!("[Trackpad] GATE_USB FAILED gesture={:?}", other);
            false
        }
    }
}
