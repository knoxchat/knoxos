/// Mouse / trackpad configuration and focus-mode settings
use spin::Mutex;

// ═══════════════════════════════════════════════════════════════════════
// MOUSE SETTINGS (sensitivity, natural scrolling, middle-click paste)
// ═══════════════════════════════════════════════════════════════════════

/// Mouse / trackpad configuration
#[derive(Debug, Clone, Copy)]
pub struct MouseSettings {
    /// Mouse sensitivity / speed multiplier (0.25 = slow, 1.0 = default, 3.0 = fast)
    pub sensitivity: f32,
    /// Enable mouse acceleration curve
    pub acceleration_enabled: bool,
    /// Natural (reverse) scrolling — scroll content follows finger direction
    pub natural_scrolling: bool,
    /// Scroll lines per wheel notch (default: 3)
    pub scroll_lines: u8,
    /// Middle-click pastes from clipboard
    pub middle_click_paste: bool,
    /// Left-handed mode (swap left/right buttons)
    pub left_handed: bool,
}

impl MouseSettings {
    pub fn default_settings() -> Self {
        Self {
            sensitivity: 1.0,
            acceleration_enabled: false,
            natural_scrolling: false,
            scroll_lines: 3,
            middle_click_paste: true,
            left_handed: false,
        }
    }
}

lazy_static::lazy_static! {
    pub static ref MOUSE_SETTINGS: Mutex<MouseSettings> = Mutex::new(MouseSettings::default_settings());
}

/// Get current mouse settings
pub fn mouse_settings() -> MouseSettings {
    *MOUSE_SETTINGS.lock()
}

/// Update mouse settings
pub fn set_mouse_settings(settings: MouseSettings) {
    *MOUSE_SETTINGS.lock() = settings;
}

// ─── Focus Mode ──────────────────────────────────────────────────────

/// Focus mode: ClickToFocus (default) or FocusFollowsMouse
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMode {
    /// Window gains focus only when clicked (default)
    ClickToFocus,
    /// Window gains focus when the mouse pointer enters it
    FocusFollowsMouse,
}

/// Global focus mode setting
static FOCUS_MODE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Set the focus mode
pub fn set_focus_mode(mode: FocusMode) {
    let val = match mode {
        FocusMode::ClickToFocus => 0,
        FocusMode::FocusFollowsMouse => 1,
    };
    FOCUS_MODE.store(val, core::sync::atomic::Ordering::Relaxed);
}

/// Get the current focus mode
pub fn focus_mode() -> FocusMode {
    match FOCUS_MODE.load(core::sync::atomic::Ordering::Relaxed) {
        1 => FocusMode::FocusFollowsMouse,
        _ => FocusMode::ClickToFocus,
    }
}
