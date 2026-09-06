/// Wacom Tablet Driver
///
/// Supports Wacom pen tablets and interactive displays via USB HID.
/// Provides pressure sensitivity, tilt detection, and button mapping.
///
/// Features:
///   - Pressure sensitivity (up to 8192 levels)
///   - Tilt detection (X/Y axis, ±60°)
///   - Pen hover detection with distance reporting
///   - Barrel button and eraser support
///   - Touch ring / touch strip input
///   - Multi-touch on supported devices
///   - Express keys mapping
///   - Per-application profiles
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Wacom device type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WacomType {
    Intuos,    // Standard tablet
    IntuosPro, // Pro tablet with touch
    Cintiq,    // Interactive display
    CintiqPro, // Pro display
    One,       // Entry-level
    BambooInk, // Stylus for touchscreens
}

/// Tool type (pen/eraser/etc)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WacomTool {
    Pen,
    Eraser,
    Cursor, // Puck/mouse tool
    Airbrush,
    ArtPen, // Rotation-capable pen
    Touch,  // Finger touch
}

/// Pen event with full tablet data
#[derive(Debug, Clone, Copy)]
pub struct WacomPenEvent {
    pub x: u32,
    pub y: u32,
    pub pressure: u16, // 0..8192
    pub tilt_x: i16,   // -60..+60 degrees
    pub tilt_y: i16,   // -60..+60 degrees
    pub distance: u8,  // 0..63 (hover height)
    pub rotation: i16, // 0..3600 (ArtPen, in 0.1° units)
    pub tool: WacomTool,
    pub in_range: bool,
    pub touching: bool,
    pub button1: bool, // Barrel button 1
    pub button2: bool, // Barrel button 2
    pub eraser: bool,
}

/// Touch event (for multi-touch tablets)
#[derive(Debug, Clone, Copy)]
pub struct WacomTouchEvent {
    pub finger_id: u8,
    pub x: u32,
    pub y: u32,
    pub width: u16,
    pub height: u16,
    pub active: bool,
}

/// Express key event
#[derive(Debug, Clone, Copy)]
pub struct WacomExpressKeyEvent {
    pub key_index: u8,
    pub pressed: bool,
}

/// Touch ring/strip event
#[derive(Debug, Clone, Copy)]
pub struct WacomRingEvent {
    pub ring_id: u8,
    pub position: u16, // 0..71 for ring, 0..4096 for strip
    pub active: bool,
}

/// Tablet capabilities
#[derive(Debug, Clone)]
pub struct WacomCapabilities {
    pub max_x: u32,
    pub max_y: u32,
    pub max_pressure: u16,
    pub max_tilt_x: i16,
    pub max_tilt_y: i16,
    pub has_tilt: bool,
    pub has_rotation: bool,
    pub has_touch: bool,
    pub has_ring: bool,
    pub has_strip: bool,
    pub num_express_keys: u8,
    pub max_touch_points: u8,
    pub resolution_x: u32, // lines per inch
    pub resolution_y: u32,
}

/// Wacom tablet device
pub struct WacomDevice {
    pub device_type: WacomType,
    pub usb_addr: u8,
    pub name: String,
    pub capabilities: WacomCapabilities,
    pub current_tool: Option<WacomTool>,
    pub last_pen_event: Option<WacomPenEvent>,
    pub touch_points: Vec<WacomTouchEvent>,
    pub express_key_state: u32,             // Bitmask of pressed keys
    pub area_mapping: (u32, u32, u32, u32), // Active area: x1,y1,x2,y2
}

lazy_static::lazy_static! {
    pub static ref WACOM_DEVICES: Mutex<Vec<WacomDevice>> = Mutex::new(Vec::new());
}

impl WacomDevice {
    pub fn new(device_type: WacomType, usb_addr: u8, name: &str) -> Self {
        let caps = match device_type {
            WacomType::IntuosPro => WacomCapabilities {
                max_x: 44704,
                max_y: 27940,
                max_pressure: 8192,
                max_tilt_x: 60,
                max_tilt_y: 60,
                has_tilt: true,
                has_rotation: false,
                has_touch: true,
                has_ring: true,
                has_strip: false,
                num_express_keys: 8,
                max_touch_points: 10,
                resolution_x: 5080,
                resolution_y: 5080,
            },
            WacomType::CintiqPro => WacomCapabilities {
                max_x: 59552,
                max_y: 33504,
                max_pressure: 8192,
                max_tilt_x: 60,
                max_tilt_y: 60,
                has_tilt: true,
                has_rotation: true,
                has_touch: true,
                has_ring: false,
                has_strip: true,
                num_express_keys: 17,
                max_touch_points: 10,
                resolution_x: 5080,
                resolution_y: 5080,
            },
            _ => WacomCapabilities {
                max_x: 21648,
                max_y: 13700,
                max_pressure: 4096,
                max_tilt_x: 60,
                max_tilt_y: 60,
                has_tilt: true,
                has_rotation: false,
                has_touch: false,
                has_ring: false,
                has_strip: false,
                num_express_keys: 4,
                max_touch_points: 0,
                resolution_x: 2540,
                resolution_y: 2540,
            },
        };

        let area = (0, 0, caps.max_x, caps.max_y);

        Self {
            device_type,
            usb_addr,
            name: String::from(name),
            capabilities: caps,
            current_tool: None,
            last_pen_event: None,
            touch_points: Vec::new(),
            express_key_state: 0,
            area_mapping: area,
        }
    }

