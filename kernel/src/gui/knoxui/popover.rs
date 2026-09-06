/// Popover — A floating popover anchored to a trigger element.
///
/// ```ignore
/// let trigger = ui.button("Show popover");
/// Popover::new("my_popup")
///     .anchor_response(&trigger)
///     .show(ui, |ui| {
///         ui.label("Popover content");
///     });
/// ```
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::{InnerResponse, Response};
use crate::gui::ui::{UI_MEMORY, Ui};

#[derive(Clone, Copy)]
pub enum PopoverDirection {
    Below,
    Above,
    Left,
    Right,
}

pub struct Popover {
    id: Id,
    direction: PopoverDirection,
    anchor_rect: Rect,
    width: u32,
    height: u32,
    bg: Pixel,
    corner_radius: u32,
    arrow: bool,
}

impl Popover {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            direction: PopoverDirection::Below,
            anchor_rect: Rect::new(0, 0, 0, 0),
            width: 200,
            height: 120,
            bg: colors::SURFACE_OVERLAY,
            corner_radius: 8,
            arrow: true,
        }
    }

    pub fn direction(mut self, d: PopoverDirection) -> Self {
        self.direction = d;
        self
    }

    pub fn anchor(mut self, rect: Rect) -> Self {
        self.anchor_rect = rect;
        self
    }

    /// Anchor to a Response's rect (from a button click, etc.)
    pub fn anchor_response(mut self, resp: &Response) -> Self {
        self.anchor_rect = resp.rect;
        self
    }

    pub fn size(mut self, w: u32, h: u32) -> Self {
        self.width = w;
        self.height = h;
        self
    }

    pub fn bg(mut self, bg: Pixel) -> Self {
        self.bg = bg;
        self
    }
    pub fn corner_radius(mut self, r: u32) -> Self {
        self.corner_radius = r;
        self
    }
    pub fn arrow(mut self, a: bool) -> Self {
        self.arrow = a;
        self
    }

    /// Toggle the popover's open state.
    pub fn toggle(id: Id) {
        let mut mem = UI_MEMORY.lock();
        let open = mem.get_bool(id.with("pop_open"), false);
        mem.set_bool(id.with("pop_open"), !open);
    }

    pub fn is_open(id: Id) -> bool {
        let mem = UI_MEMORY.lock();
        mem.get_bool(id.with("pop_open"), false)
    }

    /// Show the popover if it's open. Returns `Some(InnerResponse)` if shown.
    pub fn show<'a, R>(
        self,
        ui: &mut Ui<'a>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> Option<InnerResponse<R>> {
        let mem = UI_MEMORY.lock();
        let open = mem.get_bool(self.id.with("pop_open"), false);
        drop(mem);

        if !open {
            return None;
        }

        let gap = 4i32;

        // Calculate position based on direction
        let (px, py) = match self.direction {
            PopoverDirection::Below => (
                self.anchor_rect.x,
                self.anchor_rect.y + self.anchor_rect.height as i32 + gap,
            ),
            PopoverDirection::Above => (
                self.anchor_rect.x,
                self.anchor_rect.y - self.height as i32 - gap,
            ),
            PopoverDirection::Right => (
                self.anchor_rect.x + self.anchor_rect.width as i32 + gap,
                self.anchor_rect.y,
            ),
            PopoverDirection::Left => (
                self.anchor_rect.x - self.width as i32 - gap,
                self.anchor_rect.y,
            ),
        };

        let pop_rect = Rect::new(px, py, self.width, self.height);

        // Shadow
        let shadow = Rect::new(px + 2, py + 2, self.width, self.height);
        ui.fb
            .fill_rounded_rect_aa(shadow, Pixel::new(0, 0, 0, 60), self.corner_radius);

        // Background
        ui.fb
            .fill_rounded_rect_aa(pop_rect, self.bg, self.corner_radius);
        ui.fb
            .draw_rounded_rect(pop_rect, colors::SURFACE_BORDER, self.corner_radius, 1);

        // Arrow indicator
        if self.arrow {
            let arrow_size = 6i32;
            match self.direction {
                PopoverDirection::Below => {
                    let ax = px + self.anchor_rect.width as i32 / 2;
                    let ay = py;
                    // Simple triangle pointing up
                    for dy in 0..arrow_size {
                        let half = dy;
                        ui.fb.draw_line_aa(
                            ax - half,
                            ay - arrow_size + dy,
                            ax + half,
                            ay - arrow_size + dy,
                            self.bg,
                        );
                    }
                }
                PopoverDirection::Above => {
                    let ax = px + self.anchor_rect.width as i32 / 2;
                    let ay = py + self.height as i32;
                    for dy in 0..arrow_size {
                        let half = arrow_size - dy;
                        ui.fb
                            .draw_line_aa(ax - half, ay + dy, ax + half, ay + dy, self.bg);
                    }
                }
                _ => {} // Left/Right arrows omitted for simplicity
            }
        }

        // Content
        let saved = (ui.region, ui.layout, ui.clip_rect, ui.id);
        let padding = 8u32;
        let content_rect = Rect::new(
            px + padding as i32,
            py + padding as i32,
            self.width.saturating_sub(padding * 2),
            self.height.saturating_sub(padding * 2),
        );
        ui.region = Region::from_max_rect(&Layout::top_down(Align::Min), content_rect);
        ui.layout = Layout::top_down(Align::Min);
        ui.id = self.id;

        let inner = add_contents(ui);

        ui.region = saved.0;
        ui.layout = saved.1;
        ui.clip_rect = saved.2;
        ui.id = saved.3;

        // Close if clicked outside
        let outside_id = self.id.with("outside");
        let screen_rect = Rect::new(0, 0, 9999, 9999);
        let outside_resp = ui.interact(screen_rect, outside_id, true, false);
        if outside_resp.clicked {
            // Check if click is outside popover
            let mx = ui.input.pointer_x;
            let my = ui.input.pointer_y;
            if !pop_rect.contains(mx, my) && !self.anchor_rect.contains(mx, my) {
                let mut mem = UI_MEMORY.lock();
                mem.set_bool(self.id.with("pop_open"), false);
            }
        }

        let resp = ui.interact(pop_rect, self.id, true, false);
        Some(InnerResponse {
            inner,
            response: resp,
        })
    }
}
