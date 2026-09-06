//! Theme Switching — Light/Dark/Custom themes with accent color customization
//!
//! Provides runtime theme switching between built-in themes (Dark Nebula,
//! Light, High Contrast) and custom user-defined themes. Also manages accent
//! color customization.
//! Covers status.md items 7.29 (Theme switching) and 7.30 (Accent color).

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Built-in theme variants
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeVariant {
    /// Default dark theme (Nebula Depth)
    Dark,
    /// Light theme for bright environments
    Light,
    /// High contrast for accessibility
    HighContrast,
    /// User-defined custom theme
    Custom,
}

/// Complete color theme definition
#[derive(Debug, Clone)]
pub struct ThemeColors {
    /// Background colors
    pub bg_primary: u32,
    pub bg_secondary: u32,
    pub bg_surface: u32,
    pub bg_elevated: u32,

    /// Text colors
    pub text_primary: u32,
    pub text_secondary: u32,
    pub text_muted: u32,
    pub text_on_accent: u32,

    /// Accent colors (customizable)
    pub accent_primary: u32,
    pub accent_secondary: u32,
    pub accent_hover: u32,
    pub accent_pressed: u32,

    /// Semantic colors
    pub success: u32,
    pub warning: u32,
    pub error: u32,
    pub info: u32,

    /// Window chrome
    pub titlebar_bg: u32,
    pub titlebar_text: u32,
    pub border_color: u32,
    pub shadow_color: u32,

    /// Taskbar / dock
    pub dock_bg: u32,
    pub dock_icon_active: u32,

    /// Name for display
    pub name: String,
}

impl ThemeColors {
    /// The default dark (Nebula) theme
    pub fn dark() -> Self {
        Self {
            bg_primary: 0xFF0A0A1A,
            bg_secondary: 0xFF12122A,
            bg_surface: 0xFF1A1A3E,
            bg_elevated: 0xFF222250,
            text_primary: 0xFFE0E0FF,
            text_secondary: 0xFFA0A0CC,
            text_muted: 0xFF606088,
            text_on_accent: 0xFF000000,
            accent_primary: 0xFF00D4FF,   // Cyan
            accent_secondary: 0xFF8B5CF6, // Violet
            accent_hover: 0xFF33DDFF,
            accent_pressed: 0xFF00AADD,
            success: 0xFF22C55E,
            warning: 0xFFF59E0B,
            error: 0xFFEF4444,
            info: 0xFF3B82F6,
            titlebar_bg: 0xC0161640,
            titlebar_text: 0xFFE0E0FF,
            border_color: 0xFF333366,
            shadow_color: 0x60000000,
            dock_bg: 0xB00D0D2B,
            dock_icon_active: 0xFF00D4FF,
            name: String::from("Nebula Dark"),
        }
    }

    /// Light theme
    pub fn light() -> Self {
        Self {
            bg_primary: 0xFFF5F5F5,
            bg_secondary: 0xFFE8E8E8,
            bg_surface: 0xFFFFFFFF,
            bg_elevated: 0xFFFFFFFF,
            text_primary: 0xFF1A1A1A,
            text_secondary: 0xFF555555,
            text_muted: 0xFF999999,
            text_on_accent: 0xFFFFFFFF,
            accent_primary: 0xFF0066CC,
            accent_secondary: 0xFF7C3AED,
            accent_hover: 0xFF0077EE,
            accent_pressed: 0xFF0055AA,
            success: 0xFF16A34A,
            warning: 0xFFD97706,
            error: 0xFFDC2626,
            info: 0xFF2563EB,
            titlebar_bg: 0xE0E0E0E0,
            titlebar_text: 0xFF1A1A1A,
            border_color: 0xFFCCCCCC,
            shadow_color: 0x30000000,
            dock_bg: 0xC0F0F0F0,
            dock_icon_active: 0xFF0066CC,
            name: String::from("Light"),
        }
    }

    /// High contrast theme
    pub fn high_contrast() -> Self {
        Self {
            bg_primary: 0xFF000000,
            bg_secondary: 0xFF111111,
            bg_surface: 0xFF000000,
            bg_elevated: 0xFF1A1A1A,
            text_primary: 0xFFFFFFFF,
            text_secondary: 0xFFFFFF00,
            text_muted: 0xFF00FF00,
            text_on_accent: 0xFF000000,
            accent_primary: 0xFF00FFFF,
            accent_secondary: 0xFFFF00FF,
            accent_hover: 0xFFFFFF00,
            accent_pressed: 0xFF00CCCC,
            success: 0xFF00FF00,
            warning: 0xFFFFFF00,
            error: 0xFFFF0000,
            info: 0xFF00FFFF,
            titlebar_bg: 0xFF000000,
            titlebar_text: 0xFFFFFFFF,
            border_color: 0xFFFFFFFF,
            shadow_color: 0x00000000,
            dock_bg: 0xFF000000,
            dock_icon_active: 0xFF00FFFF,
            name: String::from("High Contrast"),
        }
    }
}

