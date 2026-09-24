//! Trackpad probe and multi-finger gesture recognition.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// Trackpad with Gesture Support
// ═══════════════════════════════════════════════════════════════════════

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

/// Recognize gesture from touch points
pub fn trackpad_recognize_gesture(
    points: &[TouchPoint],
    _prev_points: &[TouchPoint],
) -> TrackpadGesture {
    match points.len() {
        0 => TrackpadGesture::None,
        1 => TrackpadGesture::Tap,
        2 => TrackpadGesture::TwoFingerScroll,
        3 => TrackpadGesture::ThreeFingerSwipe,
        4 => TrackpadGesture::FourFingerSwipe,
        _ => TrackpadGesture::None,
    }
}
