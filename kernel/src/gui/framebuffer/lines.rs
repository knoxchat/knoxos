use super::{FrameBuffer, Pixel};

impl FrameBuffer {
    /// Draw a line between two points (Bresenham's algorithm)
    pub fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Pixel) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            if x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32 {
                self.blend_pixel(x as usize, y as usize, color);
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw an anti-aliased line using Wu's algorithm
    /// Produces smooth lines with sub-pixel alpha blending
    pub fn draw_line_aa(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Pixel) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();

        // Trivial cases: horizontal or vertical lines don't need AA
        if dy == 0 {
            let xs = x0.min(x1);
            let xe = x0.max(x1);
            for x in xs..=xe {
                if x >= 0 && x < self.width as i32 && y0 >= 0 && y0 < self.height as i32 {
                    self.blend_pixel(x as usize, y0 as usize, color);
                }
            }
            return;
        }
        if dx == 0 {
            let ys = y0.min(y1);
            let ye = y0.max(y1);
            for y in ys..=ye {
                if x0 >= 0 && x0 < self.width as i32 && y >= 0 && y < self.height as i32 {
                    self.blend_pixel(x0 as usize, y as usize, color);
                }
            }
            return;
        }

        let base_alpha = color.a as u32;
        let steep = dy > dx;

        // Work in fixed-point (8 fractional bits = 256 scale)
        let (mut ax, mut ay, mut bx, mut by) = if steep {
            (y0, x0, y1, x1) // swap axes so we always iterate along the longer axis
        } else {
            (x0, y0, x1, y1)
        };

        if ax > bx {
            core::mem::swap(&mut ax, &mut bx);
            core::mem::swap(&mut ay, &mut by);
        }

        let run = bx - ax;
        if run == 0 {
            return;
        }
        let rise = by - ay;
        // gradient in 8.8 fixed-point
        let gradient = (rise * 256) / run;

        // First endpoint
        let mut y_fp = ay * 256 + 128; // start at pixel center

        for x in ax..=bx {
            let y_int = y_fp >> 8;
            let frac = (y_fp & 0xFF) as u32; // fractional part 0..255
            let inv_frac = 255 - frac;

            // Plot two pixels straddling the ideal line
            let (px1, py1, px2, py2) = if steep {
                (y_int, x, y_int + 1, x)
            } else {
                (x, y_int, x, y_int + 1)
            };

            if px1 >= 0 && px1 < self.width as i32 && py1 >= 0 && py1 < self.height as i32 {
                let a1 = ((base_alpha * inv_frac) / 255).min(255) as u8;
                self.blend_pixel(
                    px1 as usize,
                    py1 as usize,
                    Pixel::new(color.r, color.g, color.b, a1),
                );
            }
            if px2 >= 0 && px2 < self.width as i32 && py2 >= 0 && py2 < self.height as i32 {
                let a2 = ((base_alpha * frac) / 255).min(255) as u8;
                self.blend_pixel(
                    px2 as usize,
                    py2 as usize,
                    Pixel::new(color.r, color.g, color.b, a2),
                );
            }

            y_fp += gradient;
        }
    }

    /// Draw a thick anti-aliased line (for UI elements like close buttons)
    /// Uses distance-to-line for smooth edges on any thickness
    pub fn draw_line_thick_aa(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: Pixel,
        thickness: f32,
    ) {
        let dx = (x1 - x0) as f32;
        let dy = (y1 - y0) as f32;
        let len = libm::sqrtf(dx * dx + dy * dy);
        if len < 0.001 {
            return;
        }
        // Normal vector components (perpendicular to line)
        let nx = -dy / len;
        let ny = dx / len;

        let half_t = thickness * 0.5;
        let base_alpha = color.a as u32;

        // Bounding box with 1px padding for AA fringe
        let min_x = (x0.min(x1) as f32 - half_t - 1.0) as i32;
        let max_x = (x0.max(x1) as f32 + half_t + 1.0) as i32;
        let min_y = (y0.min(y1) as f32 - half_t - 1.0) as i32;
        let max_y = (y0.max(y1) as f32 + half_t + 1.0) as i32;

        for py in min_y.max(0)..=max_y.min(self.height as i32 - 1) {
            for px in min_x.max(0)..=max_x.min(self.width as i32 - 1) {
                let fx = px as f32 + 0.5;
                let fy = py as f32 + 0.5;

                // Project point onto line to get distance along and perpendicular
                let to_px = fx - x0 as f32;
                let to_py = fy - y0 as f32;
                let along = (to_px * dx + to_py * dy) / len;
                let perp = (to_px * nx + to_py * ny).abs();

                // Clamp: only draw between endpoints (with small extension for caps)
                if along < -0.5 || along > len + 0.5 {
                    continue;
                }

                // Distance from line edge
                let dist_from_edge = perp - half_t;
                if dist_from_edge >= 1.0 {
                    continue;
                }

                let coverage = if dist_from_edge <= 0.0 {
                    255u32
                } else {
                    (255.0 * (1.0 - dist_from_edge)) as u32
                };

                // Also fade at endpoints
                let end_fade = if along < 0.0 {
                    (255.0 * (1.0 + along)) as u32
                } else if along > len {
                    (255.0 * (1.0 - (along - len))).max(0.0) as u32
                } else {
                    255u32
                };

                let final_alpha = ((base_alpha * coverage * end_fade) / (255 * 255)).min(255) as u8;
                if final_alpha > 0 {
                    self.blend_pixel(
                        px as usize,
                        py as usize,
                        Pixel::new(color.r, color.g, color.b, final_alpha),
                    );
                }
            }
        }
    }

    /// Draw an anti-aliased arc (portion of a circle outline)
    /// Useful for speaker wave icons, wifi arcs, etc.
    /// `start_angle` and `end_angle` in radians, `thickness` in pixels
    pub fn draw_arc_aa(
        &mut self,
        cx: i32,
        cy: i32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        thickness: f32,
        color: Pixel,
    ) {
        let half_t = thickness * 0.5;
        let r_outer = radius + half_t + 1.0;
        let r_inner = (radius - half_t - 1.0).max(0.0);
        let base_alpha = color.a as u32;

        let min_x = (cx as f32 - r_outer) as i32;
        let max_x = (cx as f32 + r_outer) as i32;
        let min_y = (cy as f32 - r_outer) as i32;
        let max_y = (cy as f32 + r_outer) as i32;

        for py in min_y.max(0)..=max_y.min(self.height as i32 - 1) {
            for px in min_x.max(0)..=max_x.min(self.width as i32 - 1) {
                let fx = px as f32 + 0.5 - cx as f32;
                let fy = py as f32 + 0.5 - cy as f32;

                let dist = libm::sqrtf(fx * fx + fy * fy);
                if dist < r_inner || dist > r_outer {
                    continue;
                }

                // Check angle (atan2)
                let mut angle = libm::atan2f(fy, fx);
                if angle < 0.0 {
                    angle += 2.0 * core::f32::consts::PI;
                }

                // Normalize angles
                let mut sa = start_angle;
                let mut ea = end_angle;
                if sa < 0.0 {
                    sa += 2.0 * core::f32::consts::PI;
                }
                if ea < 0.0 {
                    ea += 2.0 * core::f32::consts::PI;
                }

                let in_arc = if sa <= ea {
                    angle >= sa && angle <= ea
                } else {
                    angle >= sa || angle <= ea
                };
                if !in_arc {
                    continue;
                }

                // Distance from ideal circle
                let dist_from_circle = (dist - radius).abs();
                let dist_from_edge = dist_from_circle - half_t;

                if dist_from_edge >= 1.0 {
                    continue;
                }

                let coverage = if dist_from_edge <= 0.0 {
                    255u32
                } else {
                    (255.0 * (1.0 - dist_from_edge)) as u32
                };

                let final_alpha = ((base_alpha * coverage) / 255).min(255) as u8;
                if final_alpha > 0 {
                    self.blend_pixel(
                        px as usize,
                        py as usize,
                        Pixel::new(color.r, color.g, color.b, final_alpha),
                    );
                }
            }
        }
    }
}