/// Theme state
struct ThemeState {
    current_variant: ThemeVariant,
    current_colors: ThemeColors,
    custom_themes: Vec<ThemeColors>,
    custom_accent: Option<u32>,
    switch_count: u64,
}

lazy_static::lazy_static! {
    static ref STATE: Mutex<ThemeState> = Mutex::new(ThemeState {
        current_variant: ThemeVariant::Dark,
        current_colors: ThemeColors::dark(),
        custom_themes: Vec::new(),
        custom_accent: None,
        switch_count: 0,
    });
}

static SWITCH_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Switch to a built-in theme variant
pub fn switch_theme(variant: ThemeVariant) {
    let mut state = STATE.lock();
    let mut colors = match variant {
        ThemeVariant::Dark => ThemeColors::dark(),
        ThemeVariant::Light => ThemeColors::light(),
        ThemeVariant::HighContrast => ThemeColors::high_contrast(),
        ThemeVariant::Custom => {
            if let Some(custom) = state.custom_themes.last() {
                custom.clone()
            } else {
                ThemeColors::dark()
            }
        }
    };

    // Apply custom accent if set
    if let Some(accent) = state.custom_accent {
        colors.accent_primary = accent;
        colors.dock_icon_active = accent;
        // Derive hover/pressed from accent
        let r = (accent >> 16) & 0xFF;
        let g = (accent >> 8) & 0xFF;
        let b = accent & 0xFF;
        colors.accent_hover = 0xFF000000
            | ((r.min(255) + 30).min(255) << 16)
            | ((g.min(255) + 30).min(255) << 8)
            | (b.min(255) + 30).min(255);
        colors.accent_pressed = 0xFF000000
            | (r.saturating_sub(20) << 16)
            | (g.saturating_sub(20) << 8)
            | b.saturating_sub(20);
    }

    state.current_variant = variant;
    state.current_colors = colors;
    state.switch_count += 1;
    SWITCH_TOTAL.fetch_add(1, Ordering::Relaxed);
    crate::serial_println!("[theme_switch] Switched to {:?}", variant);
}

/// Set a custom accent color (ARGB)
pub fn set_accent_color(color: u32) {
    let mut state = STATE.lock();
    state.custom_accent = Some(color | 0xFF000000);
    // Re-apply to current theme
    state.current_colors.accent_primary = color | 0xFF000000;
    state.current_colors.dock_icon_active = color | 0xFF000000;
    crate::serial_println!("[theme_switch] Accent color set to 0x{:08X}", color);
}

/// Get the current theme colors
pub fn current_colors() -> ThemeColors {
    STATE.lock().current_colors.clone()
}

/// Get the current theme variant
pub fn current_variant() -> ThemeVariant {
    STATE.lock().current_variant
}

/// Register a custom theme
pub fn register_custom_theme(theme: ThemeColors) {
    STATE.lock().custom_themes.push(theme);
}

/// Get theme switch count
pub fn switch_count() -> u64 {
    SWITCH_TOTAL.load(Ordering::Relaxed)
}

/// Initialize the theme switching subsystem
pub fn init() {
    crate::serial_println!(
        "[theme_switch] Theme switching initialized (Dark, Light, HighContrast, Custom)"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// THEME IMPORT / EXPORT
// ═══════════════════════════════════════════════════════════════════════

/// Export theme as a serialized string (key=value format)
pub fn export_theme(colors: &ThemeColors) -> alloc::string::String {
    use alloc::format;
    let mut out = alloc::string::String::new();
    out.push_str(&format!(
        "bg_primary={},{},{}\n",
        (colors.bg_primary >> 16) & 0xFF,
        (colors.bg_primary >> 8) & 0xFF,
        colors.bg_primary & 0xFF
    ));
    out.push_str(&format!(
        "text_primary={},{},{}\n",
        (colors.text_primary >> 16) & 0xFF,
        (colors.text_primary >> 8) & 0xFF,
        colors.text_primary & 0xFF
    ));
    out.push_str(&format!(
        "accent_primary={},{},{}\n",
        (colors.accent_primary >> 16) & 0xFF,
        (colors.accent_primary >> 8) & 0xFF,
        colors.accent_primary & 0xFF
    ));
    out.push_str(&format!(
        "dock_bg={},{},{}\n",
        (colors.dock_bg >> 16) & 0xFF,
        (colors.dock_bg >> 8) & 0xFF,
        colors.dock_bg & 0xFF
    ));
    out.push_str(&format!(
        "titlebar_bg={},{},{}\n",
        (colors.titlebar_bg >> 16) & 0xFF,
        (colors.titlebar_bg >> 8) & 0xFF,
        colors.titlebar_bg & 0xFF
    ));
    out
}

/// Import a theme from key=value lines
pub fn import_theme(data: &str) -> Option<ThemeColors> {
    let mut colors = ThemeColors::dark();
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            let parts: alloc::vec::Vec<&str> = val.split(',').collect();
            if parts.len() >= 3 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    parts[0].trim().parse::<u32>(),
                    parts[1].trim().parse::<u32>(),
                    parts[2].trim().parse::<u32>(),
                ) {
                    let argb = 0xFF000000 | (r << 16) | (g << 8) | b;
                    match key.trim() {
                        "bg_primary" => colors.bg_primary = argb,
                        "text_primary" => colors.text_primary = argb,
                        "accent_primary" => colors.accent_primary = argb,
                        "dock_bg" => colors.dock_bg = argb,
                        "titlebar_bg" => colors.titlebar_bg = argb,
                        _ => {}
                    }
                }
            }
        }
    }
    Some(colors)
}

