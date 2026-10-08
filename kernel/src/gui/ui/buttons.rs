use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::response::Response;

use super::Ui;

impl<'a> Ui<'a> {
    /// Place a button at an absolute rect (does not advance layout).
    /// Use this to wire KnoxUI interaction onto existing app grids.
    pub fn button_at(&mut self, rect: Rect, text: &str) -> Response {
        let id = self.id_from(text);
        let resp = self.interact(rect, id, true, false);

        let bg = if resp.is_pointer_button_down_on {
            self.style.widget_bg_active
        } else if resp.hovered {
            self.style.widget_bg_hovered
        } else {
            self.style.widget_bg
        };

        self.fb
            .fill_rounded_rect_aa(rect, bg, self.style.corner_radius);
        if resp.hovered {
            self.fb
                .draw_rounded_rect(rect, self.style.border_focused, self.style.corner_radius, 1);
        }
        fonts::draw_string_centered_compact(
            self.fb,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            text,
            self.style.text_color,
            1,
        );
        resp
    }

    // ── Button ───────────────────────────────────────────────────

    /// Draw a clickable button.
    pub fn button(&mut self, text: &str) -> Response {
        let id = self.id_from(text);
        let text_w = (text.len() as u32) * 8;
        let pad_x = self.style.spacing.button_padding_x as u32;
        let pad_y = self.style.spacing.button_padding_y as u32;
        let w = text_w + pad_x * 2;
        let h = 12 + pad_y * 2;
        let rect = self.allocate_space(w, h);

        let resp = self.interact(rect, id, true, false);

        let bg = if resp.is_pointer_button_down_on {
            self.style.widget_bg_active
        } else if resp.hovered {
            self.style.widget_bg_hovered
        } else {
            self.style.widget_bg
        };

        self.fb
            .fill_rounded_rect_aa(rect, bg, self.style.corner_radius);
        self.fb.draw_rounded_rect(
            rect,
            if resp.hovered {
                self.style.border_focused
            } else {
                self.style.border_color
            },
            self.style.corner_radius,
            1,
        );
        fonts::draw_string_centered_compact(
            self.fb,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            text,
            self.style.text_color,
            1,
        );

        resp
    }

    /// Draw a primary accent-colored button.
    pub fn primary_button(&mut self, text: &str) -> Response {
        let id = self.id_from(text);
        let text_w = (text.len() as u32) * 8;
        let pad_x = self.style.spacing.button_padding_x as u32;
        let pad_y = self.style.spacing.button_padding_y as u32;
        let w = text_w + pad_x * 2;
        let h = 12 + pad_y * 2;
        let rect = self.allocate_space(w, h);

        let resp = self.interact(rect, id, true, false);

        let bg = if resp.is_pointer_button_down_on {
            Pixel::rgb(50, 120, 220)
        } else if resp.hovered {
            self.style.accent_hovered
        } else {
            self.style.accent
        };

        self.fb
            .fill_rounded_rect_aa(rect, bg, self.style.corner_radius);
        fonts::draw_string_centered_compact(
            self.fb,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            text,
            colors::WHITE,
            1,
        );

        resp
    }

    /// Draw a small button (less padding).
    pub fn small_button(&mut self, text: &str) -> Response {
        let id = self.id_from(text);
        let text_w = (text.len() as u32) * 8;
        let w = text_w + 8;
        let h = 16u32;
        let rect = self.allocate_space(w, h);

        let resp = self.interact(rect, id, true, false);

        let bg = if resp.is_pointer_button_down_on {
            self.style.widget_bg_active
        } else if resp.hovered {
            self.style.widget_bg_hovered
        } else {
            Pixel::new(0, 0, 0, 0) // transparent when idle
        };
        if bg.a > 0 {
            self.fb
                .fill_rounded_rect_aa(rect, bg, self.style.corner_radius);
        }
        fonts::draw_string_centered_compact(
            self.fb,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            text,
            self.style.text_color,
            1,
        );

        resp
    }

    /// Draw a selectable label (like a button but styled as text, highlighted when selected).
    pub fn selectable_label(&mut self, selected: bool, text: &str) -> Response {
        let id = self.id_from(text);
        let text_w = (text.len() as u32) * 8;
        let w = text_w + 12;
        let h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(w, h);

        let resp = self.interact(rect, id, true, false);

        if selected || resp.hovered {
            let bg = if selected {
                self.style.selection_bg
            } else {
                self.style.widget_bg_hovered
            };
            self.fb.fill_rounded_rect_aa(rect, bg, 4);
        }

        let text_color = if selected {
            colors::WHITE
        } else if resp.hovered {
            self.style.text_color
        } else {
            self.style.text_dimmed
        };
        fonts::draw_string_compact(
            self.fb,
            rect.x + 6,
            rect.y + (rect.height as i32 - 12) / 2,
            text,
            text_color,
            1,
        );

        resp
    }

    /// Draw a selectable value: if clicked, set `current` to `value`.
    pub fn selectable_value<V: PartialEq + Copy>(
        &mut self,
        current: &mut V,
        value: V,
        text: &str,
    ) -> Response {
        let selected = *current == value;
        let resp = self.selectable_label(selected, text);
        if resp.clicked() {
            *current = value;
        }
        resp
    }
}
