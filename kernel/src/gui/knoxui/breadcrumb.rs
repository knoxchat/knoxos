use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::Response;
use crate::gui::ui::Ui;
/// Breadcrumb — Breadcrumb navigation trail.
///
/// ```ignore
/// let items = vec!["Home", "Documents", "Projects"];
/// if let Some(clicked) = Breadcrumb::new(&items).show(ui) {
///     // Navigate to items[clicked]
/// }
/// ```
use alloc::string::String;
use alloc::vec::Vec;

pub struct Breadcrumb<'a> {
    items: &'a [&'a str],
    separator: &'a str,
    active_color: Pixel,
    inactive_color: Pixel,
}

impl<'a> Breadcrumb<'a> {
    pub fn new(items: &'a [&'a str]) -> Self {
        Self {
            items,
            separator: ">",
            active_color: colors::ACCENT_PRIMARY,
            inactive_color: colors::TEXT_MUTED,
        }
    }

    pub fn separator(mut self, s: &'a str) -> Self {
        self.separator = s;
        self
    }
    pub fn active_color(mut self, c: Pixel) -> Self {
        self.active_color = c;
        self
    }

    /// Show breadcrumb. Returns `Some(index)` if an item was clicked.
    pub fn show(self, ui: &mut Ui) -> Option<usize> {
        let item_height = 16u32;

        // Calculate total width
        let sep_w = (self.separator.len() as u32 + 2) * 8; // space + sep + space
        let mut total_w = 0u32;
        for (i, item) in self.items.iter().enumerate() {
            total_w += item.len() as u32 * 8;
            if i + 1 < self.items.len() {
                total_w += sep_w;
            }
        }

        let row_rect = ui.allocate_space(total_w, item_height);
        let id_base = ui.id.with("breadcrumb");

        let mut x = row_rect.x;
        let y = row_rect.y + 3;
        let mut clicked_idx: Option<usize> = None;

        for (i, item) in self.items.iter().enumerate() {
            let is_last = i + 1 >= self.items.len();
            let item_w = item.len() as u32 * 8;
            let item_rect = Rect::new(x, row_rect.y, item_w, item_height);
            let item_id = id_base.with_index(i);

            let resp = ui.interact(item_rect, item_id, true, false);

            let color = if is_last {
                colors::TEXT_PRIMARY
            } else if resp.hovered {
                self.active_color
            } else {
                self.inactive_color
            };

            // Underline on hover
            if resp.hovered && !is_last {
                ui.fb
                    .draw_line_aa(x, y + 11, x + item_w as i32, y + 11, color);
            }

            fonts::draw_string_compact(ui.fb, item, x, y, color);
            x += item_w as i32;

            if resp.clicked && !is_last {
                clicked_idx = Some(i);
            }

            // Separator
            if !is_last {
                let sx = x + 8;
                fonts::draw_string_compact(ui.fb, self.separator, sx, y, colors::TEXT_MUTED);
                x += sep_w as i32;
            }
        }

        clicked_idx
    }
}
