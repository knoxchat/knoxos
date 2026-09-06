use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::Response;
use crate::gui::ui::Ui;
/// Chip — Pill-shaped tag component, optionally closable.
///
/// ```ignore
/// let mut tags = vec!["Rust", "OS", "GUI"];
/// Chip::new("Rust").closable(true).show(ui);
/// ```
use alloc::string::String;

pub struct Chip {
    text: String,
    closable: bool,
    selected: bool,
    bg: Pixel,
    text_color: Pixel,
}

impl Chip {
    pub fn new(text: &str) -> Self {
        Self {
            text: String::from(text),
            closable: false,
            selected: false,
            bg: colors::SURFACE_RAISED,
            text_color: colors::TEXT_SECONDARY,
        }
    }

    pub fn closable(mut self, c: bool) -> Self {
        self.closable = c;
        self
    }
    pub fn selected(mut self, s: bool) -> Self {
        self.selected = s;
        self
    }
    pub fn bg(mut self, bg: Pixel) -> Self {
        self.bg = bg;
        self
    }
    pub fn text_color(mut self, c: Pixel) -> Self {
        self.text_color = c;
        self
    }

    /// Show the chip. Returns a `ChipResponse` indicating click and close actions.
    pub fn show(self, ui: &mut Ui) -> ChipResponse {
        let text_w = self.text.len() as u32 * 8;
        let close_w = if self.closable { 16u32 } else { 0 };
        let pad = 8u32;
        let h = 22u32;
        let w = text_w + close_w + pad * 2;

        let rect = ui.allocate_space(w, h);
        let id = ui.id.with("chip");

        let bg = if self.selected {
            colors::ACCENT_PRIMARY
        } else {
            self.bg
        };
        let text_col = if self.selected {
            Pixel::new(255, 255, 255, 240)
        } else {
            self.text_color
        };

        // Pill shape
        ui.fb.fill_rounded_rect_aa(rect, bg, h / 2);

        // Border
        if !self.selected {
            ui.fb
                .draw_rounded_rect(rect, colors::SURFACE_BORDER, h / 2, 1);
        }

        // Text
        fonts::draw_string_compact(ui.fb, &self.text, rect.x + pad as i32, rect.y + 6, text_col);

        let mut close_clicked = false;

        // Close button
        if self.closable {
            let cx = rect.x + w as i32 - pad as i32 - 8;
            let cy = rect.y + 6;
            let close_rect = Rect::new(cx - 2, cy - 2, 12, 14);
            let close_id = id.with("close");
            let close_resp = ui.interact(close_rect, close_id, true, false);

            let xc = if close_resp.hovered {
                colors::ERROR
            } else {
                colors::TEXT_MUTED
            };
            fonts::draw_string_compact(ui.fb, "x", cx, cy, xc);

            close_clicked = close_resp.clicked;
        }

        let resp = ui.interact(rect, id, true, false);

        ChipResponse {
            clicked: resp.clicked,
            close_clicked,
            hovered: resp.hovered,
        }
    }
}

pub struct ChipResponse {
    pub clicked: bool,
    pub close_clicked: bool,
    pub hovered: bool,
}
