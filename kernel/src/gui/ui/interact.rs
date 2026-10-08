use crate::gui::framebuffer::Rect;
use crate::gui::id::Id;
use crate::gui::response::Response;

use super::{UI_MEMORY, Ui};

impl<'a> Ui<'a> {
    // ── Allocation & interaction ─────────────────────────────────

    /// Allocate space for a widget of given size. Returns the assigned rect.
    pub fn allocate_space(&mut self, width: u32, height: u32) -> Rect {
        self.layout
            .allocate(&mut self.region, width, height, &self.style.spacing)
    }

    /// Check interaction (hover, click, drag) for a widget at `rect`.
    pub fn interact(&self, rect: Rect, id: Id, sense_click: bool, sense_drag: bool) -> Response {
        let mut resp = Response::none(id, rect);
        resp.enabled = self.enabled;

        if !self.enabled {
            return resp;
        }

        // Check if pointer is inside the widget rect AND the clip rect
        let px = self.input.pointer_x;
        let py = self.input.pointer_y;
        let in_rect = rect.contains(px, py);
        let in_clip = self.clip_rect.contains(px, py);
        let pointer_over = in_rect && in_clip;

        resp.hovered = pointer_over;

        let mem = UI_MEMORY.lock();
        let is_active = mem.active_id == Some(id);

        if sense_click {
            // Click detection: pointer released while hovering
            if pointer_over && self.input.pointer_primary_released && is_active {
                resp.clicked = true;
            }
            if pointer_over && self.input.double_click && self.input.pointer_primary_pressed {
                resp.double_clicked = true;
            }
            // Right-click
            if pointer_over && self.input.pointer_secondary_released {
                resp.secondary_clicked = true;
            }
        }

        if (sense_click || sense_drag) && is_active && self.input.pointer_primary_down {
            resp.is_pointer_button_down_on = true;
        }

        if sense_drag {
            if is_active && self.input.pointer_primary_down {
                if mem.active_is_dragging {
                    resp.dragged = true;
                    resp.drag_delta_x = self.input.pointer_x - mem.prev_pointer_x;
                    resp.drag_delta_y = self.input.pointer_y - mem.prev_pointer_y;
                }
            }
            if is_active && self.input.pointer_primary_released && mem.active_is_dragging {
                resp.drag_stopped = true;
            }
        }

        // Focus tracking
        if let Some(fid) = mem.focused_id {
            if fid == id {
                resp.has_focus = true;
                if mem.prev_focused_id != Some(id) {
                    resp.gained_focus = true;
                }
            }
        }
        if mem.prev_focused_id == Some(id) && mem.focused_id != Some(id) {
            resp.lost_focus = true;
        }

        drop(mem);

        // Update active_id on press
        if pointer_over && self.input.pointer_primary_pressed && (sense_click || sense_drag) {
            let mut mem = UI_MEMORY.lock();
            mem.active_id = Some(id);
            mem.active_start_x = px;
            mem.active_start_y = py;
            mem.active_is_dragging = false;
        }

        // Detect drag start (moved > 3px threshold)
        if self.input.pointer_primary_down {
            let mut mem = UI_MEMORY.lock();
            if mem.active_id == Some(id) && !mem.active_is_dragging && sense_drag {
                let dx = self.input.pointer_x - mem.active_start_x;
                let dy = self.input.pointer_y - mem.active_start_y;
                if dx * dx + dy * dy > 9 {
                    mem.active_is_dragging = true;
                    resp.drag_started = true;
                }
            }
        }

        resp
    }
}
