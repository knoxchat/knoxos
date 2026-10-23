use crate::gui::framebuffer::{FrameBuffer, Pixel};
use crate::gui::hack_font;
use crate::gui::hack_font_hd;

use super::gamma::{COVERAGE_SHARPEN_STRENGTH, gamma_blend_channel, sharpen_coverage};
use super::metrics::{FONT_HEIGHT_HD, FONT_WIDTH_HD};
use super::quality::{STEM_DARKENING, TextQuality, apply_stem_darkening, get_text_quality};
use super::subpixel::{
    draw_subpixel_bold_glyph_compact, draw_subpixel_bold_glyph_std, draw_subpixel_glyph,
};

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

    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    let c00 = unsafe { *data.add(y0 * gw + x0) } as u32;
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    let c10 = unsafe { *data.add(y0 * gw + x1) } as u32;
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    let c01 = unsafe { *data.add(y1 * gw + x0) } as u32;
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
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
                // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
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
