// SPDX-License-Identifier: MIT
//! Coverage for rounded rectangles.
//!
//! Same idea as `kernel/src/gui/framebuffer/aa.rs` + `shapes.rs`: the body of
//! the rect is fully covered; only the four corner squares do a 1px anti-alias
//! fringe against a quarter-circle. This crate writes that coverage into a
//! standalone [`crate::raster::RasterBuffer`], not the kernel framebuffer.

use crate::geometry::{Bounds, Corners};

/// Empty-rect SDF: far outside, so coverage is zero.
const SDF_EMPTY: f32 = 1.0e9;

/// Pixel coverage 0..=255 for a rounded rect. `px`/`py` are integer pixel
/// coordinates (sample at the pixel center). Pixels outside `bounds` are 0.
///
/// `radii` are clamped to `bounds` (CSS-style) before sampling.
pub(crate) fn rounded_rect_coverage(px: i32, py: i32, bounds: Bounds, radii: Corners) -> u8 {
    if bounds.is_empty() {
        return 0;
    }
    let lx = px - bounds.x();
    let ly = py - bounds.y();
    let w = bounds.width();
    let h = bounds.height();
    if lx < 0 || ly < 0 || lx >= w || ly >= h {
        return 0;
    }

    let radii = radii.clamp_to(bounds.size);
    if radii.is_sharp() {
        return 255;
    }

    let tl = radii.top_left.get();
    let tr = radii.top_right.get();
    let br = radii.bottom_right.get();
    let bl = radii.bottom_left.get();

    // Geometric quarter-circle centers sit `r` in from the rect edges.
    let (ccx, ccy, r) = if lx < tl && ly < tl {
        (tl as f32, tl as f32, tl as f32)
    } else if lx >= w - tr && ly < tr {
        ((w - tr) as f32, tr as f32, tr as f32)
    } else if lx >= w - br && ly >= h - br {
        ((w - br) as f32, (h - br) as f32, br as f32)
    } else if lx < bl && ly >= h - bl {
        (bl as f32, (h - bl) as f32, bl as f32)
    } else {
        return 255;
    };

    if r <= 0.0 {
        return 255;
    }

    let dx = (lx as f32 + 0.5) - ccx;
    let dy = (ly as f32 + 0.5) - ccy;
    let dist = libm::sqrtf(dx * dx + dy * dy);
    // 1px AA: coverage = clamp(r + 0.5 - dist, 0, 1), matching the kernel
    // `rounded_rect_coverage` falloff (`r + 0.5` vs distance).
    let cov = r + 0.5 - dist;
    if cov <= 0.0 {
        0
    } else if cov >= 1.0 {
        255
    } else {
        libm::roundf(cov * 255.0) as u8
    }
}

/// Signed distance from the pixel center to a rounded rectangle.
/// Negative inside, positive outside. Empty bounds are far outside.
pub(crate) fn rounded_rect_sdf(px: i32, py: i32, bounds: Bounds, radii: Corners) -> f32 {
    if bounds.is_empty() {
        return SDF_EMPTY;
    }
    let radii = radii.clamp_to(bounds.size);
    let cx = bounds.x() as f32 + bounds.width() as f32 * 0.5;
    let cy = bounds.y() as f32 + bounds.height() as f32 * 0.5;
    let hx = bounds.width() as f32 * 0.5;
    let hy = bounds.height() as f32 * 0.5;
    let dx = (px as f32 + 0.5) - cx;
    let dy = (py as f32 + 0.5) - cy;
    // Screen y grows down: (+x,+y) is bottom-right.
    let r = match (dx > 0.0, dy > 0.0) {
        (true, false) => radii.top_right.get(),
        (true, true) => radii.bottom_right.get(),
        (false, false) => radii.top_left.get(),
        (false, true) => radii.bottom_left.get(),
    }
    .max(0) as f32;
    // Inigo Quilez rounded-box SDF (per-corner radius of this quadrant).
    let qx = libm::fabsf(dx) - hx + r;
    let qy = libm::fabsf(dy) - hy + r;
    let max_q = if qx > qy { qx } else { qy };
    let inside = if max_q < 0.0 { max_q } else { 0.0 };
    let ox = if qx > 0.0 { qx } else { 0.0 };
    let oy = if qy > 0.0 { qy } else { 0.0 };
    inside + libm::sqrtf(ox * ox + oy * oy) - r
}

/// Linear coverage from an SDF. `blur_px == 0` is a 1px AA fringe; otherwise
/// the ramp spans `±blur` (CSS-like: most of a σ = blur/2 gaussian).
fn coverage_from_sdf(sdf: f32, blur_px: f32) -> u8 {
    let half = if blur_px <= 0.0 { 0.5 } else { blur_px };
    let t = (half - sdf) / (2.0 * half);
    if t <= 0.0 {
        0
    } else if t >= 1.0 {
        255
    } else {
        libm::roundf(t * 255.0) as u8
    }
}

/// Coverage 0..=255 for a box shadow at pixel `(px, py)`.
///
/// `shape` is the unblurred rect (offset + spread already applied).
/// `inset` inverts the SDF; the raster still scissors inset shadows to the
/// original box so nothing leaks outside.
pub(crate) fn box_shadow_coverage(
    px: i32,
    py: i32,
    shape: Bounds,
    radii: Corners,
    blur_radius: i32,
    inset: bool,
) -> u8 {
    let mut sdf = rounded_rect_sdf(px, py, shape, radii);
    if inset {
        sdf = -sdf;
    }
    coverage_from_sdf(sdf, blur_radius.max(0) as f32)
}

