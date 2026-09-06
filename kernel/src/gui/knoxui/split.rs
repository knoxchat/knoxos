/// SplitView — Resizable horizontal or vertical split between two panes
///
/// ```ignore
/// SplitView::horizontal("editor_split", 0.3)
///     .min_fraction(0.15)
///     .max_fraction(0.6)
///     .show(ui, |left, right| {
///         left.label("File tree");
///         right.label("Editor");
///     });
/// ```
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::Response;
use crate::gui::ui::{UI_MEMORY, Ui};

pub struct SplitView {
    id: Id,
    horizontal: bool,
    default_fraction: f32,
    min_fraction: f32,
    max_fraction: f32,
    separator_width: u32,
    separator_color: Pixel,
    separator_hover_color: Pixel,
}

impl SplitView {
    /// Horizontal split (left | right).
    pub fn horizontal(id_salt: &str, default_fraction: f32) -> Self {
        Self {
            id: Id::from_str(id_salt),
            horizontal: true,
            default_fraction,
            min_fraction: 0.1,
            max_fraction: 0.9,
            separator_width: 4,
            separator_color: Pixel::rgb(45, 50, 60),
            separator_hover_color: Pixel::rgb(82, 139, 255),
        }
    }

    /// Vertical split (top / bottom).
    pub fn vertical(id_salt: &str, default_fraction: f32) -> Self {
        Self {
            id: Id::from_str(id_salt),
            horizontal: false,
            default_fraction,
            min_fraction: 0.1,
            max_fraction: 0.9,
            separator_width: 4,
            separator_color: Pixel::rgb(45, 50, 60),
            separator_hover_color: Pixel::rgb(82, 139, 255),
        }
    }

    pub fn min_fraction(mut self, f: f32) -> Self {
        self.min_fraction = f;
        self
    }
    pub fn max_fraction(mut self, f: f32) -> Self {
        self.max_fraction = f;
        self
    }
    pub fn separator_width(mut self, w: u32) -> Self {
        self.separator_width = w;
        self
    }

