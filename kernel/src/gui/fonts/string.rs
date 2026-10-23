use crate::gui::framebuffer::{FrameBuffer, Pixel};

use super::glyph::{draw_char, draw_char_bold, draw_char_bold_compact, draw_char_compact};
use super::metrics::{
    FONT_HEIGHT, FONT_HEIGHT_COMPACT, FONT_WIDTH, FONT_WIDTH_COMPACT, LINE_SPACING_EXTRA,
    LINE_SPACING_EXTRA_COMPACT,
};

// String Drawing Functions

/// Draw a string using standard glyphs (10x20).
pub fn draw_string(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel, scale: u32) {
    let char_width = (FONT_WIDTH as u32 * scale) as i32;
    let line_height = (FONT_HEIGHT as u32 * scale) as i32 + LINE_SPACING_EXTRA;
    let mut cx = x;
    let mut cy = y;
    for ch in text.chars() {
        if ch == '\n' {
            cx = x;
            cy += line_height;
            continue;
        }
        draw_char(fb, cx, cy, ch, color, scale);
        cx += char_width;
    }
}

/// Draw a bold string (standard 10x20).
pub fn draw_string_bold(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    text: &str,
    color: Pixel,
    scale: u32,
) {
    let char_width = (FONT_WIDTH as u32 * scale) as i32;
    let line_height = (FONT_HEIGHT as u32 * scale) as i32 + LINE_SPACING_EXTRA;
    let mut cx = x;
    let mut cy = y;
    for ch in text.chars() {
        if ch == '\n' {
            cx = x;
            cy += line_height;
            continue;
        }
        draw_char_bold(fb, cx, cy, ch, color, scale);
        cx += char_width;
    }
}

/// Draw a string using compact glyphs (8x14).
pub fn draw_string_compact(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    text: &str,
    color: Pixel,
    scale: u32,
) {
    let char_width = (FONT_WIDTH_COMPACT as u32 * scale) as i32;
    let line_height = (FONT_HEIGHT_COMPACT as u32 * scale) as i32 + LINE_SPACING_EXTRA_COMPACT;
    let mut cx = x;
    let mut cy = y;
    for ch in text.chars() {
        if ch == '\n' {
            cx = x;
            cy += line_height;
            continue;
        }
        draw_char_compact(fb, cx, cy, ch, color, scale);
        cx += char_width;
    }
}

/// Draw a bold string (compact 8x14).
pub fn draw_string_bold_compact(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    text: &str,
    color: Pixel,
    scale: u32,
) {
    let char_width = (FONT_WIDTH_COMPACT as u32 * scale) as i32;
    let line_height = (FONT_HEIGHT_COMPACT as u32 * scale) as i32 + LINE_SPACING_EXTRA_COMPACT;
    let mut cx = x;
    let mut cy = y;
    for ch in text.chars() {
        if ch == '\n' {
            cx = x;
            cy += line_height;
            continue;
        }
        draw_char_bold_compact(fb, cx, cy, ch, color, scale);
        cx += char_width;
    }
}

/// Draw a string with a 1px drop shadow.
pub fn draw_string_shadow(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    text: &str,
    color: Pixel,
    shadow_color: Pixel,
    scale: u32,
) {
    let soft = Pixel::new(
        shadow_color.r,
        shadow_color.g,
        shadow_color.b,
        (shadow_color.a as u16 * 128 / 255) as u8,
    );
    draw_string(fb, x + 1, y + 1, text, soft, scale);
    draw_string(fb, x, y, text, color, scale);
}

/// Draw a compact string with 1px drop shadow.
pub fn draw_string_shadow_compact(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    text: &str,
    color: Pixel,
    shadow_color: Pixel,
    scale: u32,
) {
    let soft = Pixel::new(
        shadow_color.r,
        shadow_color.g,
        shadow_color.b,
        (shadow_color.a as u16 * 128 / 255) as u8,
    );
    draw_string_compact(fb, x + 1, y + 1, text, soft, scale);
    draw_string_compact(fb, x, y, text, color, scale);
}

/// Draw centered text within a bounding box.
#[allow(clippy::too_many_arguments)]
pub fn draw_string_centered(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    text: &str,
    color: Pixel,
    scale: u32,
) {
    let tw = text.len() as i32 * FONT_WIDTH as i32 * scale as i32;
    let th = FONT_HEIGHT as i32 * scale as i32;
    draw_string(
        fb,
        x + (width as i32 - tw) / 2,
        y + (height as i32 - th) / 2,
        text,
        color,
        scale,
    );
}

/// Draw centered bold text.
#[allow(clippy::too_many_arguments)]
pub fn draw_string_centered_bold(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    text: &str,
    color: Pixel,
    scale: u32,
) {
    let tw = text.len() as i32 * FONT_WIDTH as i32 * scale as i32;
    let th = FONT_HEIGHT as i32 * scale as i32;
    draw_string_bold(
        fb,
        x + (width as i32 - tw) / 2,
        y + (height as i32 - th) / 2,
        text,
        color,
        scale,
    );
}

/// Draw centered compact text.
#[allow(clippy::too_many_arguments)]
pub fn draw_string_centered_compact(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    text: &str,
    color: Pixel,
    scale: u32,
) {
    let tw = text.len() as i32 * FONT_WIDTH_COMPACT as i32 * scale as i32;
    let th = FONT_HEIGHT_COMPACT as i32 * scale as i32;
    draw_string_compact(
        fb,
        x + (width as i32 - tw) / 2,
        y + (height as i32 - th) / 2,
        text,
        color,
        scale,
    );
}

/// Draw centered compact bold text.
#[allow(clippy::too_many_arguments)]
pub fn draw_string_centered_bold_compact(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    text: &str,
    color: Pixel,
    scale: u32,
) {
    let tw = text.len() as i32 * FONT_WIDTH_COMPACT as i32 * scale as i32;
    let th = FONT_HEIGHT_COMPACT as i32 * scale as i32;
    draw_string_bold_compact(
        fb,
        x + (width as i32 - tw) / 2,
        y + (height as i32 - th) / 2,
        text,
        color,
        scale,
    );
}

/// Draw text centered below an icon.
#[allow(clippy::too_many_arguments)]
pub fn draw_icon_label(
    fb: &mut FrameBuffer,
    icon_x: i32,
    icon_y: i32,
    icon_width: u32,
    text: &str,
    color: Pixel,
    shadow_color: Pixel,
    scale: u32,
) {
    let text_width = text.len() as i32 * FONT_WIDTH_COMPACT as i32 * scale as i32;
    let actual_icon_size: i32 = 48;
    let margin_top: i32 = 8;
    let padding_left: i32 = 8;
    let available_width = icon_width as i32 - (padding_left * 2);
    let text_offset = (available_width - text_width) / 2;
    let cx = icon_x + padding_left + text_offset;
    let cy = icon_y + actual_icon_size + margin_top;
    draw_string_shadow_compact(fb, cx, cy, text, color, shadow_color, scale);
}
