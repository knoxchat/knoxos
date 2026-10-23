use crate::gui::framebuffer::{FrameBuffer, Pixel};
use crate::gui::hack_font;

use super::gamma::{gamma_blend_channel, sharpen_coverage};
use super::quality::{SubpixelOrder, apply_stem_darkening, get_subpixel_order};

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
        // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
        let c = unsafe { *row_data.add(col) } as u16;
        let cl = if col > 0 {
            // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
            (unsafe { *row_data.add(col - 1) }) as u16
        } else {
            0
        };
        let cr = if col + 1 < gw {
            // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
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
pub(super) fn draw_subpixel_glyph(
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
        // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
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
pub(super) fn draw_subpixel_bold_glyph_std(
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
pub(super) fn draw_subpixel_bold_glyph_compact(
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
