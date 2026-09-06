use super::framebuffer::Pixel;
/// Theme Engine — Light / Dark / Custom Theme Support
///
/// Provides a global theme system with named color tokens that all GUI
/// components use for consistent styling. Ships with three built-in themes:
///   - **Aurora Dark** (default) — warm slate with coral/rose gold accents
///   - **Arctic Light** — clean warm-white minimal design
///   - **Sunset Warm** — rich amber/terracotta tones
///
/// Custom themes can be created by setting individual color tokens.
/// Themes are stored atomically so changes take effect immediately.
use core::sync::atomic::{AtomicU8, Ordering};
use spin::Mutex;

// ─── Theme ID ────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ThemeId {
    NebulaDark = 0,
    ArcticLight = 1,
    SunsetWarm = 2,
    Custom = 3,
}

impl ThemeId {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => ThemeId::NebulaDark,
            1 => ThemeId::ArcticLight,
            2 => ThemeId::SunsetWarm,
            _ => ThemeId::Custom,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ThemeId::NebulaDark => "Aurora Dark",
            ThemeId::ArcticLight => "Arctic Light",
            ThemeId::SunsetWarm => "Sunset Warm",
            ThemeId::Custom => "Custom",
        }
    }
}

// ─── Color Tokens ────────────────────────────────────────────────────

/// Complete set of semantic color tokens for the theme
#[derive(Clone, Copy)]
pub struct ThemeColors {
    // Background layers
    pub bg_primary: Pixel,
    pub bg_secondary: Pixel,
    pub bg_tertiary: Pixel,
    pub bg_surface: Pixel,
    pub bg_elevated: Pixel,
    pub bg_overlay: Pixel,

    // Text
    pub text_primary: Pixel,
    pub text_secondary: Pixel,
    pub text_muted: Pixel,
    pub text_disabled: Pixel,
    pub text_inverse: Pixel,

    // Accents
    pub accent_primary: Pixel,
    pub accent_secondary: Pixel,
    pub accent_success: Pixel,
    pub accent_warning: Pixel,
    pub accent_error: Pixel,
    pub accent_info: Pixel,

    // Borders
    pub border_subtle: Pixel,
    pub border_default: Pixel,
    pub border_strong: Pixel,
    pub border_focus: Pixel,

    // Interactive states
    pub hover_bg: Pixel,
    pub active_bg: Pixel,
    pub selected_bg: Pixel,

    // Taskbar
    pub taskbar_bg: Pixel,
    pub taskbar_text: Pixel,
    pub taskbar_active: Pixel,

    // Window chrome
    pub titlebar_bg: Pixel,
    pub titlebar_text: Pixel,
    pub titlebar_close: Pixel,
    pub titlebar_minimize: Pixel,
    pub titlebar_maximize: Pixel,

    // Scrollbar
    pub scrollbar_track: Pixel,
    pub scrollbar_thumb: Pixel,
    pub scrollbar_thumb_hover: Pixel,

    // Wallpaper gradient
    pub wallpaper_top: Pixel,
    pub wallpaper_bottom: Pixel,
    pub wallpaper_accent1: Pixel,
    pub wallpaper_accent2: Pixel,
}

// ─── Built-in Themes ─────────────────────────────────────────────────

