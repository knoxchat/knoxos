//! Web font registration from downloaded TTF/OTF bytes.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::font::TtfFont;
use super::registry::FONT_REGISTRY;

// ═══════════════════════════════════════════════════════════════════════
// WEB FONT LOADING
// ═══════════════════════════════════════════════════════════════════════

/// A web font descriptor (loaded from CSS @font-face)
#[derive(Clone, Debug)]
pub struct WebFont {
    pub family: String,
    pub style: String, // "normal", "italic"
    pub weight: u16,   // 100-900
    pub data: Vec<u8>, // Raw TTF/OTF data
    pub loaded: bool,
}

lazy_static::lazy_static! {
    static ref WEB_FONTS: Mutex<Vec<WebFont>> = Mutex::new(Vec::new());
}

/// Register a web font from downloaded TTF/OTF data
pub fn register_web_font(family: &str, style: &str, weight: u16, data: Vec<u8>) -> bool {
    // Validate it's a TTF/OTF by checking magic bytes
    if data.len() < 12 {
        return false;
    }
    let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    if magic != 0x00010000 && magic != 0x4F54544F {
        // Not TrueType (0x00010000) or OpenType/CFF (OTTO)
        return false;
    }

    // Parse and register in the font system
    if let Ok(font) = TtfFont::parse(&data) {
        let mut registry = FONT_REGISTRY.lock();
        registry.push(font);

        let mut web_fonts = WEB_FONTS.lock();
        web_fonts.push(WebFont {
            family: String::from(family),
            style: String::from(style),
            weight,
            data,
            loaded: true,
        });
        crate::serial_println!(
            "[fonts] Registered web font: {} {} w{}",
            family,
            style,
            weight
        );
        true
    } else {
        false
    }
}

/// List loaded web fonts
pub fn list_web_fonts() -> Vec<(String, String, u16)> {
    WEB_FONTS
        .lock()
        .iter()
        .map(|f| (f.family.clone(), f.style.clone(), f.weight))
        .collect()
}
