/// Sides — Left/right aligned content on the same row (like egui::Sides)
///
/// ```ignore
/// Sides::new("header").show(ui,
///     |ui| { ui.heading("Title"); },
///     |ui| { ui.small_button("X"); },
/// );
/// ```
use crate::gui::framebuffer::Rect;
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::{InnerResponse, Response};
use crate::gui::ui::Ui;

pub struct Sides {
    id: Id,
    height: u32,
}

impl Sides {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            height: 24,
        }
    }

    pub fn height(mut self, h: u32) -> Self {
        self.height = h;
        self
    }

    /// Show left-aligned and right-aligned content on the same row.
    pub fn show<'a, L, R>(
        self,
        ui: &mut Ui<'a>,
        left: impl FnOnce(&mut Ui) -> L,
        right: impl FnOnce(&mut Ui) -> R,
    ) -> (L, R) {
        let avail_w = ui.available_width().max(0) as u32;
        let row_rect = ui.allocate_space(avail_w, self.height);

        let saved = (ui.region, ui.layout, ui.id);

        // Left side
        let left_rect = Rect::new(row_rect.x, row_rect.y, avail_w / 2, self.height);
        ui.region = Region::from_max_rect(&Layout::left_to_right(Align::Center), left_rect);
        ui.layout = Layout::left_to_right(Align::Center);
        ui.id = self.id.with("left");
        let l = left(ui);

        // Right side
        let right_rect = Rect::new(
            row_rect.x + avail_w as i32 / 2,
            row_rect.y,
            avail_w / 2,
            self.height,
        );
        ui.region = Region::from_max_rect(&Layout::right_to_left(Align::Center), right_rect);
        ui.layout = Layout::right_to_left(Align::Center);
        ui.id = self.id.with("right");
        let r = right(ui);

        ui.region = saved.0;
        ui.layout = saved.1;
        ui.id = saved.2;

        (l, r)
    }
}
