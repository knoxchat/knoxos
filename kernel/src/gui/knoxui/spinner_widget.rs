/// Spinner — Animated loading spinner widget.
///
/// ```ignore
/// SpinnerWidget::new().size(24).show(ui);
/// ```
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::Response;
use crate::gui::ui::Ui;

use core::f64::consts::PI;

pub struct SpinnerWidget {
    size: u32,
    color: Pixel,
    thickness: u32,
}

impl SpinnerWidget {
    pub fn new() -> Self {
        Self {
            size: 20,
            color: colors::ACCENT_PRIMARY,
            thickness: 3,
        }
    }

    pub fn size(mut self, s: u32) -> Self {
        self.size = s;
        self
    }
    pub fn color(mut self, c: Pixel) -> Self {
        self.color = c;
        self
    }
    pub fn thickness(mut self, t: u32) -> Self {
        self.thickness = t;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let rect = ui.allocate_space(self.size, self.size);
        let id = ui.id.with("spinner");

        let cx = rect.x + self.size as i32 / 2;
        let cy = rect.y + self.size as i32 / 2;
        let radius = (self.size / 2).saturating_sub(self.thickness) as i32;

        // Use frame tick as animation phase
        let tick = ui.input.frame_tick;
        let segments = 12u32;
        let active_segment = ((tick / 3) % segments as u64) as u32; // rotate every ~3 frames

        for i in 0..segments {
            // Angle for this segment
            let angle_start = (i as f64 * 2.0 * PI) / segments as f64;
            let angle_end = ((i + 1) as f64 * 2.0 * PI) / segments as f64;

            // Distance from active segment determines brightness
            let dist = {
                let d = (i as i32 - active_segment as i32).abs();
                let d2 = segments as i32 - d;
                d.min(d2) as u32
            };

            let alpha = if dist == 0 {
                255u8
            } else if dist == 1 {
                180
            } else if dist == 2 {
                120
            } else if dist == 3 {
                70
            } else {
                35
            };

            let seg_color = Pixel::new(self.color.r, self.color.g, self.color.b, alpha);

            // Draw a small dot at segment position
            let mid_angle = (angle_start + angle_end) / 2.0;
            // Approximate sin/cos using integer
            let sx = cx + ((radius as f64 * cos_approx(mid_angle)) as i32);
            let sy = cy + ((radius as f64 * sin_approx(mid_angle)) as i32);

            let dot_r = self.thickness.max(2);
            ui.fb.fill_circle_aa(sx, sy, dot_r, seg_color);
        }

        ui.interact(rect, id, true, false)
    }
}

/// Simple sine approximation (Taylor 3-term)
fn sin_approx(x: f64) -> f64 {
    // Normalize to [-PI, PI]
    let mut x = x % (2.0 * PI);
    if x > PI {
        x -= 2.0 * PI;
    }
    if x < -PI {
        x += 2.0 * PI;
    }
    // Taylor: sin(x) ≈ x - x^3/6 + x^5/120
    let x2 = x * x;
    let x3 = x2 * x;
    let x5 = x3 * x2;
    x - x3 / 6.0 + x5 / 120.0
}

fn cos_approx(x: f64) -> f64 {
    sin_approx(x + PI / 2.0)
}