// ═══════════════════════════════════════════════════════════════════════
// AUTO DARK/LIGHT MODE
// ═══════════════════════════════════════════════════════════════════════

/// Auto theme schedule
#[derive(Clone, Copy, Debug)]
pub struct AutoThemeSchedule {
    /// Hour (0-23) to switch to light mode
    pub light_hour: u8,
    /// Hour (0-23) to switch to dark mode
    pub dark_hour: u8,
    /// Whether automatic switching is enabled
    pub enabled: bool,
}

static AUTO_SCHEDULE: Mutex<AutoThemeSchedule> = Mutex::new(AutoThemeSchedule {
    light_hour: 7,
    dark_hour: 19,
    enabled: false,
});

/// Enable auto dark/light switching by time of day
pub fn set_auto_theme_schedule(light_hour: u8, dark_hour: u8) {
    let mut sched = AUTO_SCHEDULE.lock();
    sched.light_hour = light_hour.min(23);
    sched.dark_hour = dark_hour.min(23);
    sched.enabled = true;
    crate::serial_println!(
        "[theme] Auto schedule: light at {}:00, dark at {}:00",
        light_hour,
        dark_hour
    );
}

/// Disable auto theme switching
pub fn disable_auto_theme() {
    AUTO_SCHEDULE.lock().enabled = false;
}

/// Check and apply auto theme based on current RTC hour
pub fn check_auto_theme() {
    let sched = AUTO_SCHEDULE.lock();
    if !sched.enabled {
        return;
    }
    let light_h = sched.light_hour;
    let dark_h = sched.dark_hour;
    drop(sched);

    let rtc = crate::rtc::read_rtc();
    let hour = rtc.hour;

    let should_be_light = if light_h < dark_h {
        hour >= light_h && hour < dark_h
    } else {
        hour >= light_h || hour < dark_h
    };

    let current = current_variant();
    if should_be_light && current != ThemeVariant::Light {
        switch_theme(ThemeVariant::Light);
    } else if !should_be_light && current != ThemeVariant::Dark {
        switch_theme(ThemeVariant::Dark);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PER-APP THEME OVERRIDE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref APP_THEME_OVERRIDES: Mutex<alloc::collections::BTreeMap<alloc::string::String, ThemeVariant>> =
        Mutex::new(alloc::collections::BTreeMap::new());
}

/// Set a theme override for a specific application
pub fn set_app_theme_override(app_name: &str, variant: ThemeVariant) {
    APP_THEME_OVERRIDES
        .lock()
        .insert(alloc::string::String::from(app_name), variant);
}

/// Get theme variant for a specific app (uses override if set, else system theme)
pub fn get_app_theme(app_name: &str) -> ThemeVariant {
    APP_THEME_OVERRIDES
        .lock()
        .get(app_name)
        .copied()
        .unwrap_or_else(current_variant)
}

/// Remove a per-app theme override
pub fn clear_app_theme_override(app_name: &str) {
    APP_THEME_OVERRIDES.lock().remove(app_name);
}

// ═══════════════════════════════════════════════════════════════════════
// WINDOW DECORATION STYLES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowDecorationStyle {
    Rounded,
    Square,
    Minimal, // No titlebar, just thin border
}

static DECORATION_STYLE: Mutex<WindowDecorationStyle> = Mutex::new(WindowDecorationStyle::Rounded);

/// Set the global window decoration style
pub fn set_decoration_style(style: WindowDecorationStyle) {
    *DECORATION_STYLE.lock() = style;
    crate::serial_println!("[theme] Window decoration style: {:?}", style);
}

/// Get the current window decoration style
pub fn decoration_style() -> WindowDecorationStyle {
    *DECORATION_STYLE.lock()
}

// ═══════════════════════════════════════════════════════════════════════
// WINDOW TRANSPARENCY SETTINGS
// ═══════════════════════════════════════════════════════════════════════

/// Global opacity for inactive windows (0-255)
static WINDOW_OPACITY: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(255);
/// Global taskbar opacity (0-255)
static PANEL_OPACITY: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(230);

/// Set inactive window opacity (0=fully transparent, 255=opaque)
pub fn set_window_opacity(opacity: u8) {
    WINDOW_OPACITY.store(opacity, Ordering::Relaxed);
}

/// Get inactive window opacity
pub fn window_opacity() -> u8 {
    WINDOW_OPACITY.load(Ordering::Relaxed)
}

/// Set panel/taskbar opacity
pub fn set_panel_opacity(opacity: u8) {
    PANEL_OPACITY.store(opacity, Ordering::Relaxed);
}

/// Get panel/taskbar opacity
pub fn panel_opacity() -> u8 {
    PANEL_OPACITY.load(Ordering::Relaxed)
}