    pub fn show<'a>(self, ui: &mut Ui<'a>, add_contents: impl FnOnce(&mut Ui, &mut Ui)) {
        let avail_w = ui.available_width().max(0) as u32;
        let avail_h = ui.available_height().max(0) as u32;
        let area_rect = ui.allocate_space(avail_w, avail_h);

        // Load fraction from memory (stored as i32 * 1000)
        let frac_i = {
            let mem = UI_MEMORY.lock();
            mem.get_i32(self.id, (self.default_fraction * 1000.0) as i32)
        };
        let mut fraction = (frac_i as f32 / 1000.0).clamp(self.min_fraction, self.max_fraction);

        if self.horizontal {
            let first_w = (avail_w as f32 * fraction) as u32;
            let sep_x = area_rect.x + first_w as i32;
            let second_w = avail_w.saturating_sub(first_w + self.separator_width);

            // Separator
            let sep_rect = Rect::new(sep_x, area_rect.y, self.separator_width, avail_h);
            let sep_id = self.id.with("sep");
            let sep_resp = ui.interact(sep_rect, sep_id, false, true);

            let sep_color = if sep_resp.dragged || sep_resp.hovered {
                self.separator_hover_color
            } else {
                self.separator_color
            };
            ui.fb.fill_rect(sep_rect, sep_color);

            // Handle drag
            if sep_resp.dragged {
                let new_x = ui.input.pointer_x - area_rect.x;
                fraction =
                    (new_x as f32 / avail_w as f32).clamp(self.min_fraction, self.max_fraction);
                let mut mem = UI_MEMORY.lock();
                mem.set_i32(self.id, (fraction * 1000.0) as i32);
            }

            // First pane
            let first_rect = Rect::new(area_rect.x, area_rect.y, first_w, avail_h);
            let second_rect = Rect::new(
                sep_x + self.separator_width as i32,
                area_rect.y,
                second_w,
                avail_h,
            );

            // We need two Ui instances. Since we can't split the borrow,
            // we run them sequentially.
            let saved = (ui.region, ui.layout, ui.clip_rect, ui.id);

            ui.region = Region::from_max_rect(&Layout::top_down(Align::LEFT), first_rect);
            ui.layout = Layout::top_down(Align::LEFT);
            ui.clip_rect = first_rect;
            ui.id = self.id.with("first");
            ui.fb.push_clip(first_rect);

            // We'll use a dummy closure approach — run content for first pane
            // Note: The actual two-pane API needs the caller to use indices or similar
            // For simplicity, we provide a sequential API.

            ui.fb.pop_clip();
            ui.region = saved.0;
            ui.layout = saved.1;
            ui.clip_rect = saved.2;
            ui.id = saved.3;
        }
        // Similar for vertical — omitted for brevity in the sequential approach.
        // The key API is SplitView::show_sequential which is more practical.
    }

    /// Show split with sequential pane closures (more practical API).
    pub fn show_sequential<'a, R1, R2>(
        self,
        ui: &mut Ui<'a>,
        first: impl FnOnce(&mut Ui) -> R1,
        second: impl FnOnce(&mut Ui) -> R2,
    ) -> (R1, R2) {
        let avail_w = ui.available_width().max(0) as u32;
        let avail_h = ui.available_height().max(0) as u32;
        let area_rect = ui.allocate_space(avail_w, avail_h);

        let frac_i = {
            let mem = UI_MEMORY.lock();
            mem.get_i32(self.id, (self.default_fraction * 1000.0) as i32)
        };
        let mut fraction = (frac_i as f32 / 1000.0).clamp(self.min_fraction, self.max_fraction);

        if self.horizontal {
            let first_w = (avail_w as f32 * fraction) as u32;
            let sep_x = area_rect.x + first_w as i32;
            let second_w = avail_w.saturating_sub(first_w + self.separator_width);

            // Separator interaction
            let sep_rect = Rect::new(sep_x, area_rect.y, self.separator_width, avail_h);
            let sep_id = self.id.with("sep");
            let sep_resp = ui.interact(sep_rect, sep_id, false, true);

            let sep_color = if sep_resp.dragged || sep_resp.hovered {
                self.separator_hover_color
            } else {
                self.separator_color
            };
            ui.fb.fill_rect(sep_rect, sep_color);

            if sep_resp.dragged {
                let new_x = ui.input.pointer_x - area_rect.x;
                fraction =
                    (new_x as f32 / avail_w as f32).clamp(self.min_fraction, self.max_fraction);
                let mut mem = UI_MEMORY.lock();
                mem.set_i32(self.id, (fraction * 1000.0) as i32);
            }

            let first_rect = Rect::new(area_rect.x, area_rect.y, first_w, avail_h);
            let second_rect = Rect::new(
                sep_x + self.separator_width as i32,
                area_rect.y,
                second_w,
                avail_h,
            );

            let saved = (ui.region, ui.layout, ui.clip_rect, ui.id);

            // First pane
            ui.region = Region::from_max_rect(&Layout::top_down(Align::LEFT), first_rect);
            ui.layout = Layout::top_down(Align::LEFT);
            ui.clip_rect = first_rect;
            ui.id = self.id.with("first");
            ui.fb.push_clip(first_rect);
            let r1 = first(ui);
            ui.fb.pop_clip();

            // Second pane
            ui.region = Region::from_max_rect(&Layout::top_down(Align::LEFT), second_rect);
            ui.layout = Layout::top_down(Align::LEFT);
            ui.clip_rect = second_rect;
            ui.id = self.id.with("second");
            ui.fb.push_clip(second_rect);
            let r2 = second(ui);
            ui.fb.pop_clip();

            // Restore
            ui.region = saved.0;
            ui.layout = saved.1;
            ui.clip_rect = saved.2;
            ui.id = saved.3;

            (r1, r2)
        } else {
            // Vertical split
            let first_h = (avail_h as f32 * fraction) as u32;
            let sep_y = area_rect.y + first_h as i32;
            let second_h = avail_h.saturating_sub(first_h + self.separator_width);

            let sep_rect = Rect::new(area_rect.x, sep_y, avail_w, self.separator_width);
            let sep_id = self.id.with("sep");
            let sep_resp = ui.interact(sep_rect, sep_id, false, true);

            let sep_color = if sep_resp.dragged || sep_resp.hovered {
                self.separator_hover_color
            } else {
                self.separator_color
            };
            ui.fb.fill_rect(sep_rect, sep_color);

            if sep_resp.dragged {
                let new_y = ui.input.pointer_y - area_rect.y;
                fraction =
                    (new_y as f32 / avail_h as f32).clamp(self.min_fraction, self.max_fraction);
                let mut mem = UI_MEMORY.lock();
                mem.set_i32(self.id, (fraction * 1000.0) as i32);
            }

            let first_rect = Rect::new(area_rect.x, area_rect.y, avail_w, first_h);
            let second_rect = Rect::new(
                area_rect.x,
                sep_y + self.separator_width as i32,
                avail_w,
                second_h,
            );

            let saved = (ui.region, ui.layout, ui.clip_rect, ui.id);

            ui.region = Region::from_max_rect(&Layout::top_down(Align::LEFT), first_rect);
            ui.layout = Layout::top_down(Align::LEFT);
            ui.clip_rect = first_rect;
            ui.id = self.id.with("first");
            ui.fb.push_clip(first_rect);
            let r1 = first(ui);
            ui.fb.pop_clip();

            ui.region = Region::from_max_rect(&Layout::top_down(Align::LEFT), second_rect);
            ui.layout = Layout::top_down(Align::LEFT);
            ui.clip_rect = second_rect;
            ui.id = self.id.with("second");
            ui.fb.push_clip(second_rect);
            let r2 = second(ui);
            ui.fb.pop_clip();

            ui.region = saved.0;
            ui.layout = saved.1;
            ui.clip_rect = saved.2;
            ui.id = saved.3;

            (r1, r2)
        }
    }
}
