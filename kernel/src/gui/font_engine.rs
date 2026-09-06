/// Font Engine — Scalable TrueType font rendering for KnoxOS
///
/// Replaces the fixed-size bitmap fonts (Hack 10×20, 8×14, 20×40) with a
/// scalable engine that can render any embedded TTF at any point size.
///
/// Architecture: uses the existing `truetype::TtfFont` parser as backend,
/// adds a glyph bitmap cache keyed by (font_id, glyph_id, size_px),
/// and provides a simple text drawing API.
///
/// Two font families are embedded:
///   - **UI font** (proportional): for window titles, menus, buttons, labels
///   - **Mono font** (monospace): for terminal, code editor, log viewer
///
/// Rendering quality: grayscale anti-aliased by default, with gamma-correct
/// blending using the sRGB lookup tables from fonts.rs.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use super::framebuffer::{FrameBuffer, Pixel};
use super::truetype;

// ═══════════════════════════════════════════════════════════════════════
// FONT IDS
// ═══════════════════════════════════════════════════════════════════════

/// Identifies which embedded font to use
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontId {
    /// Proportional UI font (Inter / Noto Sans)
    Ui,
    /// Bold proportional UI font
    UiBold,
    /// Monospace font (Hack / JetBrains Mono)
    Mono,
}

// ═══════════════════════════════════════════════════════════════════════
// GLYPH CACHE
// ═══════════════════════════════════════════════════════════════════════

/// Cache key: (font, char_codepoint, pixel_size)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct GlyphKey {
    font: FontId,
    codepoint: u32,
    size_px: u16,
}

/// Cached rasterized glyph bitmap
struct CachedGlyph {
    /// Width in pixels
    width: u16,
    /// Height in pixels
    height: u16,
    /// X bearing (offset from pen position)
    bearing_x: i16,
    /// Y bearing (offset from baseline)
    bearing_y: i16,
    /// Advance width in pixels (distance to next glyph)
    advance: u16,
    /// 8-bit alpha coverage bitmap (row-major, width × height)
    bitmap: Vec<u8>,
}

lazy_static::lazy_static! {
    static ref GLYPH_CACHE: Mutex<BTreeMap<GlyphKey, CachedGlyph>> =
        Mutex::new(BTreeMap::new());
}

/// Maximum number of cached glyphs before eviction
const MAX_CACHE_ENTRIES: usize = 2048;

/// Font registry indices — set during init()
use core::sync::atomic::{AtomicUsize, Ordering};
static FONT_IDX_UI: AtomicUsize = AtomicUsize::new(usize::MAX);
static FONT_IDX_UI_BOLD: AtomicUsize = AtomicUsize::new(usize::MAX);
static FONT_IDX_MONO: AtomicUsize = AtomicUsize::new(usize::MAX);

// ═══════════════════════════════════════════════════════════════════════
// GAMMA LUTs (from fonts.rs — sRGB correct blending)
// ═══════════════════════════════════════════════════════════════════════

/// sRGB [0..255] → Linear [0..255] using gamma ≈ 2.2
/// Built with: round(((i/255)^2.2) * 255)
/// These MUST match the tables in fonts.rs for consistent rendering.
static SRGB_TO_LINEAR: [u8; 256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2,
    3, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 7, 7, 7, 8, 8, 8, 9, 9, 9, 10, 10, 11, 11,
    11, 12, 12, 13, 13, 13, 14, 14, 15, 15, 16, 16, 17, 17, 18, 18, 19, 19, 20, 20, 21, 22, 22, 23,
    23, 24, 25, 25, 26, 26, 27, 28, 28, 29, 30, 30, 31, 32, 33, 33, 34, 35, 35, 36, 37, 38, 39, 39,
    40, 41, 42, 43, 43, 44, 45, 46, 47, 48, 49, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61,
    62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 73, 74, 75, 76, 77, 78, 79, 81, 82, 83, 84, 85, 87, 88,
    89, 90, 91, 93, 94, 95, 97, 98, 99, 100, 102, 103, 105, 106, 107, 109, 110, 111, 113, 114, 116,
    117, 119, 120, 121, 123, 124, 126, 127, 129, 130, 132, 133, 135, 137, 138, 140, 141, 143, 145,
    146, 148, 149, 151, 153, 154, 156, 158, 159, 161, 163, 165, 166, 168, 170, 172, 173, 175, 177,
    179, 181, 182, 184, 186, 188, 190, 192, 194, 196, 197, 199, 201, 203, 205, 207, 209, 211, 213,
    215, 217, 219, 221, 223, 225, 227, 229, 231, 234, 236, 238, 240, 242, 244, 246, 248, 251, 253,
    255,
];

