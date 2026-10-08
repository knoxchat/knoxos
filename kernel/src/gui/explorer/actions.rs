/// Context menus, file operations, and rename
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::gui::window::{
    self, ExplorerAction, ExplorerContextMenu, ExplorerCtxItem, ExplorerSort, WindowContentType,
    WindowId,
};

use super::entries::{read_entries, sort_entries};
use super::nav::navigate_to;
use super::path::current_path;

// ═════════════════════════════════════════════════════════════════════════
// CONTEXT MENU
// ═════════════════════════════════════════════════════════════════════════

/// Build a context menu for right-clicking on a file entry or empty space
pub fn build_context_menu(
    x: i32,
    y: i32,
    target_index: Option<usize>,
    target_path: String,
    has_clipboard: bool,
) -> ExplorerContextMenu {
    let mut items = Vec::new();

    if target_index.is_some() {
        // Right-clicked on a file/folder
        items.push(ExplorerCtxItem {
            label: String::from("Open"),
            action: ExplorerAction::Open,
            enabled: true,
        });
        items.push(ExplorerCtxItem {
            label: String::from("Copy"),
            action: ExplorerAction::Copy,
            enabled: true,
        });
        items.push(ExplorerCtxItem {
            label: String::from("Cut"),
            action: ExplorerAction::Cut,
            enabled: true,
        });
        items.push(ExplorerCtxItem {
            label: String::from("Paste"),
            action: ExplorerAction::Paste,
            enabled: has_clipboard,
        });
        items.push(ExplorerCtxItem {
            label: String::from("Rename"),
            action: ExplorerAction::Rename,
            enabled: true,
        });
        items.push(ExplorerCtxItem {
            label: String::from("Delete"),
            action: ExplorerAction::Delete,
            enabled: true,
        });
        items.push(ExplorerCtxItem {
            label: String::from("Properties"),
            action: ExplorerAction::Properties,
            enabled: true,
        });
    } else {
        // Right-clicked on empty space
        items.push(ExplorerCtxItem {
            label: String::from("New Folder"),
            action: ExplorerAction::NewFolder,
            enabled: true,
        });
        items.push(ExplorerCtxItem {
            label: String::from("New File"),
            action: ExplorerAction::NewFile,
            enabled: true,
        });
        items.push(ExplorerCtxItem {
            label: String::from("Paste"),
            action: ExplorerAction::Paste,
            enabled: has_clipboard,
        });
        items.push(ExplorerCtxItem {
            label: String::from("Toggle Hidden Files"),
            action: ExplorerAction::ToggleHidden,
            enabled: true,
        });
    }

    ExplorerContextMenu {
        x,
        y,
        target_index,
        target_path,
        items,
    }
}

// ═════════════════════════════════════════════════════════════════════════
// FILE OPERATIONS
// ═════════════════════════════════════════════════════════════════════════

/// Execute a file operation from the context menu
pub fn execute_action(wid: WindowId, action: ExplorerAction) {
    let (cur_path, target_path, target_idx) = {
        let wm = window::WINDOW_MANAGER.lock();
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            let cp = current_path(&win.title);
            let (tp, ti) = if let Some(ctx) = &win.explorer_ctx_menu {
                (ctx.target_path.clone(), ctx.target_index)
            } else {
                (String::new(), None)
            };
            (cp, tp, ti)
        } else {
            return;
        }
    };

    match action {
        ExplorerAction::Open => {
            if !target_path.is_empty() {
                // Check if it's a directory
                let is_dir = {
                    let vfs = crate::vfs::VFS.lock();
                    vfs.resolve_path(&target_path)
                        .and_then(|ino| vfs.get_inode(ino))
                        .map(|i| i.file_type == crate::vfs::FileType::Directory)
                        .unwrap_or(false)
                };
                if is_dir {
                    navigate_to(wid, &target_path);
                } else {
                    // Open file in text editor
                    open_file_in_editor(&target_path);
                }
            }
        }
        ExplorerAction::Copy => {
            if !target_path.is_empty() {
                crate::clipboard::copy_files(&[target_path.as_str()]);
            }
        }
        ExplorerAction::Cut => {
            if !target_path.is_empty() {
                crate::clipboard::cut_files(&[target_path.as_str()]);
            }
        }
        ExplorerAction::Paste => {
            if let Some((paths, is_cut)) = crate::clipboard::paste_files() {
                for src_path in &paths {
                    let file_name = src_path.rsplit('/').next().unwrap_or(src_path);
                    let dest = if cur_path == "/" {
                        format!("/{}", file_name)
                    } else {
                        format!("{}/{}", cur_path, file_name)
                    };

                    if is_cut {
                        // Move: rename in VFS
                        let mut vfs = crate::vfs::VFS.lock();
                        let _ = vfs.rename(src_path, &dest);
                    } else {
                        // Copy: read data then create at destination
                        let data = {
                            let vfs = crate::vfs::VFS.lock();
                            vfs.read_file(src_path).map(|d| d.to_vec())
                        };
                        if let Some(data) = data {
                            let mut vfs = crate::vfs::VFS.lock();
                            vfs.create_file_at_path(
                                &dest,
                                crate::vfs::FileType::Regular,
                                &data,
                                0o644,
                            );
                        }
                    }
                }
                if is_cut {
                    crate::clipboard::clear();
                }
            }
        }
        ExplorerAction::Delete => {
            if !target_path.is_empty() {
                let is_dir = {
                    let vfs = crate::vfs::VFS.lock();
                    vfs.resolve_path(&target_path)
                        .and_then(|ino| vfs.get_inode(ino))
                        .map(|i| i.file_type == crate::vfs::FileType::Directory)
                        .unwrap_or(false)
                };
                let mut vfs = crate::vfs::VFS.lock();
                if is_dir {
                    let _ = vfs.rmdir(&target_path);
                } else {
                    let _ = vfs.unlink(&target_path);
                }
            }
        }
        ExplorerAction::Rename => {
            // Enter rename mode
            if let Some(idx) = target_idx {
                let mut wm = window::WINDOW_MANAGER.lock();
                if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                    let name = String::from(target_path.rsplit('/').next().unwrap_or(""));
                    win.explorer_renaming = Some(idx);
                    win.explorer_rename_buf = name.clone();
                }
            }
        }
        ExplorerAction::NewFolder => {
            let new_path = if cur_path == "/" {
                String::from("/New Folder")
            } else {
                format!("{}/New Folder", cur_path)
            };
            let mut vfs = crate::vfs::VFS.lock();
            let _ = vfs.mkdir(&new_path, 0o755);
        }
        ExplorerAction::NewFile => {
            let new_path = if cur_path == "/" {
                String::from("/untitled")
            } else {
                format!("{}/untitled", cur_path)
            };
            let mut vfs = crate::vfs::VFS.lock();
            vfs.create_file_at_path(&new_path, crate::vfs::FileType::Regular, b"", 0o644);
        }
        ExplorerAction::ToggleHidden => {
            let mut wm = window::WINDOW_MANAGER.lock();
            if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                win.explorer_show_hidden = !win.explorer_show_hidden;
            }
        }
        ExplorerAction::Properties => {
            // Show a notification with file properties
            if !target_path.is_empty() {
                let info = {
                    let vfs = crate::vfs::VFS.lock();
                    if let Some(ino) = vfs.resolve_path(&target_path) {
                        if let Some(inode) = vfs.get_inode(ino) {
                            format!(
                                "Path: {}\nSize: {} bytes\nType: {:?}\nPerms: {:o}",
                                target_path, inode.size, inode.file_type, inode.permissions
                            )
                        } else {
                            format!("Path: {}", target_path)
                        }
                    } else {
                        format!("Path: {} (not found)", target_path)
                    }
                };
                crate::gui::notifications::info(
                    &format!(
                        "Properties: {}",
                        target_path.rsplit('/').next().unwrap_or("")
                    ),
                    &info,
                );
            }
        }
    }

    // Close context menu after executing action
    close_context_menu(wid);
}

