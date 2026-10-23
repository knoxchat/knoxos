//! System-wide font configuration (family, hinting, antialiasing).
use alloc::string::String;
use spin::Mutex;

// ═══════════════════════════════════════════════════════════════════════
// FONT CONFIGURATION
// ═══════════════════════════════════════════════════════════════════════

/// System-wide font configuration
#[derive(Clone, Debug)]
pub struct FontConfig {
    /// Default UI font family
    pub default_family: String,
    /// Default monospace font family
    pub monospace_family: String,
    /// Default UI font size in pixels
    pub default_size: u32,
    /// Hinting mode
    pub hinting: HintingMode,
    /// Anti-aliasing mode
    pub antialiasing: AaMode,
    /// Enable ligatures globally
    pub enable_ligatures: bool,
    /// Enable kerning globally
    pub enable_kerning: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HintingMode {
    None,
    Slight,
    Medium,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AaMode {
    None,
    Grayscale,
    Subpixel,
}

lazy_static::lazy_static! {
    static ref FONT_CONFIG: Mutex<FontConfig> = Mutex::new(FontConfig {
        default_family: String::from("Inter"),
        monospace_family: String::from("Hack"),
        default_size: 14,
        hinting: HintingMode::Slight,
        antialiasing: AaMode::Grayscale,
        enable_ligatures: true,
        enable_kerning: true,
    });
}

/// Get the current font configuration
pub fn get_font_config() -> FontConfig {
    FONT_CONFIG.lock().clone()
}

/// Update font configuration
pub fn set_font_config(config: FontConfig) {
    crate::serial_println!(
        "[fonts] Config updated: family={}, size={}, hint={:?}, aa={:?}",
        config.default_family,
        config.default_size,
        config.hinting,
        config.antialiasing
    );
    *FONT_CONFIG.lock() = config;
}
