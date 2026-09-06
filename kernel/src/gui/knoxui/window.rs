// ─── Window — Full floating window container ─────────────────────────
//
// Inspired by egui::Window. A complete floating window that composes
// Area (dragging), Frame (border/shadow), title bar (with close/collapse),
// Resize (handle), and ScrollArea (content scrolling).

use alloc::string::String;

use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::ui::{UI_MEMORY, Ui};

use super::text_helpers as fonts;

/// A complete floating window with title bar, close/collapse controls,
/// dragging, optional resize, and scrollable content.
///
/// ```ignore
/// let mut open = true;
/// Window::new("My Window")
///     .default_pos(100, 100)
///     .default_size(400, 300)
///     .closable(true)
///     .collapsible(true)
///     .show(ui, &mut open, |ui| {
///         ui.label("Hello from window!");
///     });
/// ```
pub struct Window<'a> {
    title: &'a str,
    default_x: i32,
    default_y: i32,
    default_w: u32,
    default_h: u32,
    min_w: u32,
    min_h: u32,
    closable: bool,
    collapsible: bool,
    resizable: bool,
    title_bar: bool,
    bg: Pixel,
    corner_radius: u32,
}

impl<'a> Window<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            default_x: 100,
            default_y: 100,
            default_w: 400,
            default_h: 300,
            min_w: 120,
            min_h: 80,
            closable: true,
            collapsible: true,
            resizable: true,
            title_bar: true,
            bg: colors::SURFACE_RAISED,
            corner_radius: 8,
        }
    }

    pub fn default_pos(mut self, x: i32, y: i32) -> Self {
        self.default_x = x;
        self.default_y = y;
        self
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

    pub fn closable(mut self, c: bool) -> Self {
        self.closable = c;
        self
    }

    pub fn collapsible(mut self, c: bool) -> Self {
        self.collapsible = c;
        self
    }

    pub fn resizable(mut self, r: bool) -> Self {
        self.resizable = r;
        self
    }

    pub fn title_bar(mut self, t: bool) -> Self {
        self.title_bar = t;
        self
    }

    pub fn bg(mut self, bg: Pixel) -> Self {
        self.bg = bg;
        self
    }

    /// Show the window. `open` is set to `false` when the close button is clicked.
    pub fn show(self, ui: &mut Ui, open: &mut bool, add_contents: impl FnOnce(&mut Ui)) {
        if !*open {
            return;
        }

        let id = Id::from_str(self.title);
        let pos_x_key = id.with("win_x");
        let pos_y_key = id.with("win_y");
        let size_w_key = id.with("win_w");
        let size_h_key = id.with("win_h");
        let collapsed_key = id.with("collapsed");

        let mut mem = UI_MEMORY.lock();
        let mut wx = mem.get_i32(pos_x_key, self.default_x);
        let mut wy = mem.get_i32(pos_y_key, self.default_y);
        let mut ww = mem.get_i32(size_w_key, self.default_w as i32) as u32;
        let mut wh = mem.get_i32(size_h_key, self.default_h as i32) as u32;
        let mut collapsed = mem.get_bool(collapsed_key, false);
        drop(mem);

        let title_h: u32 = if self.title_bar { 30 } else { 0 };
        let display_h = if collapsed { title_h } else { wh };

        let win_rect = Rect::new(wx, wy, ww, display_h);

        // ── Shadow ──
        ui.fb.fill_rounded_rect_aa(
            Rect::new(wx + 3, wy + 3, ww, display_h),
            Pixel::new(0, 0, 0, 60),
            self.corner_radius,
        );

        // ── Background ──
        ui.fb
            .fill_rounded_rect_aa(win_rect, self.bg, self.corner_radius);
        ui.fb
            .draw_rounded_rect(win_rect, colors::SURFACE_BORDER, self.corner_radius, 1);

        // ── Title bar ──
        if self.title_bar {
            let title_rect = Rect::new(wx, wy, ww, title_h);
            let title_id = id.with("titlebar");
            let title_resp = ui.interact(title_rect, title_id, true, true);

            // Title bar background — slightly different shade
            ui.fb.fill_rounded_rect_aa(
                Rect::new(wx, wy, ww, title_h),
                Pixel::new(255, 255, 255, 5),
                self.corner_radius,
            );

            // Drag to move
            if title_resp.dragged {
                wx += title_resp.drag_delta_x;
                wy += title_resp.drag_delta_y;
            }

            // Double-click to collapse
            if self.collapsible && title_resp.double_clicked {
                collapsed = !collapsed;
            }

            // Title text
            let mut title_x = wx + 10;

            // Collapse arrow
            if self.collapsible {
                let arrow_y = wy + title_h as i32 / 2;
                if collapsed {
                    // Right-pointing triangle ▶
                    ui.fb.draw_line_aa(
                        title_x,
                        arrow_y - 4,
                        title_x + 6,
                        arrow_y,
                        ui.style().text_dimmed,
                    );
                    ui.fb.draw_line_aa(
                        title_x + 6,
                        arrow_y,
                        title_x,
                        arrow_y + 4,
                        ui.style().text_dimmed,
                    );
                } else {
                    // Down-pointing triangle ▼
                    ui.fb.draw_line_aa(
                        title_x - 2,
                        arrow_y - 3,
                        title_x + 4,
                        arrow_y + 3,
                        ui.style().text_dimmed,
                    );
                    ui.fb.draw_line_aa(
                        title_x + 4,
                        arrow_y + 3,
                        title_x + 10,
                        arrow_y - 3,
                        ui.style().text_dimmed,
                    );
                }
                title_x += 14;
            }

            fonts::draw_string_bold_compact(
                ui.fb,
                self.title,
                title_x,
                wy + (title_h as i32 - 10) / 2,
                colors::TEXT_PRIMARY,
            );

            // ── Window buttons (right side) ──
            let mut btn_x = wx + ww as i32 - 10;

            // Close button
            if self.closable {
                btn_x -= 18;
                let close_rect = Rect::new(btn_x, wy + 6, 18, 18);
                let close_id = id.with("close");
                let close_resp = ui.interact(close_rect, close_id, true, false);
                if close_resp.hovered {
                    ui.fb
                        .fill_rounded_rect_aa(close_rect, Pixel::rgb(200, 60, 60), 4);
                }
                // × cross
                let cx = btn_x + 4;
                let cy = wy + 10;
                let cross_color = if close_resp.hovered {
                    colors::WHITE
                } else {
                    ui.style().text_dimmed
                };
                ui.fb.draw_line_aa(cx, cy, cx + 10, cy + 10, cross_color);
                ui.fb.draw_line_aa(cx + 10, cy, cx, cy + 10, cross_color);

                if close_resp.clicked() {
                    *open = false;
                }
                btn_x -= 4;
            }

            // Collapse button
            if self.collapsible {
                btn_x -= 18;
                let coll_rect = Rect::new(btn_x, wy + 6, 18, 18);
                let coll_id = id.with("coll_btn");
                let coll_resp = ui.interact(coll_rect, coll_id, true, false);
                if coll_resp.hovered {
                    ui.fb
                        .fill_rounded_rect_aa(coll_rect, Pixel::new(255, 255, 255, 20), 4);
                }
                // Minimize dash —
                let dash_y = wy + 14;
                let dash_color = if coll_resp.hovered {
                    colors::WHITE
                } else {
                    ui.style().text_dimmed
                };
                ui.fb.draw_hline(btn_x + 4, dash_y, 10, dash_color);

                if coll_resp.clicked() {
                    collapsed = !collapsed;
                }
            }

            // Separator below title
            if !collapsed {
                ui.fb
                    .draw_hline(wx + 1, wy + title_h as i32, ww - 2, colors::SURFACE_BORDER);
            }
        }

        // ── Content area ──
        if !collapsed {
            let content_rect = Rect::new(
                wx + 1,
                wy + title_h as i32 + 1,
                ww - 2,
                wh.saturating_sub(title_h + 2),
            );

            ui.fb.push_clip(content_rect);

            // Set up UI region for content
            let old_cursor_x = ui.region.cursor_x;
            let old_cursor_y = ui.region.cursor_y;
            let old_max_rect = ui.region.max_rect;

            ui.region.cursor_x = content_rect.x + 8;
            ui.region.cursor_y = content_rect.y + 4;
            ui.region.max_rect = Rect::new(
                content_rect.x + 8,
                content_rect.y + 4,
                content_rect.width.saturating_sub(16),
                content_rect.height.saturating_sub(8),
            );

            add_contents(ui);

            ui.region.cursor_x = old_cursor_x;
            ui.region.cursor_y = old_cursor_y;
            ui.region.max_rect = old_max_rect;

            ui.fb.pop_clip();

            // ── Resize handle ──
            if self.resizable {
                let grip_size = 12u32;
                let grip_rect = Rect::new(
                    wx + ww as i32 - grip_size as i32,
                    wy + wh as i32 - grip_size as i32,
                    grip_size,
                    grip_size,
                );
                let grip_id = id.with("resize_grip");
                let grip_resp = ui.interact(grip_rect, grip_id, true, true);

                if grip_resp.dragged {
                    let new_w = (ww as i32 + grip_resp.drag_delta_x).max(self.min_w as i32) as u32;
                    let new_h = (wh as i32 + grip_resp.drag_delta_y).max(self.min_h as i32) as u32;
                    ww = new_w;
                    wh = new_h;
                }

                // Draw grip lines
                let gc = if grip_resp.hovered || grip_resp.dragged {
                    Pixel::new(255, 255, 255, 80)
                } else {
                    Pixel::new(255, 255, 255, 30)
                };
                let gx = wx + ww as i32 - 4;
                let gy = wy + wh as i32 - 4;
                ui.fb.draw_line_aa(gx - 8, gy, gx, gy - 8, gc);
                ui.fb.draw_line_aa(gx - 4, gy, gx, gy - 4, gc);
            }
        }

        // ── Persist state ──
        let mut mem = UI_MEMORY.lock();
        mem.set_i32(pos_x_key, wx);
        mem.set_i32(pos_y_key, wy);
        mem.set_i32(size_w_key, ww as i32);
        mem.set_i32(size_h_key, wh as i32);
        mem.set_bool(collapsed_key, collapsed);
    }
}
