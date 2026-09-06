/// Font Rendering — ClearType-quality subpixel text for KnoxOS
///
/// Three rendering modes:
///   - **Subpixel (ClearType)**: 3× horizontal resolution by addressing LCD
///     subpixels individually. Each glyph column is sampled at 3× and the
///     coverage is distributed across R/G/B channels, then filtered with a
///     3-tap low-pass to suppress color fringing. This is what makes text
///     on macOS, Windows ClearType, and FreeType look razor-sharp.
///   - **Grayscale AA**: Pre-baked 8-bit alpha coverage with gamma-correct
///     blending. Good quality, no color fringing risk.
///   - **Legacy 1-bit**: Fast fallback, no antialiasing.
///
/// Two font sizes:
///   - Standard: 10x20 (UI text, window titles, menus, terminal)
///   - Compact:  8x14  (status bars, labels, small text)
///
/// Gamma-correct blending: all alpha compositing operates in linear light space
/// for correct color mixing without dark fringing halos.
use super::framebuffer::{FrameBuffer, Pixel};
use super::hack_font;
use super::hack_font_hd;

/// Font cell dimensions — standard 10x20 (high-quality AA monospace)
pub const FONT_WIDTH: usize = hack_font::GLYPH_WIDTH;
pub const FONT_HEIGHT: usize = hack_font::GLYPH_HEIGHT;
/// Compact font dimensions — 8x14 (for small UI text)
pub const FONT_WIDTH_COMPACT: usize = hack_font::GLYPH_WIDTH_COMPACT;
pub const FONT_HEIGHT_COMPACT: usize = hack_font::GLYPH_HEIGHT_COMPACT;
/// HD font dimensions — 20x40 (for crisp rendering at scale ≥ 2)
pub const FONT_WIDTH_HD: usize = hack_font_hd::GLYPH_WIDTH_HD;
pub const FONT_HEIGHT_HD: usize = hack_font_hd::GLYPH_HEIGHT_HD;

/// Line spacing: 2px extra for comfortable reading
const LINE_SPACING_EXTRA: i32 = 2;
/// Compact line spacing: 1px extra
const LINE_SPACING_EXTRA_COMPACT: i32 = 1;

// ═══════════════════════════════════════════════════════════════════════
// SUBPIXEL RENDERING MODE
// ═══════════════════════════════════════════════════════════════════════

/// LCD subpixel geometry — determines how R/G/B subpixels are ordered.
/// Standard LCD panels use RGB (left-to-right). Some panels use BGR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubpixelOrder {
    /// RGB subpixel order (most common: Dell, LG, Apple non-Retina, QEMU)
    Rgb,
    /// BGR subpixel order (some Samsung, older panels)
    Bgr,
    /// No subpixel rendering (use grayscale AA instead)
    /// Use for projectors, OLED, CRT, or when subpixels cause color fringing
    None,
}

/// Text rendering quality levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextQuality {
    /// Subpixel (ClearType-style) — 3× horizontal resolution, highest quality
    Subpixel,
    /// Grayscale antialiased (high quality, no color fringing)
    Grayscale,
    /// Legacy 1-bit rendering (fast fallback)
    Legacy,
}

/// Global rendering quality: 0=Subpixel, 1=Legacy, 2=Grayscale
/// Default to Grayscale — Subpixel (ClearType) causes visible color fringing
/// on virtual displays (QEMU/KVM), OLED, and non-RGB-stripe LCDs. Grayscale
/// AA is universally safe, crisp, and avoids the "blurry rainbow" artifacts.
static TEXT_QUALITY: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(2);

/// Global subpixel order: 0=RGB, 1=BGR, 2=None
static SUBPIXEL_ORDER: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Set the global text rendering quality
pub fn set_text_quality(quality: TextQuality) {
    let val = match quality {
        TextQuality::Subpixel => 0,
        TextQuality::Legacy => 1,
        TextQuality::Grayscale => 2,
    };
    TEXT_QUALITY.store(val, core::sync::atomic::Ordering::Relaxed);
}

/// Get the current text rendering quality
pub fn get_text_quality() -> TextQuality {
    match TEXT_QUALITY.load(core::sync::atomic::Ordering::Relaxed) {
        1 => TextQuality::Legacy,
        2 => TextQuality::Grayscale,
        _ => TextQuality::Subpixel,
    }
}

/// Set the LCD subpixel order for ClearType rendering
pub fn set_subpixel_order(order: SubpixelOrder) {
    let val = match order {
        SubpixelOrder::Rgb => 0,
        SubpixelOrder::Bgr => 1,
        SubpixelOrder::None => 2,
    };
    SUBPIXEL_ORDER.store(val, core::sync::atomic::Ordering::Relaxed);
}

/// Get the current subpixel order
pub fn get_subpixel_order() -> SubpixelOrder {
    match SUBPIXEL_ORDER.load(core::sync::atomic::Ordering::Relaxed) {
        1 => SubpixelOrder::Bgr,
        2 => SubpixelOrder::None,
        _ => SubpixelOrder::Rgb,
    }
}

/// Stem darkening factor. Default = 40 (~16%) for improved thin-stem
/// legibility on dark backgrounds, similar to macOS Core Text and
/// FreeType's auto-hinter stem darkening. Range: 0 (off) to 255 (max).
static STEM_DARKENING: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(25);

