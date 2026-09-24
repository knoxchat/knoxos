/// Customizable window decorations and scale-aware chrome geometry
use spin::Mutex;

use crate::gui::scale;

use super::types::PORTAL_RADIUS;

// ─── Window Decorations Customization ────────────────────────────────

/// Customizable window decoration parameters
pub struct WindowDecorations {
    /// Shadow size in pixels (0 = no shadow, default ~12)
    pub shadow_size: u8,
    /// Window corner radius override (0 = square, default = PORTAL_RADIUS)
    pub border_radius: u8,
    /// Window content opacity (0-255, 255 = fully opaque)
    pub opacity: u8,
}

impl Default for WindowDecorations {
    fn default() -> Self {
        Self {
            shadow_size: 12,
            border_radius: PORTAL_RADIUS as u8,
            opacity: 255,
        }
    }
}

lazy_static::lazy_static! {
    /// Global window decoration settings
    pub static ref WINDOW_DECORATIONS: Mutex<WindowDecorations> =
        Mutex::new(WindowDecorations::default());
}

/// Set window shadow size (0 = no shadow, max 32)
pub fn set_shadow_size(size: u8) {
    WINDOW_DECORATIONS.lock().shadow_size = size.min(32);
}

/// Set window border radius (0 = square corners, max 32)
pub fn set_border_radius(radius: u8) {
    WINDOW_DECORATIONS.lock().border_radius = radius.min(32);
}

/// Set window opacity (0 = fully transparent, 255 = fully opaque)
pub fn set_window_opacity(opacity: u8) {
    WINDOW_DECORATIONS.lock().opacity = opacity;
}

/// Get the current effective border radius (customized or default)
pub fn effective_border_radius() -> u32 {
    WINDOW_DECORATIONS.lock().border_radius as u32
}

/// Get the current shadow size
pub fn effective_shadow_size() -> u8 {
    WINDOW_DECORATIONS.lock().shadow_size
}

// ─── Scale-aware decoration geometry ─────────────────────────────────
// These functions replace raw constant usage so that all window chrome
/// Scaled title bar height
#[inline]
pub fn scaled_title_bar_height() -> u32 {
    scale::title_bar_height()
}
/// Scaled button width
#[inline]
pub fn scaled_btn_width() -> i32 {
    scale::btn_width() as i32
}
/// Scaled button height
#[inline]
pub fn scaled_btn_height() -> i32 {
    scale::btn_height() as i32
}
/// Scaled gap between buttons
#[inline]
pub fn scaled_btn_gap() -> i32 {
    scale::btn_gap() as i32
}
/// Scaled button margin from right edge
#[inline]
pub fn scaled_btn_margin_right() -> i32 {
    scale::btn_margin_right() as i32
}
/// Scaled corner radius
#[inline]
pub fn scaled_portal_radius() -> u32 {
    scale::portal_radius()
}
/// Scaled resize border
#[inline]
pub fn scaled_resize_border() -> i32 {
    scale::resize_border()
}
/// Scaled minimum width
#[inline]
pub fn scaled_min_width() -> u32 {
    scale::min_width()
}
/// Scaled minimum height
#[inline]
pub fn scaled_min_height() -> u32 {
    scale::min_height()
}
/// Scaled minimum visible pixels
#[inline]
pub fn scaled_min_visible() -> i32 {
    scale::min_visible_px()
}
/// Scaled glyph half for close X
#[inline]
pub fn scaled_glyph_half() -> i32 {
    scale::glyph_half()
}
/// Scaled dash half for minimize
#[inline]
pub fn scaled_dash_half() -> i32 {
    scale::dash_half()
}
/// Scaled box half for maximize
#[inline]
pub fn scaled_max_box_half() -> i32 {
    scale::max_box_half()
}
/// Scaled title left padding
#[inline]
pub fn scaled_title_pad_left() -> i32 {
    scale::title_pad_left()
}
