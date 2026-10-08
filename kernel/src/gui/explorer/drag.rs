/// File drag-and-drop (9.27 & 9.70)
use crate::gui::window::{self, WindowContentType, WindowId};

use super::entries::{read_entries, sort_entries};
use super::path::current_path;

// ═══════════════════════════════════════════════════════════════════════════
// FILE DRAG-AND-DROP (9.27 & 9.70)
// ═══════════════════════════════════════════════════════════════════════════

/// Start dragging the currently selected file/folder in the explorer.
/// Called when user begins a drag gesture on an entry.
pub fn start_file_drag(wid: WindowId) {
    let wm = window::WINDOW_MANAGER.lock();
    let win = match wm.windows.iter().find(|w| w.id == wid) {
        Some(w) => w,
        None => return,
    };
    let selected = win.explorer_selected;
    if selected < 0 {
        return;
    }

    let cur_path = current_path(&win.title);
    let show_hidden = win.explorer_show_hidden;
    let sort = win.explorer_sort;
    let sort_asc = win.explorer_sort_asc;
    drop(wm);

    let mut entries = read_entries(&cur_path, show_hidden);
    sort_entries(&mut entries, sort, sort_asc);

    if let Some(entry) = entries.get(selected as usize) {
        let full_path = if cur_path == "/" {
            alloc::format!("/{}", entry.name)
        } else {
            alloc::format!("{}/{}", cur_path, entry.name)
        };

        // Store the drag path in a global so the drop handler can pick it up
        let mut drag = EXPLORER_DRAG.lock();
        drag.active = true;
        drag.source_window = wid;
        drag.source_path = full_path;
        drag.entry_index = selected as usize;
    }
}

/// Accept a file drop onto this explorer window.
/// Moves/copies the file from the source path to this window's current directory.
pub fn accept_file_drop(wid: WindowId) -> bool {
    let drag = EXPLORER_DRAG.lock();
    if !drag.active || drag.source_window == wid {
        return false; // Can't drop onto same window
    }
    let src_path = drag.source_path.clone();
    drop(drag);

    // Get destination directory from window title
    let wm = window::WINDOW_MANAGER.lock();
    let win = match wm.windows.iter().find(|w| w.id == wid) {
        Some(w) => w,
        None => return false,
    };
    if win.content_type != WindowContentType::FileExplorer {
        return false;
    }
    let dst_dir = current_path(&win.title);
    drop(wm);

    // Move the file to the destination directory
    let name = src_path.rsplit('/').next().unwrap_or("file");
    let dst_path = alloc::format!("{}/{}", dst_dir.trim_end_matches('/'), name);

    match crate::vfs::move_file_dispatch(&src_path, &dst_path) {
        Ok(()) => {
            crate::gui::notifications::info(
                "File Manager",
                &alloc::format!("Moved {} to {}", name, dst_dir),
            );
            // Clear drag state
            let mut drag = EXPLORER_DRAG.lock();
            drag.active = false;
            crate::gui::request_redraw();
            true
        }
        Err(_) => {
            crate::gui::notifications::error(
                "File Manager",
                &alloc::format!("Failed to move {} to {}", name, dst_dir),
            );
            let mut drag = EXPLORER_DRAG.lock();
            drag.active = false;
            false
        }
    }
}

/// Cancel any active file drag
pub fn cancel_file_drag() {
    let mut drag = EXPLORER_DRAG.lock();
    drag.active = false;
}

/// Check if a file drag is in progress from any explorer window
pub fn is_file_drag_active() -> bool {
    EXPLORER_DRAG.lock().active
}

/// Get the currently dragged file path (if drag is active)
pub fn dragged_file_path() -> Option<alloc::string::String> {
    let drag = EXPLORER_DRAG.lock();
    if drag.active {
        Some(drag.source_path.clone())
    } else {
        None
    }
}

/// Explorer file drag state
struct ExplorerDragState {
    active: bool,
    source_window: WindowId,
    source_path: alloc::string::String,
    entry_index: usize,
}

static EXPLORER_DRAG: spin::Mutex<ExplorerDragState> = spin::Mutex::new(ExplorerDragState {
    active: false,
    source_window: 0,
    source_path: alloc::string::String::new(),
    entry_index: 0,
});