/// Set stem darkening factor (0-255)
pub fn set_stem_darkening(factor: u8) {
    STEM_DARKENING.store(factor, core::sync::atomic::Ordering::Relaxed);
}

#[inline(always)]
fn apply_stem_darkening(coverage: u8) -> u8 {
    if coverage == 0 || coverage == 255 {
        return coverage;
    }
    let dark = STEM_DARKENING.load(core::sync::atomic::Ordering::Relaxed) as u32;
    if dark == 0 {
        return coverage;
    }
    let c = coverage as u32;
    let boost = dark * (255 - c) / 255;
    (c + boost).min(255) as u8
}

// ═══════════════════════════════════════════════════════════════════════
// sRGB <-> Linear Conversion (proper gamma ≈ 2.2)
// ═══════════════════════════════════════════════════════════════════════
//
// Real sRGB uses a piecewise function:
//   linear = srgb/12.92                   if srgb <= 0.04045
//   linear = ((srgb+0.055)/1.055)^2.4     otherwise
//
// We approximate with gamma 2.2 via a const-friendly integer power
// approximation. The key insight: gamma 2.2 ≈ x^2 * x^0.2.
// For 8-bit precision, a carefully built LUT is exact enough.
//
// These tables are the #1 factor in font rendering quality.
// Incorrect tables cause dark fringing halos (gamma too high)
// or washed-out blending (gamma too low).

/// sRGB [0..255] → Linear [0..255] using gamma ≈ 2.2
/// Built with: round(((i/255)^2.2) * 255)
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

/// Sharpen glyph alpha coverage to reduce AA fringe blur.
/// Applies a contrast-boost curve: pushes mid-alpha values toward 0 or 255.
/// This is the key to "macOS-quality" crisp text — Core Text uses a similar
/// technique internally (coverage quantization + stem contrast boosting).
///
/// The curve: out = clamp(((cov - 128) * SHARPEN_STRENGTH / 256) + cov, 0, 255)
/// At strength 96: a pixel at alpha 84 becomes ~48, at alpha 203 becomes ~238.
/// Higher values = crisper text but less smooth AA gradients.
const COVERAGE_SHARPEN_STRENGTH: i32 = 128;

#[inline(always)]
fn sharpen_coverage(cov: u8) -> u8 {
    if cov == 0 || cov == 255 {
        return cov;
    }
    let c = cov as i32;
    // Contrast-boost: push away from midpoint (128)
    let delta = (c - 128) * COVERAGE_SHARPEN_STRENGTH / 256;
    let result = c + delta;
    result.clamp(0, 255) as u8
}

#[inline(always)]
fn gamma_blend_channel(fg: u8, bg: u8, alpha: u16) -> u8 {
    if alpha >= 255 {
        return fg;
    }
    if alpha == 0 {
        return bg;
    }
    // Blend in linear light space for correct color mixing
    let fg_lin = SRGB_TO_LINEAR[fg as usize] as u32;
    let bg_lin = SRGB_TO_LINEAR[bg as usize] as u32;
    let a = alpha as u32;
    let inv_a = 255 - a;
    let blended = (fg_lin * a + bg_lin * inv_a + 128) / 255;
    LINEAR_TO_SRGB[blended.min(255) as usize]
}

/// Public gamma-correct blend for a single channel (used by truetype.rs)
#[inline(always)]
pub fn gamma_blend_public(fg: u8, bg: u8, alpha: u32, inv_alpha: u32) -> u8 {
    let fg_lin = SRGB_TO_LINEAR[fg as usize] as u32;
    let bg_lin = SRGB_TO_LINEAR[bg as usize] as u32;
    let blended = (fg_lin * alpha + bg_lin * inv_alpha + 128) / 255;
    LINEAR_TO_SRGB[blended.min(255) as usize]
}

// ═══════════════════════════════════════════════════════════════════════
// SUBPIXEL (ClearType) RENDERING ENGINE
// ═══════════════════════════════════════════════════════════════════════
//
// How ClearType works:
//
// An LCD pixel is actually 3 vertical stripe sub-pixels: R | G | B.
// By controlling each subpixel independently, we get 3× the horizontal
// resolution. A 10-pixel wide glyph effectively becomes 30 subpixels.
//
// Algorithm:
//   1. For each glyph row, generate a "3× coverage" array: for each
//      original coverage pixel, produce 3 subpixel coverage values
//      by interpolating between adjacent columns.
//   2. Apply a 3-tap low-pass filter [1/4, 1/2, 1/4] (ClearType filter)
//      to suppress color fringing while preserving sharpness.
//   3. Map the filtered subpixel coverages to R/G/B channels based on
//      the LCD subpixel order (RGB or BGR).
//   4. Blend each channel independently with gamma correction.
//
// This is the same technique used by:
//   - Microsoft ClearType (Windows)
//   - FreeType's LCD filter (Linux)
//   - macOS Core Text (pre-Mojave on non-Retina)

/// ClearType-style 3-tap low-pass filter weights.
/// [1/4, 2/4, 1/4] = classic ClearType. Suppresses color fringing while
/// maintaining most of the subpixel sharpness gain.
/// Values are in 8.8 fixed-point (64 = 1/4, 128 = 1/2).
const SP_FILTER_SIDE: u32 = 64; // 1/4
const SP_FILTER_CENTER: u32 = 128; // 1/2

