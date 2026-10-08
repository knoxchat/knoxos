/// Explorer directory navigation (back / forward / up / path)
use alloc::string::String;

use crate::gui::window::{self, WindowId};

use super::path::{current_path, set_path};

// ═════════════════════════════════════════════════════════════════════════
// NAVIGATION
// ═════════════════════════════════════════════════════════════════════════

/// Navigate the explorer to a new directory path
pub fn navigate_to(wid: WindowId, path: &str) {
    // Verify it's a valid directory
    {
        let vfs = crate::vfs::VFS.lock();
        if let Some(ino) = vfs.resolve_path(path) {
            if let Some(inode) = vfs.get_inode(ino) {
                if inode.file_type != crate::vfs::FileType::Directory {
                    return; // Not a directory
                }
            }
        } else {
            return; // Path doesn't exist
        }
    }

    // Update history
    {
        let mut wm = window::WINDOW_MANAGER.lock();
        if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
            // Truncate forward history
            win.explorer_history.truncate(win.explorer_history_idx + 1);
            win.explorer_history.push(String::from(path));
            win.explorer_history_idx = win.explorer_history.len() - 1;
        }
    }

    set_path(wid, path);
}

/// Navigate back in history
pub fn navigate_back(wid: WindowId) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        if win.explorer_history_idx > 0 {
            win.explorer_history_idx -= 1;
            let path = win.explorer_history[win.explorer_history_idx].clone();
            drop(wm);
            set_path(wid, &path);
        }
    }
}

/// Navigate forward in history
pub fn navigate_forward(wid: WindowId) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        if win.explorer_history_idx + 1 < win.explorer_history.len() {
            win.explorer_history_idx += 1;
            let path = win.explorer_history[win.explorer_history_idx].clone();
            drop(wm);
            set_path(wid, &path);
        }
    }
}

/// Navigate up to parent directory
pub fn navigate_up(wid: WindowId) {
    let wm = window::WINDOW_MANAGER.lock();
    let cur = if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
        current_path(&win.title)
    } else {
        return;
    };
    drop(wm);

    if cur == "/" {
        return; // Already at root
    }

    let parent = if let Some(pos) = cur.rfind('/') {
        if pos == 0 {
            String::from("/")
        } else {
            String::from(&cur[..pos])
        }
    } else {
        String::from("/")
    };

    navigate_to(wid, &parent);
}
