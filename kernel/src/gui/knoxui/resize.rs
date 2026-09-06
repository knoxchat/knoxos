/// Resize — A resizable container with a resize grip in the bottom-right corner.
///
/// ```ignore
/// Resize::new("resizable_panel")
///     .default_size(200, 150)
///     .show(ui, |ui| {
///         ui.label("Resize me!");
///     });
/// ```
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::{InnerResponse, Response};
use crate::gui::ui::{UI_MEMORY, Ui};

pub struct Resize {
    id: Id,
    default_w: u32,
    default_h: u32,
    min_w: u32,
    min_h: u32,
    max_w: u32,
    max_h: u32,
    grip_size: u32,
    show_grip: bool,
}

impl Resize {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            default_w: 200,
            default_h: 150,
            min_w: 60,
            min_h: 40,
            max_w: 2000,
            max_h: 2000,
            grip_size: 12,
            show_grip: true,
        }
    }

    pub fn default_size(mut self, w: u32, h: u32) -> Self {
        self.default_w = w;
        self.default_h = h;
        self
    }

    pub fn min_size(mut self, w: u32, h: u32) -> Self {
        self.min_w = w;
        self.min_h = h;
        self
    }

    pub fn max_size(mut self, w: u32, h: u32) -> Self {
        self.max_w = w;
        self.max_h = h;
        self
    }

    pub fn show_grip(mut self, show: bool) -> Self {
        self.show_grip = show;
        self
    }

    pub fn show<'a, R>(
        self,
        ui: &mut Ui<'a>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let id = self.id;

        // Retrieve persisted size or use defaults
        let mem = UI_MEMORY.lock();
        let cur_w = mem.get_i32(id.with("rw"), self.default_w as i32) as u32;
        let cur_h = mem.get_i32(id.with("rh"), self.default_h as i32) as u32;
        drop(mem);

        let w = cur_w.clamp(self.min_w, self.max_w);
        let h = cur_h.clamp(self.min_h, self.max_h);

        // Allocate the space
        let outer = ui.allocate_space(w, h);

        // Render child content
        let saved = (ui.region, ui.layout, ui.clip_rect, ui.id);
        let inner_rect = Rect::new(outer.x, outer.y, w, h.saturating_sub(2));
        ui.region = Region::from_max_rect(&Layout::top_down(Align::Min), inner_rect);
        ui.layout = Layout::top_down(Align::Min);
        ui.id = id;

        let inner = add_contents(ui);

        ui.region = saved.0;
        ui.layout = saved.1;
        ui.clip_rect = saved.2;
        ui.id = saved.3;

        // Draw resize grip triangle
        if self.show_grip {
            let gs = self.grip_size as i32;
            let gx = outer.x + w as i32 - gs;
            let gy = outer.y + h as i32 - gs;
            let grip_rect = Rect::new(gx, gy, self.grip_size, self.grip_size);
            let grip_color = colors::SURFACE_BORDER;

            // Draw 3 diagonal lines as grip indicator

            for i in 0..3 {
                let offset = i * 4;
                let x0 = outer.x + w as i32 - 2;
                let y0 = outer.y + h as i32 - gs + offset;
                let x1 = outer.x + w as i32 - gs + offset;
                let y1 = outer.y + h as i32 - 2;
                ui.fb.draw_line_aa(x0, y0, x1, y1, grip_color);
            }

            // Handle grip drag
            let grip_id = id.with("grip");
            let resp = ui.interact(grip_rect, grip_id, false, true);
            if resp.dragged {
                let new_w =
                    (w as i32 + resp.drag_delta_x).clamp(self.min_w as i32, self.max_w as i32);
                let new_h =
                    (h as i32 + resp.drag_delta_y).clamp(self.min_h as i32, self.max_h as i32);
                let mut mem = UI_MEMORY.lock();
                mem.set_i32(id.with("rw"), new_w);
                mem.set_i32(id.with("rh"), new_h);
            }
        }

        let resp = ui.interact(outer, id, true, false);
        InnerResponse {
            inner,
            response: resp,
        }
    }
}
