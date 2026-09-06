// SPDX-License-Identifier: MIT
//! Touchscreen / multitouch input driver (item 6.11)
//!
//! Handles multitouch input events from touchscreens, trackpads, and
//! VirtIO input devices. Supports gestures like tap, pinch, swipe.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Maximum simultaneous touch points
const MAX_TOUCH_POINTS: usize = 10;

/// Touch point state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    /// Finger touched the screen
    Started,
    /// Finger moved on the screen
    Moved,
    /// Finger lifted from the screen
    Ended,
    /// Touch was cancelled (e.g., palm rejection)
    Cancelled,
}

/// A single touch point
#[derive(Debug, Clone, Copy)]
pub struct TouchPoint {
    /// Unique ID for this finger/touch
    pub id: u64,
    /// X position (0.0 - 1.0, normalized)
    pub x: f32,
    /// Y position (0.0 - 1.0, normalized)
    pub y: f32,
    /// Pressure (0.0 - 1.0)
    pub pressure: f32,
    /// Touch phase
    pub phase: TouchPhase,
    /// Major axis of contact ellipse (pixels)
    pub major_axis: f32,
    /// Minor axis of contact ellipse (pixels)
    pub minor_axis: f32,
}

/// Multi-touch event containing all active touch points
#[derive(Debug, Clone)]
pub struct TouchEvent {
    pub timestamp: u64,
    pub points: Vec<TouchPoint>,
}

/// Recognized gesture types
#[derive(Debug, Clone)]
pub enum Gesture {
    /// Single tap at position
    Tap { x: f32, y: f32 },
    /// Double tap at position
    DoubleTap { x: f32, y: f32 },
    /// Long press at position
    LongPress { x: f32, y: f32 },
    /// Swipe with direction and velocity
    Swipe { dx: f32, dy: f32, velocity: f32 },
    /// Pinch with scale factor (>1 = zoom in, <1 = zoom out)
    Pinch {
        scale: f32,
        center_x: f32,
        center_y: f32,
    },
    /// Rotation with angle in radians
    Rotate {
        angle: f32,
        center_x: f32,
        center_y: f32,
    },
    /// Two-finger scroll
    TwoFingerScroll { dx: f32, dy: f32 },
    /// Three-finger swipe (workspace switch)
    ThreeFingerSwipe { dx: f32, dy: f32 },
}

/// Gesture recognizer state
struct GestureRecognizer {
    /// Active touch points
    active_points: BTreeMap<u64, TouchHistory>,
    /// Last tap time for double-tap detection
    last_tap_time: u64,
    /// Last tap position
    last_tap_pos: (f32, f32),
    /// Touch start time for long-press detection
    touch_start_time: u64,
    /// Initial pinch distance
    initial_pinch_dist: f32,
    /// Initial angle between two fingers (for rotation detection)
    initial_angle: f32,
    /// Pending gestures
    pending_gestures: Vec<Gesture>,
}

struct TouchHistory {
    start_x: f32,
    start_y: f32,
    current_x: f32,
    current_y: f32,
    prev_x: f32,
    prev_y: f32,
    start_time: u64,
}

lazy_static::lazy_static! {
    static ref TOUCH_STATE: Mutex<TouchState> = Mutex::new(TouchState::new());
    static ref GESTURE_ENGINE: Mutex<GestureRecognizer> = Mutex::new(GestureRecognizer::new());
}

static EVENTS_PROCESSED: AtomicU64 = AtomicU64::new(0);
static GESTURES_RECOGNIZED: AtomicU64 = AtomicU64::new(0);

/// Global touch state
struct TouchState {
    active_touches: BTreeMap<u64, TouchPoint>,
    screen_width: u32,
    screen_height: u32,
    enabled: bool,
    next_id: u64,
}

impl TouchState {
    fn new() -> Self {
        Self {
            active_touches: BTreeMap::new(),
            screen_width: 1920,
            screen_height: 1080,
            enabled: false,
            next_id: 1,
        }
    }
}