/// Generate 3× subpixel coverage for one glyph row.
/// Input: `gw` coverage values (one per glyph column).
/// Output: `gw * 3` subpixel coverage values.
///
/// For each source pixel at column `c` with coverage `cov[c]`:
///   sub[c*3 + 0] = lerp(cov[c-1], cov[c], 0.83)  — left third
///   sub[c*3 + 1] = cov[c]                          — center third
///   sub[c*3 + 2] = lerp(cov[c], cov[c+1], 0.17)   — right third
///
/// This creates smooth subpixel transitions at glyph edges.
#[inline]
fn generate_subpixel_row(row_data: *const u8, gw: usize, out: &mut [u16]) {
    // We need gw*3 entries in `out`
    for col in 0..gw {
        let c = unsafe { *row_data.add(col) } as u16;
        let cl = if col > 0 {
            (unsafe { *row_data.add(col - 1) }) as u16
        } else {
            0
        };
        let cr = if col + 1 < gw {
            (unsafe { *row_data.add(col + 1) }) as u16
        } else {
            0
        };

        // Left subpixel: weighted toward current, slight bleed from left neighbor
        // 5/6 current + 1/6 left ≈ (c*213 + cl*43) >> 8
        out[col * 3] = (c * 213 + cl * 43 + 128) >> 8;
        // Center subpixel: pure current coverage
        out[col * 3 + 1] = c;
        // Right subpixel: weighted toward current, slight bleed from right neighbor
        out[col * 3 + 2] = (c * 213 + cr * 43 + 128) >> 8;
    }
}

/// Apply 3-tap ClearType low-pass filter in-place.
/// Filter: [1/4, 1/2, 1/4] — classic Microsoft ClearType.
/// This is applied to the 3× subpixel array to reduce color fringing.
#[inline]
fn filter_subpixel_row(buf: &mut [u16], len: usize) {
    if len < 3 {
        return;
    }
    // Need a temp copy because the filter reads neighbors
    // Use a small stack buffer — max glyph width is 10, so 30 subpixels
    let mut tmp = [0u16; 64]; // 64 > 10*3=30, plenty of room
    let n = len.min(64);
    tmp[..n].copy_from_slice(&buf[..n]);

    for i in 0..n {
        let left = if i > 0 { tmp[i - 1] } else { 0 } as u32;
        let center = tmp[i] as u32;
        let right = if i + 1 < n { tmp[i + 1] } else { 0 } as u32;
        buf[i] =
            ((left * SP_FILTER_SIDE + center * SP_FILTER_CENTER + right * SP_FILTER_SIDE + 128)
                >> 8)
                .min(255) as u16;
    }
}

/// Subpixel glyph renderer — ClearType-style per-channel alpha blending.
///
/// This is the heart of the subpixel engine. For each pixel, it computes
/// separate R, G, B alpha values from the filtered 3× subpixel coverage,
/// then blends each channel independently in linear light space.
fn draw_subpixel_glyph(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    data: *const u8,
    gw: usize,
    gh: usize,
    color: Pixel,
    _scale: u32,
) {
    let order = get_subpixel_order();
    let sp_len = gw * 3;
    let mut sp_raw = [0u16; 64]; // 3× subpixel coverage (raw)
    let mut sp_filt = [0u16; 64]; // 3× subpixel coverage (filtered)

    for row in 0..gh {
        let py = y + row as i32;
        if py < 0 || py >= fb.height as i32 {
            continue;
        }

        // Step 1: Generate 3× subpixel coverage from glyph row
        let row_ptr = unsafe { data.add(row * gw) };
        generate_subpixel_row(row_ptr, gw, &mut sp_raw[..sp_len]);

        // Step 2: Apply ClearType filter
        sp_filt[..sp_len].copy_from_slice(&sp_raw[..sp_len]);
        filter_subpixel_row(&mut sp_filt[..sp_len], sp_len);

        // Step 3: Apply stem darkening + sharpening to each subpixel
        for i in 0..sp_len {
            let v = sp_filt[i] as u8;
            sp_filt[i] = apply_stem_darkening(sharpen_coverage(v)) as u16;
        }

        // Step 4: For each output pixel, extract R/G/B subpixel alphas and blend
        for col in 0..gw {
            let px = x + col as i32;
            if px < 0 || px >= fb.width as i32 {
                continue;
            }

            let base = col * 3;
            let (alpha_r, alpha_g, alpha_b) = match order {
                SubpixelOrder::Rgb => (sp_filt[base], sp_filt[base + 1], sp_filt[base + 2]),
                SubpixelOrder::Bgr => (sp_filt[base + 2], sp_filt[base + 1], sp_filt[base]),
                SubpixelOrder::None => {
                    // Fallback to grayscale: average the 3 subpixels
                    let avg = (sp_filt[base] + sp_filt[base + 1] + sp_filt[base + 2] + 1) / 3;
                    (avg, avg, avg)
                }
            };

            // Skip fully transparent pixels
            if alpha_r == 0 && alpha_g == 0 && alpha_b == 0 {
                continue;
            }

            // Fast path: all three channels near-opaque
            if alpha_r >= 250 && alpha_g >= 250 && alpha_b >= 250 {
                fb.set_pixel(px as usize, py as usize, color);
                continue;
            }

            let bg = fb.get_pixel(px as usize, py as usize);

            // Per-channel gamma-correct blending (the ClearType magic)
            // Each channel has its own alpha, derived from its subpixel coverage
            let ar = (color.a as u16 * alpha_r + 128) >> 8;
            let ag = (color.a as u16 * alpha_g + 128) >> 8;
            let ab = (color.a as u16 * alpha_b + 128) >> 8;

            let r = gamma_blend_channel(color.r, bg.r, ar);
            let g = gamma_blend_channel(color.g, bg.g, ag);
            let b = gamma_blend_channel(color.b, bg.b, ab);

            fb.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
        }
    }
}

