/// ScrollArea — Scrollable container with optional horizontal/vertical scrollbars
///
/// Inspired by egui::ScrollArea. Supports vertical scrolling, horizontal scrolling,
/// or both. Renders smooth scrollbar thumbs with the KnoxOS design language.
///
/// ```ignore
/// ScrollArea::vertical("log", 300).show(ui, |ui| {
///     for line in &log_lines {
///         ui.label(line);
///     }
/// });
/// ```
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::{InnerResponse, Response};
use crate::gui::ui::{UI_MEMORY, Ui};

/// Configuration for a scrollable area.
pub struct ScrollArea {
    id: Id,
    max_height: Option<u32>,
    max_width: Option<u32>,
    vertical: bool,
    horizontal: bool,
    /// Always show scrollbar (vs auto-hide)
    always_show: bool,
    /// Enable smooth scrolling (animated offset)
    stick_to_bottom: bool,
    /// Scrollbar width in pixels
    bar_width: u32,
}

impl ScrollArea {
    /// Vertically scrollable area with a max visible height.
    pub fn vertical(id_salt: &str, max_height: u32) -> Self {
        Self {
            id: Id::from_str(id_salt),
            max_height: Some(max_height),
            max_width: None,
            vertical: true,
            horizontal: false,
            always_show: false,
            stick_to_bottom: false,
            bar_width: 8,
        }
    }

    /// Horizontally scrollable area with a max visible width.
    pub fn horizontal(id_salt: &str, max_width: u32) -> Self {
        Self {
            id: Id::from_str(id_salt),
            max_height: None,
            max_width: Some(max_width),
            vertical: false,
            horizontal: true,
            always_show: false,
            stick_to_bottom: false,
            bar_width: 8,
        }
    }

    /// Both vertical and horizontal scrolling.
    pub fn both(id_salt: &str, max_width: u32, max_height: u32) -> Self {
        Self {
            id: Id::from_str(id_salt),
            max_height: Some(max_height),
            max_width: Some(max_width),
            vertical: true,
            horizontal: true,
            always_show: false,
            stick_to_bottom: false,
            bar_width: 8,
        }
    }

    pub fn always_show_scrollbar(mut self, v: bool) -> Self {
        self.always_show = v;
        self
    }

    pub fn stick_to_bottom(mut self, v: bool) -> Self {
        self.stick_to_bottom = v;
        self
    }

    pub fn bar_width(mut self, w: u32) -> Self {
        self.bar_width = w;
        self
    }

    /// Show the scroll area. The closure receives a `Ui` that can be arbitrarily tall/wide.
    pub fn show<'a, R>(
        self,
        ui: &mut Ui<'a>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let style = ui.style().clone();
        let avail_w = ui.available_width().max(0) as u32;
        let avail_h = ui.available_height().max(0) as u32;

        let vis_w = self.max_width.unwrap_or(avail_w).min(avail_w);
        let vis_h = self.max_height.unwrap_or(avail_h).min(avail_h);
        let area_rect = ui.allocate_space(vis_w, vis_h);

        // Current scroll offsets
        let (scroll_x, scroll_y) = {
            let mem = UI_MEMORY.lock();
            mem.get_scroll(self.id)
        };

        // Content area (minus scrollbar space)
        let content_w = if self.vertical {
            vis_w.saturating_sub(self.bar_width + 2)
        } else {
            vis_w
        };
        let content_h = if self.horizontal {
            vis_h.saturating_sub(self.bar_width + 2)
        } else {
            vis_h
        };

        // Push clip to visible area
        ui.fb.push_clip(area_rect);

        let content_rect = Rect::new(
            area_rect.x - scroll_x,
            area_rect.y - scroll_y,
            if self.horizontal { 100_000 } else { content_w },
            if self.vertical { 100_000 } else { content_h },
        );

        let saved_region = ui.region;
        let saved_layout = ui.layout;
        let saved_clip = ui.clip_rect;
        let saved_id = ui.id;

        ui.region = Region::from_max_rect(&Layout::top_down(Align::LEFT), content_rect);
        ui.layout = Layout::top_down(Align::LEFT);
        ui.clip_rect = area_rect;
        ui.id = self.id;

        let inner = add_contents(ui);

        let content_used_h = (ui.region.cursor_y - (area_rect.y - scroll_y)).max(0);
        let content_used_w = ui.region.min_rect.width as i32;

        ui.region = saved_region;
        ui.layout = saved_layout;
        ui.clip_rect = saved_clip;
        ui.id = saved_id;

        ui.fb.pop_clip();