impl GestureRecognizer {
    fn new() -> Self {
        Self {
            active_points: BTreeMap::new(),
            last_tap_time: 0,
            last_tap_pos: (0.0, 0.0),
            touch_start_time: 0,
            initial_pinch_dist: 0.0,
            initial_angle: 0.0,
            pending_gestures: Vec::new(),
        }
    }

    fn process_event(&mut self, event: &TouchEvent) {
        for point in &event.points {
            match point.phase {
                TouchPhase::Started => {
                    self.active_points.insert(
                        point.id,
                        TouchHistory {
                            start_x: point.x,
                            start_y: point.y,
                            current_x: point.x,
                            current_y: point.y,
                            prev_x: point.x,
                            prev_y: point.y,
                            start_time: event.timestamp,
                        },
                    );
                    if self.active_points.len() == 1 {
                        self.touch_start_time = event.timestamp;
                    }
                    if self.active_points.len() == 2 {
                        self.initial_pinch_dist = self.current_pinch_distance();
                        self.initial_angle = self.current_angle();
                    }
                }
                TouchPhase::Moved => {
                    if let Some(h) = self.active_points.get_mut(&point.id) {
                        h.prev_x = h.current_x;
                        h.prev_y = h.current_y;
                        h.current_x = point.x;
                        h.current_y = point.y;
                    }
                    let n = self.active_points.len();

                    // Two-finger gestures: pinch, rotate, two-finger scroll
                    if n == 2 {
                        if self.initial_pinch_dist > 0.01 {
                            let current_dist = self.current_pinch_distance();
                            let scale = current_dist / self.initial_pinch_dist;

                            // Pinch zoom
                            if (scale - 1.0).abs() > 0.05 {
                                let (cx, cy) = self.pinch_center();
                                self.pending_gestures.push(Gesture::Pinch {
                                    scale,
                                    center_x: cx,
                                    center_y: cy,
                                });
                            }

                            // Rotation detection
                            let angle = self.current_angle();
                            let delta_angle = angle - self.initial_angle;
                            if delta_angle.abs() > 0.05 {
                                let (cx, cy) = self.pinch_center();
                                self.pending_gestures.push(Gesture::Rotate {
                                    angle: delta_angle,
                                    center_x: cx,
                                    center_y: cy,
                                });
                            }
                        }

                        // Two-finger scroll: both fingers moving in same direction
                        let pts: Vec<&TouchHistory> = self.active_points.values().collect();
                        let dx0 = pts[0].current_x - pts[0].prev_x;
                        let dy0 = pts[0].current_y - pts[0].prev_y;
                        let dx1 = pts[1].current_x - pts[1].prev_x;
                        let dy1 = pts[1].current_y - pts[1].prev_y;
                        // If both fingers move in roughly the same direction
                        if (dx0 * dx1 + dy0 * dy1) > 0.0 {
                            let avg_dx = (dx0 + dx1) / 2.0;
                            let avg_dy = (dy0 + dy1) / 2.0;
                            if avg_dx.abs() > 0.001 || avg_dy.abs() > 0.001 {
                                self.pending_gestures.push(Gesture::TwoFingerScroll {
                                    dx: avg_dx,
                                    dy: avg_dy,
                                });
                            }
                        }
                    }

                    // Three-finger swipe (workspace switch)
                    if n == 3 {
                        let pts: Vec<&TouchHistory> = self.active_points.values().collect();
                        let avg_dx = (pts[0].current_x - pts[0].start_x + pts[1].current_x
                            - pts[1].start_x
                            + pts[2].current_x
                            - pts[2].start_x)
                            / 3.0;
                        let avg_dy = (pts[0].current_y - pts[0].start_y + pts[1].current_y
                            - pts[1].start_y
                            + pts[2].current_y
                            - pts[2].start_y)
                            / 3.0;
                        if avg_dx.abs() > 0.08 || avg_dy.abs() > 0.08 {
                            self.pending_gestures.push(Gesture::ThreeFingerSwipe {
                                dx: avg_dx,
                                dy: avg_dy,
                            });
                        }
                    }
                }
                TouchPhase::Ended => {
                    if let Some(h) = self.active_points.remove(&point.id) {
                        let dx = h.current_x - h.start_x;
                        let dy = h.current_y - h.start_y;
                        let dist = libm::sqrtf(dx * dx + dy * dy);
                        let duration = event.timestamp.saturating_sub(h.start_time);

                        if self.active_points.is_empty() {
                            if dist < 0.02 {
                                // Tap or double-tap
                                if duration < 500 {
                                    let time_since_last =
                                        event.timestamp.saturating_sub(self.last_tap_time);
                                    let pos_dist = {
                                        let tdx = h.start_x - self.last_tap_pos.0;
                                        let tdy = h.start_y - self.last_tap_pos.1;
                                        libm::sqrtf(tdx * tdx + tdy * tdy)
                                    };
                                    if time_since_last < 300 && pos_dist < 0.05 {
                                        self.pending_gestures.push(Gesture::DoubleTap {
                                            x: h.start_x,
                                            y: h.start_y,
                                        });
                                    } else {
                                        self.pending_gestures.push(Gesture::Tap {
                                            x: h.start_x,
                                            y: h.start_y,
                                        });
                                    }
                                    self.last_tap_time = event.timestamp;
                                    self.last_tap_pos = (h.start_x, h.start_y);
                                } else if duration > 500 {
                                    self.pending_gestures.push(Gesture::LongPress {
                                        x: h.start_x,
                                        y: h.start_y,
                                    });
                                }
                            } else if dist > 0.05 {
                                // Swipe
                                let velocity = dist / (duration as f32 / 1000.0).max(0.001);
                                self.pending_gestures
                                    .push(Gesture::Swipe { dx, dy, velocity });
                            }
                        }
                    }
                }
                TouchPhase::Cancelled => {
                    self.active_points.remove(&point.id);
                }
            }
        }
    }

