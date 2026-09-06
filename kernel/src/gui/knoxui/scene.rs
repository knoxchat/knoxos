// ─── Scene — Zoomable and pannable canvas container ──────────────────
//
// Inspired by egui::Scene. A container that allows the user to zoom in/out
// and pan around a virtual canvas using mouse scroll and drag. Useful for
// node editors, canvas apps, map views, etc.

use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::ui::{UI_MEMORY, Ui};

/// A zoomable and pannable scene container.
///
/// ```ignore
/// Scene::new("canvas")
///     .zoom_range(50, 400)  // 50% to 400%
///     .show(ui, |scene_ui| {
///         // Draw content at scene coordinates
///         scene_ui.draw_rect(100, 100, 200, 80, Pixel::rgb(80, 90, 120));
///     });
/// ```
pub struct Scene<'a> {
    id_str: &'a str,
    zoom_min: i32, // percentage, e.g. 25 = 25%
    zoom_max: i32, // percentage, e.g. 400 = 400%
    default_zoom: i32,
    show_grid: bool,
    grid_size: i32,
}

impl<'a> Scene<'a> {
    pub fn new(id_str: &'a str) -> Self {
        Self {
            id_str,
            zoom_min: 25,
            zoom_max: 400,
            default_zoom: 100,
            show_grid: true,
            grid_size: 32,
        }
    }

    pub fn zoom_range(mut self, min: i32, max: i32) -> Self {
        self.zoom_min = min;
        self.zoom_max = max;
        self
    }

    pub fn default_zoom(mut self, z: i32) -> Self {
        self.default_zoom = z;
        self
    }

    pub fn show_grid(mut self, g: bool) -> Self {
        self.show_grid = g;
        self
    }

    pub fn grid_size(mut self, s: i32) -> Self {
        self.grid_size = s;
        self
    }

    pub fn show(self, ui: &mut Ui, add_contents: impl FnOnce(&mut SceneUi)) {
        let id = ui.id.with(self.id_str);
        let pan_x_key = id.with("pan_x");
        let pan_y_key = id.with("pan_y");
        let zoom_key = id.with("zoom");

        let avail_w = ui.available_width().max(100) as u32;
        let avail_h = ui.available_height().max(100) as u32;
        let outer = ui.allocate_space(avail_w, avail_h);

        let resp = ui.interact(outer, id, true, true);

        let mut mem = UI_MEMORY.lock();
        let mut pan_x = mem.get_i32(pan_x_key, 0);
        let mut pan_y = mem.get_i32(pan_y_key, 0);
        let mut zoom = mem.get_i32(zoom_key, self.default_zoom);
        drop(mem);

        // Pan with drag
        if resp.dragged {
            pan_x += resp.drag_delta_x;
            pan_y += resp.drag_delta_y;
        }

        // Zoom with scroll
        if resp.hovered && ui.input.scroll_delta != 0 {
            let zoom_step = if ui.input.scroll_delta > 0 { 10 } else { -10 };
            zoom = (zoom + zoom_step).clamp(self.zoom_min, self.zoom_max);
        }

        let mut mem = UI_MEMORY.lock();
        mem.set_i32(pan_x_key, pan_x);
        mem.set_i32(pan_y_key, pan_y);
        mem.set_i32(zoom_key, zoom);
        drop(mem);

        // Clip to scene area
        ui.fb.push_clip(outer);

        // Background
        ui.fb.fill_rect(outer, Pixel::rgb(22, 24, 30));

        // Grid overlay
        if self.show_grid {
            let grid_step = (self.grid_size * zoom / 100).max(4);
            let offset_x = pan_x % grid_step;
            let offset_y = pan_y % grid_step;
            let grid_color = Pixel::new(255, 255, 255, 8);

            let mut gx = outer.x + offset_x;
            while gx < outer.x + avail_w as i32 {
                ui.fb.draw_vline(gx, outer.y, avail_h, grid_color);
                gx += grid_step;
            }

            let mut gy = outer.y + offset_y;
            while gy < outer.y + avail_h as i32 {
                ui.fb.draw_hline(outer.x, gy, avail_w, grid_color);
                gy += grid_step;
            }
        }

        // Scene content
        let mut scene_ui = SceneUi {
            ui,
            origin_x: outer.x + pan_x,
            origin_y: outer.y + pan_y,
            zoom,
        };
        add_contents(&mut scene_ui);

        scene_ui.ui.fb.pop_clip();

        // Zoom indicator
        let zoom_text = alloc::format!("{}%", zoom);
        let zi_x = outer.x + avail_w as i32 - (zoom_text.len() as i32 * 8) - 8;
        let zi_y = outer.y + avail_h as i32 - 18;
        super::text_helpers::draw_string_compact(
            scene_ui.ui.fb,
            &zoom_text,
            zi_x,
            zi_y,
            Pixel::new(255, 255, 255, 80),
        );
    }
}

