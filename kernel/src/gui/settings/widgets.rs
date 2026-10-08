/// Shared drawing helpers for Settings tab content.
use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

// ─── TrueType font helpers ──────────────────────────────
/// Regular proportional text at 13px (replaces draw_string_compact at scale 1)
#[inline]
pub(super) fn ttf(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel) {
    font_engine::draw_ui_text(fb, x, y, text, 13, color);
}

/// Bold proportional text at 13px (replaces draw_string_bold_compact at scale 1)
#[inline]
pub(super) fn ttf_b(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel) {
    font_engine::draw_ui_bold(fb, x, y, text, 13, color);
}

/// Bold header text at 15px (replaces draw_string_bold at scale 1)
#[inline]
pub(super) fn ttf_h(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel) {
    font_engine::draw_ui_bold(fb, x, y, text, 15, color);
}

/// Draw text centered horizontally in a region
pub(super) fn ttf_centered(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    region_w: u32,
    text: &str,
    size: u16,
    color: Pixel,
) {
    let tw = font_engine::measure_ui_text(text, size) as i32;
    let cx = x + (region_w as i32 - tw) / 2;
    font_engine::draw_ui_text(fb, cx, y, text, size, color);
}

/// Draw bold text centered horizontally in a region
pub(super) fn ttf_centered_b(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    region_w: u32,
    text: &str,
    size: u16,
    color: Pixel,
) {
    let tw = font_engine::measure_ui_text(text, size) as i32;
    let cx = x + (region_w as i32 - tw) / 2;
    font_engine::draw_ui_bold(fb, cx, y, text, size, color);
}

// ─── Helper: Section Header ──────────────────────────────
pub(super) fn draw_section_header(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, title: &str) {
    font_engine::draw_ui_bold(fb, x + 20, y, title, 15, colors::WHITE);
    fb.draw_hline(x + 16, y + 18, (w - 32) as u32, Pixel::rgb(50, 50, 55));
}

// ─── Helper: Toggle Row ─────────────────────────────────
pub(super) fn draw_toggle_row(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: i32,
    label: &str,
    sublabel: &str,
    enabled: bool,
) {
    font_engine::draw_ui_text(fb, x + 24, y, label, 13, colors::WHITE);
    if !sublabel.is_empty() {
        font_engine::draw_ui_text(fb, x + 24, y + 16, sublabel, 11, Pixel::rgb(120, 120, 120));
    }

    // Toggle switch
    let toggle_x = x + w - 60;
    let bg = if enabled {
        Pixel::rgb(82, 139, 255)
    } else {
        Pixel::rgb(60, 60, 65)
    };
    fb.fill_rounded_rect_aa(Rect::new(toggle_x, y + 2, 36, 18), bg, 9);
    let knob_x = if enabled { toggle_x + 20 } else { toggle_x + 4 };
    fb.fill_circle_aa(knob_x + 6, y + 11, 7, colors::WHITE);
}

// ─── Helper: Slider Row ─────────────────────────────────
pub(super) fn draw_slider_row(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: i32,
    label: &str,
    value: u8,
    color: Pixel,
) {
    font_engine::draw_ui_text(fb, x + 24, y, label, 13, colors::WHITE);
    let val_str = alloc::format!("{}%", value);
    let val_x = x + w - 60;
    font_engine::draw_ui_text(fb, val_x, y, &val_str, 13, Pixel::rgb(140, 140, 140));

    // Slider bar
    let bar_y = y + 18;
    let bar_w = (w - 52) as u32;
    fb.fill_rounded_rect_aa(
        Rect::new(x + 24, bar_y, bar_w, 6),
        Pixel::rgb(50, 50, 55),
        3,
    );
    let fill_w = (bar_w * value as u32) / 100;
    if fill_w > 0 {
        fb.fill_rounded_rect_aa(Rect::new(x + 24, bar_y, fill_w, 6), color, 3);
    }
    fb.fill_circle_aa(x + 24 + fill_w as i32, bar_y + 3, 7, color);
    fb.fill_circle_aa(x + 24 + fill_w as i32, bar_y + 3, 4, colors::WHITE);
}

// ─── Helper: Info Row ───────────────────────────────────
pub(super) fn draw_info_row(fb: &mut FrameBuffer, x: i32, y: i32, label: &str, value: &str) {
    font_engine::draw_ui_text(fb, x + 24, y, label, 13, Pixel::rgb(140, 140, 140));
    font_engine::draw_ui_text(fb, x + 160, y, value, 13, colors::WHITE);
}

/// Draw a focus ring around a rect if keyboard navigation is active and index matches
pub(super) fn draw_focus_indicator(fb: &mut FrameBuffer, rect: Rect, focused: bool) {
    if focused {
        crate::gui::accessibility::draw_focus_ring(fb, rect);
    }
}
