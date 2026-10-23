//! Quadratic outline flattening, stem emboldening, and scanline fill.
use alloc::vec;
use alloc::vec::Vec;

use super::types::{Contour, GlyphBitmap, GlyphOutline};

// ─── Rasterizer ──────────────────────────────────────────────────────

fn add_quad(out: &mut Vec<(f32, f32)>, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32)) {
    let dx = p2.0 - p0.0;
    let dy = p2.1 - p0.1;
    let cdx = p1.0 - p0.0;
    let cdy = p1.1 - p0.1;
    let approx = 0.5 * (libm::sqrtf(dx * dx + dy * dy) + libm::sqrtf(cdx * cdx + cdy * cdy));
    let steps = (approx * 3.0).clamp(6.0, 24.0) as usize;
    for s in 1..=steps {
        let t = s as f32 / steps as f32;
        let inv = 1.0 - t;
        out.push((
            inv * inv * p0.0 + 2.0 * inv * t * p1.0 + t * t * p2.0,
            inv * inv * p0.1 + 2.0 * inv * t * p1.1 + t * t * p2.1,
        ));
    }
}

/// Flatten a quadratic Bézier contour into pixel-space line segments.
fn flatten_contour(contour: &Contour, scale: f32) -> Vec<(f32, f32)> {
    let n = contour.points.len();
    if n < 2 {
        return Vec::new();
    }

    let pts: Vec<(f32, f32, bool)> = contour
        .points
        .iter()
        .map(|p| (p.x as f32 * scale, p.y as f32 * scale, p.on_curve))
        .collect();

    let mut result: Vec<(f32, f32)> = Vec::new();
    let first = if pts[0].2 {
        (pts[0].0, pts[0].1)
    } else if pts[n - 1].2 {
        (pts[n - 1].0, pts[n - 1].1)
    } else {
        (
            (pts[0].0 + pts[n - 1].0) * 0.5,
            (pts[0].1 + pts[n - 1].1) * 0.5,
        )
    };
    result.push(first);
    let mut last = first;
    let mut i = if pts[0].2 { 1 } else { 0 };

    while i < n {
        if pts[i].2 {
            last = (pts[i].0, pts[i].1);
            result.push(last);
            i += 1;
        } else {
            let ctrl = (pts[i].0, pts[i].1);
            let next_idx = (i + 1) % n;
            let end = if pts[next_idx].2 {
                i += 2;
                (pts[next_idx].0, pts[next_idx].1)
            } else {
                i += 1;
                (
                    (ctrl.0 + pts[next_idx].0) * 0.5,
                    (ctrl.1 + pts[next_idx].1) * 0.5,
                )
            };
            add_quad(&mut result, last, ctrl, end);
            last = end;
        }
    }

    result
}

/// Grow the filled region: TrueType outers are clockwise, inners
/// counter-clockwise, so the left normal expands ink on both.
fn offset_polyline(pts: &[(f32, f32)], amount: f32) -> Vec<(f32, f32)> {
    let n = pts.len();
    if n < 3 || amount == 0.0 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (x0, y0) = pts[(i + n - 1) % n];
        let (x1, y1) = pts[i];
        let (x2, y2) = pts[(i + 1) % n];
        let mut nx = 0.0f32;
        let mut ny = 0.0f32;
        let e1x = x1 - x0;
        let e1y = y1 - y0;
        let l1 = libm::sqrtf(e1x * e1x + e1y * e1y);
        if l1 > 1e-6 {
            nx += -e1y / l1;
            ny += e1x / l1;
        }
        let e2x = x2 - x1;
        let e2y = y2 - y1;
        let l2 = libm::sqrtf(e2x * e2x + e2y * e2y);
        if l2 > 1e-6 {
            nx += -e2y / l2;
            ny += e2x / l2;
        }
        let ln = libm::sqrtf(nx * nx + ny * ny);
        if ln > 1e-6 {
            out.push((x1 + nx / ln * amount, y1 + ny / ln * amount));
        } else {
            out.push((x1, y1));
        }
    }
    out
}

