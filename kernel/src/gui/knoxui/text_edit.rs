/// TextEdit — Enhanced multiline text editor with line numbers and syntax highlighting
///
/// ```ignore
/// TextEdit::multiline("code_editor")
///     .line_numbers(true)
///     .monospace(true)
///     .show(ui, &mut text);
/// ```
use alloc::string::String;
use alloc::vec::Vec;

use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::Response;
use crate::gui::ui::{UI_MEMORY, Ui};

pub struct TextEdit {
    id: Id,
    multiline: bool,
    line_numbers: bool,
    monospace: bool,
    desired_width: Option<u32>,
    desired_rows: u32,
    placeholder: String,
    read_only: bool,
}

impl TextEdit {
    pub fn singleline(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            multiline: false,
            line_numbers: false,
            monospace: false,
            desired_width: None,
            desired_rows: 1,
            placeholder: String::new(),
            read_only: false,
        }
    }

    pub fn multiline(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            multiline: true,
            line_numbers: false,
            monospace: true,
            desired_width: None,
            desired_rows: 8,
            placeholder: String::new(),
            read_only: false,
        }
    }

    pub fn line_numbers(mut self, v: bool) -> Self {
        self.line_numbers = v;
        self
    }
    pub fn monospace(mut self, v: bool) -> Self {
        self.monospace = v;
        self
    }
    pub fn desired_width(mut self, w: u32) -> Self {
        self.desired_width = Some(w);
        self
    }
    pub fn desired_rows(mut self, r: u32) -> Self {
        self.desired_rows = r;
        self
    }
    pub fn placeholder(mut self, p: &str) -> Self {
        self.placeholder = String::from(p);
        self
    }
    pub fn read_only(mut self, v: bool) -> Self {
        self.read_only = v;
        self
    }

    pub fn show<'a>(&self, ui: &mut Ui<'a>, text: &mut String) -> Response {
        let avail_w = ui.available_width().max(0) as u32;
        let w = self.desired_width.unwrap_or(avail_w.min(500));
        let line_h = 14u32;
        let row_count = if self.multiline { self.desired_rows } else { 1 };
        let h = row_count * line_h + 8;
        let gutter_w = if self.line_numbers { 40u32 } else { 0 };

        let rect = ui.allocate_space(w, h);
        let resp = ui.interact(rect, self.id, true, false);

        // Focus
        if resp.clicked() && !self.read_only {
            let mut mem = UI_MEMORY.lock();
            mem.focused_id = Some(self.id);
        }

        let focused = {
            let mem = UI_MEMORY.lock();
            mem.focused_id == Some(self.id)
        };

        // Background
        let bg = if focused {
            Pixel::rgb(28, 31, 40)
        } else {
            ui.style().widget_bg
        };
        ui.fb.fill_rounded_rect_aa(rect, bg, 4);
        ui.fb.draw_rounded_rect(
            rect,
            if focused {
                ui.style().border_focused
            } else {
                ui.style().border_color
            },
            4,
            1,
        );

        // Handle input
        let mut changed = false;
        if focused && !self.read_only {
            if let Some(ch) = ui.input.char_typed {
                if (' '..='~').contains(&ch) {
                    text.push(ch);
                    changed = true;
                }
            }
            if ui.input.enter_pressed && self.multiline {
                text.push('\n');
                changed = true;
            }
            if ui.input.backspace_pressed && !text.is_empty() {
                text.pop();
                changed = true;
            }
            if ui.input.escape_pressed {
                let mut mem = UI_MEMORY.lock();
                mem.focused_id = None;
            }
        }

        // Render text
        let content_x = rect.x + gutter_w as i32 + 4;
        let content_y = rect.y + 4;
        let content_w = (w as i32 - gutter_w as i32 - 8).max(0) as u32;

        ui.fb.push_clip(rect);

        if self.multiline {
            let lines: Vec<&str> = text.split('\n').collect();
            let max_visible = row_count as usize;

            // Scroll to keep cursor visible
            let cursor_id = self.id.with("cursor_line");
            let scroll_offset = {
                let mem = UI_MEMORY.lock();
                mem.get_i32(cursor_id, 0) as usize
            };
            let start_line = if lines.len() > max_visible {
                lines.len().saturating_sub(max_visible)
            } else {
                0
            };

            for (i, line) in lines.iter().enumerate().skip(start_line).take(max_visible) {
                let ly = content_y + (i - start_line) as i32 * line_h as i32;

                // Line numbers
                if self.line_numbers {
                    let num_str = alloc::format!("{:>3}", i + 1);
                    fonts::draw_string_compact(
                        ui.fb,
                        &num_str,
                        rect.x + 4,
                        ly,
                        Pixel::rgb(80, 85, 100),
                    );
                }

                // Line text
                let max_chars = (content_w / 8).max(1) as usize;
                let visible = if line.len() > max_chars {
                    &line[..max_chars]
                } else {
                    line
                };
                fonts::draw_string_compact(ui.fb, visible, content_x, ly, ui.style().text_color);
            }

            // Gutter separator
            if self.line_numbers {
                ui.fb
                    .draw_vline(rect.x + gutter_w as i32, rect.y, h, Pixel::rgb(40, 44, 55));
            }
        } else {
            // Single line
            let display = if text.is_empty() && !focused {
                &self.placeholder
            } else {
                text.as_str()
            };
            let text_color = if text.is_empty() && !focused {
                Pixel::rgb(100, 105, 115)
            } else {
                ui.style().text_color
            };
            let max_chars = (content_w / 8).max(1) as usize;
            let visible = if display.len() > max_chars {
                &display[display.len() - max_chars..]
            } else {
                display
            };
            fonts::draw_string_compact(ui.fb, visible, content_x, content_y, text_color);
        }

        // Cursor
        if focused {
            let cursor_x = if self.multiline {
                let last_line = text.split('\n').next_back().unwrap_or("");
                content_x + (last_line.len() as i32 * 8)
            } else {
                content_x + (text.len() as i32 * 8)
            };
            let cursor_y = if self.multiline {
                let line_count = text.matches('\n').count();
                let visible_line = line_count.min(row_count as usize - 1);
                content_y + visible_line as i32 * line_h as i32
            } else {
                content_y
            };

            let phase = (ui.input.frame_tick / 8) % 2;
            if phase == 0 {
                ui.fb.fill_rect(
                    Rect::new(cursor_x, cursor_y, 2, line_h - 2),
                    ui.style().text_color,
                );
            }
        }

        ui.fb.pop_clip();

        let mut r = resp;
        r.changed = changed;
        r.has_focus = focused;
        r
    }
}
