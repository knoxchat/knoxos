#!/usr/bin/env python3
"""Generate the new fonts.rs that uses pre-baked AA coverage directly."""

import os

CONTENT = '''/// Font Rendering \u2014 GNOME-quality antialiased text for KnoxOS
///
/// Pre-baked 8-bit alpha coverage glyphs generated from Hack-Regular bitmaps
/// using 16x supersampling with Gaussian smoothing, gamma correction, and
/// stem darkening. No runtime convolution needed \u2014 just alpha-blend directly.
///
/// Two sizes:
///   - Standard: 10x20 (UI text, window titles, menus, terminal)
///   - Compact:  8x14  (status bars, labels, small text)
///
/// Gamma-correct blending: all alpha compositing operates in linear light space
/// for correct color mixing without dark fringing halos.
use super::framebuffer::{FrameBuffer, Pixel};
use super::hack_font;

/// Font cell dimensions \u2014 standard 10x20 (high-quality AA monospace)
pub const FONT_WIDTH: usize = hack_font::GLYPH_WIDTH;
pub const FONT_HEIGHT: usize = hack_font::GLYPH_HEIGHT;
/// Compact font dimensions \u2014 8x14 (for small UI text)
pub const FONT_WIDTH_COMPACT: usize = hack_font::GLYPH_WIDTH_COMPACT;
pub const FONT_HEIGHT_COMPACT: usize = hack_font::GLYPH_HEIGHT_COMPACT;

/// Line spacing: 2px extra for comfortable reading
const LINE_SPACING_EXTRA: i32 = 2;
/// Compact line spacing: 1px extra
const LINE_SPACING_EXTRA_COMPACT: i32 = 1;

/// Text rendering quality levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextQuality {
    /// Grayscale antialiased (highest quality, pre-baked AA)
    Grayscale,
    /// Legacy 1-bit rendering (fast fallback)
    Legacy,
    /// Subpixel placeholder (falls back to grayscale)
    Subpixel,
}

/// Global rendering quality setting
static TEXT_QUALITY: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Set the global text rendering quality
pub fn set_text_quality(quality: TextQuality) {
    let val = match quality {
        TextQuality::Grayscale | TextQuality::Subpixel => 0,
        TextQuality::Legacy => 1,
    };
    TEXT_QUALITY.store(val, core::sync::atomic::Ordering::Relaxed);
}

/// Get the current text rendering quality
pub fn get_text_quality() -> TextQuality {
    match TEXT_QUALITY.load(core::sync::atomic::Ordering::Relaxed) {
        0 => TextQuality::Grayscale,
        _ => TextQuality::Legacy,
    }
}

/// Stem darkening factor. Default = 30.
static STEM_DARKENING: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(30);

/// Set stem darkening factor (0-255)
pub fn set_stem_darkening(factor: u8) {
    STEM_DARKENING.store(factor, core::sync::atomic::Ordering::Relaxed);
}

#[inline(always)]
fn apply_stem_darkening(coverage: u8) -> u8 {
    if coverage == 0 || coverage == 255 { return coverage; }
    let dark = STEM_DARKENING.load(core::sync::atomic::Ordering::Relaxed) as u32;
    if dark == 0 { return coverage; }
    let c = coverage as u32;
    let boost = dark * (255 - c) / 255;
    (c + boost).min(255) as u8
}

// sRGB <-> Linear Conversion
static SRGB_TO_LINEAR: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut i = 0u32;
    while i < 256 {
        let val = (i * i + 127) / 255;
        table[i as usize] = if val > 255 { 255 } else { val as u8 };
        i += 1;
    }
    table
};

static LINEAR_TO_SRGB: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut i = 0u32;
    while i < 256 {
        let n = i * 255;
        let mut guess = n;
        if guess > 0 {
            let mut x = n;
            let mut y = (x + 1) / 2;
            while y < x { x = y; y = (x + n / x) / 2; }
            guess = x;
        }
        table[i as usize] = if guess > 255 { 255 } else { guess as u8 };
        i += 1;
    }
    table
};

#[inline(always)]
fn gamma_blend_channel(fg: u8, bg: u8, alpha: u16) -> u8 {
    if alpha == 255 { return fg; }
    if alpha == 0 { return bg; }
    let fg_lin = SRGB_TO_LINEAR[fg as usize] as u16;
    let bg_lin = SRGB_TO_LINEAR[bg as usize] as u16;
    let inv_alpha = 255 - alpha;
    let blended = ((fg_lin * alpha + bg_lin * inv_alpha) + 128) >> 8;
    LINEAR_TO_SRGB[blended.min(255) as usize]
}

// Character Drawing \u2014 Pre-baked AA Coverage

/// Draw a single character (standard 10x20).
pub fn draw_char(fb: &mut FrameBuffer, x: i32, y: i32, ch: char, color: Pixel, scale: u32) {
    if let Some(coverage) = hack_font::get_glyph(ch) {
        draw_aa_glyph(fb, x, y, coverage.as_ptr() as *const u8,
                       hack_font::GLYPH_WIDTH, hack_font::GLYPH_HEIGHT, color, scale);
    }
}

/// Draw a single character (compact 8x14).
pub fn draw_char_compact(fb: &mut FrameBuffer, x: i32, y: i32, ch: char, color: Pixel, scale: u32) {
    if let Some(coverage) = hack_font::get_glyph_compact(ch) {
        draw_aa_glyph(fb, x, y, coverage.as_ptr() as *const u8,
                       hack_font::GLYPH_WIDTH_COMPACT, hack_font::GLYPH_HEIGHT_COMPACT, color, scale);
    }
}

/// Draw a bold character (standard 10x20).
pub fn draw_char_bold(fb: &mut FrameBuffer, x: i32, y: i32, ch: char, color: Pixel, scale: u32) {
    if let Some(coverage) = hack_font::get_glyph(ch) {
        draw_bold_glyph_std(fb, x, y, coverage, color, scale);
    }
}

/// Draw a bold character (compact 8x14).
pub fn draw_char_bold_compact(fb: &mut FrameBuffer, x: i32, y: i32, ch: char, color: Pixel, scale: u32) {
    if let Some(coverage) = hack_font::get_glyph_compact(ch) {
        draw_bold_glyph_compact(fb, x, y, coverage, color, scale);
    }
}

/// Core AA glyph renderer: reads pre-baked alpha and blends with gamma correction.
fn draw_aa_glyph(
    fb: &mut FrameBuffer, x: i32, y: i32,
    data: *const u8, gw: usize, gh: usize,
    color: Pixel, scale: u32,
) {
    if scale == 1 {
        for row in 0..gh {
            let py = y + row as i32;
            if py < 0 || py >= fb.height as i32 { continue; }
            for col in 0..gw {
                let cov = unsafe { *data.add(row * gw + col) };
                if cov == 0 { continue; }
                let px = x + col as i32;
                if px < 0 || px >= fb.width as i32 { continue; }
                let cov = apply_stem_darkening(cov);
                if cov == 255 {
                    fb.set_pixel(px as usize, py as usize, color);
                } else {
                    let bg = fb.get_pixel(px as usize, py as usize);
                    let alpha = (color.a as u16 * cov as u16 + 128) >> 8;
                    let r = gamma_blend_channel(color.r, bg.r, alpha);
                    let g = gamma_blend_channel(color.g, bg.g, alpha);
                    let b = gamma_blend_channel(color.b, bg.b, alpha);
                    fb.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
                }
            }
        }
    } else {
        let s = scale as i32;
        for row in 0..gh {
            for col in 0..gw {
                let cov = unsafe { *data.add(row * gw + col) };
                if cov == 0 { continue; }
                let cov = apply_stem_darkening(cov);
                let bx = x + col as i32 * s;
                let by = y + row as i32 * s;
                for sy in 0..s {
                    for sx in 0..s {
                        let px = bx + sx;
                        let py = by + sy;
                        if px < 0 || py < 0 || px >= fb.width as i32 || py >= fb.height as i32 { continue; }
                        if cov == 255 {
                            fb.set_pixel(px as usize, py as usize, color);
                        } else {
                            let bg = fb.get_pixel(px as usize, py as usize);
                            let alpha = (color.a as u16 * cov as u16 + 128) >> 8;
                            let r = gamma_blend_channel(color.r, bg.r, alpha);
                            let g = gamma_blend_channel(color.g, bg.g, alpha);
                            let b = gamma_blend_channel(color.b, bg.b, alpha);
                            fb.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
                        }
                    }
                }
            }
        }
    }
}

/// Bold renderer for standard glyphs \u2014 synthetic bold via max(here, left).
fn draw_bold_glyph_std(
    fb: &mut FrameBuffer, x: i32, y: i32,
    coverage: &[[u8; hack_font::GLYPH_WIDTH]; hack_font::GLYPH_HEIGHT],
    color: Pixel, scale: u32,
) {
    let gw = hack_font::GLYPH_WIDTH;
    let gh = hack_font::GLYPH_HEIGHT;
    if scale == 1 {
        for row in 0..gh {
            let py = y + row as i32;
            if py < 0 || py >= fb.height as i32 { continue; }
            for col in 0..gw {
                let c0 = coverage[row][col];
                let cl = if col > 0 { coverage[row][col - 1] } else { 0 };
                let cov = c0.max(cl);
                if cov == 0 { continue; }
                let px = x + col as i32;
                if px < 0 || px >= fb.width as i32 { continue; }
                let cov = apply_stem_darkening(cov);
                if cov == 255 {
                    fb.set_pixel(px as usize, py as usize, color);
                } else {
                    let bg = fb.get_pixel(px as usize, py as usize);
                    let alpha = (color.a as u16 * cov as u16 + 128) >> 8;
                    let r = gamma_blend_channel(color.r, bg.r, alpha);
                    let g = gamma_blend_channel(color.g, bg.g, alpha);
                    let b = gamma_blend_channel(color.b, bg.b, alpha);
                    fb.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
                }
            }
        }
    } else {
        let s = scale as i32;
        for row in 0..gh {
            for col in 0..gw {
                let c0 = coverage[row][col];
                let cl = if col > 0 { coverage[row][col - 1] } else { 0 };
                let cov = c0.max(cl);
                if cov == 0 { continue; }
                let cov = apply_stem_darkening(cov);
                let bx = x + col as i32 * s;
                let by = y + row as i32 * s;
                for sy in 0..s { for sx in 0..s {
                    let px = bx + sx; let py = by + sy;
                    if px < 0 || py < 0 || px >= fb.width as i32 || py >= fb.height as i32 { continue; }
                    if cov == 255 {
                        fb.set_pixel(px as usize, py as usize, color);
                    } else {
                        let bg = fb.get_pixel(px as usize, py as usize);
                        let alpha = (color.a as u16 * cov as u16 + 128) >> 8;
                        let r = gamma_blend_channel(color.r, bg.r, alpha);
                        let g = gamma_blend_channel(color.g, bg.g, alpha);
                        let b = gamma_blend_channel(color.b, bg.b, alpha);
                        fb.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
                    }
                }}
            }
        }
    }
}

/// Bold renderer for compact glyphs.
fn draw_bold_glyph_compact(
    fb: &mut FrameBuffer, x: i32, y: i32,
    coverage: &[[u8; hack_font::GLYPH_WIDTH_COMPACT]; hack_font::GLYPH_HEIGHT_COMPACT],
    color: Pixel, scale: u32,
) {
    let gw = hack_font::GLYPH_WIDTH_COMPACT;
    let gh = hack_font::GLYPH_HEIGHT_COMPACT;
    if scale == 1 {
        for row in 0..gh {
            let py = y + row as i32;
            if py < 0 || py >= fb.height as i32 { continue; }
            for col in 0..gw {
                let c0 = coverage[row][col];
                let cl = if col > 0 { coverage[row][col - 1] } else { 0 };
                let cov = c0.max(cl);
                if cov == 0 { continue; }
                let px = x + col as i32;
                if px < 0 || px >= fb.width as i32 { continue; }
                let cov = apply_stem_darkening(cov);
                if cov == 255 {
                    fb.set_pixel(px as usize, py as usize, color);
                } else {
                    let bg = fb.get_pixel(px as usize, py as usize);
                    let alpha = (color.a as u16 * cov as u16 + 128) >> 8;
                    let r = gamma_blend_channel(color.r, bg.r, alpha);
                    let g = gamma_blend_channel(color.g, bg.g, alpha);
                    let b = gamma_blend_channel(color.b, bg.b, alpha);
                    fb.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
                }
            }
        }
    } else {
        let s = scale as i32;
        for row in 0..gh {
            for col in 0..gw {
                let c0 = coverage[row][col];
                let cl = if col > 0 { coverage[row][col - 1] } else { 0 };
                let cov = c0.max(cl);
                if cov == 0 { continue; }
                let cov = apply_stem_darkening(cov);
                let bx = x + col as i32 * s;
                let by = y + row as i32 * s;
                for sy in 0..s { for sx in 0..s {
                    let px = bx + sx; let py = by + sy;
                    if px < 0 || py < 0 || px >= fb.width as i32 || py >= fb.height as i32 { continue; }
                    if cov == 255 {
                        fb.set_pixel(px as usize, py as usize, color);
                    } else {
                        let bg = fb.get_pixel(px as usize, py as usize);
                        let alpha = (color.a as u16 * cov as u16 + 128) >> 8;
                        let r = gamma_blend_channel(color.r, bg.r, alpha);
                        let g = gamma_blend_channel(color.g, bg.g, alpha);
                        let b = gamma_blend_channel(color.b, bg.b, alpha);
                        fb.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
                    }
                }}
            }
        }
    }
}

// String Drawing Functions

/// Draw a string using standard glyphs (10x20).
pub fn draw_string(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel, scale: u32) {
    let char_width = (FONT_WIDTH as u32 * scale) as i32;
    let line_height = (FONT_HEIGHT as u32 * scale) as i32 + LINE_SPACING_EXTRA;
    let mut cx = x;
    let mut cy = y;
    for ch in text.chars() {
        if ch == '\\n' { cx = x; cy += line_height; continue; }
        draw_char(fb, cx, cy, ch, color, scale);
        cx += char_width;
    }
}

/// Draw a bold string (standard 10x20).
pub fn draw_string_bold(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel, scale: u32) {
    let char_width = (FONT_WIDTH as u32 * scale) as i32;
    let line_height = (FONT_HEIGHT as u32 * scale) as i32 + LINE_SPACING_EXTRA;
    let mut cx = x;
    let mut cy = y;
    for ch in text.chars() {
        if ch == '\\n' { cx = x; cy += line_height; continue; }
        draw_char_bold(fb, cx, cy, ch, color, scale);
        cx += char_width;
    }
}

/// Draw a string using compact glyphs (8x14).
pub fn draw_string_compact(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel, scale: u32) {
    let char_width = (FONT_WIDTH_COMPACT as u32 * scale) as i32;
    let line_height = (FONT_HEIGHT_COMPACT as u32 * scale) as i32 + LINE_SPACING_EXTRA_COMPACT;
    let mut cx = x;
    let mut cy = y;
    for ch in text.chars() {
        if ch == '\\n' { cx = x; cy += line_height; continue; }
        draw_char_compact(fb, cx, cy, ch, color, scale);
        cx += char_width;
    }
}

/// Draw a bold string (compact 8x14).
pub fn draw_string_bold_compact(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel, scale: u32) {
    let char_width = (FONT_WIDTH_COMPACT as u32 * scale) as i32;
    let line_height = (FONT_HEIGHT_COMPACT as u32 * scale) as i32 + LINE_SPACING_EXTRA_COMPACT;
    let mut cx = x;
    let mut cy = y;
    for ch in text.chars() {
        if ch == '\\n' { cx = x; cy += line_height; continue; }
        draw_char_bold_compact(fb, cx, cy, ch, color, scale);
        cx += char_width;
    }
}

/// Draw a string with a 1px drop shadow.
pub fn draw_string_shadow(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel, shadow_color: Pixel, scale: u32) {
    let soft = Pixel::new(shadow_color.r, shadow_color.g, shadow_color.b, (shadow_color.a as u16 * 128 / 255) as u8);
    draw_string(fb, x + 1, y + 1, text, soft, scale);
    draw_string(fb, x, y, text, color, scale);
}

/// Draw a compact string with 1px drop shadow.
pub fn draw_string_shadow_compact(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel, shadow_color: Pixel, scale: u32) {
    let soft = Pixel::new(shadow_color.r, shadow_color.g, shadow_color.b, (shadow_color.a as u16 * 128 / 255) as u8);
    draw_string_compact(fb, x + 1, y + 1, text, soft, scale);
    draw_string_compact(fb, x, y, text, color, scale);
}

/// Draw centered text within a bounding box.
#[allow(clippy::too_many_arguments)]
pub fn draw_string_centered(fb: &mut FrameBuffer, x: i32, y: i32, width: u32, height: u32, text: &str, color: Pixel, scale: u32) {
    let tw = text.len() as i32 * FONT_WIDTH as i32 * scale as i32;
    let th = FONT_HEIGHT as i32 * scale as i32;
    draw_string(fb, x + (width as i32 - tw) / 2, y + (height as i32 - th) / 2, text, color, scale);
}

/// Draw centered bold text.
#[allow(clippy::too_many_arguments)]
pub fn draw_string_centered_bold(fb: &mut FrameBuffer, x: i32, y: i32, width: u32, height: u32, text: &str, color: Pixel, scale: u32) {
    let tw = text.len() as i32 * FONT_WIDTH as i32 * scale as i32;
    let th = FONT_HEIGHT as i32 * scale as i32;
    draw_string_bold(fb, x + (width as i32 - tw) / 2, y + (height as i32 - th) / 2, text, color, scale);
}

/// Draw centered compact text.
#[allow(clippy::too_many_arguments)]
pub fn draw_string_centered_compact(fb: &mut FrameBuffer, x: i32, y: i32, width: u32, height: u32, text: &str, color: Pixel, scale: u32) {
    let tw = text.len() as i32 * FONT_WIDTH_COMPACT as i32 * scale as i32;
    let th = FONT_HEIGHT_COMPACT as i32 * scale as i32;
    draw_string_compact(fb, x + (width as i32 - tw) / 2, y + (height as i32 - th) / 2, text, color, scale);
}

/// Draw centered compact bold text.
#[allow(clippy::too_many_arguments)]
pub fn draw_string_centered_bold_compact(fb: &mut FrameBuffer, x: i32, y: i32, width: u32, height: u32, text: &str, color: Pixel, scale: u32) {
    let tw = text.len() as i32 * FONT_WIDTH_COMPACT as i32 * scale as i32;
    let th = FONT_HEIGHT_COMPACT as i32 * scale as i32;
    draw_string_bold_compact(fb, x + (width as i32 - tw) / 2, y + (height as i32 - th) / 2, text, color, scale);
}

/// Draw text centered below an icon.
#[allow(clippy::too_many_arguments)]
pub fn draw_icon_label(fb: &mut FrameBuffer, icon_x: i32, icon_y: i32, icon_width: u32, text: &str, color: Pixel, shadow_color: Pixel, scale: u32) {
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

/// Measure string width (standard font)
pub fn measure_string_width(text: &str, scale: u32) -> u32 {
    text.len() as u32 * FONT_WIDTH as u32 * scale
}

/// Measure string width (compact font)
pub fn measure_string_width_compact(text: &str, scale: u32) -> u32 {
    text.len() as u32 * FONT_WIDTH_COMPACT as u32 * scale
}

/// Measure string height (standard)
pub fn measure_string_height(scale: u32) -> u32 { FONT_HEIGHT as u32 * scale }

/// Measure string height (compact)
pub fn measure_string_height_compact(scale: u32) -> u32 { FONT_HEIGHT_COMPACT as u32 * scale }

/// Word-wrap text to fit within a given width.
pub fn word_wrap(text: &str, max_width: u32, scale: u32) -> alloc::vec::Vec<alloc::string::String> {
    let mut lines = alloc::vec::Vec::new();
    let char_width = FONT_WIDTH as u32 * scale;
    let max_chars = if char_width > 0 { (max_width / char_width) as usize } else { return lines };
    if max_chars == 0 { return lines; }
    for line in text.split('\\n') {
        if line.len() <= max_chars {
            lines.push(alloc::string::String::from(line));
        } else {
            let words: alloc::vec::Vec<&str> = line.split_whitespace().collect();
            let mut current_line = alloc::string::String::new();
            for word in words {
                if current_line.is_empty() {
                    if word.len() > max_chars {
                        let mut remaining = word;
                        while remaining.len() > max_chars {
                            lines.push(alloc::string::String::from(&remaining[..max_chars]));
                            remaining = &remaining[max_chars..];
                        }
                        current_line = alloc::string::String::from(remaining);
                    } else { current_line.push_str(word); }
                } else if current_line.len() + 1 + word.len() <= max_chars {
                    current_line.push(' ');
                    current_line.push_str(word);
                } else {
                    lines.push(current_line);
                    current_line = alloc::string::String::from(word);
                }
            }
            if !current_line.is_empty() { lines.push(current_line); }
        }
    }
    lines
}

/// Draw text with word wrapping. Returns total height consumed.
pub fn draw_text_wrapped(fb: &mut FrameBuffer, x: i32, y: i32, max_width: u32, text: &str, color: Pixel, scale: u32) -> i32 {
    let lines = word_wrap(text, max_width, scale);
    let line_h = (FONT_HEIGHT as u32 * scale) as i32 + LINE_SPACING_EXTRA;
    for (i, line) in lines.iter().enumerate() {
        draw_string(fb, x, y + i as i32 * line_h, line, color, scale);
    }
    lines.len() as i32 * line_h
}

/// Draw text with word wrapping (compact font).
pub fn draw_text_wrapped_compact(fb: &mut FrameBuffer, x: i32, y: i32, max_width: u32, text: &str, color: Pixel, scale: u32) -> i32 {
    let char_width = FONT_WIDTH_COMPACT as u32 * scale;
    let max_chars = if char_width > 0 { (max_width / char_width) as usize } else { return 0 };
    if max_chars == 0 { return 0; }
    let mut lines = alloc::vec::Vec::new();
    for line in text.split('\\n') {
        if line.len() <= max_chars {
            lines.push(alloc::string::String::from(line));
        } else {
            let words: alloc::vec::Vec<&str> = line.split_whitespace().collect();
            let mut current = alloc::string::String::new();
            for word in words {
                if current.is_empty() {
                    if word.len() > max_chars {
                        let mut rem = word;
                        while rem.len() > max_chars {
                            lines.push(alloc::string::String::from(&rem[..max_chars]));
                            rem = &rem[max_chars..];
                        }
                        current = alloc::string::String::from(rem);
                    } else { current.push_str(word); }
                } else if current.len() + 1 + word.len() <= max_chars {
                    current.push(' ');
                    current.push_str(word);
                } else {
                    lines.push(current);
                    current = alloc::string::String::from(word);
                }
            }
            if !current.is_empty() { lines.push(current); }
        }
    }
    let line_h = (FONT_HEIGHT_COMPACT as u32 * scale) as i32 + LINE_SPACING_EXTRA_COMPACT;
    for (i, line) in lines.iter().enumerate() {
        draw_string_compact(fb, x, y + i as i32 * line_h, line, color, scale);
    }
    lines.len() as i32 * line_h
}

/// Truncate with ellipsis (standard font)
pub fn truncate_with_ellipsis(text: &str, max_width: u32, scale: u32) -> alloc::string::String {
    let char_width = FONT_WIDTH as u32 * scale;
    let max_chars = if char_width > 0 { (max_width / char_width) as usize } else { return alloc::string::String::from(text) };
    if text.len() <= max_chars || max_chars < 4 { return alloc::string::String::from(text); }
    let mut result = alloc::string::String::from(&text[..max_chars - 3]);
    result.push_str("...");
    result
}

/// Truncate with ellipsis (compact font)
pub fn truncate_with_ellipsis_compact(text: &str, max_width: u32, scale: u32) -> alloc::string::String {
    let char_width = FONT_WIDTH_COMPACT as u32 * scale;
    let max_chars = if char_width > 0 { (max_width / char_width) as usize } else { return alloc::string::String::from(text) };
    if text.len() <= max_chars || max_chars < 4 { return alloc::string::String::from(text); }
    let mut result = alloc::string::String::from(&text[..max_chars - 3]);
    result.push_str("...");
    result
}
'''

def main():
    out_path = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                            '..', 'kernel', 'src', 'gui', 'fonts.rs')
    with open(out_path, 'w') as f:
        f.write(CONTENT)
    print(f"Written {out_path} ({len(CONTENT)} bytes)")

if __name__ == "__main__":
    main()
