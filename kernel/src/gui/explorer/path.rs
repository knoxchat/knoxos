/// Path helpers for explorer window titles
use alloc::format;
use alloc::string::String;

use crate::gui::window::{self, WindowId};

/// Get the current path for a file explorer window (parsed from title)
pub fn current_path(win_title: &str) -> String {
    if win_title.contains(" - ") {
        if let Some(p) = win_title.split(" - ").nth(1) {
            String::from(p)
        } else {
            String::from("/home/user")
        }
    } else {
        String::from("/home/user")
    }
}

/// Set the explorer's current path (updates the window title)
pub(super) fn set_path(wid: WindowId, path: &str) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        let display = if path == "/home/user" {
            String::from("Files")
        } else {
            format!("Files - {}", path)
        };
        win.title = display;
        win.scroll_y = 0;
        win.explorer_selected = -1;
        win.explorer_renaming = None;
    }
}
