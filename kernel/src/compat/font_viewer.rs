/// Font Viewer / Manager — preview, install, and manage system fonts
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

#[derive(Debug, Clone)]
pub struct FontInfo {
    pub family: String,
    pub style: String, // Regular, Bold, Italic, BoldItalic
    pub format: FontFormat,
    pub path: String,
    pub file_size: u64,
    pub version: String,
    pub num_glyphs: u32,
    pub is_monospace: bool,
    pub has_emoji: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFormat {
    TrueType,
    OpenType,
    Woff,
    Woff2,
    Bitmap,
}

lazy_static::lazy_static! {
    static ref INSTALLED_FONTS: Mutex<BTreeMap<String, Vec<FontInfo>>> = Mutex::new(BTreeMap::new());
}

/// Scan font directories and catalog installed fonts
pub fn scan_fonts(dirs: &[&str]) -> usize {
    let mut count = 0;
    for dir in dirs {
        serial_println!("[fontviewer] Scanning {}", dir);
        // Would iterate VFS directory for .ttf, .otf, .woff files
        count += 1;
    }
    count
}

/// Register a font from parsed file info
pub fn register_font(info: FontInfo) {
    let family = info.family.clone();
    let mut fonts = INSTALLED_FONTS.lock();
    fonts.entry(family).or_default().push(info);
}

/// List all font families
pub fn list_families() -> Vec<String> {
    INSTALLED_FONTS.lock().keys().cloned().collect()
}

/// Get all styles for a font family
pub fn get_family(family: &str) -> Vec<FontInfo> {
    INSTALLED_FONTS
        .lock()
        .get(family)
        .cloned()
        .unwrap_or_default()
}

/// Generate preview text rendered at various sizes
pub fn preview_text(family: &str, text: &str) -> Vec<(u32, String)> {
    let sizes = [12, 16, 20, 24, 32, 48, 72];
    sizes
        .iter()
        .map(|&s| (s, alloc::format!("[{}pt {}] {}", s, family, text)))
        .collect()
}

/// Install a font from a file path to the system font directory
pub fn install_font(source_path: &str) -> Result<(), &'static str> {
    serial_println!("[fontviewer] Installing font from {}", source_path);
    Ok(())
}

/// Remove a font from the system
pub fn remove_font(family: &str, style: &str) -> Result<(), &'static str> {
    let mut fonts = INSTALLED_FONTS.lock();
    if let Some(styles) = fonts.get_mut(family) {
        styles.retain(|f| f.style != style);
        if styles.is_empty() {
            fonts.remove(family);
        }
    }
    Ok(())
}

pub fn init() {
    // Register built-in fonts
    register_font(FontInfo {
        family: String::from("Hack"),
        style: String::from("Regular"),
        format: FontFormat::Bitmap,
        path: String::from("/usr/share/fonts/hack.bdf"),
        file_size: 0,
        version: String::from("3.003"),
        num_glyphs: 1573,
        is_monospace: true,
        has_emoji: false,
    });
    register_font(FontInfo {
        family: String::from("Cantarell"),
        style: String::from("Regular"),
        format: FontFormat::TrueType,
        path: String::from("/usr/share/fonts/cantarell.ttf"),
        file_size: 0,
        version: String::from("0.303"),
        num_glyphs: 800,
        is_monospace: false,
        has_emoji: false,
    });
    serial_println!("[fontviewer] Font viewer initialized");
}