/// Linear [0..255] → sRGB [0..255] using gamma ≈ 1/2.2
/// Built with: round(((i/255)^(1/2.2)) * 255)
static LINEAR_TO_SRGB: [u8; 256] = [
    0, 21, 28, 34, 39, 43, 46, 50, 53, 56, 59, 61, 64, 66, 68, 70, 72, 74, 76, 78, 80, 82, 84, 85,
    87, 89, 90, 92, 93, 95, 96, 98, 99, 101, 102, 103, 105, 106, 107, 109, 110, 111, 112, 114, 115,
    116, 117, 118, 119, 120, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135,
    136, 137, 138, 139, 140, 141, 142, 143, 144, 144, 145, 146, 147, 148, 149, 150, 151, 151, 152,
    153, 154, 155, 156, 156, 157, 158, 159, 160, 160, 161, 162, 163, 164, 164, 165, 166, 167, 167,
    168, 169, 170, 170, 171, 172, 173, 173, 174, 175, 175, 176, 177, 178, 178, 179, 180, 180, 181,
    182, 182, 183, 184, 184, 185, 186, 186, 187, 188, 188, 189, 190, 190, 191, 192, 192, 193, 194,
    194, 195, 195, 196, 197, 197, 198, 199, 199, 200, 200, 201, 202, 202, 203, 203, 204, 205, 205,
    206, 206, 207, 207, 208, 209, 209, 210, 210, 211, 212, 212, 213, 213, 214, 214, 215, 215, 216,
    217, 217, 218, 218, 219, 219, 220, 220, 221, 221, 222, 223, 223, 224, 224, 225, 225, 226, 226,
    227, 227, 228, 228, 229, 229, 230, 230, 231, 231, 232, 232, 233, 233, 234, 234, 235, 235, 236,
    236, 237, 237, 238, 238, 239, 239, 240, 240, 241, 241, 242, 242, 243, 243, 244, 244, 245, 245,
    246, 246, 247, 247, 248, 248, 249, 249, 249, 250, 250, 251, 251, 252, 252, 253, 253, 254, 254,
    255, 255,
];

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the font engine with embedded font data.
/// Call this once during GUI init.
pub fn init() {
    // Load embedded fonts via the truetype font registry
    let ui_data = super::embedded_fonts::FONT_UI_DATA;
    let ui_bold_data = super::embedded_fonts::FONT_UI_BOLD_DATA;
    let mono_data = super::embedded_fonts::FONT_MONO_DATA;

    match truetype::load_font(ui_data) {
        Ok(idx) => {
            FONT_IDX_UI.store(idx, Ordering::Relaxed);
            crate::serial_println!("[FontEngine] UI font loaded (registry idx {})", idx);
        }
        Err(e) => crate::serial_println!("[FontEngine] Failed to load UI font: {}", e),
    }

    match truetype::load_font(ui_bold_data) {
        Ok(idx) => {
            FONT_IDX_UI_BOLD.store(idx, Ordering::Relaxed);
            crate::serial_println!("[FontEngine] UI Bold font loaded (registry idx {})", idx);
        }
        Err(e) => crate::serial_println!("[FontEngine] Failed to load UI Bold font: {}", e),
    }

    match truetype::load_font(mono_data) {
        Ok(idx) => {
            FONT_IDX_MONO.store(idx, Ordering::Relaxed);
            crate::serial_println!("[FontEngine] Mono font loaded (registry idx {})", idx);
        }
        Err(e) => crate::serial_println!("[FontEngine] Failed to load Mono font: {}", e),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GLYPH RASTERIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Rasterize a glyph at the given pixel size, returning cache key on success.
fn rasterize_glyph(font_id: FontId, ch: char, size_px: u16) -> Option<GlyphKey> {
    let key = GlyphKey {
        font: font_id,
        codepoint: ch as u32,
        size_px,
    };

    // Check cache first
    if GLYPH_CACHE.lock().contains_key(&key) {
        return Some(key);
    }

    // Get font registry index
    let font_idx = match font_id {
        FontId::Ui => FONT_IDX_UI.load(Ordering::Relaxed),
        FontId::UiBold => FONT_IDX_UI_BOLD.load(Ordering::Relaxed),
        FontId::Mono => FONT_IDX_MONO.load(Ordering::Relaxed),
    };
    if font_idx == usize::MAX {
        return None; // Font not loaded
    }

    // Rasterize via truetype module
    let bitmap = truetype::rasterize_glyph(font_idx, ch, size_px as f32)?;

    let cached = CachedGlyph {
        width: bitmap.width as u16,
        height: bitmap.height as u16,
        bearing_x: bitmap.bearing_x as i16,
        bearing_y: bitmap.bearing_y as i16,
        advance: bitmap.advance as u16,
        bitmap: bitmap.pixels,
    };

    let mut cache = GLYPH_CACHE.lock();
    // Evict oldest entries if cache is full
    if cache.len() >= MAX_CACHE_ENTRIES {
        let to_remove: Vec<GlyphKey> = cache.keys().take(MAX_CACHE_ENTRIES / 4).copied().collect();
        for k in to_remove {
            cache.remove(&k);
        }
    }
    cache.insert(key, cached);
    Some(key)
}

// ═══════════════════════════════════════════════════════════════════════
// TEXT DRAWING
// ═══════════════════════════════════════════════════════════════════════

/// Draw a string using the font engine.
///
/// - `fb`: target framebuffer
/// - `x, y`: top-left position (y is the top of the text line, not baseline)
/// - `text`: UTF-8 string to render
/// - `font`: which font to use
/// - `size_px`: pixel height of the font
/// - `color`: text color (alpha is used for sub-pixel blending)
pub fn draw_text(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    text: &str,
    font: FontId,
    size_px: u16,
    color: Pixel,
) {
    let mut pen_x = x;
    let baseline_y = y + size_px as i32; // approximate baseline

    for ch in text.chars() {
        if ch == '\n' {
            pen_x = x;
            continue;
        }

        if let Some(key) = rasterize_glyph(font, ch, size_px) {
            let cache = GLYPH_CACHE.lock();
            if let Some(glyph) = cache.get(&key) {
                let gx = pen_x + glyph.bearing_x as i32;
                let gy = baseline_y - glyph.bearing_y as i32;

                // Blit the alpha bitmap with gamma-correct blending
                blit_glyph_aa(fb, gx, gy, glyph.width, glyph.height, &glyph.bitmap, color);

                pen_x += glyph.advance as i32;
            }
        } else {
            // Fallback: skip unknown glyphs, advance by approximate width
            pen_x += (size_px as i32 * 6) / 10;
        }
    }
}

/// Measure the width of a string in pixels (without drawing).
pub fn measure_text(text: &str, font: FontId, size_px: u16) -> u32 {
    let mut width: i32 = 0;

    for ch in text.chars() {
        if let Some(key) = rasterize_glyph(font, ch, size_px) {
            let cache = GLYPH_CACHE.lock();
            if let Some(glyph) = cache.get(&key) {
                width += glyph.advance as i32;
            }
        } else {
            width += (size_px as i32 * 6) / 10;
        }
    }

    width.max(0) as u32
}

/// Sharpen coverage to reduce AA fringe blur (matches fonts.rs behavior).
#[inline(always)]
fn sharpen_coverage(cov: u8) -> u8 {
    if cov == 0 || cov == 255 {
        return cov;
    }
    let c = cov as i32;
    let delta = (c - 128) * 128 / 256; // strength = 128
    let result = c + delta;
    result.clamp(0, 255) as u8
}

/// Apply stem darkening to improve thin-stem legibility (matches fonts.rs).
#[inline(always)]
fn apply_stem_darkening(coverage: u8) -> u8 {
    if coverage == 0 || coverage == 255 {
        return coverage;
    }
    let c = coverage as u32;
    let boost = 25u32 * (255 - c) / 255; // factor = 25
    (c + boost).min(255) as u8
}

/// Blit an alpha-coverage glyph bitmap onto the framebuffer with gamma-correct blending.
fn blit_glyph_aa(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: u16,
    h: u16,
    bitmap: &[u8],
    color: Pixel,
) {
    let fb_w = fb.width as i32;
    let fb_h = fb.height as i32;

    // Pre-convert text color to linear space
    let cr_lin = SRGB_TO_LINEAR[color.r as usize] as u32;
    let cg_lin = SRGB_TO_LINEAR[color.g as usize] as u32;
    let cb_lin = SRGB_TO_LINEAR[color.b as usize] as u32;

    for row in 0..h as i32 {
        let py = y + row;
        if py < 0 || py >= fb_h {
            continue;
        }
        for col in 0..w as i32 {
            let px = x + col;
            if px < 0 || px >= fb_w {
                continue;
            }
            let raw_alpha = bitmap[(row as usize) * (w as usize) + col as usize];
            if raw_alpha == 0 {
                continue;
            }

            // Apply coverage sharpening and stem darkening for crisp text
            let alpha = apply_stem_darkening(sharpen_coverage(raw_alpha));

            if alpha == 255 {
                fb.set_pixel(px as usize, py as usize, color);
            } else {
                // Gamma-correct alpha blending in linear light space
                let bg = fb.get_pixel(px as usize, py as usize);
                let bg_r_lin = SRGB_TO_LINEAR[bg.r as usize] as u32;
                let bg_g_lin = SRGB_TO_LINEAR[bg.g as usize] as u32;
                let bg_b_lin = SRGB_TO_LINEAR[bg.b as usize] as u32;

                let a = alpha as u32;
                let inv_a = 255 - a;

                let r_lin = (cr_lin * a + bg_r_lin * inv_a + 128) / 255;
                let g_lin = (cg_lin * a + bg_g_lin * inv_a + 128) / 255;
                let b_lin = (cb_lin * a + bg_b_lin * inv_a + 128) / 255;

                let r = LINEAR_TO_SRGB[r_lin.min(255) as usize];
                let g = LINEAR_TO_SRGB[g_lin.min(255) as usize];
                let b = LINEAR_TO_SRGB[b_lin.min(255) as usize];

                fb.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CONVENIENCE FUNCTIONS (backward-compatible API)
// ═══════════════════════════════════════════════════════════════════════

/// Draw UI text (proportional font) at a given size.
pub fn draw_ui_text(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, size_px: u16, color: Pixel) {
    draw_text(fb, x, y, text, FontId::Ui, size_px, color);
}

/// Draw bold UI text.
pub fn draw_ui_bold(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, size_px: u16, color: Pixel) {
    draw_text(fb, x, y, text, FontId::UiBold, size_px, color);
}

/// Draw monospace text (terminal, code).
pub fn draw_mono_text(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    text: &str,
    size_px: u16,
    color: Pixel,
) {
    draw_text(fb, x, y, text, FontId::Mono, size_px, color);
}

/// Measure UI text width.
pub fn measure_ui_text(text: &str, size_px: u16) -> u32 {
    measure_text(text, FontId::Ui, size_px)
}

/// Measure monospace text width.
pub fn measure_mono_text(text: &str, size_px: u16) -> u32 {
    measure_text(text, FontId::Mono, size_px)
}

/// Clear the glyph cache (call on theme/font change).
pub fn clear_cache() {
    GLYPH_CACHE.lock().clear();
}

/// Default UI font size in pixels
pub const UI_SIZE: u16 = 14;
/// Small UI font size (labels, status bars)
pub const UI_SIZE_SMALL: u16 = 11;
/// Large UI font size (window titles, headings)
pub const UI_SIZE_LARGE: u16 = 16;
/// Terminal/mono font size
pub const MONO_SIZE: u16 = 14;