    /// Compute angle between two active fingers in radians
    fn current_angle(&self) -> f32 {
        let points: Vec<&TouchHistory> = self.active_points.values().collect();
        if points.len() >= 2 {
            let dx = points[1].current_x - points[0].current_x;
            let dy = points[1].current_y - points[0].current_y;
            libm::atan2f(dy, dx)
        } else {
            0.0
        }
    }

    fn current_pinch_distance(&self) -> f32 {
        let points: Vec<&TouchHistory> = self.active_points.values().collect();
        if points.len() >= 2 {
            let dx = points[1].current_x - points[0].current_x;
            let dy = points[1].current_y - points[0].current_y;
            libm::sqrtf(dx * dx + dy * dy)
        } else {
            0.0
        }
    }

    fn pinch_center(&self) -> (f32, f32) {
        let points: Vec<&TouchHistory> = self.active_points.values().collect();
        if points.len() >= 2 {
            (
                (points[0].current_x + points[1].current_x) / 2.0,
                (points[0].current_y + points[1].current_y) / 2.0,
            )
        } else {
            (0.5, 0.5)
        }
    }

    fn take_gestures(&mut self) -> Vec<Gesture> {
        core::mem::take(&mut self.pending_gestures)
    }
}

/// Inject a single touch point as a TouchEvent into the processing pipeline
pub fn inject_touch(contact_id: u64, nx: f32, ny: f32, pressure: f32, phase: TouchPhase) {
    let event = TouchEvent {
        timestamp: crate::clock::get_ticks(),
        points: alloc::vec![TouchPoint {
            id: contact_id,
            x: nx,
            y: ny,
            pressure,
            phase,
            major_axis: 0.0,
            minor_axis: 0.0,
        }],
    };
    process_touch(event);
}

