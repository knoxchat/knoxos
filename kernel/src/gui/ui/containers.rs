use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::{InnerResponse, Response};

use super::{UI_MEMORY, Ui};

impl<'a> Ui<'a> {
    // ── Spacing & separators ─────────────────────────────────────

    /// Add empty vertical or horizontal space.
    pub fn add_space(&mut self, amount: i32) {
        self.layout.add_space(&mut self.region, amount);
    }

    /// Draw a horizontal separator line.
    pub fn separator(&mut self) -> Response {
        let id = self.auto_id();
        let w = self.available_width().max(0) as u32;
        let rect = self.allocate_space(w, 1);
        self.fb
            .draw_hline(rect.x, rect.y, rect.width, self.style.separator_color);
        self.add_space(self.style.spacing.item_spacing_y);
        Response::none(id, rect)
    }

    // ── Layout helpers ───────────────────────────────────────────

    /// Run a closure with a horizontal (left-to-right) layout.
    /// Returns the `InnerResponse` containing the closure's result and the container's response.
    pub fn horizontal<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        self.with_layout(Layout::left_to_right(Align::Center), add_contents)
    }

    /// Run a closure with a vertical (top-down) layout.
    pub fn vertical<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        self.with_layout(Layout::top_down(Align::LEFT), add_contents)
    }

    /// Run a closure with a specific layout, in a child region.
    pub fn with_layout<R>(
        &mut self,
        layout: Layout,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let child_id = self.auto_id();
        let avail_w = self.available_width().max(0) as u32;
        let avail_h = self.available_height().max(0) as u32;
        let child_rect = Rect::new(self.region.cursor_x, self.region.cursor_y, avail_w, avail_h);

        // Create the child Ui by temporarily borrowing our framebuffer
        // We need to use unsafe to split the borrow, since the child needs &mut fb
        // but we also need to update our region afterwards.
        let saved_region = self.region;
        let saved_layout = self.layout;

        self.layout = layout;
        let old_cursor_x = self.region.cursor_x;
        let old_cursor_y = self.region.cursor_y;

        // Reset cursor within the child region
        let child_region = Region::from_max_rect(&layout, child_rect);
        self.region = child_region;
        let old_id = self.id;
        self.id = child_id;

        let inner = add_contents(self);

        let child_min_rect = self.region.min_rect;

        // Restore parent layout
        self.layout = saved_layout;
        self.region = saved_region;
        self.id = old_id;

        // Allocate the space used by the child in the parent layout
        let used_w = if child_min_rect.width > 0 {
            child_min_rect.width
        } else {
            0
        };
        let used_h = if child_min_rect.height > 0 {
            child_min_rect.height
        } else {
            0
        };
        let alloc_rect = self.allocate_space(used_w, used_h);

        let resp = Response::none(child_id, alloc_rect);
        InnerResponse {
            inner,
            response: resp,
        }
    }

    /// Run a closure with additional left indent.
    pub fn indent<R>(
        &mut self,
        id_salt: &str,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let indent = self.style.spacing.indent;
        self.region.cursor_x += indent;
        self.region.max_rect.x += indent;
        self.region.max_rect.width = (self.region.max_rect.width as i32 - indent).max(0) as u32;

        let child_id = self.id_from(id_salt);
        let old_id = self.id;
        self.id = child_id;

        let inner = add_contents(self);

        self.id = old_id;
        self.region.cursor_x -= indent;
        self.region.max_rect.x -= indent;
        self.region.max_rect.width = (self.region.max_rect.width as i32 + indent) as u32;

        let resp = Response::none(child_id, self.region.min_rect);
        InnerResponse {
            inner,
            response: resp,
        }
    }

    /// Draw a visual group (bordered box) around child widgets.
    pub fn group<R>(&mut self, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let group_id = self.auto_id();
        let avail_w = self.available_width().max(0) as u32;
        let group_start_y = self.region.cursor_y;
        let group_x = self.region.cursor_x;

        // Inset for padding
        let pad = 8i32;
        self.region.cursor_x += pad;
        self.region.cursor_y += pad;
        let orig_max_w = self.region.max_rect.width;
        self.region.max_rect.x += pad;
        self.region.max_rect.width = (self.region.max_rect.width as i32 - pad * 2).max(0) as u32;

        let inner = add_contents(self);

        self.region.cursor_x -= pad;
        self.region.max_rect.x -= pad;
        self.region.max_rect.width = orig_max_w;

        let group_end_y = self.region.cursor_y + pad;
        let group_h = (group_end_y - group_start_y).max(0) as u32;
        let group_rect = Rect::new(group_x, group_start_y, avail_w, group_h);

        self.fb.draw_rounded_rect(
            group_rect,
            self.style.border_color,
            self.style.corner_radius,
            1,
        );

        self.region.cursor_y = group_end_y + self.style.spacing.item_spacing_y;

        let resp = Response::none(group_id, group_rect);
        InnerResponse {
            inner,
            response: resp,
        }
    }

    // ── Scroll area ──────────────────────────────────────────────

    /// Create a vertically scrollable area. The closure receives a child `Ui`
    /// whose content can exceed the visible height.
    pub fn scroll_area<R>(
        &mut self,
        id_salt: &str,
        height: u32,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let id = self.id_from(id_salt);
        let w = self.available_width().max(0) as u32;
        let area_rect = self.allocate_space(w, height);

        // Get current scroll offset
        let (_, scroll_y) = {
            let mem = UI_MEMORY.lock();
            mem.get_scroll(id)
        };

        // Push clip
        self.fb.push_clip(area_rect);

        // Create a tall child region offset by scroll
        let content_rect = Rect::new(
            area_rect.x,
            area_rect.y - scroll_y,
            w - self.style.spacing.scroll_bar_width as u32 - 2,
            100_000, // virtually infinite height
        );

        let saved_region = self.region;
        let saved_layout = self.layout;
        let saved_clip = self.clip_rect;
        let saved_id = self.id;

        self.region = Region::from_max_rect(&Layout::top_down(Align::LEFT), content_rect);
        self.layout = Layout::top_down(Align::LEFT);
        self.clip_rect = area_rect;
        self.id = id;

        let inner = add_contents(self);

        let content_height = (self.region.cursor_y - (area_rect.y - scroll_y)).max(0);
        let child_min = self.region.min_rect;

        // Restore
        self.region = saved_region;
        self.layout = saved_layout;
        self.clip_rect = saved_clip;
        self.id = saved_id;

        self.fb.pop_clip();

        // Handle scroll input
        let pointer_in_area = area_rect.contains(self.input.pointer_x, self.input.pointer_y);
        let mut new_scroll_y = scroll_y;
        if pointer_in_area && self.input.scroll_delta != 0 {
            new_scroll_y -= self.input.scroll_delta * 20;
        }
        // Clamp scroll
        let max_scroll = (content_height - height as i32).max(0);
        new_scroll_y = new_scroll_y.clamp(0, max_scroll);

        {
            let mut mem = UI_MEMORY.lock();
            mem.set_scroll(id, 0, new_scroll_y);
        }

        // Draw scrollbar if content overflows
        if content_height > height as i32 {
            let sb_w = self.style.spacing.scroll_bar_width as u32;
            let sb_x = area_rect.x + area_rect.width as i32 - sb_w as i32;
            let sb_rect = Rect::new(sb_x, area_rect.y, sb_w, height);

            // Track
            self.fb
                .fill_rounded_rect_aa(sb_rect, Pixel::rgb(30, 32, 38), sb_w / 2);

            // Thumb
            let thumb_h = ((height as i64 * height as i64) / content_height as i64)
                .max(20)
                .min(height as i64) as u32;
            let thumb_y = area_rect.y
                + if max_scroll > 0 {
                    (new_scroll_y as i64 * (height as i64 - thumb_h as i64) / max_scroll as i64)
                        as i32
                } else {
                    0
                };
            let thumb_rect = Rect::new(sb_x, thumb_y, sb_w, thumb_h);
            let thumb_color = if pointer_in_area {
                Pixel::rgb(100, 105, 115)
            } else {
                Pixel::rgb(70, 75, 85)
            };
            self.fb
                .fill_rounded_rect_aa(thumb_rect, thumb_color, sb_w / 2);
        }

        let resp = Response::none(id, area_rect);
        InnerResponse {
            inner,
            response: resp,
        }
    }

    // ── Grid layout ──────────────────────────────────────────────

    /// Draw widgets in a grid with `columns` columns.
    /// The closure is called once; widgets are auto-wrapped into columns.
    pub fn grid<R>(
        &mut self,
        id_salt: &str,
        columns: usize,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let child_id = self.id_from(id_salt);
        let avail_w = self.available_width().max(0) as u32;
        let col_w = if columns > 0 {
            avail_w / columns as u32
        } else {
            avail_w
        };

        // Use a wrapping horizontal layout with known column width
        let layout = Layout::left_to_right(Align::TOP).with_main_wrap(true);
        let child_rect = Rect::new(
            self.region.cursor_x,
            self.region.cursor_y,
            avail_w,
            self.available_height().max(0) as u32,
        );

        let saved_region = self.region;
        let saved_layout = self.layout;
        let saved_id = self.id;

        self.region = Region::from_max_rect(&layout, child_rect);
        self.layout = layout;
        self.id = child_id;

        let inner = add_contents(self);

        let child_min = self.region.min_rect;
        self.layout = saved_layout;
        self.region = saved_region;
        self.id = saved_id;

        let used_h = if child_min.height > 0 {
            child_min.height
        } else {
            0
        };
        let alloc_rect = self.allocate_space(avail_w, used_h);

        let resp = Response::none(child_id, alloc_rect);
        InnerResponse {
            inner,
            response: resp,
        }
    }
}