const NEBULA_DARK: ThemeColors = ThemeColors {
    bg_primary: Pixel::new(22, 20, 26, 255),
    bg_secondary: Pixel::new(28, 26, 34, 255),
    bg_tertiary: Pixel::new(36, 33, 42, 255),
    bg_surface: Pixel::new(32, 30, 38, 230),
    bg_elevated: Pixel::new(44, 40, 50, 240),
    bg_overlay: Pixel::new(10, 8, 12, 160),

    text_primary: Pixel::new(240, 232, 224, 240),
    text_secondary: Pixel::new(200, 190, 180, 220),
    text_muted: Pixel::new(140, 130, 120, 170),
    text_disabled: Pixel::new(80, 74, 68, 120),
    text_inverse: Pixel::new(22, 20, 26, 255),

    accent_primary: Pixel::new(232, 121, 100, 255),
    accent_secondary: Pixel::new(180, 140, 200, 255),
    accent_success: Pixel::new(130, 190, 150, 255),
    accent_warning: Pixel::new(230, 180, 90, 255),
    accent_error: Pixel::new(220, 85, 85, 255),
    accent_info: Pixel::new(140, 170, 210, 255),

    border_subtle: Pixel::new(55, 50, 60, 100),
    border_default: Pixel::new(70, 65, 78, 140),
    border_strong: Pixel::new(100, 92, 110, 180),
    border_focus: Pixel::new(232, 121, 100, 200),

    hover_bg: Pixel::new(55, 50, 62, 120),
    active_bg: Pixel::new(80, 60, 70, 150),
    selected_bg: Pixel::new(232, 121, 100, 50),

    taskbar_bg: Pixel::new(26, 24, 30, 220),
    taskbar_text: Pixel::new(210, 200, 190, 240),
    taskbar_active: Pixel::new(232, 121, 100, 255),

    titlebar_bg: Pixel::new(30, 28, 36, 240),
    titlebar_text: Pixel::new(220, 212, 204, 240),
    titlebar_close: Pixel::new(220, 85, 85, 255),
    titlebar_minimize: Pixel::new(230, 180, 90, 255),
    titlebar_maximize: Pixel::new(130, 190, 150, 255),

    scrollbar_track: Pixel::new(36, 33, 42, 100),
    scrollbar_thumb: Pixel::new(70, 65, 78, 160),
    scrollbar_thumb_hover: Pixel::new(100, 92, 110, 200),

    wallpaper_top: Pixel::new(22, 20, 26, 255),
    wallpaper_bottom: Pixel::new(14, 12, 16, 255),
    wallpaper_accent1: Pixel::new(232, 121, 100, 60),
    wallpaper_accent2: Pixel::new(180, 140, 200, 45),
};

const ARCTIC_LIGHT: ThemeColors = ThemeColors {
    bg_primary: Pixel::new(250, 246, 242, 255),
    bg_secondary: Pixel::new(244, 238, 232, 255),
    bg_tertiary: Pixel::new(236, 228, 220, 255),
    bg_surface: Pixel::new(255, 252, 248, 245),
    bg_elevated: Pixel::new(255, 255, 252, 255),
    bg_overlay: Pixel::new(255, 252, 248, 200),

    text_primary: Pixel::new(42, 36, 32, 240),
    text_secondary: Pixel::new(80, 72, 64, 220),
    text_muted: Pixel::new(140, 130, 120, 180),
    text_disabled: Pixel::new(190, 182, 174, 140),
    text_inverse: Pixel::new(250, 246, 242, 255),

    accent_primary: Pixel::new(200, 95, 75, 255),
    accent_secondary: Pixel::new(150, 110, 170, 255),
    accent_success: Pixel::new(90, 165, 110, 255),
    accent_warning: Pixel::new(200, 150, 40, 255),
    accent_error: Pixel::new(200, 50, 50, 255),
    accent_info: Pixel::new(100, 140, 190, 255),

    border_subtle: Pixel::new(220, 212, 204, 120),
    border_default: Pixel::new(200, 192, 184, 160),
    border_strong: Pixel::new(175, 165, 155, 200),
    border_focus: Pixel::new(200, 95, 75, 200),

    hover_bg: Pixel::new(238, 230, 222, 140),
    active_bg: Pixel::new(228, 218, 208, 180),
    selected_bg: Pixel::new(200, 95, 75, 35),

    taskbar_bg: Pixel::new(248, 242, 236, 235),
    taskbar_text: Pixel::new(50, 44, 38, 240),
    taskbar_active: Pixel::new(200, 95, 75, 255),

    titlebar_bg: Pixel::new(250, 246, 242, 250),
    titlebar_text: Pixel::new(42, 36, 32, 240),
    titlebar_close: Pixel::new(200, 50, 50, 255),
    titlebar_minimize: Pixel::new(200, 150, 40, 255),
    titlebar_maximize: Pixel::new(90, 165, 110, 255),

    scrollbar_track: Pixel::new(236, 228, 220, 120),
    scrollbar_thumb: Pixel::new(200, 192, 184, 180),
    scrollbar_thumb_hover: Pixel::new(175, 165, 155, 220),

    wallpaper_top: Pixel::new(240, 232, 224, 255),
    wallpaper_bottom: Pixel::new(248, 242, 236, 255),
    wallpaper_accent1: Pixel::new(200, 95, 75, 35),
    wallpaper_accent2: Pixel::new(150, 110, 170, 25),
};