/// Process a raw touch event
pub fn process_touch(event: TouchEvent) {
    EVENTS_PROCESSED.fetch_add(1, Ordering::Relaxed);

    // Update global state
    {
        let mut state = TOUCH_STATE.lock();
        if !state.enabled {
            return;
        }
        for point in &event.points {
            match point.phase {
                TouchPhase::Started | TouchPhase::Moved => {
                    state.active_touches.insert(point.id, *point);
                }
                TouchPhase::Ended | TouchPhase::Cancelled => {
                    state.active_touches.remove(&point.id);
                }
            }
        }
    }

    // Run gesture recognition
    GESTURE_ENGINE.lock().process_event(&event);
}

/// Poll for recognized gestures
pub fn poll_gestures() -> Vec<Gesture> {
    let gestures = GESTURE_ENGINE.lock().take_gestures();
    if !gestures.is_empty() {
        GESTURES_RECOGNIZED.fetch_add(gestures.len() as u64, Ordering::Relaxed);
    }
    gestures
}

/// Convert absolute touch coordinates to screen pixels
pub fn touch_to_screen(nx: f32, ny: f32) -> (i32, i32) {
    let state = TOUCH_STATE.lock();
    let x = (nx * state.screen_width as f32) as i32;
    let y = (ny * state.screen_height as f32) as i32;
    (x, y)
}

/// Set screen dimensions for coordinate mapping
pub fn set_screen_size(width: u32, height: u32) {
    let mut state = TOUCH_STATE.lock();
    state.screen_width = width;
    state.screen_height = height;
}

/// Get number of active touch points
pub fn active_touch_count() -> usize {
    TOUCH_STATE.lock().active_touches.len()
}

pub fn stats() -> (u64, u64) {
    (
        EVENTS_PROCESSED.load(Ordering::Relaxed),
        GESTURES_RECOGNIZED.load(Ordering::Relaxed),
    )
}

/// Initialize the touchscreen subsystem
pub fn init() {
    let mut state = TOUCH_STATE.lock();
    state.enabled = true;

    // Probe for VirtIO input touchscreen device
    let virtio_count = detect_virtio_input_touch();

    // Probe for USB HID touchscreen devices
    let usb_count = detect_usb_hid_touch();

    crate::serial_println!(
        "[touchscreen] initialized, max_points={}, virtio={}, usb_hid={}",
        MAX_TOUCH_POINTS,
        virtio_count,
        usb_count
    );
}

// ═══════════════════════════════════════════════════════════════════════
// VIRTIO INPUT TOUCHSCREEN DRIVER
// ═══════════════════════════════════════════════════════════════════════

/// VirtIO input event types (Linux input event codes)
const EV_SYN: u16 = 0x00;
const EV_ABS: u16 = 0x03;

/// ABS event codes for multitouch
const ABS_MT_SLOT: u16 = 0x2F;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_PRESSURE: u16 = 0x3A;
const ABS_MT_TOUCH_MAJOR: u16 = 0x30;

/// VirtIO input event (from virtio spec §5.8)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct VirtioInputEvent {
    ev_type: u16,
    code: u16,
    value: u32,
}

/// VirtIO input device state
struct VirtioTouchDevice {
    mmio_base: u64,
    abs_x_max: u32,
    abs_y_max: u32,
    abs_pressure_max: u32,
    current_slot: usize,
    slots: [VirtioTouchSlot; MAX_TOUCH_POINTS],
}

#[derive(Debug, Clone, Copy, Default)]
struct VirtioTouchSlot {
    tracking_id: i32, // -1 = not active
    x: u32,
    y: u32,
    pressure: u32,
    touch_major: u32,
}

lazy_static::lazy_static! {
    static ref VIRTIO_TOUCH: Mutex<Option<VirtioTouchDevice>> = Mutex::new(None);
}