/// Subpixel bold glyph renderer (standard size).
fn draw_subpixel_bold_glyph_std(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    coverage: &[[u8; hack_font::GLYPH_WIDTH]; hack_font::GLYPH_HEIGHT],
    color: Pixel,
    _scale: u32,
) {
    let gw = hack_font::GLYPH_WIDTH;
    let gh = hack_font::GLYPH_HEIGHT;
    let order = get_subpixel_order();
    let sp_len = gw * 3;
    let mut bold_row = [0u8; 16]; // max glyph width
    let mut sp_raw = [0u16; 64];
    let mut sp_filt = [0u16; 64];

    for row in 0..gh {
        let py = y + row as i32;
        if py < 0 || py >= fb.height as i32 {
            continue;
        }

        // Build bold coverage: max(current, left)
        for col in 0..gw {
            let c0 = coverage[row][col];
            let cl = if col > 0 { coverage[row][col - 1] } else { 0 };
            bold_row[col] = c0.max(cl);
        }

        generate_subpixel_row(bold_row.as_ptr(), gw, &mut sp_raw[..sp_len]);
        sp_filt[..sp_len].copy_from_slice(&sp_raw[..sp_len]);
        filter_subpixel_row(&mut sp_filt[..sp_len], sp_len);

        for i in 0..sp_len {
            let v = sp_filt[i] as u8;
            sp_filt[i] = apply_stem_darkening(sharpen_coverage(v)) as u16;
        }

        for col in 0..gw {
            let px = x + col as i32;
            if px < 0 || px >= fb.width as i32 {
                continue;
            }

            let base = col * 3;
            let (alpha_r, alpha_g, alpha_b) = match order {
                SubpixelOrder::Rgb => (sp_filt[base], sp_filt[base + 1], sp_filt[base + 2]),
                SubpixelOrder::Bgr => (sp_filt[base + 2], sp_filt[base + 1], sp_filt[base]),
                SubpixelOrder::None => {
                    let avg = (sp_filt[base] + sp_filt[base + 1] + sp_filt[base + 2] + 1) / 3;
                    (avg, avg, avg)
                }
            };

            if alpha_r == 0 && alpha_g == 0 && alpha_b == 0 {
                continue;
            }
            if alpha_r >= 250 && alpha_g >= 250 && alpha_b >= 250 {
                fb.set_pixel(px as usize, py as usize, color);
                continue;
            }

            let bg = fb.get_pixel(px as usize, py as usize);
            let ar = (color.a as u16 * alpha_r + 128) >> 8;
            let ag = (color.a as u16 * alpha_g + 128) >> 8;
            let ab = (color.a as u16 * alpha_b + 128) >> 8;
            let r = gamma_blend_channel(color.r, bg.r, ar);
            let g = gamma_blend_channel(color.g, bg.g, ag);
            let b = gamma_blend_channel(color.b, bg.b, ab);
            fb.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
        }
    }
}