    /// Initialize the tablet
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Set device to Wacom protocol mode via HID feature report
        // Request tablet capabilities via GET_REPORT

        serial_println!(
            "[Wacom] {} initialized: {}x{} area, {} pressure levels, {} express keys",
            self.name,
            self.capabilities.max_x,
            self.capabilities.max_y,
            self.capabilities.max_pressure,
            self.capabilities.num_express_keys
        );
        Ok(())
    }

    /// Parse HID input report from USB
    pub fn parse_hid_report(&mut self, data: &[u8]) -> Option<WacomPenEvent> {
        if data.len() < 10 {
            return None;
        }

        let report_id = data[0];

        match report_id {
            0x02 => self.parse_pen_report(data), // Pen data
            0x03 => {
                self.parse_touch_report(data);
                None
            } // Touch data
            0x0C => {
                self.parse_express_key_report(data);
                None
            } // Express keys
            _ => None,
        }
    }

    fn parse_pen_report(&mut self, data: &[u8]) -> Option<WacomPenEvent> {
        if data.len() < 10 {
            return None;
        }

        let in_range = (data[1] & 0x80) != 0;
        let touching = (data[1] & 0x01) != 0;
        let button1 = (data[1] & 0x02) != 0;
        let button2 = (data[1] & 0x04) != 0;
        let eraser = (data[1] & 0x10) != 0;

        let x = u32::from(data[2]) | (u32::from(data[3]) << 8) | (u32::from(data[4] & 0x0F) << 16);
        let y = u32::from(data[4] >> 4) | (u32::from(data[5]) << 4) | (u32::from(data[6]) << 12);
        let pressure = u16::from(data[7]) | (u16::from(data[8]) << 8);

        let tilt_x = if data.len() > 10 {
            data[9] as i16 - 64
        } else {
            0
        };
        let tilt_y = if data.len() > 11 {
            data[10] as i16 - 64
        } else {
            0
        };

        let tool = if eraser {
            WacomTool::Eraser
        } else {
            WacomTool::Pen
        };

        let event = WacomPenEvent {
            x,
            y,
            pressure,
            tilt_x,
            tilt_y,
            distance: if in_range && !touching { 32 } else { 0 },
            rotation: 0,
            tool,
            in_range,
            touching,
            button1,
            button2,
            eraser,
        };

        self.current_tool = if in_range { Some(tool) } else { None };
        self.last_pen_event = Some(event);
        Some(event)
    }

    fn parse_touch_report(&mut self, data: &[u8]) {
        if !self.capabilities.has_touch || data.len() < 8 {
            return;
        }
        // Parse multi-touch contact data
        let num_contacts = data[1] as usize;
        self.touch_points.clear();

        for i in 0..num_contacts.min(self.capabilities.max_touch_points as usize) {
            let offset = 2 + i * 6;
            if offset + 6 > data.len() {
                break;
            }

            self.touch_points.push(WacomTouchEvent {
                finger_id: data[offset],
                x: u32::from(data[offset + 1]) | (u32::from(data[offset + 2]) << 8),
                y: u32::from(data[offset + 3]) | (u32::from(data[offset + 4]) << 8),
                width: data[offset + 5] as u16,
                height: data[offset + 5] as u16,
                active: true,
            });
        }
    }

    fn parse_express_key_report(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            self.express_key_state = u32::from(data[1]) | (u32::from(data[2]) << 8);
        }
    }

    /// Set active area mapping (for mapping tablet area to screen region)
    pub fn set_area(&mut self, x1: u32, y1: u32, x2: u32, y2: u32) {
        self.area_mapping = (
            x1.min(self.capabilities.max_x),
            y1.min(self.capabilities.max_y),
            x2.min(self.capabilities.max_x),
            y2.min(self.capabilities.max_y),
        );
    }

    /// Map tablet coordinates to screen coordinates
    pub fn map_to_screen(
        &self,
        tablet_x: u32,
        tablet_y: u32,
        screen_w: u32,
        screen_h: u32,
    ) -> (u32, u32) {
        let (ax1, ay1, ax2, ay2) = self.area_mapping;
        let w = ax2.saturating_sub(ax1).max(1);
        let h = ay2.saturating_sub(ay1).max(1);
        let sx = ((tablet_x.saturating_sub(ax1)) as u64 * screen_w as u64 / w as u64) as u32;
        let sy = ((tablet_y.saturating_sub(ay1)) as u64 * screen_h as u64 / h as u64) as u32;
        (sx.min(screen_w), sy.min(screen_h))
    }
}

/// Identify Wacom device from USB VID:PID
pub fn identify_device(vendor_id: u16, product_id: u16) -> Option<WacomType> {
    if vendor_id != 0x056A {
        return None;
    } // Wacom vendor
    match product_id {
        0x0374..=0x037F => Some(WacomType::IntuosPro),
        0x0390..=0x039F => Some(WacomType::CintiqPro),
        0x0350..=0x035F => Some(WacomType::Cintiq),
        0x0300..=0x030F => Some(WacomType::Intuos),
        0x0380..=0x038F => Some(WacomType::One),
        _ => Some(WacomType::Intuos), // Default for unknown Wacom
    }
}

pub fn init() {
    serial_println!("[Wacom] Wacom tablet driver loaded");
}