/// Detect VirtIO input touchscreen devices via PCI
fn detect_virtio_input_touch() -> usize {
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
            let device = ((id >> 16) & 0xFFFF) as u16;

            // VirtIO vendor 0x1AF4, input device ID 0x1052
            if vendor == 0x1AF4 && device == 0x1052 {
                // Read subdevice ID to distinguish touch from keyboard/mouse
                let subsys = unsafe {
                    let mut p = Port::<u32>::new(0xCF8);
                    let mut d = Port::<u32>::new(0xCFC);
                    p.write(addr | 0x2C);
                    d.read()
                };
                let _subsys_id = ((subsys >> 16) & 0xFFFF) as u16;

                // Read BAR0
                let bar0 = unsafe {
                    let mut p = Port::<u32>::new(0xCF8);
                    let mut d = Port::<u32>::new(0xCFC);
                    p.write(addr | 0x10);
                    d.read()
                } as u64
                    & !0xF;

                if bar0 != 0 {
                    let touch_dev = VirtioTouchDevice {
                        mmio_base: bar0,
                        abs_x_max: 32767,
                        abs_y_max: 32767,
                        abs_pressure_max: 255,
                        current_slot: 0,
                        slots: [VirtioTouchSlot {
                            tracking_id: -1,
                            ..Default::default()
                        }; MAX_TOUCH_POINTS],
                    };

                    *VIRTIO_TOUCH.lock() = Some(touch_dev);
                    count += 1;
                    crate::serial_println!(
                        "[touchscreen] VirtIO input touch at PCI {:02x}:{:02x}.0, MMIO={:#x}",
                        bus,
                        dev,
                        bar0
                    );
                }
                break;
            }
        }
        if count > 0 {
            break;
        }
    }
    count
}

/// Process a VirtIO input event into touch events
pub fn process_virtio_event(ev: &[u8]) {
    if ev.len() < 8 {
        return;
    }

    let event = VirtioInputEvent {
        ev_type: u16::from_le_bytes([ev[0], ev[1]]),
        code: u16::from_le_bytes([ev[2], ev[3]]),
        value: u32::from_le_bytes([ev[4], ev[5], ev[6], ev[7]]),
    };

    let mut vtouch = VIRTIO_TOUCH.lock();
    let dev = match vtouch.as_mut() {
        Some(d) => d,
        None => return,
    };

    match event.ev_type {
        EV_ABS => match event.code {
            ABS_MT_SLOT => {
                dev.current_slot = (event.value as usize).min(MAX_TOUCH_POINTS - 1);
            }
            ABS_MT_TRACKING_ID => {
                dev.slots[dev.current_slot].tracking_id = event.value as i32;
            }
            ABS_MT_POSITION_X => {
                dev.slots[dev.current_slot].x = event.value;
            }
            ABS_MT_POSITION_Y => {
                dev.slots[dev.current_slot].y = event.value;
            }
            ABS_MT_PRESSURE => {
                dev.slots[dev.current_slot].pressure = event.value;
            }
            ABS_MT_TOUCH_MAJOR => {
                dev.slots[dev.current_slot].touch_major = event.value;
            }
            _ => {}
        },
        EV_SYN => {
            // SYN_REPORT: commit all pending slot changes as touch events
            for i in 0..MAX_TOUCH_POINTS {
                let slot = &dev.slots[i];
                if slot.tracking_id >= 0 {
                    let nx = slot.x as f32 / dev.abs_x_max as f32;
                    let ny = slot.y as f32 / dev.abs_y_max as f32;
                    let np = slot.pressure as f32 / dev.abs_pressure_max.max(1) as f32;
                    let tid = slot.tracking_id as u64;

                    // Drop the lock before calling inject_touch
                    drop(vtouch);
                    inject_touch(tid, nx, ny, np, TouchPhase::Moved);
                    return; // Re-lock needed, simplified for now
                }
            }
        }
        _ => {}
    }
}

/// VirtIO input IRQ handler — reads events from the virtqueue
pub fn virtio_touch_irq_handler() {
    // In a real implementation, this reads VirtioInputEvent structs from
    // the RX virtqueue and calls process_virtio_event() for each one
    let vtouch = VIRTIO_TOUCH.lock();
    if vtouch.is_none() {}
    // Read ISR to acknowledge, then poll used ring for completed buffers
}

// ═══════════════════════════════════════════════════════════════════════
// USB HID TOUCHSCREEN SUPPORT
// ═══════════════════════════════════════════════════════════════════════

