use crate::gui::fonts;
use crate::gui::framebuffer::Pixel;
use crate::gui::response::Response;

use super::Ui;

impl<'a> Ui<'a> {
    // ── Text widgets ─────────────────────────────────────────────

    /// Draw a label (non-interactive text).
    pub fn label(&mut self, text: &str) -> Response {
        let id = self.auto_id();
        let text_w = (text.len() as u32) * 8; // compact font: 8px wide
        let text_h = 12u32;
        let rect = self.allocate_space(text_w, text_h);
        fonts::draw_string_compact(self.fb, rect.x, rect.y, text, self.style.text_color, 1);
        self.interact(rect, id, false, false)
    }

    /// Draw colored label.
    pub fn colored_label(&mut self, text: &str, color: Pixel) -> Response {
        let id = self.auto_id();
        let text_w = (text.len() as u32) * 8;
        let text_h = 12u32;
        let rect = self.allocate_space(text_w, text_h);
        fonts::draw_string_compact(self.fb, rect.x, rect.y, text, color, 1);
        self.interact(rect, id, false, false)
    }

    /// Draw dimmed text.
    pub fn dimmed_label(&mut self, text: &str) -> Response {
        self.colored_label(text, self.style.text_dimmed)
    }

    /// Draw a heading (larger, bold text).
    pub fn heading(&mut self, text: &str) -> Response {
        let id = self.auto_id();
        let text_w = (text.len() as u32) * 8; // same width, just bold + spacing
        let text_h = 16u32;
        let rect = self.allocate_space(text_w, text_h + 4);
        if self.style.heading_bold {
            fonts::draw_string_bold(self.fb, rect.x, rect.y + 2, text, self.style.text_color, 1);
        } else {
            fonts::draw_string_compact(self.fb, rect.x, rect.y + 2, text, self.style.text_color, 1);
        }
        self.interact(rect, id, false, false)
    }

    /// Draw a small label (dimmed).
    pub fn small(&mut self, text: &str) -> Response {
        self.colored_label(text, self.style.text_dimmed)
    }

    /// Draw a monospace code label.
    pub fn code(&mut self, text: &str) -> Response {
        let id = self.auto_id();
        let text_w = (text.len() as u32) * 8 + 8;
        let text_h = 16u32;
        let rect = self.allocate_space(text_w, text_h);
        self.fb
            .fill_rounded_rect_aa(rect, Pixel::rgb(35, 38, 45), 3);
        fonts::draw_string_compact(
            self.fb,
            rect.x + 4,
            rect.y + 2,
            text,
            Pixel::rgb(220, 180, 120),
            1,
        );
        self.interact(rect, id, false, false)
    }

    // ── Utility: formatted text ──────────────────────────────────

    /// Draw a label with a formatted string (convenience for `alloc::format!`).
    pub fn label_fmt(&mut self, args: core::fmt::Arguments) -> Response {
        let s = alloc::format!("{}", args);
        self.label(&s)
    }
}
