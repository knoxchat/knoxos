/// Color Picker Utility — screen pixel color sampling and palette management
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// A color value in multiple representations
#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn from_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as u8,
            g: ((hex >> 8) & 0xFF) as u8,
            b: (hex & 0xFF) as u8,
            a: 255,
        }
    }

    pub fn to_hex_string(&self) -> String {
        alloc::format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// Convert to HSL (returns h: 0-360, s: 0-100, l: 0-100)
    pub fn to_hsl(&self) -> (u16, u8, u8) {
        let r = self.r as f32 / 255.0;
        let g = self.g as f32 / 255.0;
        let b = self.b as f32 / 255.0;

        let max = if r > g {
            if r > b { r } else { b }
        } else {
            if g > b { g } else { b }
        };
        let min = if r < g {
            if r < b { r } else { b }
        } else {
            if g < b { g } else { b }
        };
        let delta = max - min;
        let l = (max + min) / 2.0;

        if delta < 0.001 {
            return (0, 0, (l * 100.0) as u8);
        }

        let s = if l < 0.5 {
            delta / (max + min)
        } else {
            delta / (2.0 - max - min)
        };

        let h = if (max - r).abs() < 0.001 {
            ((g - b) / delta) % 6.0
        } else if (max - g).abs() < 0.001 {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        };

        let h = ((h * 60.0) as i16 + 360) % 360;
        (h as u16, (s * 100.0) as u8, (l * 100.0) as u8)
    }

    /// Convert to CSS rgb() string
    pub fn to_css(&self) -> String {
        alloc::format!("rgb({}, {}, {})", self.r, self.g, self.b)
    }
}

/// A named color palette
#[derive(Debug, Clone)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<(String, Color)>,
}

lazy_static::lazy_static! {
    static ref PALETTES: Mutex<Vec<Palette>> = Mutex::new(Vec::new());
    static ref HISTORY: Mutex<Vec<Color>> = Mutex::new(Vec::new());
}

/// Pick a color from screen coordinates (reads framebuffer pixel)
pub fn pick_from_screen(x: u32, y: u32) -> Color {
    serial_println!("[colorpicker] Sampling pixel at ({}, {})", x, y);
    Color::from_rgb(0, 0, 0) // placeholder — actual impl reads framebuffer
}

/// Add a color to the recent history
pub fn add_to_history(color: Color) {
    let mut hist = HISTORY.lock();
    hist.push(color);
    if hist.len() > 50 {
        hist.remove(0);
    }
}

/// Get recent color history
pub fn get_history() -> Vec<Color> {
    HISTORY.lock().clone()
}

/// Create a new palette
pub fn create_palette(name: &str) -> usize {
    let mut palettes = PALETTES.lock();
    palettes.push(Palette {
        name: String::from(name),
        colors: Vec::new(),
    });
    palettes.len() - 1
}

/// Add a color to a palette
pub fn add_to_palette(palette_idx: usize, name: &str, color: Color) -> bool {
    let mut palettes = PALETTES.lock();
    if let Some(p) = palettes.get_mut(palette_idx) {
        p.colors.push((String::from(name), color));
        true
    } else {
        false
    }
}

/// List all palettes
pub fn list_palettes() -> Vec<String> {
    PALETTES.lock().iter().map(|p| p.name.clone()).collect()
}

pub fn init() {
    serial_println!("[colorpicker] Color picker utility initialized");
}
