// ─── Link — Clickable hyperlink-styled text ──────────────────────────
//
// Inspired by egui::Link / egui::Hyperlink. Renders text that looks
// like a clickable link with underline-on-hover and accent coloring.

use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::ui::Ui;

use super::text_helpers as fonts;

/// Clickable text styled as a hyperlink.
///
/// ```ignore
/// if Link::new("Documentation").show(ui).clicked() {
///     // handle link click
/// }
/// ```
pub struct Link<'a> {
    text: &'a str,
    color: Option<Pixel>,
    underline: bool,
}

impl<'a> Link<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            color: None,
            underline: true,
        }
    }

    pub fn color(mut self, c: Pixel) -> Self {
        self.color = Some(c);
        self
    }

    /// Whether to show underline on hover (default: true).
    pub fn underline(mut self, u: bool) -> Self {
        self.underline = u;
        self
    }

    pub fn show(self, ui: &mut Ui) -> crate::gui::response::Response {
        let id = ui.id.with(self.text);
        let w = self.text.len() as u32 * 8;
        let h = 14u32;
        let rect = ui.allocate_space(w, h);
        let resp = ui.interact(rect, id, true, false);

        let link_color = self.color.unwrap_or(colors::ACCENT_PRIMARY);
        let text_color = if resp.hovered {
            colors::lighten(link_color, 40)
        } else {
            link_color
        };

        fonts::draw_string_compact(ui.fb, self.text, rect.x, rect.y + 2, text_color);

        // Underline on hover
        if self.underline && resp.hovered {
            let underline_y = rect.y + h as i32 - 1;
            ui.fb.draw_hline(rect.x, underline_y, w, text_color);
        }

        resp
    }
}
