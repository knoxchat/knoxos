// ─── SelectableLabel — Toggle-style flat label ──────────────────────
//
// Inspired by egui::SelectableLabel. A label that acts as a toggle
// button — highlighted when selected, flat when not.

use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::ui::Ui;

use super::text_helpers as fonts;

/// A label that can be selected/deselected, useful for lists and toggles.
///
/// ```ignore
/// if SelectableLabel::new("Option A", selected == 0).show(ui).clicked() {
///     selected = 0;
/// }
/// ```
pub struct SelectableLabel<'a> {
    text: &'a str,
    selected: bool,
    icon: Option<&'a str>,
}

impl<'a> SelectableLabel<'a> {
    pub fn new(text: &'a str, selected: bool) -> Self {
        Self {
            text,
            selected,
            icon: None,
        }
    }

    pub fn icon(mut self, icon: &'a str) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn show(self, ui: &mut Ui) -> crate::gui::response::Response {
        let id = ui.id.with(self.text);
        let icon_w: u32 = if self.icon.is_some() { 16 } else { 0 };
        let text_w = self.text.len() as u32 * 8;
        let w = icon_w + text_w + 16; // padding
        let h = 26u32;
        let rect = ui.allocate_space(w, h);
        let resp = ui.interact(rect, id, true, false);

        // Background
        let bg = if self.selected {
            Pixel::new(
                ui.style().accent.r,
                ui.style().accent.g,
                ui.style().accent.b,
                30,
            )
        } else if resp.hovered {
            Pixel::new(255, 255, 255, 10)
        } else {
            Pixel::new(0, 0, 0, 0)
        };

        if bg.a > 0 {
            ui.fb.fill_rounded_rect_aa(rect, bg, 4);
        }

        // Selected indicator on left edge
        if self.selected {
            ui.fb.fill_rounded_rect_aa(
                Rect::new(rect.x, rect.y + 4, 3, h - 8),
                ui.style().accent,
                2,
            );
        }

        // Icon
        let mut tx = rect.x + 8;
        if let Some(icon) = self.icon {
            fonts::draw_string_compact(
                ui.fb,
                icon,
                tx,
                rect.y + (h as i32 - 10) / 2,
                if self.selected {
                    ui.style().accent
                } else {
                    ui.style().text_dimmed
                },
            );
            tx += icon_w as i32;
        }

        // Text
        let text_color = if self.selected || resp.hovered {
            ui.style().text_color
        } else {
            ui.style().text_dimmed
        };
        fonts::draw_string_compact(
            ui.fb,
            self.text,
            tx,
            rect.y + (h as i32 - 10) / 2,
            text_color,
        );

        resp
    }
}
