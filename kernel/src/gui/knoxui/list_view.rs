use super::text_helpers as fonts;
/// ListView — Virtualized list for efficiently displaying large datasets.
///
/// ```ignore
/// ListView::new("file_list", items.len(), 24)
///     .show(ui, |ui, index| {
///         ui.label(items[index]);
///     });
/// ```
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::Response;
use crate::gui::ui::{UI_MEMORY, Ui};

pub struct ListView {
    id: Id,
    item_count: usize,
    item_height: u32,
    width: Option<u32>,
    max_height: Option<u32>,
    striped: bool,
    selected_index: Option<usize>,
}

impl ListView {
    pub fn new(id_salt: &str, item_count: usize, item_height: u32) -> Self {
        Self {
            id: Id::from_str(id_salt),
            item_count,
            item_height,
            width: None,
            max_height: None,
            striped: true,
            selected_index: None,
        }
    }

    pub fn width(mut self, w: u32) -> Self {
        self.width = Some(w);
        self
    }
    pub fn max_height(mut self, h: u32) -> Self {
        self.max_height = Some(h);
        self
    }
    pub fn striped(mut self, s: bool) -> Self {
        self.striped = s;
        self
    }
    pub fn selected(mut self, idx: Option<usize>) -> Self {
        self.selected_index = idx;
        self
    }

    /// Show the list. `row_ui` is called for each visible item with (ui, index).
    /// Returns the index that was clicked, if any.
    pub fn show<'a>(self, ui: &mut Ui<'a>, row_ui: impl Fn(&mut Ui, usize)) -> Option<usize> {
        let id = self.id;
        let avail_w = self.width.unwrap_or(ui.available_width().max(0) as u32);
        let total_content_h = self.item_count as u32 * self.item_height;
        let max_h = self
            .max_height
            .unwrap_or(ui.available_height().max(0) as u32);
        let visible_h = max_h.min(total_content_h);

        let outer = ui.allocate_space(avail_w, visible_h);

        // Scroll offset
        let mem = UI_MEMORY.lock();
        let scroll_y = mem.get_i32(id.with("scroll"), 0);
        drop(mem);

        // Clamp scroll
        let max_scroll = (total_content_h as i32 - visible_h as i32).max(0);
        let scroll_y = scroll_y.clamp(0, max_scroll);

        // Handle scroll input
        let outer_resp = ui.interact(outer, id, true, false);
        if outer_resp.hovered {
            let new_scroll = (scroll_y - ui.input.scroll_delta * 20).clamp(0, max_scroll);
            let mut mem = UI_MEMORY.lock();
            mem.set_i32(id.with("scroll"), new_scroll);
        }

        // Determine visible range
        let first_visible = (scroll_y as u32 / self.item_height) as usize;
        let visible_count = (visible_h / self.item_height + 2) as usize;
        let last_visible = (first_visible + visible_count).min(self.item_count);

        ui.fb.push_clip(outer);

        let saved = (ui.region, ui.layout, ui.id);
        let mut clicked_index: Option<usize> = None;

        for i in first_visible..last_visible {
            let row_y = outer.y + (i as u32 * self.item_height) as i32 - scroll_y;
            let row_rect = Rect::new(outer.x, row_y, avail_w, self.item_height);

            // Skip if fully outside
            if row_y + self.item_height as i32 <= outer.y || row_y >= outer.y + visible_h as i32 {
                continue;
            }

            // Striped background
            if self.striped && i % 2 == 1 {
                ui.fb.fill_rect(row_rect, Pixel::new(255, 255, 255, 5));
            }

            // Selection highlight
            if Some(i) == self.selected_index {
                ui.fb.fill_rect(
                    row_rect,
                    Pixel::new(
                        colors::ACCENT_PRIMARY.r,
                        colors::ACCENT_PRIMARY.g,
                        colors::ACCENT_PRIMARY.b,
                        40,
                    ),
                );
            }

            // Row interaction
            let row_id = id.with_index(i);
            let row_resp = ui.interact(row_rect, row_id, true, false);

            if row_resp.hovered {
                ui.fb.fill_rect(row_rect, Pixel::new(255, 255, 255, 10));
            }
            if row_resp.clicked {
                clicked_index = Some(i);
            }

            // Render row content
            let content_rect = Rect::new(
                row_rect.x + 4,
                row_rect.y + 2,
                avail_w.saturating_sub(8),
                self.item_height.saturating_sub(4),
            );
            ui.region = Region::from_max_rect(&Layout::left_to_right(Align::Center), content_rect);
            ui.layout = Layout::left_to_right(Align::Center);
            ui.id = row_id;

            row_ui(ui, i);
        }

        ui.region = saved.0;
        ui.layout = saved.1;
        ui.id = saved.2;
        ui.fb.pop_clip();

        // Draw scrollbar if needed
        if total_content_h > visible_h {
            let sb_w = 6u32;
            let sb_x = outer.x + avail_w as i32 - sb_w as i32 - 2;
            let thumb_h =
                ((visible_h as u64 * visible_h as u64) / total_content_h as u64).max(20) as u32;
            let thumb_y = outer.y
                + ((scroll_y as u64 * (visible_h - thumb_h) as u64) / max_scroll.max(1) as u64)
                    as i32;
            let thumb_rect = Rect::new(sb_x, thumb_y, sb_w, thumb_h);
            ui.fb
                .fill_rounded_rect_aa(thumb_rect, Pixel::new(255, 255, 255, 50), sb_w / 2);
        }

        clicked_index
    }
}
