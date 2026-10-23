/// Font Engine — Scalable TrueType font rendering for KnoxOS
///
/// UI text is Inter via `fontdue` — the same rasterizer, faces, and metrics
/// as `os/kernel/knox-kernel/src/display/font.rs`. Terminal/code uses Hack.
///
/// Architecture: `inter_font` caches `(face, size, glyph)` coverage; this
/// module blits with gamma-correct sRGB blending and exposes the FontId API.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use super::framebuffer::{FrameBuffer, Pixel};
use super::inter_font::{self, Face};
use super::truetype;

// ═══════════════════════════════════════════════════════════════════════
// FONT IDS
// ═══════════════════════════════════════════════════════════════════════

/// Identifies which embedded font to use
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontId {
    /// Proportional UI font (Inter Regular)
    Ui,
    /// Bold proportional UI font (Inter Bold)
    UiBold,
    /// Monospace font (Hack Regular)
    Mono,
}

fn face_for(font: FontId) -> Face {
    match font {
        // Same weights the `os` chrome uses: Medium for body, SemiBold for titles.
        FontId::Ui => Face::Inter(inter_font::Weight::Medium),
        FontId::UiBold => Face::Inter(inter_font::Weight::SemiBold),
        FontId::Mono => Face::Mono,
    }
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
    inter_font::init();

    // Keep the truetype registry populated for GSUB/GPOS / fallback scans.
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
    super::embedded_fonts::install_system_fonts();
    let _ = title_self_test();
}

/// True when the Inter faces have been registered.
pub fn is_ready() -> bool {
    inter_font::is_ready()
}

pub const GATE_FONT_TITLE_MARKER: &str = "GATE_FONT title";

fn title_self_test() -> bool {
    let _ = rasterize_glyph(FontId::UiBold, 'i', 13);
    let _ = rasterize_glyph(FontId::Ui, ':', 13);
    let _ = rasterize_glyph(FontId::Ui, 'm', 13);
    let _ = rasterize_glyph(FontId::Ui, 'e', 13);
    let _ = rasterize_glyph(FontId::Ui, 's', 13);
    let _ = rasterize_glyph(FontId::Ui, 'F', 13);
    let (coverage, colon_runs, m_ink, e_ink, s_ink, f_ink) = {
        let cache = GLYPH_CACHE.lock();
        let i_key = GlyphKey {
            font: FontId::UiBold,
            codepoint: b'i' as u32,
            size_px: 13,
        };
        let colon_key = GlyphKey {
            font: FontId::Ui,
            codepoint: b':' as u32,
            size_px: 13,
        };
        let m_key = GlyphKey {
            font: FontId::Ui,
            codepoint: b'm' as u32,
            size_px: 13,
        };
        let coverage = cache
            .get(&i_key)
            .map(|g| g.bitmap.iter().any(|&b| b > 40) && g.advance >= 3)
            .unwrap_or(false);
        let colon_runs = cache
            .get(&colon_key)
            .map(|g| ink_row_runs(&g.bitmap, g.width, g.height))
            .unwrap_or(0);
        let m_ink = cache
            .get(&m_key)
            .map(|g| g.bitmap.iter().filter(|&&b| b > 40).count())
            .unwrap_or(0);
        let e_key = GlyphKey {
            font: FontId::Ui,
            codepoint: b'e' as u32,
            size_px: 13,
        };
        let s_key = GlyphKey {
            font: FontId::Ui,
            codepoint: b's' as u32,
            size_px: 13,
        };
        let f_key = GlyphKey {
            font: FontId::Ui,
            codepoint: b'F' as u32,
            size_px: 13,
        };
        let e_ink = cache
            .get(&e_key)
            .map(|g| g.bitmap.iter().filter(|&&b| b > 80).count())
            .unwrap_or(0);
        let s_ink = cache
            .get(&s_key)
            .map(|g| g.bitmap.iter().filter(|&&b| b > 80).count())
            .unwrap_or(0);
        let f_ink = cache
            .get(&f_key)
            .map(|g| g.bitmap.iter().filter(|&&b| b > 80).count())
            .unwrap_or(0);
        (coverage, colon_runs, m_ink, e_ink, s_ink, f_ink)
    };
    let full = measure_text("Terminal", FontId::UiBold, 13);
    let gap = measure_text("Termnal", FontId::UiBold, 13);
    let files = measure_text("Files", FontId::Ui, 13);
    let holes = measure_text("Fls", FontId::Ui, 13);
    // Composite ':' is two dots; a failed composite decode is empty or one blob.
    let ok = coverage
        && full > gap
        && files > holes
        && colon_runs >= 2
        && m_ink >= 12
        && e_ink >= 16
        && s_ink >= 12
        && f_ink >= 12;
    if ok {
        crate::serial_println!(
            "[fonts] {} (\"Terminal\" {}px > \"Termnal\" {}px Files={} colon_runs={} m_ink={} e={} s={} F={})",
            GATE_FONT_TITLE_MARKER,
            full,
            gap,
            files,
            colon_runs,
            m_ink,
            e_ink,
            s_ink,
            f_ink
        );
        true
    } else {
        crate::serial_println!(
            "[fonts] GATE_FONT FAILED title cov={} full={} gap={} files={} holes={} colon_runs={} m_ink={} e={} s={} F={}",
            coverage,
            full,
            gap,
            files,
            holes,
            colon_runs,
            m_ink,
            e_ink,
            s_ink,
            f_ink
        );
        false
    }
}

