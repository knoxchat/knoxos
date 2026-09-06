// ─── Separator — Immediate-mode visual separator ─────────────────────
//
// Inspired by egui::Separator. A horizontal or vertical line that
// respects the current layout direction.

use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::ui::Ui;

/// A visual separator line.
///
/// ```ignore
/// ui.label("Section A");
/// Separator::new().show(ui);
/// ui.label("Section B");
/// ```
pub struct Separator {
    spacing: u32,
    color: Pixel,
    is_horizontal: Option<bool>,
}

impl Default for Separator {
    fn default() -> Self {
        Self {
            spacing: 8,
            color: colors::SURFACE_BORDER,
            is_horizontal: None,
        }
    }
}

impl Separator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Space before and after the line.
    pub fn spacing(mut self, s: u32) -> Self {
        self.spacing = s;
        self
    }

    pub fn color(mut self, c: Pixel) -> Self {
        self.color = c;
        self
    }

    /// Force horizontal.
    pub fn horizontal(mut self) -> Self {
        self.is_horizontal = Some(true);
        self
    }

    /// Force vertical.
    pub fn vertical(mut self) -> Self {
        self.is_horizontal = Some(false);
        self
    }

    pub fn show(self, ui: &mut Ui) -> crate::gui::response::Response {
        let is_horizontal = self.is_horizontal.unwrap_or_else(|| {
            // Auto-detect from layout direction
            ui.layout.main_dir.is_vertical()
        });

        let id = ui.id.with("separator");

        if is_horizontal {
            let w = ui.available_width().max(0) as u32;
            let rect = ui.allocate_space(w, self.spacing);
            let line_y = rect.y + self.spacing as i32 / 2;
            ui.fb.draw_hline(rect.x, line_y, w, self.color);
            ui.interact(rect, id, false, false)
        } else {
            let h = ui.available_height().max(0) as u32;
            let rect = ui.allocate_space(self.spacing, h);
            let line_x = rect.x + self.spacing as i32 / 2;
            ui.fb.draw_vline(line_x, rect.y, h, self.color);
            ui.interact(rect, id, false, false)
        }
    }
}
