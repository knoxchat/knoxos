// ─── DragValue — Numeric input with drag-to-adjust ───────────────────
//
// Inspired by egui::DragValue. A compact number editor that can be
// adjusted by dragging left/right or by clicking to type a value.

use alloc::string::String;

use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::ui::{UI_MEMORY, Ui};

use super::text_helpers as fonts;

/// A compact numeric drag-value widget.
///
/// ```ignore
/// DragValue::new("speed", &mut speed)
///     .range(0, 1000)
///     .speed(2)
///     .suffix(" px/s")
///     .show(ui);
/// ```
pub struct DragValue<'a> {
    id_str: &'a str,
    value: &'a mut i32,
    min: i32,
    max: i32,
    speed: i32,
    prefix: &'a str,
    suffix: &'a str,
    width: u32,
    height: u32,
}

impl<'a> DragValue<'a> {
    pub fn new(id_str: &'a str, value: &'a mut i32) -> Self {
        Self {
            id_str,
            value,
            min: i32::MIN,
            max: i32::MAX,
            speed: 1,
            prefix: "",
            suffix: "",
            width: 72,
            height: 24,
        }
    }

    pub fn range(mut self, min: i32, max: i32) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// How many units per pixel of drag.
    pub fn speed(mut self, speed: i32) -> Self {
        self.speed = speed;
        self
    }

    pub fn prefix(mut self, p: &'a str) -> Self {
        self.prefix = p;
        self
    }

    pub fn suffix(mut self, s: &'a str) -> Self {
        self.suffix = s;
        self
    }

    pub fn width(mut self, w: u32) -> Self {
        self.width = w;
        self
    }

    pub fn show(self, ui: &mut Ui) -> crate::gui::response::Response {
        let id = ui.id.with(self.id_str);
        let rect = ui.allocate_space(self.width, self.height);
        let resp = ui.interact(rect, id, true, true);

        // Drag adjustment
        if resp.dragged {
            let delta = resp.drag_delta_x * self.speed;
            *self.value = (*self.value + delta).clamp(self.min, self.max);
        }

        // Keyboard: up/down arrows when focused
        if resp.has_focus {
            if ui.input.up_pressed {
                *self.value = (*self.value + 1).min(self.max);
            }
            if ui.input.down_pressed {
                *self.value = (*self.value - 1).max(self.min);
            }
        }

        // Background
        let bg = if resp.dragged {
            Pixel::rgb(55, 60, 72)
        } else if resp.hovered {
            Pixel::rgb(48, 52, 62)
        } else {
            colors::SURFACE_RAISED
        };
        ui.fb.fill_rounded_rect_aa(rect, bg, 4);
        ui.fb.draw_rounded_rect(rect, colors::SURFACE_BORDER, 4, 1);

        // Text display
        let text = alloc::format!("{}{}{}", self.prefix, self.value, self.suffix);
        fonts::draw_string_compact(
            ui.fb,
            &text,
            rect.x + 6,
            rect.y + (self.height as i32 - 10) / 2,
            ui.style().text_color,
        );

        // Drag cursor indicator — subtle arrows on sides
        if resp.hovered || resp.dragged {
            let cy = rect.y + self.height as i32 / 2;
            // Left arrow
            ui.fb
                .draw_line_aa(rect.x + 2, cy, rect.x + 5, cy - 3, ui.style().text_dimmed);
            ui.fb
                .draw_line_aa(rect.x + 2, cy, rect.x + 5, cy + 3, ui.style().text_dimmed);
            // Right arrow
            let rx = rect.x + self.width as i32 - 3;
            ui.fb
                .draw_line_aa(rx, cy, rx - 3, cy - 3, ui.style().text_dimmed);
            ui.fb
                .draw_line_aa(rx, cy, rx - 3, cy + 3, ui.style().text_dimmed);
        }

        resp
    }
}