const SUNSET_WARM: ThemeColors = ThemeColors {
    bg_primary: Pixel::new(30, 22, 18, 255),
    bg_secondary: Pixel::new(40, 30, 24, 255),
    bg_tertiary: Pixel::new(52, 38, 28, 255),
    bg_surface: Pixel::new(46, 34, 26, 230),
    bg_elevated: Pixel::new(60, 44, 32, 240),
    bg_overlay: Pixel::new(22, 16, 12, 180),

    text_primary: Pixel::new(252, 240, 220, 240),
    text_secondary: Pixel::new(220, 200, 175, 220),
    text_muted: Pixel::new(165, 145, 125, 170),
    text_disabled: Pixel::new(105, 90, 78, 120),
    text_inverse: Pixel::new(30, 22, 18, 255),

    accent_primary: Pixel::new(230, 140, 65, 255),
    accent_secondary: Pixel::new(200, 110, 90, 255),
    accent_success: Pixel::new(145, 195, 95, 255),
    accent_warning: Pixel::new(245, 200, 70, 255),
    accent_error: Pixel::new(230, 75, 75, 255),
    accent_info: Pixel::new(120, 170, 200, 255),

    border_subtle: Pixel::new(75, 58, 44, 100),
    border_default: Pixel::new(95, 75, 56, 140),
    border_strong: Pixel::new(125, 100, 76, 180),
    border_focus: Pixel::new(230, 140, 65, 200),

    hover_bg: Pixel::new(75, 55, 40, 120),
    active_bg: Pixel::new(105, 75, 48, 150),
    selected_bg: Pixel::new(185, 125, 55, 80),

    taskbar_bg: Pixel::new(28, 20, 16, 220),
    taskbar_text: Pixel::new(235, 218, 195, 240),
    taskbar_active: Pixel::new(230, 140, 65, 255),

    titlebar_bg: Pixel::new(40, 30, 24, 240),
    titlebar_text: Pixel::new(245, 228, 205, 240),
    titlebar_close: Pixel::new(230, 75, 75, 255),
    titlebar_minimize: Pixel::new(245, 200, 70, 255),
    titlebar_maximize: Pixel::new(145, 195, 95, 255),

    scrollbar_track: Pixel::new(52, 38, 28, 100),
    scrollbar_thumb: Pixel::new(95, 75, 56, 160),
    scrollbar_thumb_hover: Pixel::new(125, 100, 76, 200),

    wallpaper_top: Pixel::new(65, 35, 20, 255),
    wallpaper_bottom: Pixel::new(24, 14, 10, 255),
    wallpaper_accent1: Pixel::new(230, 140, 65, 55),
    wallpaper_accent2: Pixel::new(200, 110, 90, 35),
};

// ─── Global Theme State ──────────────────────────────────────────────

/// Current active theme ID
static ACTIVE_THEME_ID: AtomicU8 = AtomicU8::new(0);

/// Custom theme (when ThemeId::Custom is active)
static CUSTOM_THEME: Mutex<ThemeColors> = Mutex::new(NEBULA_DARK);

/// Active accent color index (0-5). AtomicU8 for lock-free reads.
static ACCENT_INDEX: AtomicU8 = AtomicU8::new(0);

/// Predefined accent colors
pub const ACCENT_COLORS: [Pixel; 6] = [
    Pixel::new(232, 121, 100, 255), // Coral (default)
    Pixel::new(130, 190, 150, 255), // Sage
    Pixel::new(200, 140, 160, 255), // Rose
    Pixel::new(210, 170, 100, 255), // Amber
    Pixel::new(180, 140, 200, 255), // Lavender
    Pixel::new(140, 180, 200, 255), // Slate blue
];

/// Get the active accent color
pub fn accent_color() -> Pixel {
    let idx = ACCENT_INDEX.load(Ordering::Relaxed) as usize;
    ACCENT_COLORS[idx.min(ACCENT_COLORS.len() - 1)]
}

/// Get the active accent color index
pub fn accent_index() -> u8 {
    ACCENT_INDEX.load(Ordering::Relaxed)
}

/// Set the accent color by index (0-5)
pub fn set_accent_index(idx: u8) {
    ACCENT_INDEX.store(idx.min(5), Ordering::Relaxed);
}

