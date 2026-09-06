/// StatusBar — A bottom status bar for displaying application state info.
///
/// ```ignore
/// StatusBar::new()
///     .left(|ui| { ui.label("Ready"); })
///     .right(|ui| { ui.label("Line 42, Col 8"); })
///     .show(ui);
/// ```
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::Response;
use crate::gui::ui::Ui;

pub struct StatusBar {
    id: Id,
    height: u32,
    bg: Pixel,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            id: Id::from_str("status_bar"),
            height: 22,
            bg: colors::SURFACE_SUNKEN,
        }
    }

    pub fn height(mut self, h: u32) -> Self {
        self.height = h;
        self
    }
    pub fn bg(mut self, bg: Pixel) -> Self {
        self.bg = bg;
        self
    }

    /// Show a status bar anchored to the bottom of the current region.
    /// Provides left-aligned and right-aligned sections.
    pub fn show<'a>(
        self,
        ui: &mut Ui<'a>,
        left_content: impl FnOnce(&mut Ui),
        right_content: impl FnOnce(&mut Ui),
    ) {
        let avail_w = ui.available_width().max(0) as u32;
        // Place at bottom of current region
        let bar_y = ui.region.max_rect.y + ui.region.max_rect.height as i32 - self.height as i32;
        let bar_rect = Rect::new(ui.region.max_rect.x, bar_y, avail_w, self.height);

        // Background
        ui.fb.fill_rect(bar_rect, self.bg);
        // Top border line
        ui.fb.draw_line_aa(
            bar_rect.x,
            bar_rect.y,
            bar_rect.x + avail_w as i32,
            bar_rect.y,
            colors::SURFACE_BORDER,
        );

        let saved = (ui.region, ui.layout, ui.id);

        // Left content
        let left_rect = Rect::new(
            bar_rect.x + 8,
            bar_rect.y + 2,
            avail_w / 2 - 8,
            self.height - 4,
        );
        ui.region = Region::from_max_rect(&Layout::left_to_right(Align::Center), left_rect);
        ui.layout = Layout::left_to_right(Align::Center);
        ui.id = self.id.with("left");
        left_content(ui);

        // Right content
        let right_rect = Rect::new(
            bar_rect.x + avail_w as i32 / 2,
            bar_rect.y + 2,
            avail_w / 2 - 8,
            self.height - 4,
        );
        ui.region = Region::from_max_rect(&Layout::right_to_left(Align::Center), right_rect);
        ui.layout = Layout::right_to_left(Align::Center);
        ui.id = self.id.with("right");
        right_content(ui);

        ui.region = saved.0;
        ui.layout = saved.1;
        ui.id = saved.2;

        // Shrink available region to exclude the status bar
        ui.region.max_rect.height = ui.region.max_rect.height.saturating_sub(self.height);
    }
}