fn ink_row_runs(bitmap: &[u8], width: u16, height: u16) -> u32 {
    let w = width as usize;
    if w == 0 {
        return 0;
    }
    let mut runs = 0u32;
    let mut prev = false;
    for row in 0..height as usize {
        let start = row * w;
        let end = (start + w).min(bitmap.len());
        let ink = bitmap[start..end].iter().any(|&b| b > 40);
        if ink && !prev {
            runs += 1;
        }
        prev = ink;
    }
    runs
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

    let mut glyph = inter_font::rasterize(face_for(font_id), ch, size_px)?;
    let empty = glyph.width == 0 || glyph.height == 0 || glyph.coverage.iter().all(|&p| p == 0);
    // Last-resort stem so a failed outline still occupies the advance.
    if empty && ch != ' ' && ch.is_ascii_alphabetic() {
        let h = (size_px as usize * 3 / 4).max(4);
        let w = 2usize;
        glyph.width = w;
        glyph.height = h;
        glyph.xmin = 0;
        glyph.bearing_y = h as i16;
        glyph.advance = (size_px as f32 / 4.0).max(3.0);
        glyph.coverage = vec![220u8; w * h];
    }
    // Ceil so a 9.2px Inter advance cannot land the next stem on this bowl.
    let advance = libm::ceilf(glyph.advance).max(1.0) as u16;

    let cached = CachedGlyph {
        width: glyph.width as u16,
        height: glyph.height as u16,
        bearing_x: glyph.xmin as i16,
        bearing_y: glyph.bearing_y,
        advance,
        bitmap: glyph.coverage,
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

/// Coverage bitmap for Knox GPUI (`PlatformTextSystem`, D5.1).
pub struct GlyphCoverage {
    pub width: u16,
    pub height: u16,
    pub bearing_x: i16,
    pub bearing_y: i16,
    pub advance: u16,
    pub coverage: Vec<u8>,
}

pub fn glyph_coverage(font: FontId, ch: char, size_px: u16) -> Option<GlyphCoverage> {
    let key = rasterize_glyph(font, ch, size_px)?;
    let cache = GLYPH_CACHE.lock();
    let g = cache.get(&key)?;
    Some(GlyphCoverage {
        width: g.width,
        height: g.height,
        bearing_x: g.bearing_x,
        bearing_y: g.bearing_y,
        advance: g.advance,
        coverage: g.bitmap.clone(),
    })
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
    draw_text_tracked(fb, x, y, text, font, size_px, color, 0);
}

/// Draw a string with extra tracking (pixels inserted between glyphs).
#[allow(clippy::too_many_arguments)]
pub fn draw_text_tracked(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    text: &str,
    font: FontId,
    size_px: u16,
    color: Pixel,
    tracking: i32,
) {
    let face = face_for(font);
    if tracking == 0 {
        let _ = inter_font::walk_text(text, size_px, face, y, |gx, gy, glyph| {
            blit_glyph_aa(
                fb,
                x + gx,
                gy,
                glyph.width as u16,
                glyph.height as u16,
                &glyph.coverage,
                color,
                size_px,
            );
        });
        return;
    }

    let mut pen_x = x;
    let baseline_y = y + inter_font::ascent(face, size_px);
    let mut first = true;

    for ch in text.chars() {
        if ch == '\n' {
            pen_x = x;
            first = true;
            continue;
        }
        if !first {
            pen_x += tracking;
        }
        first = false;

        if let Some(key) = rasterize_glyph(font, ch, size_px) {
            let cache = GLYPH_CACHE.lock();
            if let Some(glyph) = cache.get(&key) {
                let gx = pen_x + glyph.bearing_x as i32;
                let gy = baseline_y - glyph.bearing_y as i32;

                blit_glyph_aa(
                    fb,
                    gx,
                    gy,
                    glyph.width,
                    glyph.height,
                    &glyph.bitmap,
                    color,
                    size_px,
                );

                pen_x += glyph.advance as i32;
            }
        } else {
            // Fallback: skip unknown glyphs, advance by approximate width
            pen_x += (size_px as i32 * 6) / 10;
        }
    }
}

/// Draw text horizontally centered at `center_x`.
#[allow(clippy::too_many_arguments)]
pub fn draw_text_centered(
    fb: &mut FrameBuffer,
    center_x: i32,
    y: i32,
    text: &str,
    font: FontId,
    size_px: u16,
    color: Pixel,
    tracking: i32,
) {
    let w = measure_text_tracked(text, font, size_px, tracking) as i32;
    draw_text_tracked(
        fb,
        center_x - w / 2,
        y,
        text,
        font,
        size_px,
        color,
        tracking,
    );
}

/// Measure the width of a string in pixels (without drawing).
pub fn measure_text(text: &str, font: FontId, size_px: u16) -> u32 {
    measure_text_tracked(text, font, size_px, 0)
}

/// Measure the width of a string including tracking between glyphs.
pub fn measure_text_tracked(text: &str, font: FontId, size_px: u16, tracking: i32) -> u32 {
    if tracking == 0 {
        return inter_font::measure_text(text, size_px, face_for(font)).max(0) as u32;
    }

    let mut width: i32 = 0;
    let mut count: i32 = 0;

    for ch in text.chars() {
        if ch == '\n' {
            continue;
        }
        count += 1;
        if let Some(key) = rasterize_glyph(font, ch, size_px) {
            let cache = GLYPH_CACHE.lock();
            if let Some(glyph) = cache.get(&key) {
                width += glyph.advance as i32;
            }
        } else {
            width += (size_px as i32 * 6) / 10;
        }
    }

    if count > 1 {
        width += tracking * (count - 1);
    }
    width.max(0) as u32
}

/// Blit a packed 8-bit alpha mask (row-major) with gamma-correct blending.
pub fn blit_alpha(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: u16,
    h: u16,
    bitmap: &[u8],
    color: Pixel,
    size_px: u16,
) {
    blit_glyph_aa(fb, x, y, w, h, bitmap, color, size_px);
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
    _size_px: u16,
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

            let a = (raw_alpha as u32 * color.a as u32 + 127) / 255;
            if a == 0 {
                continue;
            }

            if a >= 250 {
                fb.set_pixel(
                    px as usize,
                    py as usize,
                    Pixel::rgb(color.r, color.g, color.b),
                );
            } else {
                // Gamma-correct alpha blending in linear light space
                let bg = fb.get_pixel(px as usize, py as usize);
                let bg_r_lin = SRGB_TO_LINEAR[bg.r as usize] as u32;
                let bg_g_lin = SRGB_TO_LINEAR[bg.g as usize] as u32;
                let bg_b_lin = SRGB_TO_LINEAR[bg.b as usize] as u32;

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

/// Matches knox-ui `TypeTokens` (ui / body / title).
pub const UI_SIZE: u16 = 16;
/// Small UI font size (labels, status bars) — TypeTokens.body
pub const UI_SIZE_SMALL: u16 = 15;
/// Large UI font size (window titles, headings) — TypeTokens.title
pub const UI_SIZE_LARGE: u16 = 18;
/// Terminal/mono font size
pub const MONO_SIZE: u16 = 15;
/// Splash / lock wordmark
pub const LOGO_SIZE: u16 = 64;
/// Boot menu / login title
pub const TITLE_SIZE: u16 = 36;
/// Splash tagline
pub const TAGLINE_SIZE: u16 = 18;
/// Body copy (menu entries, descriptions)
pub const BODY_SIZE: u16 = 16;
/// Captions (version, status, hints)
pub const CAPTION_SIZE: u16 = 14;

/// Rounded Inter/Hack ascent for GPUI line-box placement.
pub fn ascent(font: FontId, size_px: u16) -> i32 {
    inter_font::ascent(face_for(font), size_px)
}