/// Subpixel bold glyph renderer (compact size).
fn draw_subpixel_bold_glyph_compact(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    coverage: &[[u8; hack_font::GLYPH_WIDTH_COMPACT]; hack_font::GLYPH_HEIGHT_COMPACT],
    color: Pixel,
    _scale: u32,
) {
    let gw = hack_font::GLYPH_WIDTH_COMPACT;
    let gh = hack_font::GLYPH_HEIGHT_COMPACT;
    let order = get_subpixel_order();
    let sp_len = gw * 3;
    let mut bold_row = [0u8; 16];
    let mut sp_raw = [0u16; 64];
    let mut sp_filt = [0u16; 64];

    for row in 0..gh {
        let py = y + row as i32;
        if py < 0 || py >= fb.height as i32 {
            continue;
        }

        for col in 0..gw {
            let c0 = coverage[row][col];
            let cl = if col > 0 { coverage[row][col - 1] } else { 0 };
            bold_row[col] = c0.max(cl);
        }

        generate_subpixel_row(bold_row.as_ptr(), gw, &mut sp_raw[..sp_len]);
        sp_filt[..sp_len].copy_from_slice(&sp_raw[..sp_len]);
        filter_subpixel_row(&mut sp_filt[..sp_len], sp_len);

        for i in 0..sp_len {
            let v = sp_filt[i] as u8;
            sp_filt[i] = apply_stem_darkening(sharpen_coverage(v)) as u16;
        }

        for col in 0..gw {
            let px = x + col as i32;
            if px < 0 || px >= fb.width as i32 {
                continue;
            }

            let base = col * 3;
            let (alpha_r, alpha_g, alpha_b) = match order {
                SubpixelOrder::Rgb => (sp_filt[base], sp_filt[base + 1], sp_filt[base + 2]),
                SubpixelOrder::Bgr => (sp_filt[base + 2], sp_filt[base + 1], sp_filt[base]),
                SubpixelOrder::None => {
                    let avg = (sp_filt[base] + sp_filt[base + 1] + sp_filt[base + 2] + 1) / 3;
                    (avg, avg, avg)
                }
            };

            if alpha_r == 0 && alpha_g == 0 && alpha_b == 0 {
                continue;
            }
            if alpha_r >= 250 && alpha_g >= 250 && alpha_b >= 250 {
                fb.set_pixel(px as usize, py as usize, color);
                continue;
            }

            let bg = fb.get_pixel(px as usize, py as usize);
            let ar = (color.a as u16 * alpha_r + 128) >> 8;
            let ag = (color.a as u16 * alpha_g + 128) >> 8;
            let ab = (color.a as u16 * alpha_b + 128) >> 8;
            let r = gamma_blend_channel(color.r, bg.r, ar);
            let g = gamma_blend_channel(color.g, bg.g, ag);
            let b = gamma_blend_channel(color.b, bg.b, ab);
            fb.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CHARACTER DRAWING — Dispatch to Subpixel or Grayscale
// ═══════════════════════════════════════════════════════════════════════

/// Draw a single character (standard 10x20, HD 20x40 when scale is even ≥ 2).
pub fn draw_char(fb: &mut FrameBuffer, x: i32, y: i32, ch: char, color: Pixel, scale: u32) {
    // For even scale ≥ 2, use the HD font (20x40) at half the scale factor.
    // HD glyph is exactly 2× the standard, so scale/2 gives identical output size.
    // E.g. scale=4: standard 10×4=40px, HD 20×2=40px (but with 4× source detail).
    if scale >= 2 && scale % 2 == 0 {
        if let Some(hd_cov) = hack_font_hd::get_glyph_hd(ch) {
            let hd_scale = scale / 2;
            draw_aa_glyph(
                fb,
                x,
                y,
                hd_cov.as_ptr() as *const u8,
                FONT_WIDTH_HD,
                FONT_HEIGHT_HD,
                color,
                hd_scale,
            );
            return;
        }
    }
    if let Some(coverage) = hack_font::get_glyph(ch) {
        if get_text_quality() == TextQuality::Subpixel && scale == 1 {
            draw_subpixel_glyph(
                fb,
                x,
                y,
                coverage.as_ptr() as *const u8,
                hack_font::GLYPH_WIDTH,
                hack_font::GLYPH_HEIGHT,
                color,
                scale,
            );
        } else {
            draw_aa_glyph(
                fb,
                x,
                y,
                coverage.as_ptr() as *const u8,
                hack_font::GLYPH_WIDTH,
                hack_font::GLYPH_HEIGHT,
                color,
                scale,
            );
        }
    }
}

/// Draw a single character (compact 8x14).
pub fn draw_char_compact(fb: &mut FrameBuffer, x: i32, y: i32, ch: char, color: Pixel, scale: u32) {
    if let Some(coverage) = hack_font::get_glyph_compact(ch) {
        if get_text_quality() == TextQuality::Subpixel && scale == 1 {
            draw_subpixel_glyph(
                fb,
                x,
                y,
                coverage.as_ptr() as *const u8,
                hack_font::GLYPH_WIDTH_COMPACT,
                hack_font::GLYPH_HEIGHT_COMPACT,
                color,
                scale,
            );
        } else {
            draw_aa_glyph(
                fb,
                x,
                y,
                coverage.as_ptr() as *const u8,
                hack_font::GLYPH_WIDTH_COMPACT,
                hack_font::GLYPH_HEIGHT_COMPACT,
                color,
                scale,
            );
        }
    }
}

/// Draw a bold character (standard 10x20, HD 20x40 when scale is even ≥ 2).
pub fn draw_char_bold(fb: &mut FrameBuffer, x: i32, y: i32, ch: char, color: Pixel, scale: u32) {
    if scale >= 2 && scale % 2 == 0 {
        if let Some(hd_cov) = hack_font_hd::get_glyph_hd(ch) {
            let hd_scale = scale / 2;
            draw_bold_glyph_hd(fb, x, y, hd_cov, color, hd_scale);
            return;
        }
    }
    if let Some(coverage) = hack_font::get_glyph(ch) {
        if get_text_quality() == TextQuality::Subpixel && scale == 1 {
            draw_subpixel_bold_glyph_std(fb, x, y, coverage, color, scale);
        } else {
            draw_bold_glyph_std(fb, x, y, coverage, color, scale);
        }
    }
}

/// Draw a bold character (compact 8x14).
pub fn draw_char_bold_compact(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    ch: char,
    color: Pixel,
    scale: u32,
) {
    if let Some(coverage) = hack_font::get_glyph_compact(ch) {
        if get_text_quality() == TextQuality::Subpixel && scale == 1 {
            draw_subpixel_bold_glyph_compact(fb, x, y, coverage, color, scale);
        } else {
            draw_bold_glyph_compact(fb, x, y, coverage, color, scale);
        }
    }
}

/// Sample the glyph coverage at a fractional source position using bilinear
/// interpolation. This produces smooth edges when up-scaling instead of the
/// blocky staircase that nearest-neighbor gives.
///
/// `sx`, `sy` are in 8.8 fixed-point (256 = 1.0 source pixel).
#[inline]
fn sample_bilinear(data: *const u8, gw: usize, gh: usize, sx: i32, sy: i32) -> u8 {
    // Integer part (top-left source pixel)
    let x0 = (sx >> 8) as usize;
    let y0 = (sy >> 8) as usize;
    let x1 = if x0 + 1 < gw { x0 + 1 } else { x0 };
    let y1 = if y0 + 1 < gh { y0 + 1 } else { y0 };

    // Fractional part [0..255]
    let fx = (sx & 0xFF) as u32;
    let fy = (sy & 0xFF) as u32;
    let ifx = 256 - fx;
    let ify = 256 - fy;

    let c00 = unsafe { *data.add(y0 * gw + x0) } as u32;
    let c10 = unsafe { *data.add(y0 * gw + x1) } as u32;
    let c01 = unsafe { *data.add(y1 * gw + x0) } as u32;
    let c11 = unsafe { *data.add(y1 * gw + x1) } as u32;

    // Bilinear: lerp in X for both rows, then lerp in Y
    let top = (c00 * ifx + c10 * fx + 128) >> 8;
    let bot = (c01 * ifx + c11 * fx + 128) >> 8;
    ((top * ify + bot * fy + 128) >> 8).min(255) as u8
}

/// Core AA glyph renderer: reads pre-baked alpha and blends with gamma correction.
///
/// Pipeline per pixel:
///   1. Read raw 8-bit coverage from pre-baked glyph data
///   2. Sharpen coverage (tighten AA fringe for crispness)
///   3. Apply stem darkening (boost thin stems on dark BGs)
///   4. Gamma-correct alpha blend in linear light space
///
/// At scale == 1 we render pixel-for-pixel from the glyph data. At scale > 1
/// we use **bilinear interpolation** from source glyph space to output space,
/// eliminating the blocky staircase artifacts that nearest-neighbor produces.
/// The coverage sharpening and stem darkening strengths are also reduced at
/// larger scales to avoid exaggerating artifacts that are invisible at 1×.
fn draw_aa_glyph(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    data: *const u8,
    gw: usize,
    gh: usize,
    color: Pixel,
    scale: u32,
) {
    if scale == 1 {
        for row in 0..gh {
            let py = y + row as i32;
            if py < 0 || py >= fb.height as i32 {
                continue;
            }
            for col in 0..gw {
                let raw_cov = unsafe { *data.add(row * gw + col) };
                if raw_cov == 0 {
                    continue;
                }
                let px = x + col as i32;
                if px < 0 || px >= fb.width as i32 {
                    continue;
                }
                let cov = apply_stem_darkening(sharpen_coverage(raw_cov));
                if cov >= 250 {
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
        // ── Bilinear-interpolated up-scale ─────────────────────────
        // Map each output pixel back to source glyph space and sample
        // with bilinear filtering for smooth edges.
        let out_w = gw as i32 * scale as i32;
        let out_h = gh as i32 * scale as i32;

        // Reduce sharpening at high scale — at 4× the staircase is already
        // 4 px wide, so aggressive sharpening just makes it harsher.
        let scale_sharpen = scale.min(4) as i32; // 1,2,3,4
        let adj_sharpen = COVERAGE_SHARPEN_STRENGTH / scale_sharpen;

        for oy in 0..out_h {
            let py = y + oy;
            if py < 0 || py >= fb.height as i32 {
                continue;
            }
            // Map output Y back to source Y in 8.8 fixed-point
            // We offset by half-pixel to sample the center of each output pixel
            let src_y = ((oy as i64 * 256 * (gh as i64 - 1)) / (out_h as i64 - 1).max(1)) as i32;
            for ox in 0..out_w {
                let px = x + ox;
                if px < 0 || px >= fb.width as i32 {
                    continue;
                }
                let src_x =
                    ((ox as i64 * 256 * (gw as i64 - 1)) / (out_w as i64 - 1).max(1)) as i32;

                let raw_cov = sample_bilinear(data, gw, gh, src_x, src_y);
                if raw_cov == 0 {
                    continue;
                }

                // Scale-aware coverage processing
                let cov = {
                    let c = raw_cov as i32;
                    // Reduced sharpening
                    let sharpened = if c == 0 || c == 255 {
                        c
                    } else {
                        (c + ((c - 128) * adj_sharpen + 128) / 256).clamp(0, 255)
                    };
                    // Reduced stem darkening for large scale
                    let dark = STEM_DARKENING.load(core::sync::atomic::Ordering::Relaxed) as u32;
                    let dark_adj = dark / (scale.min(4));
                    if sharpened == 0 || sharpened == 255 || dark_adj == 0 {
                        sharpened as u8
                    } else {
                        let s = sharpened as u32;
                        (s + dark_adj * (255 - s) / 255).min(255) as u8
                    }
                };

                if cov >= 250 {
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

/// Bold renderer for standard glyphs — synthetic bold via multi-directional
/// coverage expansion. Instead of the simple `max(here, left)` which creates
/// a visible ghost/smear, we sample the current pixel plus immediate left and
/// top-left neighbors and take the max, giving a more uniform weight increase.
/// At scale > 1 we use bilinear sampling from a pre-built bold coverage buffer.
fn draw_bold_glyph_std(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    coverage: &[[u8; hack_font::GLYPH_WIDTH]; hack_font::GLYPH_HEIGHT],
    color: Pixel,
    scale: u32,
) {
    let gw = hack_font::GLYPH_WIDTH;
    let gh = hack_font::GLYPH_HEIGHT;

    // Pre-compute bold coverage buffer: max of (self, left, above) for a
    // uniform weight increase without directional ghosting
    let mut bold_buf = [[0u8; hack_font::GLYPH_WIDTH]; hack_font::GLYPH_HEIGHT];
    for row in 0..gh {
        for col in 0..gw {
            let c0 = coverage[row][col];
            let cl = if col > 0 { coverage[row][col - 1] } else { 0 };
            // Also peek up-left for diagonal weight balance
            let cu = if row > 0 { coverage[row - 1][col] } else { 0 };
            // Take max of these 3 neighbors — adds weight without horizontal smear
            bold_buf[row][col] = c0.max(cl).max(cu / 2);
        }
    }

    if scale == 1 {
        for row in 0..gh {
            let py = y + row as i32;
            if py < 0 || py >= fb.height as i32 {
                continue;
            }
            for col in 0..gw {
                let raw_cov = bold_buf[row][col];
                if raw_cov == 0 {
                    continue;
                }
                let px = x + col as i32;
                if px < 0 || px >= fb.width as i32 {
                    continue;
                }
                let cov = apply_stem_darkening(sharpen_coverage(raw_cov));
                if cov >= 250 {
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
        // Bilinear-interpolated bold at scale > 1
        let ptr = bold_buf.as_ptr() as *const u8;
        draw_aa_glyph(fb, x, y, ptr, gw, gh, color, scale);
    }
}

/// Bold renderer for compact glyphs — same improved synthesis as standard.
fn draw_bold_glyph_compact(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    coverage: &[[u8; hack_font::GLYPH_WIDTH_COMPACT]; hack_font::GLYPH_HEIGHT_COMPACT],
    color: Pixel,
    scale: u32,
) {
    let gw = hack_font::GLYPH_WIDTH_COMPACT;
    let gh = hack_font::GLYPH_HEIGHT_COMPACT;

    let mut bold_buf = [[0u8; hack_font::GLYPH_WIDTH_COMPACT]; hack_font::GLYPH_HEIGHT_COMPACT];
    for row in 0..gh {
        for col in 0..gw {
            let c0 = coverage[row][col];
            let cl = if col > 0 { coverage[row][col - 1] } else { 0 };
            let cu = if row > 0 { coverage[row - 1][col] } else { 0 };
            bold_buf[row][col] = c0.max(cl).max(cu / 2);
        }
    }

    if scale == 1 {
        for row in 0..gh {
            let py = y + row as i32;
            if py < 0 || py >= fb.height as i32 {
                continue;
            }
            for col in 0..gw {
                let raw_cov = bold_buf[row][col];
                if raw_cov == 0 {
                    continue;
                }
                let px = x + col as i32;
                if px < 0 || px >= fb.width as i32 {
                    continue;
                }
                let cov = apply_stem_darkening(sharpen_coverage(raw_cov));
                if cov >= 250 {
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
        let ptr = bold_buf.as_ptr() as *const u8;
        draw_aa_glyph(fb, x, y, ptr, gw, gh, color, scale);
    }
}

/// Bold renderer for HD glyphs (20×40) — synthetic bold on the high-res data.
fn draw_bold_glyph_hd(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    coverage: &[[u8; hack_font_hd::GLYPH_WIDTH_HD]; hack_font_hd::GLYPH_HEIGHT_HD],
    color: Pixel,
    scale: u32,
) {
    let gw = hack_font_hd::GLYPH_WIDTH_HD;
    let gh = hack_font_hd::GLYPH_HEIGHT_HD;

    // Build bold buffer — at 20×40 we can afford a wider offset
    let mut bold_buf = [[0u8; hack_font_hd::GLYPH_WIDTH_HD]; hack_font_hd::GLYPH_HEIGHT_HD];
    for row in 0..gh {
        for col in 0..gw {
            let c0 = coverage[row][col];
            let cl = if col > 0 { coverage[row][col - 1] } else { 0 };
            let cu = if row > 0 { coverage[row - 1][col] } else { 0 };
            bold_buf[row][col] = c0.max(cl).max(cu / 2);
        }
    }

    let ptr = bold_buf.as_ptr() as *const u8;
    draw_aa_glyph(fb, x, y, ptr, gw, gh, color, scale);
}

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

/// Measure string width (standard font)
pub fn measure_string_width(text: &str, scale: u32) -> u32 {
    text.len() as u32 * FONT_WIDTH as u32 * scale
}

/// Measure string width (compact font)
pub fn measure_string_width_compact(text: &str, scale: u32) -> u32 {
    text.len() as u32 * FONT_WIDTH_COMPACT as u32 * scale
}

/// Measure string height (standard)
pub fn measure_string_height(scale: u32) -> u32 {
    FONT_HEIGHT as u32 * scale
}

/// Measure string height (compact)
pub fn measure_string_height_compact(scale: u32) -> u32 {
    FONT_HEIGHT_COMPACT as u32 * scale
}

/// Word-wrap text to fit within a given width.
pub fn word_wrap(text: &str, max_width: u32, scale: u32) -> alloc::vec::Vec<alloc::string::String> {
    let mut lines = alloc::vec::Vec::new();
    let char_width = FONT_WIDTH as u32 * scale;
    let max_chars = max_width.checked_div(char_width).unwrap_or(0) as usize;
    if max_chars == 0 {
        return lines;
    }
    for line in text.split('\n') {
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
                    } else {
                        current_line.push_str(word);
                    }
                } else if current_line.len() + 1 + word.len() <= max_chars {
                    current_line.push(' ');
                    current_line.push_str(word);
                } else {
                    lines.push(current_line);
                    current_line = alloc::string::String::from(word);
                }
            }
            if !current_line.is_empty() {
                lines.push(current_line);
            }
        }
    }
    lines
}

/// Draw text with word wrapping. Returns total height consumed.
pub fn draw_text_wrapped(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    max_width: u32,
    text: &str,
    color: Pixel,
    scale: u32,
) -> i32 {
    let lines = word_wrap(text, max_width, scale);
    let line_h = (FONT_HEIGHT as u32 * scale) as i32 + LINE_SPACING_EXTRA;
    for (i, line) in lines.iter().enumerate() {
        draw_string(fb, x, y + i as i32 * line_h, line, color, scale);
    }
    lines.len() as i32 * line_h
}

/// Draw text with word wrapping (compact font).
pub fn draw_text_wrapped_compact(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    max_width: u32,
    text: &str,
    color: Pixel,
    scale: u32,
) -> i32 {
    let char_width = FONT_WIDTH_COMPACT as u32 * scale;
    let max_chars = max_width.checked_div(char_width).unwrap_or(0) as usize;
    if max_chars == 0 {
        return 0;
    }
    let mut lines = alloc::vec::Vec::new();
    for line in text.split('\n') {
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
                    } else {
                        current.push_str(word);
                    }
                } else if current.len() + 1 + word.len() <= max_chars {
                    current.push(' ');
                    current.push_str(word);
                } else {
                    lines.push(current);
                    current = alloc::string::String::from(word);
                }
            }
            if !current.is_empty() {
                lines.push(current);
            }
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
    let max_chars = max_width.checked_div(char_width).unwrap_or(0) as usize;
    if text.len() <= max_chars || max_chars < 4 {
        return alloc::string::String::from(text);
    }
    let mut result = alloc::string::String::from(&text[..max_chars - 3]);
    result.push_str("...");
    result
}

/// Truncate with ellipsis (compact font)
pub fn truncate_with_ellipsis_compact(
    text: &str,
    max_width: u32,
    scale: u32,
) -> alloc::string::String {
    let char_width = FONT_WIDTH_COMPACT as u32 * scale;
    let max_chars = max_width.checked_div(char_width).unwrap_or(0) as usize;
    if text.len() <= max_chars || max_chars < 4 {
        return alloc::string::String::from(text);
    }
    let mut result = alloc::string::String::from(&text[..max_chars - 3]);
    result.push_str("...");
    result
}

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
    let sf = super::scale::scale_factor();
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
    let sf = super::scale::scale_factor();
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
    let sf = super::scale::scale_factor();
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
    let sf = super::scale::scale_factor();
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
    let sf = super::scale::scale_factor();
    libm::round(text.len() as f64 * FONT_WIDTH as f64 * sf) as u32
}

/// Measure DPI-aware compact string width in physical pixels.
pub fn measure_string_width_compact_dpi(text: &str) -> u32 {
    let sf = super::scale::scale_factor();
    libm::round(text.len() as f64 * FONT_WIDTH_COMPACT as f64 * sf) as u32
}

/// Measure DPI-aware string height in physical pixels.
pub fn measure_string_height_dpi() -> u32 {
    let sf = super::scale::scale_factor();
    libm::round(FONT_HEIGHT as f64 * sf) as u32
}

/// Measure DPI-aware compact string height in physical pixels.
pub fn measure_string_height_compact_dpi() -> u32 {
    let sf = super::scale::scale_factor();
    libm::round(FONT_HEIGHT_COMPACT as f64 * sf) as u32
}

// ═══════════════════════════════════════════════════════════════════════
// FONT ENGINE BRIDGE
// ═══════════════════════════════════════════════════════════════════════
// These functions delegate to the new scalable font_engine when TTF fonts
// are loaded, falling back to the bitmap path above otherwise.

/// Draw text using the new scalable font engine (UI proportional font).
/// Falls back to bitmap `draw_string` if TTF fonts aren't loaded.
pub fn draw_ui(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel, size_px: u16) {
    super::font_engine::draw_ui_text(fb, x, y, text, size_px, color);
}

/// Draw text using the new scalable font engine (monospace font).
/// Falls back to bitmap `draw_string` if TTF fonts aren't loaded.
pub fn draw_mono(fb: &mut FrameBuffer, x: i32, y: i32, text: &str, color: Pixel, size_px: u16) {
    super::font_engine::draw_mono_text(fb, x, y, text, size_px, color);
}

/// Measure UI text width using new scalable font engine.
pub fn measure_ui(text: &str, size_px: u16) -> u32 {
    super::font_engine::measure_ui_text(text, size_px)
}

/// Measure monospace text width using new scalable font engine.
pub fn measure_mono(text: &str, size_px: u16) -> u32 {
    super::font_engine::measure_mono_text(text, size_px)
}