/// USB HID usage page for Digitizer
const HID_USAGE_PAGE_DIGITIZER: u16 = 0x0D;
/// USB HID usage: Touch Screen
const HID_USAGE_TOUCH_SCREEN: u16 = 0x04;
/// USB HID usage: Finger
const HID_USAGE_FINGER: u16 = 0x22;
/// USB HID usage: Tip Switch
const HID_USAGE_TIP_SWITCH: u16 = 0x42;
/// USB HID usage: Contact ID
const HID_USAGE_CONTACT_ID: u16 = 0x51;
/// USB HID usage: Contact Count
const HID_USAGE_CONTACT_COUNT: u16 = 0x54;

/// Detect USB HID touchscreen devices
fn detect_usb_hid_touch() -> usize {
    // Scan for USB HID devices with touchscreen usage page
    let usb_controllers = crate::pcie_ecam::find_by_class(0x0C, 0x03);
    let mut count = 0;

    for _ctrl in &usb_controllers {
        // In QEMU with -device usb-tablet or USB touchscreen:
        // The XHCI driver enumerates USB devices and checks HID report descriptors
        // for Usage Page 0x0D (Digitizer) / Usage 0x04 (Touch Screen)
        // For now, register as detected if any USB controllers exist
    }

    if !usb_controllers.is_empty() {
        crate::serial_println!(
            "[touchscreen] {} USB controller(s) available for HID touchscreen",
            usb_controllers.len()
        );
    }
    count
}

