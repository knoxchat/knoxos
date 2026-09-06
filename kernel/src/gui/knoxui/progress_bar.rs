// ─── ProgressBar — Immediate-mode progress bar widget ────────────────
//
// Inspired by egui::ProgressBar. Builder-pattern progress bar that
// integrates with the Ui layout system.

use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::ui::Ui;

use super::text_helpers as fonts;

/// An immediate-mode progress bar.
///
/// ```ignore
/// ProgressBar::new(75)
///     .text("Loading…")
///     .color(colors::ACCENT_PRIMARY)
///     .show(ui);
/// ```
pub struct ProgressBar<'a> {
    /// Progress 0–100.
    progress: u8,
    width: Option<u32>,
    height: u32,
    color: Pixel,
    bg: Pixel,
    text: Option<&'a str>,
    show_percentage: bool,
    corner_radius: u32,
}

impl<'a> ProgressBar<'a> {
    /// Create with progress in 0–100 range.
    pub fn new(progress: u8) -> Self {
        Self {
            progress: progress.min(100),
            width: None,
            height: 20,
            color: colors::ACCENT_PRIMARY,
            bg: Pixel::rgb(40, 44, 52),
            text: None,
            show_percentage: false,
            corner_radius: 4,
        }
    }

    pub fn width(mut self, w: u32) -> Self {
        self.width = Some(w);
        self
    }

    pub fn height(mut self, h: u32) -> Self {
        self.height = h;
        self
    }

    pub fn color(mut self, c: Pixel) -> Self {
        self.color = c;
        self
    }

    pub fn text(mut self, t: &'a str) -> Self {
        self.text = Some(t);
        self
    }

    pub fn show_percentage(mut self) -> Self {
        self.show_percentage = true;
        self
    }

    pub fn corner_radius(mut self, r: u32) -> Self {
        self.corner_radius = r;
        self
    }

    pub fn show(self, ui: &mut Ui) -> crate::gui::response::Response {
        let w = self.width.unwrap_or(ui.available_width().max(40) as u32);
        let rect = ui.allocate_space(w, self.height);
        let id = ui.id.with("progress_bar");
        let resp = ui.interact(rect, id, false, false);

        // Background track
        ui.fb
            .fill_rounded_rect_aa(rect, self.bg, self.corner_radius);

        // Fill
        let fill_w = (w * self.progress as u32 / 100).max(if self.progress > 0 { 2 } else { 0 });
        if fill_w > 0 {
            let fill_rect = Rect::new(rect.x, rect.y, fill_w, self.height);
            ui.fb
                .fill_rounded_rect_aa(fill_rect, self.color, self.corner_radius);
        }

        // Text overlay
        let display_text = if let Some(t) = self.text {
            Some(alloc::format!("{}", t))
        } else if self.show_percentage {
            Some(alloc::format!("{}%", self.progress))
        } else {
            None
        };

        if let Some(ref txt) = display_text {
            if self.height >= 12 {
                fonts::draw_string_centered_compact(
                    ui.fb,
                    txt,
                    rect.x + w as i32 / 2,
                    rect.y + (self.height as i32 - 10) / 2,
                    colors::WHITE,
                );
            }
        }

        resp
    }
}