/// Get the active theme ID
pub fn active_theme() -> ThemeId {
    ThemeId::from_u8(ACTIVE_THEME_ID.load(Ordering::Relaxed))
}

/// Get the current theme colors
pub fn colors() -> ThemeColors {
    match active_theme() {
        ThemeId::NebulaDark => NEBULA_DARK,
        ThemeId::ArcticLight => ARCTIC_LIGHT,
        ThemeId::SunsetWarm => SUNSET_WARM,
        ThemeId::Custom => *CUSTOM_THEME.lock(),
    }
}

/// Switch to a built-in theme
pub fn set_theme(theme: ThemeId) {
    ACTIVE_THEME_ID.store(theme as u8, Ordering::Relaxed);
    // Invalidate wallpaper cache so the new theme's wallpaper renders
    super::desktop::invalidate_wallpaper_cache();
}

/// Set a custom theme
pub fn set_custom_theme(colors: ThemeColors) {
    *CUSTOM_THEME.lock() = colors;
    ACTIVE_THEME_ID.store(ThemeId::Custom as u8, Ordering::Relaxed);
    super::desktop::invalidate_wallpaper_cache();
}

/// Cycle to the next theme
pub fn cycle_theme() {
    let current = ACTIVE_THEME_ID.load(Ordering::Relaxed);
    let next = match current {
        0 => 1, // Dark → Light
        1 => 2, // Light → Warm
        2 => 0, // Warm → Dark
        _ => 0,
    };
    ACTIVE_THEME_ID.store(next, Ordering::Relaxed);
    super::desktop::invalidate_wallpaper_cache();
}

/// Get a list of available theme names
pub fn theme_names() -> [&'static str; 3] {
    ["Aurora Dark", "Arctic Light", "Sunset Warm"]
}

/// Load a theme from a VFS config file (format: "key = r,g,b,a" lines)
pub fn load_theme_from_file(path: &str) -> Result<(), &'static str> {
    let data = crate::vfs::read_file_dispatch(path).ok_or("Theme file not found")?;
    let text = core::str::from_utf8(&data).map_err(|_| "Invalid UTF-8")?;

    let mut theme = NEBULA_DARK; // start from dark base

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let pixel = parse_pixel(val.trim());
            if let Some(px) = pixel {
                apply_color_token(&mut theme, key, px);
            }
        }
    }

    set_custom_theme(theme);
    Ok(())
}

fn parse_pixel(s: &str) -> Option<Pixel> {
    let parts: alloc::vec::Vec<&str> = s.split(',').collect();
    if parts.len() < 3 {
        return None;
    }
    let r = parts[0].trim().parse::<u8>().ok()?;
    let g = parts[1].trim().parse::<u8>().ok()?;
    let b = parts[2].trim().parse::<u8>().ok()?;
    let a = if parts.len() > 3 {
        parts[3].trim().parse::<u8>().unwrap_or(255)
    } else {
        255
    };
    Some(Pixel::new(r, g, b, a))
}

fn apply_color_token(theme: &mut ThemeColors, key: &str, px: Pixel) {
    match key {
        "bg_primary" => theme.bg_primary = px,
        "bg_secondary" => theme.bg_secondary = px,
        "bg_tertiary" => theme.bg_tertiary = px,
        "bg_surface" => theme.bg_surface = px,
        "bg_elevated" => theme.bg_elevated = px,
        "bg_overlay" => theme.bg_overlay = px,
        "text_primary" => theme.text_primary = px,
        "text_secondary" => theme.text_secondary = px,
        "text_muted" => theme.text_muted = px,
        "accent_primary" => theme.accent_primary = px,
        "accent_secondary" => theme.accent_secondary = px,
        "accent_success" => theme.accent_success = px,
        "accent_warning" => theme.accent_warning = px,
        "accent_error" => theme.accent_error = px,
        "border_default" => theme.border_default = px,
        "border_focus" => theme.border_focus = px,
        "taskbar_bg" => theme.taskbar_bg = px,
        "titlebar_bg" => theme.titlebar_bg = px,
        "wallpaper_top" => theme.wallpaper_top = px,
        "wallpaper_bottom" => theme.wallpaper_bottom = px,
        _ => {}
    }
}
