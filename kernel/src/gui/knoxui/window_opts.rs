/// WindowOptions — Extended window chrome and behavior options
///
/// Provides fine-grained control over window appearance and behavior,
/// inspired by egui::Window. Includes title bar style, resize edges,
/// close/minimize/maximize buttons, drag constraints, and more.
///
/// ```ignore
/// WindowOptions::new("Settings")
///     .resizable(true)
///     .collapsible(true)
///     .min_size(300, 200)
///     .title_bar_height(32)
///     .show(ui, |ui| {
///         ui.label("Window content");
///     });
/// ```
use alloc::string::String;

use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::{InnerResponse, Response};
use crate::gui::ui::{UI_MEMORY, Ui};

/// Window behavior and appearance configuration.
pub struct WindowOptions {
    id: Id,
    title: String,
    resizable: bool,
    movable: bool,
    collapsible: bool,
    closable: bool,
    minimizable: bool,
    maximizable: bool,
    title_bar: bool,
    title_bar_height: u32,
    min_width: u32,
    min_height: u32,
    max_width: u32,
    max_height: u32,
    default_width: u32,
    default_height: u32,
    default_x: i32,
    default_y: i32,
    bg_color: Pixel,
    title_bg: Pixel,
    border_color: Pixel,
    corner_radius: u32,
    shadow: bool,
    always_on_top: bool,
}

impl WindowOptions {
    pub fn new(title: &str) -> Self {
        Self {
            id: Id::from_str(title),
            title: String::from(title),
            resizable: true,
            movable: true,
            collapsible: true,
            closable: true,
            minimizable: true,
            maximizable: true,
            title_bar: true,
            title_bar_height: 30,
            min_width: 150,
            min_height: 100,
            max_width: 4000,
            max_height: 3000,
            default_width: 400,
            default_height: 300,
            default_x: 100,
            default_y: 100,
            bg_color: Pixel::rgb(20, 22, 28),
            title_bg: Pixel::rgb(14, 18, 28),
            border_color: Pixel::new(80, 160, 255, 80),
            corner_radius: 10,
            shadow: true,
            always_on_top: false,
        }
    }

    pub fn resizable(mut self, v: bool) -> Self {
        self.resizable = v;
        self
    }
    pub fn movable(mut self, v: bool) -> Self {
        self.movable = v;
        self
    }
    pub fn collapsible(mut self, v: bool) -> Self {
        self.collapsible = v;
        self
    }
    pub fn closable(mut self, v: bool) -> Self {
        self.closable = v;
        self
    }
    pub fn minimizable(mut self, v: bool) -> Self {
        self.minimizable = v;
        self
    }
    pub fn maximizable(mut self, v: bool) -> Self {
        self.maximizable = v;
        self
    }
    pub fn title_bar(mut self, v: bool) -> Self {
        self.title_bar = v;
        self
    }
    pub fn title_bar_height(mut self, h: u32) -> Self {
        self.title_bar_height = h;
        self
    }
    pub fn min_size(mut self, w: u32, h: u32) -> Self {
        self.min_width = w;
        self.min_height = h;
        self
    }
    pub fn max_size(mut self, w: u32, h: u32) -> Self {
        self.max_width = w;
        self.max_height = h;
        self
    }
    pub fn default_size(mut self, w: u32, h: u32) -> Self {
        self.default_width = w;
        self.default_height = h;
        self
    }
    pub fn default_pos(mut self, x: i32, y: i32) -> Self {
        self.default_x = x;
        self.default_y = y;
        self
    }
    pub fn bg_color(mut self, c: Pixel) -> Self {
        self.bg_color = c;
        self
    }
    pub fn shadow(mut self, v: bool) -> Self {
        self.shadow = v;
        self
    }
    pub fn always_on_top(mut self, v: bool) -> Self {
        self.always_on_top = v;
        self
    }
    pub fn corner_radius(mut self, r: u32) -> Self {
        self.corner_radius = r;
        self
    }

