use crate::gui::fonts as core_fonts;
/// KnoxUI text rendering helpers — thin wrappers around `gui::fonts`
/// providing a simplified API used across all KnoxUI components.
///
/// These call the core font functions with `scale = 1` and derive
/// width/height from the text and drawing rect automatically.
///
/// # Usage
/// All KnoxUI components should `use super::text_helpers as fonts;` and call:
/// - `fonts::draw_string_compact(fb, text, x, y, color)`
/// - `fonts::draw_string_bold_compact(fb, text, x, y, color)`
/// - `fonts::draw_string_centered_compact(fb, text, cx, y, color)`
/// - `fonts::draw_string_centered_bold_compact(fb, text, cx, y, color)`
use crate::gui::framebuffer::{FrameBuffer, Pixel};

/// Draw a string at (x, y) with the compact font, scale 1.
#[inline]
pub fn draw_string_compact(fb: &mut FrameBuffer, text: &str, x: i32, y: i32, color: Pixel) {
    core_fonts::draw_string_compact(fb, x, y, text, color, 1);
}

/// Draw a bold string at (x, y) with the compact font, scale 1.
#[inline]
pub fn draw_string_bold_compact(fb: &mut FrameBuffer, text: &str, x: i32, y: i32, color: Pixel) {
    core_fonts::draw_string_bold_compact(fb, x, y, text, color, 1);
}

/// Draw a string centered at (cx, y) with an auto-computed bounding box.
#[inline]
pub fn draw_string_centered_compact(
    fb: &mut FrameBuffer,
    text: &str,
    cx: i32,
    y: i32,
    color: Pixel,
) {
    let w = (text.len() as u32) * 8;
    let h = 10u32;
    let x = cx - w as i32 / 2;
    core_fonts::draw_string_centered_compact(fb, x, y, w, h, text, color, 1);
}

/// Draw a bold string centered at (cx, y).
#[inline]
pub fn draw_string_centered_bold_compact(
    fb: &mut FrameBuffer,
    text: &str,
    cx: i32,
    y: i32,
    color: Pixel,
) {
    let w = (text.len() as u32) * 8;
    let h = 10u32;
    let x = cx - w as i32 / 2;
    core_fonts::draw_string_centered_bold_compact(fb, x, y, w, h, text, color, 1);
}

/// Draw a string centered within a rect (x, y, w, h) — 7-arg version for rect-based centering.
#[inline]
pub fn draw_string_centered_in_rect(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    text: &str,
    color: Pixel,
) {
    core_fonts::draw_string_centered_compact(fb, x, y, w, h, text, color, 1);
}

/// Draw a bold string centered within a rect (x, y, w, h).
#[inline]
pub fn draw_string_centered_bold_in_rect(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    text: &str,
    color: Pixel,
) {
    core_fonts::draw_string_centered_bold_compact(fb, x, y, w, h, text, color, 1);
}
