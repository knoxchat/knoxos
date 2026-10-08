use super::aa::{isqrt_256, rounded_rect_coverage};
use super::{FrameBuffer, Pixel, Rect};

impl FrameBuffer {
    /// Draw a filled rounded rectangle
    pub fn fill_rounded_rect(&mut self, rect: Rect, color: Pixel, radius: u32) {
        // Main body
        self.fill_rect(
            Rect::new(
                rect.x + radius as i32,
                rect.y,
                rect.width - radius * 2,
                rect.height,
            ),
            color,
        );
        self.fill_rect(
            Rect::new(
                rect.x,
                rect.y + radius as i32,
                rect.width,
                rect.height - radius * 2,
            ),
            color,
        );

        // Corners (filled circles at each corner)
        let r = radius as i32;
        self.fill_circle(rect.x + r, rect.y + r, radius, color);
        self.fill_circle(
            rect.x + rect.width as i32 - r - 1,
            rect.y + r,
            radius,
            color,
        );
        self.fill_circle(
            rect.x + r,
            rect.y + rect.height as i32 - r - 1,
            radius,
            color,
        );
        self.fill_circle(
            rect.x + rect.width as i32 - r - 1,
            rect.y + rect.height as i32 - r - 1,
            radius,
            color,
        );
    }

    /// Draw a rounded rectangle outline (border only)
    pub fn draw_rounded_rect(&mut self, rect: Rect, color: Pixel, radius: u32, thickness: u32) {
        let r = radius as i32;
        let t = thickness.max(1) as i32;
        // Top edge (between corners)
        for i in 0..t {
            self.draw_hline(rect.x + r, rect.y + i, rect.width - radius * 2, color);
        }
        // Bottom edge
        for i in 0..t {
            self.draw_hline(
                rect.x + r,
                rect.y + rect.height as i32 - 1 - i,
                rect.width - radius * 2,
                color,
            );
        }
        // Left edge (between corners)
        for i in 0..t {
            self.draw_vline(rect.x + i, rect.y + r, rect.height - radius * 2, color);
        }
        // Right edge
        for i in 0..t {
            self.draw_vline(
                rect.x + rect.width as i32 - 1 - i,
                rect.y + r,
                rect.height - radius * 2,
                color,
            );
        }
        // Corner arcs using AA circle outline
        self.draw_circle_aa(rect.x + r, rect.y + r, radius, color);
        self.draw_circle_aa(
            rect.x + rect.width as i32 - r - 1,
            rect.y + r,
            radius,
            color,
        );
        self.draw_circle_aa(
            rect.x + r,
            rect.y + rect.height as i32 - r - 1,
            radius,
            color,
        );
        self.draw_circle_aa(
            rect.x + rect.width as i32 - r - 1,
            rect.y + rect.height as i32 - r - 1,
            radius,
            color,
        );
    }

    /// Fill a circle using horizontal line spans (much faster than per-pixel)
    pub fn fill_circle(&mut self, cx: i32, cy: i32, radius: u32, color: Pixel) {
        let r = radius as i32;
        let r_sq = r * r;
        for dy in -r..=r {
            let dy_sq = dy * dy;
            if dy_sq > r_sq {
                continue;
            }
            let mut dx_max = 0i32;
            while (dx_max + 1) * (dx_max + 1) + dy_sq <= r_sq {
                dx_max += 1;
            }
            let py = cy + dy;
            let x_start = cx - dx_max;
            let x_end = cx + dx_max;
            if py >= 0 && py < self.height as i32 {
                let xs = x_start.max(0) as usize;
                let xe = (x_end + 1).min(self.width as i32).max(0) as usize;
                let span_w = xe.saturating_sub(xs);
                if span_w > 0 {
                    self.fill_rect(Rect::new(xs as i32, py, span_w as u32, 1), color);
                }
            }
        }
    }

