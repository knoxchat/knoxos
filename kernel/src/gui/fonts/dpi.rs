use crate::gui::framebuffer::{FrameBuffer, Pixel};

use super::glyph::{draw_char, draw_char_bold, draw_char_bold_compact, draw_char_compact};
use super::metrics::{
    FONT_HEIGHT, FONT_HEIGHT_COMPACT, FONT_WIDTH, FONT_WIDTH_COMPACT, LINE_SPACING_EXTRA,
    LINE_SPACING_EXTRA_COMPACT,
};

// ═══════════════════════════════════════════════════════════════════════
// DPI-AWARE TEXT RENDERING
// ═══════════════════════════════════════════════════════════════════════
//
// These functions use `libm::round()` (via the dpi module) to correctly
// position each glyph at fractional scale factors. Without proper rounding,
// character advances accumulate truncation error: at 1.25× scale, the 10px
// glyph becomes 12.5px → truncated to 12px per character, losing 0.5px each.
// After 20 characters that's 10px of drift — visibly uneven spacing.
//
// With `libm::round()`, we compute each character's absolute position from
// the logical grid: physical_x = round(char_index * logical_width * scale).
// This is the same technique winit/egui use for subpixel text positioning.

/// Draw a string with DPI-aware positioning using the current scale factor.
/// Each glyph position is computed from the logical grid with proper rounding,
/// eliminating cumulative drift at fractional scale factors.
pub fn draw_string_dpi(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel) {
    let sf = crate::gui::scale::scale_factor();
    let scale = if sf > 1.5 { 2u32 } else { 1u32 };
    let logical_width = FONT_WIDTH as f64;
    let logical_height = FONT_HEIGHT as f64 + LINE_SPACING_EXTRA as f64;
    let mut col_idx: usize = 0;
    let mut row_idx: usize = 0;
    for ch in text.chars() {
        if ch == '\n' {
            col_idx = 0;
            row_idx += 1;
            continue;
        }
        // Compute absolute pixel position from logical grid — no cumulative error
        let px = x + libm::round(col_idx as f64 * logical_width * sf) as i32;
        let py = y + libm::round(row_idx as f64 * logical_height * sf) as i32;
        draw_char(fb, px, py, ch, color, scale);
        col_idx += 1;
    }
}

/// Draw a compact string with DPI-aware positioning.
pub fn draw_string_compact_dpi(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel) {
    let sf = crate::gui::scale::scale_factor();
    let scale = if sf > 1.5 { 2u32 } else { 1u32 };
    let logical_width = FONT_WIDTH_COMPACT as f64;
    let logical_height = FONT_HEIGHT_COMPACT as f64 + LINE_SPACING_EXTRA_COMPACT as f64;
    let mut col_idx: usize = 0;
    let mut row_idx: usize = 0;
    for ch in text.chars() {
        if ch == '\n' {
            col_idx = 0;
            row_idx += 1;
            continue;
        }
        let px = x + libm::round(col_idx as f64 * logical_width * sf) as i32;
        let py = y + libm::round(row_idx as f64 * logical_height * sf) as i32;
        draw_char_compact(fb, px, py, ch, color, scale);
        col_idx += 1;
    }
}

/// Draw a bold string with DPI-aware positioning.
pub fn draw_string_bold_dpi(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel) {
    let sf = crate::gui::scale::scale_factor();
    let scale = if sf > 1.5 { 2u32 } else { 1u32 };
    let logical_width = FONT_WIDTH as f64;
    let logical_height = FONT_HEIGHT as f64 + LINE_SPACING_EXTRA as f64;
    let mut col_idx: usize = 0;
    let mut row_idx: usize = 0;
    for ch in text.chars() {
        if ch == '\n' {
            col_idx = 0;
            row_idx += 1;
            continue;
        }
        let px = x + libm::round(col_idx as f64 * logical_width * sf) as i32;
        let py = y + libm::round(row_idx as f64 * logical_height * sf) as i32;
        draw_char_bold(fb, px, py, ch, color, scale);
        col_idx += 1;
    }
}

/// Draw a bold compact string with DPI-aware positioning.
pub fn draw_string_bold_compact_dpi(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    text: &str,
    color: Pixel,
) {
    let sf = crate::gui::scale::scale_factor();
    let scale = if sf > 1.5 { 2u32 } else { 1u32 };
    let logical_width = FONT_WIDTH_COMPACT as f64;
    let logical_height = FONT_HEIGHT_COMPACT as f64 + LINE_SPACING_EXTRA_COMPACT as f64;
    let mut col_idx: usize = 0;
    let mut row_idx: usize = 0;
    for ch in text.chars() {
        if ch == '\n' {
            col_idx = 0;
            row_idx += 1;
            continue;
        }
        let px = x + libm::round(col_idx as f64 * logical_width * sf) as i32;
        let py = y + libm::round(row_idx as f64 * logical_height * sf) as i32;
        draw_char_bold_compact(fb, px, py, ch, color, scale);
        col_idx += 1;
    }
}

/// Measure DPI-aware string width in physical pixels.
pub fn measure_string_width_dpi(text: &str) -> u32 {
    let sf = crate::gui::scale::scale_factor();
    libm::round(text.len() as f64 * FONT_WIDTH as f64 * sf) as u32
}

/// Measure DPI-aware compact string width in physical pixels.
pub fn measure_string_width_compact_dpi(text: &str) -> u32 {
    let sf = crate::gui::scale::scale_factor();
    libm::round(text.len() as f64 * FONT_WIDTH_COMPACT as f64 * sf) as u32
}

/// Measure DPI-aware string height in physical pixels.
pub fn measure_string_height_dpi() -> u32 {
    let sf = crate::gui::scale::scale_factor();
    libm::round(FONT_HEIGHT as f64 * sf) as u32
}

/// Measure DPI-aware compact string height in physical pixels.
pub fn measure_string_height_compact_dpi() -> u32 {
    let sf = crate::gui::scale::scale_factor();
    libm::round(FONT_HEIGHT_COMPACT as f64 * sf) as u32
}