/// Parse a USB HID multitouch report
pub fn parse_hid_touch_report(report: &[u8]) {
    // Standard multitouch HID report format:
    // Byte 0: Report ID
    // Per-contact (repeated N times):
    //   Byte: Tip Switch (bit 0) | In Range (bit 1)
    //   Byte: Contact ID
    //   Word LE: X coordinate
    //   Word LE: Y coordinate
    // Final bytes: Contact Count, Scan Time

    if report.len() < 8 {
        return;
    }

    let _report_id = report[0];
    let contact_count = if report.len() > 6 {
        report[report.len() - 2] as usize
    } else {
        1
    };

    let mut offset = 1;
    for _i in 0..contact_count.min(MAX_TOUCH_POINTS) {
        if offset + 5 > report.len() {
            break;
        }

        let tip_switch = report[offset] & 0x01 != 0;
        let contact_id = report[offset + 1] as u64;
        let x = u16::from_le_bytes([report[offset + 2], report[offset + 3]]);
        let y = u16::from_le_bytes([report[offset + 4], report[offset + 5]]);

        let phase = if tip_switch {
            TouchPhase::Moved
        } else {
            TouchPhase::Ended
        };
        let nx = x as f32 / 32767.0;
        let ny = y as f32 / 32767.0;

        inject_touch(
            contact_id,
            nx,
            ny,
            if tip_switch { 1.0 } else { 0.0 },
            phase,
        );
        offset += 6;
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PRECISION TOUCHPAD (PTP) DRIVER
// ═══════════════════════════════════════════════════════════════════════

/// Windows Precision Touchpad HID usage page
const HID_USAGE_PAGE_DIGITIZER_PTP: u16 = 0x0D;
/// PTP usage: Touchpad
const HID_USAGE_TOUCHPAD: u16 = 0x05;
/// PTP usage: Confidence
const HID_USAGE_CONFIDENCE: u16 = 0x47;

/// Precision touchpad configuration
#[derive(Debug, Clone, Copy)]
pub struct TrackpadConfig {
    /// Enable tap-to-click
    pub tap_to_click: bool,
    /// Enable two-finger tap for right-click
    pub two_finger_tap_right_click: bool,
    /// Enable natural (reverse) scrolling
    pub natural_scrolling: bool,
    /// Scroll speed multiplier (0.5 - 3.0)
    pub scroll_speed: f32,
    /// Enable three-finger gestures (workspace switch)
    pub three_finger_gestures: bool,
    /// Enable four-finger gestures (expose/mission control)
    pub four_finger_gestures: bool,
    /// Palm rejection sensitivity (0.0 = off, 1.0 = aggressive)
    pub palm_rejection: f32,
}

impl TrackpadConfig {
    pub fn default_config() -> Self {
        Self {
            tap_to_click: true,
            two_finger_tap_right_click: true,
            natural_scrolling: false,
            scroll_speed: 1.0,
            three_finger_gestures: true,
            four_finger_gestures: true,
            palm_rejection: 0.6,
        }
    }
}

lazy_static::lazy_static! {
    static ref TRACKPAD_CONFIG: Mutex<TrackpadConfig> = Mutex::new(TrackpadConfig::default_config());
}

/// Get current trackpad configuration
pub fn trackpad_config() -> TrackpadConfig {
    *TRACKPAD_CONFIG.lock()
}

/// Update trackpad configuration
pub fn set_trackpad_config(config: TrackpadConfig) {
    *TRACKPAD_CONFIG.lock() = config;
}

/// Process a Precision Touchpad HID report
/// PTP reports contain per-finger data with confidence bits for palm rejection
pub fn parse_ptp_report(report: &[u8]) {
    if report.len() < 10 {
        return;
    }

    let config = TRACKPAD_CONFIG.lock();
    let palm_threshold = config.palm_rejection;
    let natural = config.natural_scrolling;
    drop(config);

    let _report_id = report[0];
    let contact_count = report[1] as usize;
    let _scan_time = u16::from_le_bytes([report[2], report[3]]);

    let mut offset = 4;
    for _ in 0..contact_count.min(MAX_TOUCH_POINTS) {
        if offset + 9 > report.len() {
            break;
        }

        let confidence = report[offset] & 0x01 != 0;
        let tip_switch = report[offset] & 0x02 != 0;
        let contact_id = report[offset + 1] as u64;
        let x = u16::from_le_bytes([report[offset + 2], report[offset + 3]]);
        let y = u16::from_le_bytes([report[offset + 4], report[offset + 5]]);
        let width = u16::from_le_bytes([report[offset + 6], report[offset + 7]]);
        let height = report[offset + 8];

        // Palm rejection: skip contacts with low confidence or large contact area
        if palm_threshold > 0.0 {
            if !confidence {
                offset += 9;
                continue;
            }
            let contact_area = width as f32 * height as f32;
            if contact_area > 2000.0 * (1.0 - palm_threshold) {
                offset += 9;
                continue;
            }
        }

        let phase = if tip_switch {
            TouchPhase::Moved
        } else {
            TouchPhase::Ended
        };
        let nx = x as f32 / 32767.0;
        let mut ny = y as f32 / 32767.0;

        // Natural scrolling inverts the Y axis for scroll-like gestures
        if natural {
            ny = 1.0 - ny;
        }

        inject_touch(
            contact_id,
            nx,
            ny,
            if tip_switch { 1.0 } else { 0.0 },
            phase,
        );
        offset += 9;
    }
}

/// Detect precision touchpad devices on the I2C-HID bus
pub fn detect_precision_touchpad() -> usize {
    // I2C-HID touchpads are common on laptops. They register on the
    // ACPI _HID/CID as PNP0C50 or MSFT0001 (Microsoft PTP).
    // The HID descriptor contains Usage Page 0x0D, Usage 0x05 (Touchpad).
    let mut count = 0;

    // Check for I2C controllers (PCI class 0x0C, subclass 0x80)
    let i2c_controllers = crate::pcie_ecam::find_by_class(0x0C, 0x80);
    for _ctrl in &i2c_controllers {
        // Enumerate I2C-HID devices on each controller
        // Look for HID descriptor with PTP usage page
        count += 0; // Placeholder — real enumeration reads I2C-HID registers
    }

    if !i2c_controllers.is_empty() {
        crate::serial_println!(
            "[touchscreen] {} I2C controller(s) for precision touchpad",
            i2c_controllers.len()
        );
    }
    count
}
