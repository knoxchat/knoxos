// ─── DragAndDrop — Drag payload tracking system ─────────────────────
//
// Inspired by egui's DragAndDrop plugin. Provides a global drag payload
// system for transferring data between drag sources and drop zones.

use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::ui::Ui;

use super::text_helpers as fonts;

/// Global drag state — stores the current payload if a drag is in progress.
static DRAG_ACTIVE: AtomicBool = AtomicBool::new(false);

use spin::Mutex;

/// The payload being dragged (simplified as an i32 tag + optional string label).
pub struct DragPayload {
    pub tag: i32,
    pub label: String,
    pub source_id: Id,
}

static DRAG_PAYLOAD: Mutex<Option<DragPayload>> = Mutex::new(None);

/// Start a drag operation with a typed payload.
pub fn set_drag_payload(source_id: Id, tag: i32, label: &str) {
    let mut p = DRAG_PAYLOAD.lock();
    *p = Some(DragPayload {
        tag,
        label: String::from(label),
        source_id,
    });
    DRAG_ACTIVE.store(true, Ordering::Relaxed);
}

/// Check if there is currently a drag in progress.
pub fn has_drag_payload() -> bool {
    DRAG_ACTIVE.load(Ordering::Relaxed)
}

/// Peek at the current drag payload tag without consuming it.
pub fn peek_drag_tag() -> Option<i32> {
    let p = DRAG_PAYLOAD.lock();
    p.as_ref().map(|dp| dp.tag)
}

/// Consume the drag payload when it's released over a drop zone.
/// Returns `Some((tag, label))` if a payload was dropped.
pub fn take_drag_payload() -> Option<(i32, String)> {
    let mut p = DRAG_PAYLOAD.lock();
    if let Some(payload) = p.take() {
        DRAG_ACTIVE.store(false, Ordering::Relaxed);
        Some((payload.tag, payload.label))
    } else {
        None
    }
}

/// Cancel any active drag.
pub fn clear_drag() {
    let mut p = DRAG_PAYLOAD.lock();
    *p = None;
    DRAG_ACTIVE.store(false, Ordering::Relaxed);
}

/// Called each frame to automatically cancel drags when mouse is released.
pub fn update_drag(ui: &Ui) {
    if DRAG_ACTIVE.load(Ordering::Relaxed) {
        // Cancel on escape
        if ui.input.escape_pressed {
            clear_drag();
            return;
        }
        // Cancel on mouse release
        if ui.input.pointer_primary_released {
            // Don't clear — let the drop zone consume it first.
            // The drop zone calls take_drag_payload() to consume.
            // If nobody consumed it by next frame, it'll expire.
        }
        // If mouse is not down and we still have payload, clear it
        if !ui.input.pointer_primary_down && !ui.input.pointer_primary_released {
            clear_drag();
        }
    }
}

/// Make a widget a drag source. Returns true if drag started this frame.
pub fn drag_source(ui: &mut Ui, rect: Rect, id: Id, tag: i32, label: &str) -> bool {
    let resp = ui.interact(rect, id.with("drag_src"), true, true);
    if resp.drag_started {
        set_drag_payload(id, tag, label);
        return true;
    }
    // Draw drag indicator while dragging
    if resp.dragged && has_drag_payload() {
        // Ghost at cursor
        let gx = ui.input.pointer_x;
        let gy = ui.input.pointer_y;
        let ghost_w = (label.len() as u32 * 8) + 16;
        let ghost_rect = Rect::new(gx + 8, gy + 8, ghost_w, 22);
        ui.fb
            .fill_rounded_rect_aa(ghost_rect, Pixel::new(60, 65, 80, 200), 6);
        fonts::draw_string_compact(
            ui.fb,
            label,
            ghost_rect.x + 8,
            ghost_rect.y + 6,
            colors::WHITE,
        );
    }
    false
}

/// Result of a drop zone check.
pub struct DropResult {
    pub hovered: bool,
    pub dropped_tag: Option<i32>,
    pub dropped_label: Option<String>,
}

/// Make a rect a drop zone. Returns whether something was dropped on it.
pub fn drop_zone(ui: &mut Ui, rect: Rect, id: Id) -> DropResult {
    let resp = ui.interact(rect, id.with("drop_zone"), true, false);
    let mut result = DropResult {
        hovered: false,
        dropped_tag: None,
        dropped_label: None,
    };

    if has_drag_payload() && resp.hovered {
        result.hovered = true;
        // Highlight the drop zone
        ui.fb.draw_rounded_rect(rect, colors::ACCENT_PRIMARY, 4, 2);

        // Check for release
        if ui.input.pointer_primary_released {
            if let Some((tag, label)) = take_drag_payload() {
                result.dropped_tag = Some(tag);
                result.dropped_label = Some(label);
            }
        }
    }

    result
}