    /// Fill a circle with anti-aliasing (smooth edges via coverage-based alpha)
    pub fn fill_circle_aa(&mut self, cx: i32, cy: i32, radius: u32, color: Pixel) {
        if radius == 0 {
            return;
        }
        // Use 16x fixed-point for sub-pixel precision
        // radius in 8.8 fixed point
        let r = radius as i32;
        let r_outer = r + 1; // 1px anti-alias fringe
        let base_alpha = color.a as u32;

        for dy in -r_outer..=r_outer {
            let py = cy + dy;
            if py < 0 || py >= self.height as i32 {
                continue;
            }
            for dx in -r_outer..=r_outer {
                let px = cx + dx;
                if px < 0 || px >= self.width as i32 {
                    continue;
                }

                // Distance from center (using 256x precision for smoother result)
                let dist_sq_256 = (dx * dx + dy * dy) * 256;
                let r_sq_256 = r * r * 256;

                if dist_sq_256 <= r_sq_256 - 256 * r {
                    // Fully inside: draw at full alpha
                    self.blend_pixel(px as usize, py as usize, color);
                } else if dist_sq_256 <= r_sq_256 + 256 * r {
                    // On the edge: compute coverage for anti-aliasing
                    // Approximate: linear falloff over 1px
                    // dist ≈ sqrt(dist_sq), but we can linearize around r
                    // coverage ≈ 1 - (dist - r + 0.5)
                    // Using integer approximation:
                    let dist_x256 = isqrt_256(dist_sq_256 as u32);
                    let r_x256 = r as u32 * 256;
                    let coverage = if dist_x256 <= r_x256 {
                        255u32
                    } else {
                        let overshoot = dist_x256 - r_x256; // in 1/256 units
                        255u32.saturating_sub(overshoot)
                    };
                    if coverage > 0 {
                        let aa_alpha = ((base_alpha * coverage) / 255).min(255) as u8;
                        let aa_color = Pixel::new(color.r, color.g, color.b, aa_alpha);
                        self.blend_pixel(px as usize, py as usize, aa_color);
                    }
                }
                // else: fully outside, skip
            }
        }
    }

    /// Fill a rounded rectangle with anti-aliased edges and vertical gradient
    /// Optimized: middle rows use fast span fill, only corner rows do per-pixel AA.
    pub fn fill_rounded_rect_gradient_aa(
        &mut self,
        rect: Rect,
        top_color: Pixel,
        bottom_color: Pixel,
        radius: u32,
    ) {
        let x0 = rect.x;
        let y0 = rect.y;
        let w = rect.width as i32;
        let h = rect.height as i32;
        let r = (radius as i32).min(w / 2).min(h / 2);

        if w <= 0 || h <= 0 {
            return;
        }

        for py in y0.max(0)..(y0 + h).min(self.height as i32) {
            // Compute gradient color for this row
            let t = ((py - y0) as u32 * 255) / (h.max(1) as u32);
            let row_color = Pixel::lerp(top_color, bottom_color, t as u8);

            let ly = py - y0; // local y within rect
            let in_corner = ly < r || ly >= h - r;

            if !in_corner {
                // Middle rows: full width span, no AA needed — fast path
                let xs = x0.max(0);
                let xe = (x0 + w).min(self.width as i32);
                if xe > xs {
                    if row_color.a == 255 {
                        // Opaque: direct fill (no blending)
                        self.fill_rect(Rect::new(xs, py, (xe - xs) as u32, 1), row_color);
                    } else {
                        // Semi-transparent: blend entire span
                        for px in xs..xe {
                            self.blend_pixel(px as usize, py as usize, row_color);
                        }
                    }
                }
            } else {
                // Corner rows: per-pixel AA coverage
                for px in x0.max(0)..(x0 + w).min(self.width as i32) {
                    let coverage = rounded_rect_coverage(px, py, x0, y0, w, h, r);
                    if coverage == 0 {
                        continue;
                    }

                    if coverage == 255 {
                        self.blend_pixel(px as usize, py as usize, row_color);
                    } else {
                        let aa_alpha =
                            ((row_color.a as u32 * coverage as u32) / 255).min(255) as u8;
                        let aa_color = Pixel::new(row_color.r, row_color.g, row_color.b, aa_alpha);
                        self.blend_pixel(px as usize, py as usize, aa_color);
                    }
                }
            }
        }
    }