        // Handle scroll input
        let pointer_in = area_rect.contains(ui.input.pointer_x, ui.input.pointer_y);
        let mut new_scroll_x = scroll_x;
        let mut new_scroll_y = scroll_y;

        if pointer_in && ui.input.scroll_delta != 0 {
            if self.vertical {
                new_scroll_y -= ui.input.scroll_delta * 24;
            } else if self.horizontal {
                new_scroll_x -= ui.input.scroll_delta * 24;
            }
        }

        // Clamp
        let max_scroll_y = (content_used_h - content_h as i32).max(0);
        let max_scroll_x = (content_used_w - content_w as i32).max(0);
        new_scroll_y = new_scroll_y.clamp(0, max_scroll_y);
        new_scroll_x = new_scroll_x.clamp(0, max_scroll_x);

        // Stick to bottom
        if self.stick_to_bottom && content_used_h > content_h as i32 {
            new_scroll_y = max_scroll_y;
        }

        {
            let mut mem = UI_MEMORY.lock();
            mem.set_scroll(self.id, new_scroll_x, new_scroll_y);
        }

        // Draw vertical scrollbar
        if self.vertical && (content_used_h > content_h as i32 || self.always_show) {
            let sb_x = area_rect.x + area_rect.width as i32 - self.bar_width as i32;
            let sb_rect = Rect::new(sb_x, area_rect.y, self.bar_width, vis_h);

            // Track
            ui.fb
                .fill_rounded_rect_aa(sb_rect, Pixel::rgb(25, 28, 34), self.bar_width / 2);

            if content_used_h > 0 {
                let thumb_h = ((vis_h as i64 * vis_h as i64) / content_used_h as i64)
                    .max(20)
                    .min(vis_h as i64) as u32;
                let thumb_y = area_rect.y
                    + if max_scroll_y > 0 {
                        (new_scroll_y as i64 * (vis_h as i64 - thumb_h as i64)
                            / max_scroll_y as i64) as i32
                    } else {
                        0
                    };
                let thumb_rect = Rect::new(sb_x, thumb_y, self.bar_width, thumb_h);

                // Scrollbar drag
                let thumb_id = self.id.with("vthumb");
                let thumb_resp = ui.interact(thumb_rect, thumb_id, false, true);
                let thumb_color = if thumb_resp.dragged {
                    colors::SCROLLBAR_THUMB_ACTIVE
                } else if thumb_resp.hovered || pointer_in {
                    colors::SCROLLBAR_THUMB_HOVER
                } else {
                    colors::SCROLLBAR_THUMB
                };
                ui.fb
                    .fill_rounded_rect_aa(thumb_rect, thumb_color, self.bar_width / 2);

                // Handle thumb drag
                if thumb_resp.dragged && thumb_resp.drag_delta_y != 0 {
                    let total_track = vis_h as i32 - thumb_h as i32;
                    if total_track > 0 {
                        let delta_scroll = thumb_resp.drag_delta_y * max_scroll_y / total_track;
                        let updated = (new_scroll_y + delta_scroll).clamp(0, max_scroll_y);
                        let mut mem = UI_MEMORY.lock();
                        mem.set_scroll(self.id, new_scroll_x, updated);
                    }
                }
            }
        }

        // Draw horizontal scrollbar
        if self.horizontal && (content_used_w > content_w as i32 || self.always_show) {
            let sb_y = area_rect.y + area_rect.height as i32 - self.bar_width as i32;
            let sb_rect = Rect::new(area_rect.x, sb_y, vis_w, self.bar_width);

            ui.fb
                .fill_rounded_rect_aa(sb_rect, Pixel::rgb(25, 28, 34), self.bar_width / 2);

            if content_used_w > 0 {
                let thumb_w = ((vis_w as i64 * vis_w as i64) / content_used_w as i64)
                    .max(20)
                    .min(vis_w as i64) as u32;
                let thumb_x = area_rect.x
                    + if max_scroll_x > 0 {
                        (new_scroll_x as i64 * (vis_w as i64 - thumb_w as i64)
                            / max_scroll_x as i64) as i32
                    } else {
                        0
                    };
                let thumb_rect = Rect::new(thumb_x, sb_y, thumb_w, self.bar_width);

                let thumb_color = if pointer_in {
                    colors::SCROLLBAR_THUMB_HOVER
                } else {
                    colors::SCROLLBAR_THUMB
                };
                ui.fb
                    .fill_rounded_rect_aa(thumb_rect, thumb_color, self.bar_width / 2);
            }
        }

        let resp = Response::none(self.id, area_rect);
        InnerResponse {
            inner,
            response: resp,
        }
    }
}
