/// Area — Floating area positioned at an arbitrary point on screen.
/// Similar to egui::Area.
///
/// ```ignore
/// Area::new("popup_area")
///     .position(200, 100)
///     .show(ui, |ui| {
///         ui.label("Floating content");
///     });
/// ```
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::{InnerResponse, Response};
use crate::gui::ui::{UI_MEMORY, Ui};

pub struct Area {
    id: Id,
    pos_x: i32,
    pos_y: i32,
    width: Option<u32>,
    height: Option<u32>,
    movable: bool,
    constrain: bool,
    background: Option<Pixel>,
    corner_radius: u32,
}

impl Area {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            pos_x: 100,
            pos_y: 100,
            width: None,
            height: None,
            movable: false,
            constrain: true,
            background: None,
            corner_radius: 8,
        }
    }

    pub fn position(mut self, x: i32, y: i32) -> Self {
        self.pos_x = x;
        self.pos_y = y;
        self
    }

    pub fn fixed_size(mut self, w: u32, h: u32) -> Self {
        self.width = Some(w);
        self.height = Some(h);
        self
    }

    pub fn movable(mut self, m: bool) -> Self {
        self.movable = m;
        self
    }
    pub fn constrain(mut self, c: bool) -> Self {
        self.constrain = c;
        self
    }

    pub fn background(mut self, col: Pixel) -> Self {
        self.background = Some(col);
        self
    }

    pub fn corner_radius(mut self, r: u32) -> Self {
        self.corner_radius = r;
        self
    }

    pub fn show<'a, R>(
        self,
        ui: &mut Ui<'a>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let id = self.id;

        // Restore dragged position from memory
        let mem = UI_MEMORY.lock();
        let ox = mem.get_i32(id.with("area_x"), self.pos_x);
        let oy = mem.get_i32(id.with("area_y"), self.pos_y);
        drop(mem);

        let w = self.width.unwrap_or(200);
        let h = self.height.unwrap_or(150);

        let area_rect = Rect::new(ox, oy, w, h);

        // Draw background
        if let Some(bg) = self.background {
            if self.corner_radius > 0 {
                ui.fb
                    .fill_rounded_rect_aa(area_rect, bg, self.corner_radius);
            } else {
                ui.fb.fill_rect(area_rect, bg);
            }
        }

        // Create child Ui
        let saved = (ui.region, ui.layout, ui.clip_rect, ui.id);
        let child_rect = Rect::new(
            area_rect.x + 4,
            area_rect.y + 4,
            w.saturating_sub(8),
            h.saturating_sub(8),
        );
        ui.region = Region::from_max_rect(&Layout::top_down(Align::Min), child_rect);
        ui.layout = Layout::top_down(Align::Min);
        ui.id = id;

        let inner = add_contents(ui);

        ui.region = saved.0;
        ui.layout = saved.1;
        ui.clip_rect = saved.2;
        ui.id = saved.3;

        // Handle drag if movable
        if self.movable {
            let resp = ui.interact(area_rect, id.with("drag"), false, true);
            if resp.dragged {
                let new_x = ox + resp.drag_delta_x;
                let new_y = oy + resp.drag_delta_y;
                let mut mem = UI_MEMORY.lock();
                mem.set_i32(id.with("area_x"), new_x);
                mem.set_i32(id.with("area_y"), new_y);
            }
        }

        let resp = ui.interact(area_rect, id, true, false);
        InnerResponse {
            inner,
            response: resp,
        }
    }
}