/// Close the explorer context menu
pub fn close_context_menu(wid: WindowId) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.explorer_ctx_menu = None;
    }
}

/// Confirm a rename operation (Enter key in rename mode)
pub fn confirm_rename(wid: WindowId) {
    let (cur_path, rename_idx, new_name) = {
        let wm = window::WINDOW_MANAGER.lock();
        if let Some(win) = wm.windows.iter().find(|w| w.id == wid) {
            if let Some(idx) = win.explorer_renaming {
                (
                    current_path(&win.title),
                    idx,
                    win.explorer_rename_buf.clone(),
                )
            } else {
                return;
            }
        } else {
            return;
        }
    };

    // Read entries to find the old name
    let show_hidden = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wid)
            .map(|w| w.explorer_show_hidden)
            .unwrap_or(false)
    };
    let sort = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wid)
            .map(|w| w.explorer_sort)
            .unwrap_or(ExplorerSort::Name)
    };
    let sort_asc = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == wid)
            .map(|w| w.explorer_sort_asc)
            .unwrap_or(true)
    };
    let mut entries = read_entries(&cur_path, show_hidden);
    sort_entries(&mut entries, sort, sort_asc);

    if let Some(entry) = entries.get(rename_idx) {
        let old_path = if cur_path == "/" {
            format!("/{}", entry.name)
        } else {
            format!("{}/{}", cur_path, entry.name)
        };
        let new_path = if cur_path == "/" {
            format!("/{}", new_name)
        } else {
            format!("{}/{}", cur_path, new_name)
        };

        if !new_name.is_empty() && new_name != entry.name {
            let mut vfs = crate::vfs::VFS.lock();
            let _ = vfs.rename(&old_path, &new_path);
        }
    }

    // Exit rename mode
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.explorer_renaming = None;
        win.explorer_rename_buf.clear();
    }
}

/// Cancel rename operation
pub fn cancel_rename(wid: WindowId) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.explorer_renaming = None;
        win.explorer_rename_buf.clear();
    }
}

/// Handle a character typed during rename
pub fn rename_char(wid: WindowId, ch: char) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        if win.explorer_renaming.is_some() {
            win.explorer_rename_buf.push(ch);
        }
    }
}

/// Handle backspace during rename
pub fn rename_backspace(wid: WindowId) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        if win.explorer_renaming.is_some() {
            win.explorer_rename_buf.pop();
        }
    }
}

/// Open a file in the text editor (launches a new editor window)
pub(super) fn open_file_in_editor(path: &str) {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    let title = format!("Editor - {}", file_name);

    // Create a text editor window
    let (sw, sh) = crate::gui::cached_screen_size();
    let ww = 700u32.min(sw as u32 - 100);
    let wh = 500u32.min(sh as u32 - 100);
    let wx = (sw as u32 - ww) / 2;
    let wy = (sh as u32 - wh) / 2;

    let mut wm = window::WINDOW_MANAGER.lock();
    let mut win = window::Window::new(&title, wx as i32, wy as i32, ww, wh);
    win.content_type = WindowContentType::TextEditor;
    let wid = win.id;
    wm.add_window(win);
    drop(wm);

    // Initialize editor state with the file contents
    crate::gui::editor::open_file(wid, path);

    crate::gui::taskbar::add_entry(wid, &title);

    crate::serial_println!("[KnoxOS] Opened file in editor: {}", path);
}
