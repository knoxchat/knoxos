use alloc::string::String;

use crate::gui::fonts;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::response::Response;

use super::{UI_MEMORY, Ui};

impl<'a> Ui<'a> {
    // ── Text input (single-line) ─────────────────────────────────

    /// Draw a single-line text edit field.
    pub fn text_edit_singleline(&mut self, text: &mut String) -> Response {
        let id = self.auto_id();
        let w = self.available_width().clamp(80, 300) as u32;
        let h = self.style.spacing.interact_height as u32;
        let rect = self.allocate_space(w, h);

        let resp = self.interact(rect, id, true, false);

        // Gain focus on click
        if resp.clicked() {
            let mut mem = UI_MEMORY.lock();
            mem.focused_id = Some(id);
        }

        let focused = {
            let mem = UI_MEMORY.lock();
            mem.focused_id == Some(id)
        };

        // Handle keyboard input when focused
        let mut changed = false;
        if focused {
            if let Some(ch) = self.input.char_typed {
                if (' '..='~').contains(&ch) {
                    text.push(ch);
                    changed = true;
                }
            }
            if self.input.backspace_pressed && !text.is_empty() {
                text.pop();
                changed = true;
            }
            if self.input.escape_pressed || self.input.enter_pressed {
                let mut mem = UI_MEMORY.lock();
                mem.focused_id = None;
            }
        }

        // Draw
        let bg = if focused {
            Pixel::rgb(35, 38, 45)
        } else {
            self.style.widget_bg
        };
        self.fb.fill_rounded_rect_aa(rect, bg, 4);
        self.fb.draw_rounded_rect(
            rect,
            if focused {
                self.style.border_focused
            } else {
                self.style.border_color
            },
            4,
            1,
        );

        // Text
        let display = if text.is_empty() && !focused {
            // Could show placeholder here
            ""
        } else {
            text.as_str()
        };
        // Truncate to visible width
        let max_chars = ((w as i32 - 12) / 8).max(0) as usize;
        let visible = if display.len() > max_chars {
            &display[display.len() - max_chars..]
        } else {
            display
        };
        fonts::draw_string_compact(
            self.fb,
            rect.x + 6,
            rect.y + (h as i32 - 12) / 2,
            visible,
            self.style.text_color,
            1,
        );

        // Cursor blink
        if focused {
            let cursor_x = rect.x + 6 + (visible.len() as i32 * 8);
            let phase = (self.input.frame_tick / 8) % 2;
            if phase == 0 {
                self.fb.fill_rect(
                    Rect::new(cursor_x, rect.y + 4, 2, h - 8),
                    self.style.text_color,
                );
            }
        }

        let mut r = resp;
        r.changed = changed;
        r.has_focus = focused;
        r
    }
}