/// Rasterize a glyph outline into an 8-bit coverage bitmap.
///
/// UI sizes (≤16px) use 8×8 super-sampling and pixel-grid padding.
/// Coverage is true area coverage so counters in `e` / `a` / `o` stay open.
pub(super) fn rasterize_outline(
    outline: &GlyphOutline,
    units_per_em: u16,
    px_size: f32,
) -> GlyphBitmap {
    let scale = if units_per_em == 0 {
        1.0
    } else {
        px_size / units_per_em as f32
    };
    let mut advance = libm::roundf(outline.advance_width as f32 * scale).max(1.0) as i32;

    if outline.contours.is_empty() {
        return GlyphBitmap {
            width: 1,
            height: 1,
            bearing_x: 0,
            bearing_y: 0,
            advance,
            pixels: vec![0u8; 1],
        };
    }

    // Light stem darkening at caption sizes so thin Inter fallback stems
    // do not dropout when this rasterizer is used outside fontdue.
    let embolden_px = if px_size < 24.0 {
        0.40 * (1.0 - (px_size / 24.0) * 0.4)
    } else {
        0.0
    };
    let pad = 1.0 + embolden_px;
    let x_min_px = outline.x_min as f32 * scale - pad;
    let y_min_px = outline.y_min as f32 * scale - pad;
    let x_max_px = outline.x_max as f32 * scale + pad;
    let y_max_px = outline.y_max as f32 * scale + pad;

    let origin_x = libm::floorf(x_min_px);
    let origin_y = libm::floorf(y_min_px);
    let top = libm::ceilf(y_max_px);
    let right = libm::ceilf(x_max_px);
    let width = (right - origin_x).max(1.0) as usize;
    let height = (top - origin_y).max(1.0) as usize;
    if width > 512 || height > 512 {
        return GlyphBitmap {
            width: 1,
            height: 1,
            bearing_x: 0,
            bearing_y: 0,
            advance,
            pixels: vec![0u8; 1],
        };
    }

    let (ss_x, ss_y) = if px_size <= 16.0 {
        (8usize, 8usize)
    } else if px_size <= 24.0 {
        (8usize, 4usize)
    } else {
        (4usize, 4usize)
    };
    let ss_width = width * ss_x;
    let ss_height = height * ss_y;
    let mut coverage = vec![0u32; width * height];

    let flattened: Vec<Vec<(f32, f32)>> = outline
        .contours
        .iter()
        .map(|c| offset_polyline(&flatten_contour(c, scale), embolden_px))
        .collect();

    // TrueType fill rule is non-zero winding (not even-odd). Inter Bold in
    // particular uses overlapping contours; even-odd punches holes in stems.
    for sy in 0..ss_height {
        let y_f = top - (sy as f32 + 0.5) / ss_y as f32;
        let mut hits: Vec<(f32, i32)> = Vec::new();

        for flat_pts in &flattened {
            let n = flat_pts.len();
            if n < 2 {
                continue;
            }
            for i in 0..n {
                let (x0, y0) = flat_pts[i];
                let (x1, y1) = flat_pts[(i + 1) % n];

                if (y0 < y_f && y1 < y_f) || (y0 >= y_f && y1 >= y_f) {
                    continue;
                }

                let t = (y_f - y0) / (y1 - y0);
                let x_i = x0 + t * (x1 - x0);
                let dir = if y1 > y0 { 1 } else { -1 };
                hits.push(((x_i - origin_x) * ss_x as f32, dir));
            }
        }

        hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));
        let row = sy / ss_y;
        if row >= height {
            continue;
        }

        let mut winding = 0i32;
        for i in 0..hits.len() {
            if winding != 0 && i > 0 {
                let x_start = hits[i - 1].0.max(0.0) as usize;
                let x_end = hits[i].0.max(0.0) as usize;
                let x_end = x_end.min(ss_width);
                if x_end > x_start {
                    for sx in x_start..x_end {
                        let col = sx / ss_x;
                        if col < width {
                            coverage[row * width + col] += 1;
                        }
                    }
                }
            }
            winding += hits[i].1;
        }
    }

    let samples = (ss_x * ss_y) as u32;
    let mut pixels = vec![0u8; width * height];
    for i in 0..pixels.len() {
        pixels[i] = ((coverage[i] * 255) / samples.max(1)).min(255) as u8;
    }

    let bearing_x = origin_x as i32;
    let bearing_y = top as i32;
    let painted = bearing_x + width as i32;
    if advance < painted {
        advance = painted;
    }
    if advance < 1 {
        advance = 1;
    }

    GlyphBitmap {
        width,
        height,
        bearing_x,
        bearing_y,
        advance,
        pixels,
    }
}