    /// Show the window and run the content closure.
    /// Returns the response including close/minimize/maximize button clicks.
    pub fn show<'a, R>(
        self,
        ui: &mut Ui<'a>,
        open: &mut bool,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> Option<WindowResponse<R>> {
        if !*open {
            return None;
        }

        // Load persisted position/size
        let pos_id = self.id.with("pos");
        let size_id = self.id.with("size");
        let collapsed_id = self.id.with("collapsed");

        let (mut x, mut y) = {
            let mem = UI_MEMORY.lock();
            let px = mem.get_i32(pos_id.with("x"), self.default_x);
            let py = mem.get_i32(pos_id.with("y"), self.default_y);
            (px, py)
        };
        let (mut w, mut h) = {
            let mem = UI_MEMORY.lock();
            let sw = mem.get_i32(size_id.with("w"), self.default_width as i32) as u32;
            let sh = mem.get_i32(size_id.with("h"), self.default_height as i32) as u32;
            (
                sw.clamp(self.min_width, self.max_width),
                sh.clamp(self.min_height, self.max_height),
            )
        };
        let collapsed = {
            let mem = UI_MEMORY.lock();
            mem.get_bool(collapsed_id, false)
        };

        let display_h = if collapsed { self.title_bar_height } else { h };
        let window_rect = Rect::new(x, y, w, display_h);

        // Shadow
        if self.shadow {
            ui.fb.fill_rounded_rect_aa(
                Rect::new(x + 4, y + 4, w, display_h),
                Pixel::new(0, 0, 0, 80),
                self.corner_radius + 2,
            );
        }

        // Window background
        ui.fb
            .fill_rounded_rect_aa(window_rect, self.bg_color, self.corner_radius);
        ui.fb
            .draw_rounded_rect(window_rect, self.border_color, self.corner_radius, 1);

        let mut close_clicked = false;
        let mut minimize_clicked = false;
        let mut maximize_clicked = false;

        // Title bar
        if self.title_bar {
            let tb_rect = Rect::new(x, y, w, self.title_bar_height);
            ui.fb.fill_rounded_rect_aa(
                Rect::new(x, y, w, self.title_bar_height + self.corner_radius),
                self.title_bg,
                self.corner_radius,
            );
            // Mask the bottom corners to be square (they're inside the window)
            ui.fb.fill_rect(
                Rect::new(x, y + self.title_bar_height as i32 - 2, w, 2),
                self.title_bg,
            );

            // Title bar drag (for moving)
            if self.movable {
                let drag_id = self.id.with("title_drag");
                let drag_resp = ui.interact(tb_rect, drag_id, false, true);
                if drag_resp.dragged {
                    x += drag_resp.drag_delta_x;
                    y += drag_resp.drag_delta_y;
                    let mut mem = UI_MEMORY.lock();
                    mem.set_i32(pos_id.with("x"), x);
                    mem.set_i32(pos_id.with("y"), y);
                }
            }

            // Title text
            fonts::draw_string_compact(
                ui.fb,
                &self.title,
                x + 12,
                y + (self.title_bar_height as i32 - 12) / 2,
                colors::WINDOW_TITLE_TEXT,
            );

            // Window control buttons (macOS-style pills, right-aligned)
            let mut btn_x = x + w as i32 - 16;
            let btn_y = y + self.title_bar_height as i32 / 2;

            // Close button
            if self.closable {
                let close_rect = Rect::new(btn_x - 6, btn_y - 6, 12, 12);
                let close_id = self.id.with("close");
                let close_resp = ui.interact(close_rect, close_id, true, false);
                let pill_color = if close_resp.hovered {
                    colors::PILL_CLOSE
                } else {
                    Pixel::rgb(80, 80, 90)
                };
                ui.fb.fill_circle_aa(btn_x, btn_y, 6, pill_color);
                if close_resp.hovered {
                    // X icon
                    ui.fb
                        .draw_line_aa(btn_x - 3, btn_y - 3, btn_x + 3, btn_y + 3, colors::WHITE);
                    ui.fb
                        .draw_line_aa(btn_x - 3, btn_y + 3, btn_x + 3, btn_y - 3, colors::WHITE);
                }
                if close_resp.clicked() {
                    close_clicked = true;
                    *open = false;
                }
                btn_x -= 20;
            }

            // Maximize button
            if self.maximizable {
                let max_rect = Rect::new(btn_x - 6, btn_y - 6, 12, 12);
                let max_id = self.id.with("maximize");
                let max_resp = ui.interact(max_rect, max_id, true, false);
                let pill_color = if max_resp.hovered {
                    colors::PILL_MAXIMIZE
                } else {
                    Pixel::rgb(80, 80, 90)
                };
                ui.fb.fill_circle_aa(btn_x, btn_y, 6, pill_color);
                if max_resp.clicked() {
                    maximize_clicked = true;
                }
                btn_x -= 20;
            }

            // Minimize button
            if self.minimizable {
                let min_rect = Rect::new(btn_x - 6, btn_y - 6, 12, 12);
                let min_id = self.id.with("minimize");
                let min_resp = ui.interact(min_rect, min_id, true, false);
                let pill_color = if min_resp.hovered {
                    colors::PILL_MINIMIZE
                } else {
                    Pixel::rgb(80, 80, 90)
                };
                ui.fb.fill_circle_aa(btn_x, btn_y, 6, pill_color);
                if min_resp.clicked() {
                    minimize_clicked = true;
                }
                btn_x -= 20;
            }

            // Collapse toggle (double-click title bar)
            if self.collapsible {
                let tb_id = self.id.with("collapse_dbl");
                let tb_resp = ui.interact(tb_rect, tb_id, true, false);
                if tb_resp.double_clicked() {
                    let mut mem = UI_MEMORY.lock();
                    mem.set_bool(collapsed_id, !collapsed);
                }
            }
        }

        // Content area
        let inner = if !collapsed {
            let content_y = y + if self.title_bar {
                self.title_bar_height as i32
            } else {
                0
            };
            let content_h = (display_h as i32
                - if self.title_bar {
                    self.title_bar_height as i32
                } else {
                    0
                })
            .max(0) as u32;
            let content_rect = Rect::new(
                x + 8,
                content_y + 4,
                (w as i32 - 16).max(0) as u32,
                (content_h as i32 - 8).max(0) as u32,
            );

            let saved_region = ui.region;
            let saved_layout = ui.layout;
            let saved_clip = ui.clip_rect;
            let saved_id = ui.id;

            ui.region = Region::from_max_rect(&Layout::top_down(Align::LEFT), content_rect);
            ui.layout = Layout::top_down(Align::LEFT);
            ui.clip_rect = Rect::new(x, content_y, w, content_h);
            ui.id = self.id.with("content");
            ui.fb.push_clip(Rect::new(x, content_y, w, content_h));

            let r = add_contents(ui);

            ui.fb.pop_clip();
            ui.region = saved_region;
            ui.layout = saved_layout;
            ui.clip_rect = saved_clip;
            ui.id = saved_id;

            Some(r)
        } else {
            None
        };

        // Resize handles (edges and corners)
        if self.resizable && !collapsed {
            let edge = 5i32;

            // Bottom-right corner
            let br_rect = Rect::new(
                x + w as i32 - edge,
                y + display_h as i32 - edge,
                edge as u32 * 2,
                edge as u32 * 2,
            );
            let br_id = self.id.with("resize_br");
            let br_resp = ui.interact(br_rect, br_id, false, true);
            if br_resp.dragged {
                w = (w as i32 + br_resp.drag_delta_x)
                    .clamp(self.min_width as i32, self.max_width as i32) as u32;
                h = (h as i32 + br_resp.drag_delta_y)
                    .clamp(self.min_height as i32, self.max_height as i32)
                    as u32;
                let mut mem = UI_MEMORY.lock();
                mem.set_i32(size_id.with("w"), w as i32);
                mem.set_i32(size_id.with("h"), h as i32);
            }

            // Resize grip visual (bottom-right corner dots)
            let gx = x + w as i32 - 14;
            let gy = y + display_h as i32 - 14;
            for i in 0..3 {
                for j in 0..3 {
                    if i + j >= 2 {
                        ui.fb
                            .fill_circle_aa(gx + i * 5, gy + j * 5, 1, Pixel::rgb(100, 110, 130));
                    }
                }
            }
        }

        Some(WindowResponse {
            inner,
            close_clicked,
            minimize_clicked,
            maximize_clicked,
            window_rect,
        })
    }
}

/// Result of showing a window.
pub struct WindowResponse<R> {
    pub inner: Option<R>,
    pub close_clicked: bool,
    pub minimize_clicked: bool,
    pub maximize_clicked: bool,
    pub window_rect: Rect,
}
