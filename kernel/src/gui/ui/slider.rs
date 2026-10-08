use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::response::Response;

use super::Ui;

impl<'a> Ui<'a> {
    // ── Slider ───────────────────────────────────────────────────

    /// Draw a horizontal slider for a u8 value in [min, max].
    pub fn slider_u8(&mut self, label: &str, value: &mut u8, min: u8, max: u8) -> Response {
        let id = self.id_from(label);
        let slider_w = self.style.spacing.slider_width as u32;
        let label_w = (label.len() as u32) * 8 + 8;
        let value_w = 32u32; // "255"
        let total_w = label_w + slider_w + 8 + value_w;
        let h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(total_w, h);

        // Label
        fonts::draw_string_compact(
            self.fb,
            rect.x,
            rect.y + (h as i32 - 12) / 2,
            label,
            self.style.text_color,
            1,
        );

        // Slider track
        let track_x = rect.x + label_w as i32;
        let track_y = rect.y + h as i32 / 2 - 3;
        let track_rect = Rect::new(track_x, track_y, slider_w, 6);
        self.fb
            .fill_rounded_rect_aa(track_rect, self.style.widget_bg, 3);

        // Fill portion
        let range = (max as i32 - min as i32).max(1);
        let fill_frac = (*value as i32 - min as i32) * slider_w as i32 / range;
        if fill_frac > 0 {
            self.fb.fill_rounded_rect_aa(
                Rect::new(track_x, track_y, fill_frac as u32, 6),
                self.style.accent,
                3,
            );
        }

        // Thumb
        let thumb_cx = track_x + fill_frac;
        let thumb_cy = rect.y + h as i32 / 2;

        // Interaction: drag the slider track area
        let interact_rect = Rect::new(track_x - 8, rect.y, slider_w + 16, h);
        let resp = self.interact(interact_rect, id, true, true);

        let thumb_color = if resp.dragged || resp.is_pointer_button_down_on {
            self.style.accent_hovered
        } else if resp.hovered {
            colors::WHITE
        } else {
            self.style.accent
        };

        self.fb.fill_circle_aa(thumb_cx, thumb_cy, 8, thumb_color);
        self.fb.fill_circle_aa(thumb_cx, thumb_cy, 5, colors::WHITE);

        // Update value if dragged or clicked
        let mut changed = false;
        if resp.is_pointer_button_down_on || resp.dragged {
            let relative = (self.input.pointer_x - track_x).clamp(0, slider_w as i32);
            let new_val = min as i32 + relative * range / slider_w as i32;
            let new_val = new_val.clamp(min as i32, max as i32) as u8;
            if new_val != *value {
                *value = new_val;
                changed = true;
            }
        }

        // Value text
        let val_str = alloc::format!("{}", *value);
        fonts::draw_string_compact(
            self.fb,
            track_x + slider_w as i32 + 8,
            rect.y + (h as i32 - 12) / 2,
            &val_str,
            self.style.text_color,
            1,
        );

        let mut r = resp;
        r.rect = rect;
        r.changed = changed;
        r
    }

    /// Draw a slider for an i32 value in [min, max].
    pub fn slider_i32(&mut self, label: &str, value: &mut i32, min: i32, max: i32) -> Response {
        let id = self.id_from(label);
        let slider_w = self.style.spacing.slider_width as u32;
        let label_w = (label.len() as u32) * 8 + 8;
        let value_w = 48u32;
        let total_w = label_w + slider_w + 8 + value_w;
        let h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(total_w, h);

        fonts::draw_string_compact(
            self.fb,
            rect.x,
            rect.y + (h as i32 - 12) / 2,
            label,
            self.style.text_color,
            1,
        );

        let track_x = rect.x + label_w as i32;
        let track_y = rect.y + h as i32 / 2 - 3;
        self.fb.fill_rounded_rect_aa(
            Rect::new(track_x, track_y, slider_w, 6),
            self.style.widget_bg,
            3,
        );

        let range = (max - min).max(1);
        let fill_frac = (*value - min) * slider_w as i32 / range;
        if fill_frac > 0 {
            self.fb.fill_rounded_rect_aa(
                Rect::new(track_x, track_y, fill_frac.max(0) as u32, 6),
                self.style.accent,
                3,
            );
        }

        let thumb_cx = track_x + fill_frac.clamp(0, slider_w as i32);
        let thumb_cy = rect.y + h as i32 / 2;

        let interact_rect = Rect::new(track_x - 8, rect.y, slider_w + 16, h);
        let resp = self.interact(interact_rect, id, true, true);

        let thumb_color = if resp.dragged || resp.is_pointer_button_down_on {
            self.style.accent_hovered
        } else if resp.hovered {
            colors::WHITE
        } else {
            self.style.accent
        };
        self.fb.fill_circle_aa(thumb_cx, thumb_cy, 8, thumb_color);
        self.fb.fill_circle_aa(thumb_cx, thumb_cy, 5, colors::WHITE);

        let mut changed = false;
        if resp.is_pointer_button_down_on || resp.dragged {
            let relative = (self.input.pointer_x - track_x).clamp(0, slider_w as i32);
            let new_val = min + relative * range / slider_w as i32;
            let new_val = new_val.clamp(min, max);
            if new_val != *value {
                *value = new_val;
                changed = true;
            }
        }

        let val_str = alloc::format!("{}", *value);
        fonts::draw_string_compact(
            self.fb,
            track_x + slider_w as i32 + 8,
            rect.y + (h as i32 - 12) / 2,
            &val_str,
            self.style.text_color,
            1,
        );

        let mut r = resp;
        r.rect = rect;
        r.changed = changed;
        r
    }

    // ── Progress bar ─────────────────────────────────────────────

    /// Draw a progress bar (0-100).
    pub fn progress_bar(&mut self, progress: u8, text: Option<&str>) -> Response {
        let id = self.auto_id();
        let w = self.available_width().max(100) as u32;
        let h = 20u32;
        let rect = self.allocate_space(w, h);

        self.fb.fill_rounded_rect_aa(rect, self.style.widget_bg, 3);
        let fill_w = (rect.width * progress.min(100) as u32) / 100;
        if fill_w > 0 {
            self.fb.fill_rounded_rect_aa(
                Rect::new(rect.x, rect.y, fill_w, h),
                self.style.accent,
                3,
            );
        }

        let label = if let Some(t) = text {
            alloc::format!("{} {}%", t, progress)
        } else {
            alloc::format!("{}%", progress)
        };
        fonts::draw_string_centered_compact(
            self.fb,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            &label,
            colors::WHITE,
            1,
        );

        Response::none(id, rect)
    }
}
