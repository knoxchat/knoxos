use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::Response;
use crate::gui::ui::Ui;
/// PropertyGrid — Key-value property editor, like an inspector panel.
///
/// ```ignore
/// PropertyGrid::new("inspector")
///     .row("Name", |ui| { ui.text_edit_singleline(&mut name); })
///     .row("Width", |ui| { ui.slider_i32("w", &mut w, 1, 1920); })
///     .show(ui);
/// ```
use alloc::string::String;
use alloc::vec::Vec;

type RowFn<'a> = &'a dyn Fn(&mut Ui);

pub struct PropertyGrid {
    id: Id,
    label_width: u32,
    row_height: u32,
    group_label: Option<String>,
}

impl PropertyGrid {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            label_width: 120,
            row_height: 26,
            group_label: None,
        }
    }

    pub fn label_width(mut self, w: u32) -> Self {
        self.label_width = w;
        self
    }
    pub fn row_height(mut self, h: u32) -> Self {
        self.row_height = h;
        self
    }

    pub fn group_label(mut self, label: &str) -> Self {
        self.group_label = Some(String::from(label));
        self
    }

    /// Show a single property row with label and value widget.
    pub fn row(&self, ui: &mut Ui, label: &str, index: usize, value_ui: impl FnOnce(&mut Ui)) {
        let avail_w = ui.available_width().max(0) as u32;
        let row_rect = ui.allocate_space(avail_w, self.row_height);
        let row_id = self.id.with_index(index);

        // Alternating row background
        if index % 2 == 0 {
            ui.fb.fill_rect(row_rect, Pixel::new(255, 255, 255, 4));
        }

        // Separator line at bottom
        ui.fb.draw_line_aa(
            row_rect.x,
            row_rect.y + self.row_height as i32 - 1,
            row_rect.x + avail_w as i32,
            row_rect.y + self.row_height as i32 - 1,
            Pixel::new(255, 255, 255, 15),
        );

        // Label (left side)
        fonts::draw_string_compact(
            ui.fb,
            label,
            row_rect.x + 8,
            row_rect.y + (self.row_height as i32 - 10) / 2,
            colors::TEXT_SECONDARY,
        );

        // Value widget (right side)
        let val_x = row_rect.x + self.label_width as i32;
        let val_w = avail_w.saturating_sub(self.label_width + 8);
        let val_rect = Rect::new(val_x, row_rect.y + 2, val_w, self.row_height - 4);

        let saved = (ui.region, ui.layout, ui.id);
        ui.region = Region::from_max_rect(&Layout::left_to_right(Align::Center), val_rect);
        ui.layout = Layout::left_to_right(Align::Center);
        ui.id = row_id;

        value_ui(ui);

        ui.region = saved.0;
        ui.layout = saved.1;
        ui.id = saved.2;
    }

    /// Show a group header label.
    pub fn section_header(&self, ui: &mut Ui, label: &str) {
        let avail_w = ui.available_width().max(0) as u32;
        let h = 24u32;
        let rect = ui.allocate_space(avail_w, h);

        ui.fb.fill_rect(rect, Pixel::new(255, 255, 255, 8));
        fonts::draw_string_bold_compact(ui.fb, label, rect.x + 8, rect.y + 7, colors::TEXT_PRIMARY);
    }
}