/// UI context passed inside a Scene, with coordinate transforms applied.
pub struct SceneUi<'a, 'b> {
    pub ui: &'a mut Ui<'b>,
    pub origin_x: i32,
    pub origin_y: i32,
    pub zoom: i32,
}

impl<'a, 'b> SceneUi<'a, 'b> {
    /// Transform scene coords → screen coords.
    pub fn to_screen_x(&self, scene_x: i32) -> i32 {
        self.origin_x + scene_x * self.zoom / 100
    }

    pub fn to_screen_y(&self, scene_y: i32) -> i32 {
        self.origin_y + scene_y * self.zoom / 100
    }

    /// Transform screen coords → scene coords.
    pub fn to_scene_x(&self, screen_x: i32) -> i32 {
        if self.zoom != 0 {
            (screen_x - self.origin_x) * 100 / self.zoom
        } else {
            0
        }
    }

    pub fn to_scene_y(&self, screen_y: i32) -> i32 {
        if self.zoom != 0 {
            (screen_y - self.origin_y) * 100 / self.zoom
        } else {
            0
        }
    }

    /// Draw a filled rect in scene coordinates.
    pub fn fill_rect(&mut self, sx: i32, sy: i32, sw: u32, sh: u32, color: Pixel) {
        let x = self.to_screen_x(sx);
        let y = self.to_screen_y(sy);
        let w = (sw as i32 * self.zoom / 100).max(1) as u32;
        let h = (sh as i32 * self.zoom / 100).max(1) as u32;
        self.ui.fb.fill_rect(Rect::new(x, y, w, h), color);
    }

    /// Draw a rounded rect in scene coordinates.
    pub fn fill_rounded_rect(
        &mut self,
        sx: i32,
        sy: i32,
        sw: u32,
        sh: u32,
        color: Pixel,
        radius: u32,
    ) {
        let x = self.to_screen_x(sx);
        let y = self.to_screen_y(sy);
        let w = (sw as i32 * self.zoom / 100).max(1) as u32;
        let h = (sh as i32 * self.zoom / 100).max(1) as u32;
        let r = (radius as i32 * self.zoom / 100).max(0) as u32;
        self.ui
            .fb
            .fill_rounded_rect_aa(Rect::new(x, y, w, h), color, r);
    }

    /// Draw a line in scene coordinates.
    pub fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Pixel) {
        let sx0 = self.to_screen_x(x0);
        let sy0 = self.to_screen_y(y0);
        let sx1 = self.to_screen_x(x1);
        let sy1 = self.to_screen_y(y1);
        self.ui.fb.draw_line_aa(sx0, sy0, sx1, sy1, color);
    }

    /// Draw text at scene coordinates.
    pub fn draw_text(&mut self, sx: i32, sy: i32, text: &str, color: Pixel) {
        let x = self.to_screen_x(sx);
        let y = self.to_screen_y(sy);
        super::text_helpers::draw_string_compact(self.ui.fb, text, x, y, color);
    }

    /// Draw a circle in scene coordinates.
    pub fn fill_circle(&mut self, cx: i32, cy: i32, radius: u32, color: Pixel) {
        let x = self.to_screen_x(cx);
        let y = self.to_screen_y(cy);
        let r = (radius as i32 * self.zoom / 100).max(1) as u32;
        self.ui.fb.fill_circle_aa(x, y, r, color);
    }
}
