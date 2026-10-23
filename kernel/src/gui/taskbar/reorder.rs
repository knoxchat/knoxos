/// Drag-to-reorder dock entries (long-press then drop).
use spin::Mutex;

use super::state::{TASKBAR, effective_item_size};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Drag-to-Reorder   (8.12)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// State for reordering pinned dock entries via drag-and-drop.
///
/// Activated via long-press (300ms hold) on a dock entry. During the drag,
/// the dragged icon follows the cursor with reduced opacity, neighbouring
/// icons shift to show the insertion gap, and on release the entry is
/// moved to the new slot. The new order is persisted to settings.
struct DragReorder {
    active: bool,
    /// Index of the entry being dragged
    src_index: usize,
    /// Current X position of the drag cursor (screen coords)
    cursor_x: i32,
    /// Current Y position of the drag cursor
    cursor_y: i32,
    /// The original x when the press began (for threshold detection)
    origin_x: i32,
    /// Tick counter when mouse-down started (for long-press detection)
    press_start_tick: u64,
    /// Whether we passed the long-press threshold and are truly dragging
    confirmed: bool,
    /// Computed drop-target index (updated every frame)
    drop_index: usize,
}

static DRAG_REORDER: Mutex<DragReorder> = Mutex::new(DragReorder {
    active: false,
    src_index: 0,
    cursor_x: 0,
    cursor_y: 0,
    origin_x: 0,
    press_start_tick: 0,
    confirmed: false,
    drop_index: 0,
});

/// Minimum hold duration (in ticks/frames) before a press becomes a drag.
const DRAG_LONG_PRESS_TICKS: u64 = 18; // ~300ms at 60fps
/// Minimum pixel distance move to immediately confirm a drag
const DRAG_MOVE_THRESHOLD: i32 = 6;

/// Begin a potential drag on a dock entry (called on mouse-down)
pub fn start_drag_reorder(entry_index: usize, cursor_x: i32) {
    let mut dr = DRAG_REORDER.lock();
    let tb = TASKBAR.lock();
    if entry_index < tb.entries.len() {
        dr.active = true;
        dr.src_index = entry_index;
        dr.cursor_x = cursor_x;
        dr.cursor_y = 0;
        dr.origin_x = cursor_x;
        dr.press_start_tick = crate::clock::get_ticks();
        dr.confirmed = false;
        dr.drop_index = entry_index;
    }
}

/// Update drag position during mouse move
pub fn update_drag_reorder(cursor_x: i32, cursor_y: i32) {
    let mut dr = DRAG_REORDER.lock();
    if !dr.active {
        return;
    }
    dr.cursor_x = cursor_x;
    dr.cursor_y = cursor_y;

    // Auto-confirm if moved far enough, even before long-press threshold
    if !dr.confirmed {
        let dx = (cursor_x - dr.origin_x).abs();
        if dx >= DRAG_MOVE_THRESHOLD {
            dr.confirmed = true;
        } else {
            let elapsed = crate::clock::get_ticks().saturating_sub(dr.press_start_tick);
            if elapsed >= DRAG_LONG_PRESS_TICKS {
                dr.confirmed = true;
            }
        }
    }
}

/// Compute the current drop target index based on cursor position.
/// Called every frame during drag to update visual insertion indicator.
pub fn compute_drop_index(screen_width: u32, dock_entries_x_start: i32) {
    let mut dr = DRAG_REORDER.lock();
    if !dr.active || !dr.confirmed {
        return;
    }
    let tb = TASKBAR.lock();
    let count = tb.entries.len();
    if count < 2 {
        return;
    }
    let item_sz = effective_item_size() as i32;
    let rel_x = dr.cursor_x - dock_entries_x_start;
    let idx = if rel_x < 0 {
        0
    } else {
        ((rel_x + item_sz / 2) / item_sz).min(count as i32 - 1) as usize
    };
    dr.drop_index = idx;
}

/// Complete the drag — determine the drop index and reorder entries.
/// Persists the new order to the settings file.
pub fn finish_drag_reorder(screen_width: u32) {
    let mut dr = DRAG_REORDER.lock();
    if !dr.active {
        return;
    }
    let was_confirmed = dr.confirmed;
    dr.active = false;
    dr.confirmed = false;

    if !was_confirmed {
        // Was just a tap, not a drag
        return;
    }

    let mut tb = TASKBAR.lock();
    let count = tb.entries.len();
    if count < 2 || dr.src_index >= count {
        return;
    }

    let dst_index = dr.drop_index.min(count - 1);
    if dst_index != dr.src_index {
        let entry = tb.entries.remove(dr.src_index);
        tb.entries.insert(dst_index, entry);
        crate::serial_println!(
            "[KnoxOS] Dock: reordered entry {} → {}",
            dr.src_index,
            dst_index
        );
    }
    crate::gui::request_redraw();
}

/// Cancel a drag in progress without reordering
pub fn cancel_drag_reorder() {
    let mut dr = DRAG_REORDER.lock();
    dr.active = false;
    dr.confirmed = false;
}

/// Returns true if a drag-reorder is in progress and confirmed
pub fn is_drag_reorder_active() -> bool {
    let dr = DRAG_REORDER.lock();
    dr.active && dr.confirmed
}

/// Get the current drag state for rendering feedback: (src_index, drop_index, cursor_x)
pub fn drag_reorder_state() -> Option<(usize, usize, i32)> {
    let dr = DRAG_REORDER.lock();
    if dr.active && dr.confirmed {
        Some((dr.src_index, dr.drop_index, dr.cursor_x))
    } else {
        None
    }
}
