use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::response::Response;

use super::Ui;

impl<'a> Ui<'a> {
    // ── Color picker (simple) ────────────────────────────────────

    /// Draw a simple color preview button. Returns `clicked`.
    pub fn color_button(&mut self, label: &str, color: Pixel) -> Response {
        let id = self.id_from(label);
        let swatch_size = 20u32;
        let label_w = (label.len() as u32) * 8 + 8;
        let total_w = label_w + swatch_size;
        let h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(total_w, h);

        let resp = self.interact(rect, id, true, false);

        fonts::draw_string_compact(
            self.fb,
            rect.x,
            rect.y + (h as i32 - 12) / 2,
            label,
            self.style.text_color,
            1,
        );

        let sx = rect.x + label_w as i32;
        let sy = rect.y + (h as i32 - swatch_size as i32) / 2;
        let swatch_rect = Rect::new(sx, sy, swatch_size, swatch_size);
        self.fb.fill_rounded_rect_aa(swatch_rect, color, 3);
        self.fb.draw_rounded_rect(
            swatch_rect,
            if resp.hovered {
                self.style.border_focused
            } else {
                self.style.border_color
            },
            3,
            1,
        );

        resp
    }

    // ── Image placeholder ────────────────────────────────────────

    /// Draw a colored rectangle as an image placeholder.
    pub fn image_placeholder(&mut self, width: u32, height: u32, color: Pixel) -> Response {
        let id = self.auto_id();
        let rect = self.allocate_space(width, height);
        self.fb.fill_rounded_rect_aa(rect, color, 4);
        Response::none(id, rect)
    }

    // ── Tooltip ──────────────────────────────────────────────────

    /// Show a tooltip near the pointer if the given response is hovered.
    pub fn show_tooltip(&mut self, resp: &Response, text: &str) {
        if !resp.hovered {
            return;
        }
        let text_w = (text.len() as u32) * 8 + 12;
        let text_h = 22u32;
        let tx = self.input.pointer_x + 12;
        let ty = self.input.pointer_y + 16;

        // Shadow
        self.fb.fill_rounded_rect_aa(
            Rect::new(tx + 2, ty + 2, text_w, text_h),
            Pixel::new(0, 0, 0, 120),
            6,
        );
        self.fb
            .fill_rounded_rect_aa(Rect::new(tx, ty, text_w, text_h), Pixel::rgb(50, 52, 58), 6);
        self.fb.draw_rounded_rect(
            Rect::new(tx, ty, text_w, text_h),
            self.style.border_color,
            6,
            1,
        );
        fonts::draw_string_compact(self.fb, tx + 6, ty + 5, text, colors::WHITE, 1);
    }

    // ── Spinner / loading ────────────────────────────────────────

    /// Draw a loading spinner animation.
    pub fn spinner(&mut self) -> Response {
        let id = self.auto_id();
        let size = 20u32;
        let rect = self.allocate_space(size, size);
        let cx = rect.x + size as i32 / 2;
        let cy = rect.y + size as i32 / 2;

        // Draw rotating dots
        let tick = self.input.frame_tick;
        let num_dots = 8u32;
        for i in 0..num_dots {
            let angle_step = 628 / num_dots as i32; // ~2π * 100
            let angle = (tick as i32 * 10 + i as i32 * angle_step) % 628;
            // Simple integer sin/cos approximation
            let (sin_a, cos_a) = int_sincos(angle);
            let dx = cos_a * 8 / 100;
            let dy = sin_a * 8 / 100;
            let alpha = 60 + (i as u8) * 25;
            self.fb.fill_circle_aa(
                cx + dx,
                cy + dy,
                2,
                Pixel::new(
                    self.style.accent.r,
                    self.style.accent.g,
                    self.style.accent.b,
                    alpha,
                ),
            );
        }

        Response::none(id, rect)
    }
}

/// Integer sin/cos approximation. Input: angle in centidegrees (0-628 ≈ 0-2π).
/// Returns (sin*100, cos*100).
fn int_sincos(angle: i32) -> (i32, i32) {
    // Very rough lookup using symmetry
    let a = angle.rem_euclid(628); // normalize to 0-627
    // Quarter tables for sin (0-π/2 in 0-157 steps)
    let quarter = a % 157;
    let sin_q = quarter * 100 / 157; // linear approximation of sin in [0, π/2]
    let sin_q = sin_q * (157 - quarter) * 4 / 157; // parabolic correction

    let (sin_val, cos_val) = match a / 157 {
        0 => (sin_q, 100 - sin_q),          // 0..π/2
        1 => (sin_q, -(100 - sin_q.abs())), // π/2..π  (adjusted below)
        2 => (-sin_q, -(100 - sin_q)),      // π..3π/2
        _ => (-sin_q, 100 - sin_q.abs()),   // 3π/2..2π
    };

    (sin_val.clamp(-100, 100), cos_val.clamp(-100, 100))
}