#[cfg(test)]
mod tests {
    use super::rounded_rect_coverage;
    use crate::geometry::{Bounds, Corners, px};

    fn cov(x: i32, y: i32, r: i32) -> u8 {
        rounded_rect_coverage(x, y, Bounds::from_xywh(0, 0, 16, 16), Corners::all(px(r)))
    }

    #[test]
    fn body_and_aabb_reject() {
        assert_eq!(cov(8, 8, 6), 255);
        assert_eq!(cov(8, 0, 6), 255);
        assert_eq!(cov(0, 8, 6), 255);
        assert_eq!(cov(-1, 8, 6), 0);
        assert_eq!(cov(16, 8, 6), 0);
        assert_eq!(
            rounded_rect_coverage(0, 0, Bounds::from_xywh(0, 0, 0, 8), Corners::all(px(4))),
            0
        );
    }

    #[test]
    fn corner_fringe_is_partial() {
        // Hand-checked: pixel-center disk, r=6, 16×16.
        // (1,1): dist((1.5,1.5),(6,6)) ≈ 6.364 → cov ≈ 0.136 → 35
        // (2,1): dist((2.5,1.5),(6,6)) ≈ 5.701 → cov ≈ 0.799 → 204
        assert_eq!(cov(0, 0, 6), 0);
        assert_eq!(cov(1, 1, 6), 35);
        assert_eq!(cov(2, 1, 6), 204);
        assert_eq!(cov(1, 2, 6), 204);
        assert_eq!(cov(2, 2, 6), 255);
        assert_eq!(cov(3, 3, 6), 255);
        assert_eq!(cov(15, 0, 6), 0);
        assert_eq!(cov(14, 1, 6), 35);
        assert_eq!(cov(15, 15, 6), 0);
        assert_eq!(cov(0, 15, 6), 0);
    }

    #[test]
    fn per_corner_radii() {
        let bounds = Bounds::from_xywh(0, 0, 16, 16);
        let radii = Corners {
            top_left: px(6),
            top_right: px(0),
            bottom_right: px(0),
            bottom_left: px(0),
        };
        assert_eq!(rounded_rect_coverage(1, 1, bounds, radii), 35);
        // Sharp top-right AABB corner is fully inside.
        assert_eq!(rounded_rect_coverage(15, 0, bounds, radii), 255);
        assert_eq!(rounded_rect_coverage(15, 15, bounds, radii), 255);
        assert_eq!(rounded_rect_coverage(0, 15, bounds, radii), 255);
    }

    #[test]
    fn sdf_sign_inside_outside() {
        let b = Bounds::from_xywh(0, 0, 16, 16);
        let sharp = Corners::ZERO;
        assert!(super::rounded_rect_sdf(8, 8, b, sharp) < 0.0);
        assert!(super::rounded_rect_sdf(-2, 8, b, sharp) > 0.0);
        // Pixel (0, 8): center 0.5px inside the left edge.
        let edge = super::rounded_rect_sdf(0, 8, b, sharp);
        assert!((edge + 0.5).abs() < 1e-5, "edge sdf {edge}");
        // Pixel (-1, 8): center 0.5px outside.
        let out = super::rounded_rect_sdf(-1, 8, b, sharp);
        assert!((out - 0.5).abs() < 1e-5, "outside sdf {out}");
        assert!(super::rounded_rect_sdf(0, 0, Bounds::ZERO, sharp) > 1.0e8);
    }

    #[test]
    fn zero_blur_sdf_matches_rounded_coverage_sample() {
        // D1.4 golden: (1,1) on 16×16 r=6 is coverage 35.
        let b = Bounds::from_xywh(0, 0, 16, 16);
        let r = Corners::all(px(6));
        assert_eq!(super::box_shadow_coverage(1, 1, b, r, 0, false), 35);
        assert_eq!(super::box_shadow_coverage(0, 0, b, r, 0, false), 0);
        assert_eq!(super::box_shadow_coverage(8, 8, b, r, 0, false), 255);
    }

    #[test]
    fn blur_falloff_outside_shape() {
        let b = Bounds::from_xywh(8, 8, 16, 16);
        let sharp = Corners::ZERO;
        // Pixel (7, 16) is 0.5px outside the left edge; blur=4 → α 112.
        assert_eq!(super::box_shadow_coverage(7, 16, b, sharp, 4, false), 112);
        assert_eq!(super::box_shadow_coverage(6, 16, b, sharp, 4, false), 80);
        assert_eq!(super::box_shadow_coverage(4, 16, b, sharp, 4, false), 16);
        assert_eq!(super::box_shadow_coverage(3, 16, b, sharp, 4, false), 0);
        assert_eq!(super::box_shadow_coverage(16, 16, b, sharp, 4, false), 255);
        // Inset: outside the shape is full before scissor; deep interior fades.
        assert_eq!(super::box_shadow_coverage(7, 16, b, sharp, 4, true), 143);
        assert_eq!(super::box_shadow_coverage(16, 16, b, sharp, 4, true), 0);
    }
}