    /// Fill a rounded rectangle with anti-aliased edges (single color)
    pub fn fill_rounded_rect_aa(&mut self, rect: Rect, color: Pixel, radius: u32) {
        let x0 = rect.x;
        let y0 = rect.y;
        let w = rect.width as i32;
        let h = rect.height as i32;
        let r = (radius as i32).min(w / 2).min(h / 2);

        if w <= 0 || h <= 0 {
            return;
        }

        for py in y0.max(0)..(y0 + h).min(self.height as i32) {
            // Find the span of fully-inside pixels for this row for fast fill
            let ly = py - y0; // local y within rect

            // Check if we're in a corner row
            let in_top_corner = ly < r;
            let in_bottom_corner = ly >= h - r;

            if !in_top_corner && !in_bottom_corner {
                // Middle rows: full width, no AA needed
                let xs = x0.max(0);
                let xe = (x0 + w).min(self.width as i32);
                if xe > xs {
                    self.fill_rect(Rect::new(xs, py, (xe - xs) as u32, 1), color);
                }
            } else {
                // Corner rows: need per-pixel AA
                for px in x0.max(0)..(x0 + w).min(self.width as i32) {
                    let coverage = rounded_rect_coverage(px, py, x0, y0, w, h, r);
                    if coverage == 0 {
                        continue;
                    }

                    if coverage == 255 {
                        self.blend_pixel(px as usize, py as usize, color);
                    } else {
                        let aa_alpha = ((color.a as u32 * coverage as u32) / 255).min(255) as u8;
                        let aa_color = Pixel::new(color.r, color.g, color.b, aa_alpha);
                        self.blend_pixel(px as usize, py as usize, aa_color);
                    }
                }
            }
        }
    }

    /// Draw a circle outline using horizontal spans
    pub fn draw_circle(&mut self, cx: i32, cy: i32, radius: u32, color: Pixel) {
        let r = radius as i32;
        let r_sq = r * r;
        let inner_sq = (r - 1) * (r - 1);
        for dy in -r..=r {
            let dy_sq = dy * dy;
            if dy_sq > r_sq {
                continue;
            }
            let py = cy + dy;
            if py < 0 || py >= self.height as i32 {
                continue;
            }
            // Find the range of dx where inner² < dx²+dy² <= r²
            for dx in -r..=r {
                let dist_sq = dx * dx + dy_sq;
                if dist_sq >= inner_sq && dist_sq <= r_sq {
                    let px = cx + dx;
                    if px >= 0 && (px as usize) < self.width {
                        self.blend_pixel(px as usize, py as usize, color);
                    }
                }
            }
        }
    }

    /// Draw an anti-aliased circle outline using distance-based alpha
    pub fn draw_circle_aa(&mut self, cx: i32, cy: i32, radius: u32, color: Pixel) {
        let r = radius as f32;
        let thickness = 1.0f32;
        let ri = r - thickness * 0.5;
        let ro = r + thickness * 0.5;
        let ro2 = (ro + 1.0) as i32;

        for dy in -ro2..=ro2 {
            let py = cy + dy;
            if py < 0 || py >= self.height as i32 {
                continue;
            }
            for dx in -ro2..=ro2 {
                let px = cx + dx;
                if px < 0 || px >= self.width as i32 {
                    continue;
                }
                let dist = libm::sqrtf((dx * dx + dy * dy) as f32);
                // Distance to the ring center-line (radius)
                let ring_dist = (dist - r).abs();
                if ring_dist > thickness * 0.5 + 1.0 {
                    continue;
                }
                // Alpha based on coverage
                let alpha = if ring_dist <= thickness * 0.5 {
                    1.0
                } else {
                    1.0 - (ring_dist - thickness * 0.5)
                };
                if alpha <= 0.0 {
                    continue;
                }
                let a = (alpha * color.a as f32) as u8;
                let aa_color = Pixel::new(color.r, color.g, color.b, a);
                self.blend_pixel(px as usize, py as usize, aa_color);
            }
        }
    }
}
