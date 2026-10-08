/// In-window file name/path filtering (9.64)
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::gui::window::{self, WindowId};

use super::entries::{ExplorerFileEntry, file_type_from_ext, format_date, format_size};

// ═════════════════════════════════════════════════════════════════════════
// SEARCH — In-window file name/path filtering (9.64)
// ═════════════════════════════════════════════════════════════════════════

/// Activate search mode (Ctrl+F)
pub fn start_search(wid: WindowId) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.explorer_search_active = true;
        win.explorer_search_query.clear();
        win.explorer_search_cursor = 0;
        win.explorer_selected = -1;
        win.scroll_y = 0;
    }
}

/// Cancel search mode (Escape while searching)
pub fn cancel_search(wid: WindowId) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.explorer_search_active = false;
        win.explorer_search_query.clear();
        win.explorer_search_cursor = 0;
    }
}

/// Type a character into the search bar
pub fn search_char(wid: WindowId, ch: char) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.explorer_search_query
            .insert(win.explorer_search_cursor, ch);
        win.explorer_search_cursor += ch.len_utf8();
        win.explorer_selected = -1;
        win.scroll_y = 0;
    }
}

/// Backspace in search bar
pub fn search_backspace(wid: WindowId) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        if win.explorer_search_cursor > 0 {
            // Find the previous char boundary
            let new_cursor = win.explorer_search_query[..win.explorer_search_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            win.explorer_search_query.remove(new_cursor);
            win.explorer_search_cursor = new_cursor;
        }
    }
}

/// Filter directory entries by search query (case-insensitive name matching)
pub fn filter_entries(entries: &[ExplorerFileEntry], query: &str) -> Vec<ExplorerFileEntry> {
    if query.is_empty() {
        return entries.to_vec();
    }
    let lower_query = query.to_lowercase();
    entries
        .iter()
        .filter(|e| e.name.to_lowercase().contains(&lower_query))
        .cloned()
        .collect()
}

/// Recursively search a directory tree for matching files (deeper search via Enter in search)
pub fn search_recursive(
    base_path: &str,
    query: &str,
    show_hidden: bool,
    max_results: usize,
) -> Vec<ExplorerFileEntry> {
    let mut results = Vec::new();
    let lower_query = query.to_lowercase();
    recursive_search_inner(
        base_path,
        &lower_query,
        show_hidden,
        max_results,
        &mut results,
    );
    results
}

fn recursive_search_inner(
    path: &str,
    lower_query: &str,
    show_hidden: bool,
    max_results: usize,
    results: &mut Vec<ExplorerFileEntry>,
) {
    if results.len() >= max_results {
        return;
    }
    let vfs = crate::vfs::VFS.lock();
    if let Some(children) = vfs.list_dir(path) {
        for child_name in &children {
            if results.len() >= max_results {
                return;
            }
            if !show_hidden && child_name.starts_with('.') {
                continue;
            }
            let child_path = if path == "/" {
                format!("/{}", child_name)
            } else {
                format!("{}/{}", path, child_name)
            };
            if child_name.to_lowercase().contains(lower_query) {
                if let Some(ino) = vfs.resolve_path(&child_path) {
                    if let Some(inode) = vfs.get_inode(ino) {
                        let is_dir = matches!(inode.file_type, crate::vfs::FileType::Directory);
                        let size_bytes = inode.size;
                        let size = if is_dir {
                            String::new()
                        } else {
                            format_size(size_bytes)
                        };
                        let kind = if is_dir {
                            String::from("Directory")
                        } else {
                            file_type_from_ext(child_name)
                        };
                        results.push(ExplorerFileEntry {
                            name: String::from(child_path.as_str()),
                            is_dir,
                            size_bytes,
                            size,
                            kind,
                            permissions: inode.permissions,
                            mtime: inode.mtime,
                            date_display: format_date(inode.mtime),
                        });
                    }
                }
            }
            // Recurse into directories
            if let Some(ino) = vfs.resolve_path(&child_path) {
                if let Some(inode) = vfs.get_inode(ino) {
                    if matches!(inode.file_type, crate::vfs::FileType::Directory) {
                        drop(vfs);
                        recursive_search_inner(
                            &child_path,
                            lower_query,
                            show_hidden,
                            max_results,
                            results,
                        );
                        return; // vfs was dropped, must re-acquire if continuing
                    }
                }
            }
        }
    }
}
