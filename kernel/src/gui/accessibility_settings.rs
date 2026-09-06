/// Accessibility Settings Panel
///
/// System settings UI for configuring accessibility features:
/// high contrast, large text, screen reader, magnifier, sticky keys, etc.
use alloc::string::String;
use spin::Mutex;

/// Accessibility settings state
#[derive(Debug, Clone)]
pub struct AccessibilitySettings {
    // Vision
    pub high_contrast: bool,
    pub large_text: bool,
    pub text_scale_factor: f32,
    pub cursor_size: CursorSize,
    pub reduce_motion: bool,
    pub reduce_transparency: bool,
    pub color_filter: ColorFilter,
    pub screen_reader_enabled: bool,
    pub magnifier_enabled: bool,
    pub magnifier_zoom: f32,
    // Hearing
    pub visual_alerts: bool,
    pub mono_audio: bool,
    pub closed_captions: bool,
    // Interaction
    pub sticky_keys: bool,
    pub slow_keys: bool,
    pub slow_keys_delay_ms: u32,
    pub bounce_keys: bool,
    pub bounce_keys_delay_ms: u32,
    pub mouse_keys: bool,
    pub onscreen_keyboard: bool,
    pub dwell_click: bool,
    pub dwell_delay_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorSize {
    Default,
    Large,
    ExtraLarge,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorFilter {
    None,
    Grayscale,
    Protanopia,
    Deuteranopia,
    Tritanopia,
    Invert,
}

lazy_static::lazy_static! {
    static ref SETTINGS: Mutex<AccessibilitySettings> = Mutex::new(AccessibilitySettings {
        high_contrast: false,
        large_text: false,
        text_scale_factor: 1.0,
        cursor_size: CursorSize::Default,
        reduce_motion: false,
        reduce_transparency: false,
        color_filter: ColorFilter::None,
        screen_reader_enabled: false,
        magnifier_enabled: false,
        magnifier_zoom: 2.0,
        visual_alerts: false,
        mono_audio: false,
        closed_captions: false,
        sticky_keys: false,
        slow_keys: false,
        slow_keys_delay_ms: 300,
        bounce_keys: false,
        bounce_keys_delay_ms: 300,
        mouse_keys: false,
        onscreen_keyboard: false,
        dwell_click: false,
        dwell_delay_ms: 1200,
    });
}

impl AccessibilitySettings {
    pub fn set_high_contrast(&mut self, on: bool) {
        self.high_contrast = on;
        crate::serial_println!("[A11Y] High contrast: {}", on);
    }

    pub fn set_text_scale(&mut self, factor: f32) {
        self.text_scale_factor = factor.clamp(0.5, 4.0);
        self.large_text = self.text_scale_factor > 1.2;
    }

    pub fn set_screen_reader(&mut self, on: bool) {
        self.screen_reader_enabled = on;
        if on {
            crate::atspi::enable();
        } else {
            crate::atspi::disable();
        }
    }

    pub fn set_color_filter(&mut self, filter: ColorFilter) {
        self.color_filter = filter;
    }

    /// Apply color filter to a pixel
    pub fn filter_pixel(&self, rgba: u32) -> u32 {
        match self.color_filter {
            ColorFilter::None => rgba,
            ColorFilter::Grayscale => {
                let r = (rgba >> 16) & 0xFF;
                let g = (rgba >> 8) & 0xFF;
                let b = rgba & 0xFF;
                let gray = (r * 299 + g * 587 + b * 114) / 1000;
                (rgba & 0xFF000000) | (gray << 16) | (gray << 8) | gray
            }
            ColorFilter::Invert => {
                let r = 255 - ((rgba >> 16) & 0xFF);
                let g = 255 - ((rgba >> 8) & 0xFF);
                let b = 255 - (rgba & 0xFF);
                (rgba & 0xFF000000) | (r << 16) | (g << 8) | b
            }
            _ => rgba, // Daltonization filters would go here
        }
    }
}

pub fn get_settings() -> AccessibilitySettings {
    SETTINGS.lock().clone()
}

pub fn init() {
    crate::serial_println!("[A11Y] Accessibility settings panel loaded");
}
