// ─── ImageButton — Clickable image widget ────────────────────────────
//
// Inspired by egui::ImageButton. A button that displays an image/icon
// and optionally a text label. Suitable for toolbars and icon grids.

use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::ui::Ui;

use super::text_helpers as fonts;

/// A clickable image/icon button.
///
/// Since we're in a bare-metal OS without file-based images, this renders
/// a colored icon placeholder or text glyph as the "image".
///
/// ```ignore
/// if ImageButton::new("save_btn", "💾")
///     .size(32)
///     .tooltip("Save file")
///     .show(ui).clicked() {
///     // save
/// }
/// ```
pub struct ImageButton<'a> {
    id_str: &'a str,
    icon: &'a str,
    size: u32,
    tint: Pixel,
    frame: bool,
    selected: bool,
    label: Option<&'a str>,
}

impl<'a> ImageButton<'a> {
    pub fn new(id_str: &'a str, icon: &'a str) -> Self {
        Self {
            id_str,
            icon,
            size: 28,
            tint: colors::WHITE,
            frame: true,
            selected: false,
            label: None,
        }
    }

    pub fn size(mut self, s: u32) -> Self {
        self.size = s;
        self
    }

    pub fn tint(mut self, c: Pixel) -> Self {
        self.tint = c;
        self
    }

    /// Whether to draw a frame/background.
    pub fn frame(mut self, f: bool) -> Self {
        self.frame = f;
        self
    }

    pub fn selected(mut self, s: bool) -> Self {
        self.selected = s;
        self
    }

    /// Optional text label below the icon.
    pub fn label(mut self, l: &'a str) -> Self {
        self.label = Some(l);
        self
    }

    pub fn show(self, ui: &mut Ui) -> crate::gui::response::Response {
        let id = ui.id.with(self.id_str);
        let label_h: u32 = if self.label.is_some() { 14 } else { 0 };
        let total_h = self.size + label_h;
        let rect = ui.allocate_space(self.size, total_h);
        let resp = ui.interact(rect, id, true, false);

        let icon_rect = Rect::new(rect.x, rect.y, self.size, self.size);

        // Background
        if self.frame {
            let bg = if self.selected {
                Pixel::new(
                    ui.style().accent.r,
                    ui.style().accent.g,
                    ui.style().accent.b,
                    40,
                )
            } else if resp.hovered {
                Pixel::new(255, 255, 255, 15)
            } else {
                Pixel::new(0, 0, 0, 0)
            };
            if bg.a > 0 {
                ui.fb.fill_rounded_rect_aa(icon_rect, bg, 6);
            }

            if resp.hovered || self.selected {
                let border = if self.selected {
                    ui.style().accent
                } else {
                    Pixel::new(255, 255, 255, 20)
                };
                ui.fb.draw_rounded_rect(icon_rect, border, 6, 1);
            }
        }

        // Icon (centered in the icon area)
        let icon_color = if self.selected {
            ui.style().accent
        } else {
            self.tint
        };
        fonts::draw_string_centered_compact(
            ui.fb,
            self.icon,
            rect.x + self.size as i32 / 2,
            rect.y + (self.size as i32 - 10) / 2,
            icon_color,
        );

        // Label below icon
        if let Some(lbl) = self.label {
            let lbl_w = lbl.len() as u32 * 8;
            let lbl_x = rect.x + (self.size as i32 - lbl_w as i32) / 2;
            fonts::draw_string_compact(
                ui.fb,
                lbl,
                lbl_x,
                rect.y + self.size as i32 + 2,
                ui.style().text_dimmed,
            );
        }

        resp
    }
}
