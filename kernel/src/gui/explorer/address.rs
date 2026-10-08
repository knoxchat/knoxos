/// Address bar editing
use crate::gui::window::{self, WindowId};

use super::nav::navigate_to;
use super::path::current_path;

// ═════════════════════════════════════════════════════════════════════════
// ADDRESS BAR
// ═════════════════════════════════════════════════════════════════════════

/// Start editing the address bar
pub fn start_address_edit(wid: WindowId) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        let path = current_path(&win.title);
        win.explorer_editing_path = true;
        win.explorer_path_cursor = path.len();
        win.explorer_path_buf = path;
    }
}

/// Cancel address bar editing
pub fn cancel_address_edit(wid: WindowId) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.explorer_editing_path = false;
        win.explorer_path_buf.clear();
    }
}

/// Confirm address bar navigation (Enter key)
pub fn confirm_address_edit(wid: WindowId) {
    let path = {
        let wm = window::WINDOW_MANAGER.lock();
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            win.explorer_path_buf.clone()
        } else {
            return;
        }
    };

    // Cancel editing mode first
    cancel_address_edit(wid);

    // Navigate to the entered path
    navigate_to(wid, &path);
}

/// Handle a character typed into the address bar
pub fn address_bar_char(wid: WindowId, ch: char) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        if win.explorer_editing_path {
            let pos = win.explorer_path_cursor.min(win.explorer_path_buf.len());
            win.explorer_path_buf.insert(pos, ch);
            win.explorer_path_cursor = pos + 1;
        }
    }
}

/// Handle backspace in the address bar
pub fn address_bar_backspace(wid: WindowId) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        if win.explorer_editing_path && win.explorer_path_cursor > 0 {
            win.explorer_path_cursor -= 1;
            let pos = win.explorer_path_cursor;
            if pos < win.explorer_path_buf.len() {
                win.explorer_path_buf.remove(pos);
            }
        }
    }
}
