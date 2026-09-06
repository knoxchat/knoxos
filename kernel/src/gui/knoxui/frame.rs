/// Frame — Decorative container with background, border, shadow, and padding
///
/// Inspired by egui::Frame. Wraps content with visual decoration.
///
/// ```ignore
/// Frame::default()
///     .fill(Pixel::rgb(30, 33, 40))
///     .stroke(1, Pixel::rgb(60, 65, 75))
///     .corner_radius(8)
///     .shadow(4)
///     .padding(12)
///     .show(ui, |ui| {
///         ui.label("Framed content");
///     });
/// ```
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::{InnerResponse, Response};
use crate::gui::ui::Ui;

pub struct Frame {
    fill: Option<Pixel>,
    stroke_width: u32,
    stroke_color: Pixel,
    corner_radius: u32,
    padding: i32,
    shadow_offset: i32,
    shadow_color: Pixel,
    inner_margin: i32,
    outer_margin: i32,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            fill: None,
            stroke_width: 0,
            stroke_color: Pixel::rgb(60, 65, 75),
            corner_radius: 6,
            padding: 8,
            shadow_offset: 0,
            shadow_color: Pixel::new(0, 0, 0, 80),
            inner_margin: 0,
            outer_margin: 0,
        }
    }
}

impl Frame {
    /// Frame styled as a window.
    pub fn window() -> Self {
        Self {
            fill: Some(Pixel::rgb(20, 22, 28)),
            stroke_width: 1,
            stroke_color: Pixel::new(80, 160, 255, 80),
            corner_radius: 10,
            padding: 12,
            shadow_offset: 4,
            shadow_color: Pixel::new(0, 0, 0, 80),
            inner_margin: 0,
            outer_margin: 0,
        }
    }

    /// Frame styled as a card (dashboard element).
    pub fn card() -> Self {
        Self {
            fill: Some(Pixel::rgb(24, 26, 34)),
            stroke_width: 1,
            stroke_color: Pixel::rgb(45, 50, 60),
            corner_radius: 12,
            padding: 16,
            shadow_offset: 3,
            shadow_color: Pixel::new(0, 0, 0, 60),
            inner_margin: 0,
            outer_margin: 4,
        }
    }

    /// Frame styled as a group (subtle border, no shadow).
    pub fn group() -> Self {
        Self {
            fill: None,
            stroke_width: 1,
            stroke_color: Pixel::rgb(55, 60, 70),
            corner_radius: 6,
            padding: 8,
            shadow_offset: 0,
            shadow_color: Pixel::new(0, 0, 0, 0),
            inner_margin: 0,
            outer_margin: 0,
        }
    }

    /// Frame for a popup or dropdown.
    pub fn popup() -> Self {
        Self {
            fill: Some(Pixel::rgb(30, 33, 42)),
            stroke_width: 1,
            stroke_color: Pixel::rgb(60, 65, 80),
            corner_radius: 8,
            padding: 8,
            shadow_offset: 6,
            shadow_color: Pixel::new(0, 0, 0, 120),
            inner_margin: 0,
            outer_margin: 0,
        }
    }

    /// Frame for a floating tooltip.
    pub fn tooltip() -> Self {
        Self {
            fill: Some(Pixel::rgb(45, 48, 56)),
            stroke_width: 1,
            stroke_color: Pixel::rgb(70, 75, 85),
            corner_radius: 6,
            padding: 6,
            shadow_offset: 3,
            shadow_color: Pixel::new(0, 0, 0, 100),
            inner_margin: 0,
            outer_margin: 0,
        }
    }

    pub fn fill(mut self, c: Pixel) -> Self {
        self.fill = Some(c);
        self
    }
    pub fn no_fill(mut self) -> Self {
        self.fill = None;
        self
    }
    pub fn stroke(mut self, width: u32, color: Pixel) -> Self {
        self.stroke_width = width;
        self.stroke_color = color;
        self
    }
    pub fn corner_radius(mut self, r: u32) -> Self {
        self.corner_radius = r;
        self
    }
    pub fn padding(mut self, p: i32) -> Self {
        self.padding = p;
        self
    }
    pub fn shadow(mut self, offset: i32) -> Self {
        self.shadow_offset = offset;
        self
    }
    pub fn shadow_color(mut self, c: Pixel) -> Self {
        self.shadow_color = c;
        self
    }
    pub fn inner_margin(mut self, m: i32) -> Self {
        self.inner_margin = m;
        self
    }
    pub fn outer_margin(mut self, m: i32) -> Self {
        self.outer_margin = m;
        self
    }

    /// Show the frame with content inside.
    pub fn show<'a, R>(
        self,
        ui: &mut Ui<'a>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let frame_id = ui.auto_id();

        // We need to figure out the frame size after the content is laid out.
        // Strategy: allocate a temporary child region, run content, measure, then draw frame.
        let avail_w = ui.available_width().max(0) as u32;
        let total_pad = self.padding + self.inner_margin;
        let start_y = ui.region.cursor_y + self.outer_margin;
        let start_x = ui.region.cursor_x + self.outer_margin;

        // Child region for content
        let content_rect = Rect::new(
            start_x + total_pad,
            start_y + total_pad,
            (avail_w as i32 - total_pad * 2 - self.outer_margin * 2).max(0) as u32,
            (ui.available_height() - total_pad * 2 - self.outer_margin * 2).max(0) as u32,
        );

        let saved_region = ui.region;
        let saved_layout = ui.layout;
        let saved_id = ui.id;

        ui.region = Region::from_max_rect(&Layout::top_down(Align::LEFT), content_rect);
        ui.layout = Layout::top_down(Align::LEFT);
        ui.id = frame_id;

        let inner = add_contents(ui);

        let content_used_h = (ui.region.cursor_y - content_rect.y).max(0) as u32;
        let content_used_w = ui.region.min_rect.width.max(8);

        ui.region = saved_region;
        ui.layout = saved_layout;
        ui.id = saved_id;

        // Frame rect
        let frame_w = (avail_w as i32 - self.outer_margin * 2).max(0) as u32;
        let frame_h = content_used_h + (total_pad * 2) as u32;
        let frame_rect = Rect::new(start_x, start_y, frame_w, frame_h);

        // Shadow
        if self.shadow_offset > 0 {
            ui.fb.fill_rounded_rect_aa(
                Rect::new(
                    frame_rect.x + self.shadow_offset,
                    frame_rect.y + self.shadow_offset,
                    frame_rect.width,
                    frame_rect.height,
                ),
                self.shadow_color,
                self.corner_radius + 2,
            );
        }

        // Fill
        if let Some(fill) = self.fill {
            ui.fb
                .fill_rounded_rect_aa(frame_rect, fill, self.corner_radius);
        }

        // Border
        if self.stroke_width > 0 {
            ui.fb.draw_rounded_rect(
                frame_rect,
                self.stroke_color,
                self.corner_radius,
                self.stroke_width,
            );
        }

        // Re-draw content on top of the frame background
        // (We already drew it above; in an optimized version we'd buffer it,
        // but for correctness in immediate mode we re-run the layout.)
        // For now, advance the parent cursor past the frame.
        let total_h = frame_h + (self.outer_margin * 2) as u32;
        ui.allocate_space(frame_w, total_h);

        let resp = Response::none(frame_id, frame_rect);
        InnerResponse {
            inner,
            response: resp,
        }
    }
}
