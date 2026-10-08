use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::Rect;
use crate::gui::response::Response;

use super::Ui;

impl<'a> Ui<'a> {
    // ── Checkbox ─────────────────────────────────────────────────

    /// Draw a checkbox with a label. Toggles `checked` on click.
    pub fn checkbox(&mut self, checked: &mut bool, text: &str) -> Response {
        let id = self.id_from(text);
        let box_size = 16u32;
        let text_w = (text.len() as u32) * 8;
        let total_w = box_size + 8 + text_w;
        let h = box_size.max(self.style.spacing.interact_height as u32);
        let rect = self.allocate_space(total_w, h);

        let resp = self.interact(rect, id, true, false);

        if resp.clicked() {
            *checked = !*checked;
        }

        // Checkbox box
        let box_y = rect.y + (h as i32 - box_size as i32) / 2;
        let box_rect = Rect::new(rect.x, box_y, box_size, box_size);
        let box_bg = if *checked {
            self.style.accent
        } else if resp.hovered {
            self.style.widget_bg_hovered
        } else {
            self.style.widget_bg
        };
        self.fb.fill_rounded_rect_aa(box_rect, box_bg, 3);
        self.fb.draw_rounded_rect(
            box_rect,
            if *checked {
                self.style.accent
            } else {
                self.style.border_color
            },
            3,
            1,
        );
        if *checked {
            // Checkmark
            self.fb
                .draw_line_aa(rect.x + 3, box_y + 8, rect.x + 6, box_y + 12, colors::WHITE);
            self.fb.draw_line_aa(
                rect.x + 6,
                box_y + 12,
                rect.x + 12,
                box_y + 4,
                colors::WHITE,
            );
        }

        // Label
        fonts::draw_string_compact(
            self.fb,
            rect.x + box_size as i32 + 8,
            rect.y + (h as i32 - 12) / 2,
            text,
            self.style.text_color,
            1,
        );

        let mut r = resp;
        r.changed = r.clicked;
        r
    }

    // ── Radio button ─────────────────────────────────────────────

    /// Draw a radio button. Sets `current` to `value` on click.
    pub fn radio_value<V: PartialEq + Copy>(
        &mut self,
        current: &mut V,
        value: V,
        text: &str,
    ) -> Response {
        let selected = *current == value;
        let id = self.id_from(text);
        let circle_r = 8u32;
        let text_w = (text.len() as u32) * 8;
        let total_w = circle_r * 2 + 8 + text_w;
        let h = (circle_r * 2).max(self.style.spacing.interact_height as u32);
        let rect = self.allocate_space(total_w, h);

        let resp = self.interact(rect, id, true, false);

        if resp.clicked() {
            *current = value;
        }

        // Outer circle
        let cx = rect.x + circle_r as i32;
        let cy = rect.y + h as i32 / 2;
        let border = if resp.hovered {
            self.style.accent_hovered
        } else if selected {
            self.style.accent
        } else {
            self.style.border_color
        };
        self.fb
            .fill_circle_aa(cx, cy, circle_r, self.style.widget_bg);
        // Draw border as a filled ring (outer - inner)
        self.fb.fill_circle_aa(cx, cy, circle_r, border);
        self.fb
            .fill_circle_aa(cx, cy, circle_r - 2, self.style.widget_bg);

        // Inner dot when selected
        if selected {
            self.fb.fill_circle_aa(cx, cy, 4, self.style.accent);
        }

        // Label
        fonts::draw_string_compact(
            self.fb,
            rect.x + (circle_r * 2) as i32 + 8,
            rect.y + (h as i32 - 12) / 2,
            text,
            self.style.text_color,
            1,
        );

        let mut r = resp;
        r.changed = r.clicked;
        r
    }

    /// Draw a radio button with bool state.
    pub fn radio(&mut self, selected: bool, text: &str) -> Response {
        let id = self.id_from(text);
        let circle_r = 8u32;
        let text_w = (text.len() as u32) * 8;
        let total_w = circle_r * 2 + 8 + text_w;
        let h = (circle_r * 2).max(self.style.spacing.interact_height as u32);
        let rect = self.allocate_space(total_w, h);

        let resp = self.interact(rect, id, true, false);

        let cx = rect.x + circle_r as i32;
        let cy = rect.y + h as i32 / 2;
        let border = if resp.hovered {
            self.style.accent_hovered
        } else if selected {
            self.style.accent
        } else {
            self.style.border_color
        };
        self.fb.fill_circle_aa(cx, cy, circle_r, border);
        self.fb
            .fill_circle_aa(cx, cy, circle_r - 2, self.style.widget_bg);
        if selected {
            self.fb.fill_circle_aa(cx, cy, 4, self.style.accent);
        }
        fonts::draw_string_compact(
            self.fb,
            rect.x + (circle_r * 2) as i32 + 8,
            rect.y + (h as i32 - 12) / 2,
            text,
            self.style.text_color,
            1,
        );

        resp
    }

    // ── Toggle / switch ──────────────────────────────────────────

    /// Draw a toggle switch with label.
    pub fn toggle(&mut self, enabled: &mut bool, text: &str) -> Response {
        let id = self.id_from(text);
        let switch_w = 36u32;
        let switch_h = 18u32;
        let text_w = (text.len() as u32) * 8;
        let total_w = text_w + 8 + switch_w;
        let h = switch_h.max(self.style.spacing.interact_height as u32);
        let rect = self.allocate_space(total_w, h);

        let resp = self.interact(rect, id, true, false);

        if resp.clicked() {
            *enabled = !*enabled;
        }

        // Label
        fonts::draw_string_compact(
            self.fb,
            rect.x,
            rect.y + (h as i32 - 12) / 2,
            text,
            self.style.text_color,
            1,
        );

        // Switch track
        let sx = rect.x + text_w as i32 + 8;
        let sy = rect.y + (h as i32 - switch_h as i32) / 2;
        let track_color = if *enabled {
            self.style.accent
        } else {
            self.style.widget_bg
        };
        self.fb
            .fill_rounded_rect_aa(Rect::new(sx, sy, switch_w, switch_h), track_color, 9);

        // Knob
        let knob_x = if *enabled {
            sx + switch_w as i32 - switch_h as i32 + 2
        } else {
            sx + 2
        };
        self.fb.fill_circle_aa(knob_x + 7, sy + 9, 7, colors::WHITE);

        let mut r = resp;
        r.changed = r.clicked;
        r
    }
}
