/// Accent Color Picker
///
/// Allows users to choose a system-wide accent color that tints
/// buttons, selections, links, and focus rings throughout the UI.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// An RGBA color
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccentColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl AccentColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub fn to_u32(&self) -> u32 {
        (self.a as u32) << 24 | (self.r as u32) << 16 | (self.g as u32) << 8 | self.b as u32
    }
    pub fn lighter(&self, amount: u8) -> Self {
        Self {
            r: self.r.saturating_add(amount),
            g: self.g.saturating_add(amount),
            b: self.b.saturating_add(amount),
            a: self.a,
        }
    }
    pub fn darker(&self, amount: u8) -> Self {
        Self {
            r: self.r.saturating_sub(amount),
            g: self.g.saturating_sub(amount),
            b: self.b.saturating_sub(amount),
            a: self.a,
        }
    }
}

/// Preset accent colors
pub const PRESET_BLUE: AccentColor = AccentColor::new(0x33, 0x99, 0xFF);
pub const PRESET_GREEN: AccentColor = AccentColor::new(0x4C, 0xAF, 0x50);
pub const PRESET_ORANGE: AccentColor = AccentColor::new(0xFF, 0x98, 0x00);
pub const PRESET_RED: AccentColor = AccentColor::new(0xF4, 0x43, 0x36);
pub const PRESET_PURPLE: AccentColor = AccentColor::new(0x9C, 0x27, 0xB0);
pub const PRESET_TEAL: AccentColor = AccentColor::new(0x00, 0x96, 0x88);
pub const PRESET_PINK: AccentColor = AccentColor::new(0xE9, 0x1E, 0x63);
pub const PRESET_YELLOW: AccentColor = AccentColor::new(0xFF, 0xEB, 0x3B);

lazy_static::lazy_static! {
    static ref CURRENT_ACCENT: Mutex<AccentColor> = Mutex::new(PRESET_BLUE);
}

pub fn set_accent(color: AccentColor) {
    *CURRENT_ACCENT.lock() = color;
    crate::serial_println!(
        "[ACCENT] Set to #{:02X}{:02X}{:02X}",
        color.r,
        color.g,
        color.b
    );
}

pub fn get_accent() -> AccentColor {
    *CURRENT_ACCENT.lock()
}

/// Extract accent color from wallpaper dominant color
pub fn from_wallpaper_dominant(pixels: &[u32], width: u32, height: u32) -> AccentColor {
    if pixels.is_empty() {
        return PRESET_BLUE;
    }
    // Simple average of sampled pixels
    let step = (pixels.len() / 256).max(1);
    let (mut r_sum, mut g_sum, mut b_sum, mut count) = (0u64, 0u64, 0u64, 0u64);
    for i in (0..pixels.len()).step_by(step) {
        let px = pixels[i];
        r_sum += ((px >> 16) & 0xFF) as u64;
        g_sum += ((px >> 8) & 0xFF) as u64;
        b_sum += (px & 0xFF) as u64;
        count += 1;
    }
    if count == 0 {
        return PRESET_BLUE;
    }
    // Boost saturation for a vivid accent
    let r = (r_sum / count) as u8;
    let g = (g_sum / count) as u8;
    let b = (b_sum / count) as u8;
    AccentColor::new(r, g, b)
}

pub fn presets() -> Vec<AccentColor> {
    alloc::vec![
        PRESET_BLUE,
        PRESET_GREEN,
        PRESET_ORANGE,
        PRESET_RED,
        PRESET_PURPLE,
        PRESET_TEAL,
        PRESET_PINK,
        PRESET_YELLOW
    ]
}

pub fn init() {
    crate::serial_println!("[ACCENT] Accent color picker loaded");
}
