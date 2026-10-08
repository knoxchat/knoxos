use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::response::Response;

use super::{UI_MEMORY, Ui};

impl<'a> Ui<'a> {
    // ── Combo box / dropdown ─────────────────────────────────────

    /// Draw a combo box (dropdown selector). Shows the `selected_text` and
    /// opens a popup listing items when clicked.
    pub fn combo_box(
        &mut self,
        label: &str,
        selected_text: &str,
        items: &[&str],
        current_index: &mut usize,
    ) -> Response {
        let id = self.id_from(label);
        let combo_w = self.style.spacing.combo_width as u32;
        let label_w = if label.is_empty() {
            0u32
        } else {
            (label.len() as u32) * 8 + 8
        };
        let total_w = label_w + combo_w;
        let h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(total_w, h);

        // Label
        if !label.is_empty() {
            fonts::draw_string_compact(
                self.fb,
                rect.x,
                rect.y + (h as i32 - 12) / 2,
                label,
                self.style.text_color,
                1,
            );
        }

        let combo_rect = Rect::new(rect.x + label_w as i32, rect.y, combo_w, h);
        let resp = self.interact(combo_rect, id, true, false);

        // Toggle open/closed
        let is_open = {
            let mem = UI_MEMORY.lock();
            mem.get_bool(id, false)
        };
        if resp.clicked() {
            let mut mem = UI_MEMORY.lock();
            mem.set_bool(id, !is_open);
        }

        // Draw combo box
        let bg = if is_open || resp.hovered {
            self.style.widget_bg_hovered
        } else {
            self.style.widget_bg
        };
        self.fb
            .fill_rounded_rect_aa(combo_rect, bg, self.style.corner_radius);
        self.fb.draw_rounded_rect(
            combo_rect,
            if is_open {
                self.style.border_focused
            } else {
                self.style.border_color
            },
            self.style.corner_radius,
            1,
        );
        fonts::draw_string_compact(
            self.fb,
            combo_rect.x + 6,
            combo_rect.y + (h as i32 - 12) / 2,
            selected_text,
            self.style.text_color,
            1,
        );

        // Down arrow
        let arrow_x = combo_rect.x + combo_rect.width as i32 - 16;
        let arrow_y = combo_rect.y + h as i32 / 2 - 2;
        for dy in 0..4i32 {
            self.fb.draw_hline(
                arrow_x + 2 - dy,
                arrow_y + dy,
                (dy * 2 + 1) as u32,
                self.style.text_dimmed,
            );
        }

        // Dropdown popup
        let mut changed = false;
        if is_open && !items.is_empty() {
            let popup_x = combo_rect.x;
            let popup_y = combo_rect.y + h as i32 + 2;
            let popup_w = combo_rect.width;
            let item_h = 24u32;
            let popup_h = items.len() as u32 * item_h + 4;
            let popup_rect = Rect::new(popup_x, popup_y, popup_w, popup_h);

            // Shadow
            self.fb.fill_rounded_rect_aa(
                Rect::new(popup_x + 2, popup_y + 2, popup_w, popup_h),
                Pixel::new(0, 0, 0, 100),
                self.style.corner_radius,
            );
            // Background
            self.fb.fill_rounded_rect_aa(
                popup_rect,
                Pixel::rgb(35, 38, 45),
                self.style.corner_radius,
            );
            self.fb.draw_rounded_rect(
                popup_rect,
                self.style.border_color,
                self.style.corner_radius,
                1,
            );

            for (i, item) in items.iter().enumerate() {
                let iy = popup_y + 2 + i as i32 * item_h as i32;
                let item_rect = Rect::new(popup_x + 2, iy, popup_w - 4, item_h);
                let is_selected = i == *current_index;
                let item_id = id.with_index(i);

                let pointer_in = item_rect.contains(self.input.pointer_x, self.input.pointer_y);

                if is_selected || pointer_in {
                    let bg = if is_selected {
                        self.style.accent
                    } else {
                        self.style.widget_bg_hovered
                    };
                    self.fb.fill_rounded_rect_aa(item_rect, bg, 3);
                }

                fonts::draw_string_compact(
                    self.fb,
                    item_rect.x + 6,
                    item_rect.y + (item_h as i32 - 12) / 2,
                    item,
                    colors::WHITE,
                    1,
                );

                if pointer_in && self.input.pointer_primary_released {
                    *current_index = i;
                    changed = true;
                    let mut mem = UI_MEMORY.lock();
                    mem.set_bool(id, false); // close
                }
            }

            // Close on click outside
            if self.input.pointer_primary_pressed
                && !popup_rect.contains(self.input.pointer_x, self.input.pointer_y)
                && !combo_rect.contains(self.input.pointer_x, self.input.pointer_y)
            {
                let mut mem = UI_MEMORY.lock();
                mem.set_bool(id, false);
            }
        }

        let mut r = resp;
        r.changed = changed;
        r
    }
}
