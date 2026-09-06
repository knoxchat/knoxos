use alloc::format;
/// File Explorer logic — Sprint 3 implementation
/// Handles file operations (copy/move/delete/rename), navigation (back/forward/up),
/// address bar editing, sorting, and context menus.
use alloc::string::String;
use alloc::vec::Vec;

use super::framebuffer::Rect;
use super::window::{
    self, ExplorerAction, ExplorerContextMenu, ExplorerCtxItem, ExplorerSort, WindowContentType,
    WindowId,
};

// ═════════════════════════════════════════════════════════════════════════
// EXPLORER HELPERS
// ═════════════════════════════════════════════════════════════════════════

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
fn set_path(wid: WindowId, path: &str) {
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

// ═════════════════════════════════════════════════════════════════════════
// SORTING
// ═════════════════════════════════════════════════════════════════════════

/// Cycle the sort mode for a given column header click
pub fn toggle_sort(wid: WindowId, column: ExplorerSort) {
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        if win.explorer_sort == column {
            win.explorer_sort_asc = !win.explorer_sort_asc;
        } else {
            win.explorer_sort = column;
            win.explorer_sort_asc = true;
        }
    }
}

/// Sort file entries in-place according to the window's sort settings
pub fn sort_entries(entries: &mut [ExplorerFileEntry], sort: ExplorerSort, ascending: bool) {
    entries.sort_by(|a, b| {
        // Directories always first
        let dir_cmp = match (a.is_dir, b.is_dir) {
            (true, false) => return core::cmp::Ordering::Less,
            (false, true) => return core::cmp::Ordering::Greater,
            _ => core::cmp::Ordering::Equal,
        };

        let cmp = match sort {
            ExplorerSort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            ExplorerSort::Size => a.size_bytes.cmp(&b.size_bytes),
            ExplorerSort::Type => a.kind.cmp(&b.kind),
            ExplorerSort::Date => a.mtime.cmp(&b.mtime),
        };

        if ascending { cmp } else { cmp.reverse() }
    });
}

/// A file entry used by the explorer (enriched from VFS)
#[derive(Clone)]
pub struct ExplorerFileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub size: String,
    pub kind: String,
    pub permissions: u16,
    /// Last modification time (Unix timestamp)
    pub mtime: i64,
    /// Formatted date string for display (e.g. "Mar 01 14:30")
    pub date_display: String,
}

/// Read directory entries from VFS for the given path
pub fn read_entries(path: &str, show_hidden: bool) -> Vec<ExplorerFileEntry> {
    let mut entries = Vec::new();
    let vfs = crate::vfs::VFS.lock();

    if let Some(children) = vfs.list_dir(path) {
        for child_name in &children {
            // Skip hidden files unless show_hidden is set
            if !show_hidden && child_name.starts_with('.') {
                continue;
            }

            let child_path = if path == "/" {
                format!("/{}", child_name)
            } else {
                format!("{}/{}", path, child_name)
            };
            if let Some(ino) = vfs.resolve_path(&child_path) {
                if let Some(inode) = vfs.get_inode(ino) {
                    let is_dir = inode.file_type == crate::vfs::FileType::Directory;
                    let size_bytes = inode.size;
                    let size = if is_dir {
                        String::new()
                    } else {
                        format_size(size_bytes)
                    };
                    let kind = match inode.file_type {
                        crate::vfs::FileType::Directory => String::from("Directory"),
                        crate::vfs::FileType::SymLink => String::from("Symlink"),
                        crate::vfs::FileType::CharDevice => String::from("Char Device"),
                        crate::vfs::FileType::BlockDevice => String::from("Block Device"),
                        crate::vfs::FileType::Pipe => String::from("FIFO"),
                        crate::vfs::FileType::Socket => String::from("Socket"),
                        _ => file_type_from_ext(child_name),
                    };

                    entries.push(ExplorerFileEntry {
                        name: child_name.clone(),
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
    }

    entries
}

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

fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        String::from("0 B")
    } else if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        let kb = bytes as f64 / 1024.0;
        if kb < 10.0 {
            format!("{:.1} KB", kb)
        } else {
            format!("{} KB", bytes / 1024)
        }
    } else if bytes < 1024 * 1024 * 1024 {
        let mb = bytes as f64 / (1024.0 * 1024.0);
        if mb < 10.0 {
            format!("{:.1} MB", mb)
        } else {
            format!("{} MB", bytes / (1024 * 1024))
        }
    } else {
        let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        format!("{:.1} GB", gb)
    }
}

/// Format a Unix timestamp into a human-readable date string (e.g., "Mar 01 14:30")
fn format_date(timestamp: i64) -> String {
    if timestamp <= 0 {
        return String::from("—");
    }
    // Simple Unix timestamp → date conversion
    let secs_per_minute = 60i64;
    let secs_per_hour = 3600i64;
    let secs_per_day = 86400i64;

    let total_days = timestamp / secs_per_day;
    let day_secs = timestamp % secs_per_day;
    let hour = (day_secs / secs_per_hour) % 24;
    let minute = (day_secs % secs_per_hour) / secs_per_minute;

    // Days since epoch → year/month/day (simplified Gregorian)
    let mut y = 1970i64;
    let mut remaining = total_days;
    loop {
        let days_in_year = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
            366
        } else {
            365
        };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let month_days: [i64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let month_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut m = 0usize;
    for i in 0..12 {
        if remaining < month_days[i] {
            m = i;
            break;
        }
        remaining -= month_days[i];
        if i == 11 {
            m = 11;
        }
    }
    let day = remaining + 1;

    format!("{} {:02} {:02}:{:02}", month_names[m], day, hour, minute)
}

fn file_type_from_ext(name: &str) -> String {
    if let Some(dot_pos) = name.rfind('.') {
        let ext = &name[dot_pos + 1..];
        match ext {
            "txt" | "text" | "log" => String::from("Text"),
            "md" | "markdown" => String::from("Markdown"),
            "rs" => String::from("Rust"),
            "py" => String::from("Python"),
            "js" => String::from("JavaScript"),
            "ts" => String::from("TypeScript"),
            "c" | "h" => String::from("C Source"),
            "cpp" | "cc" | "hpp" => String::from("C++ Source"),
            "html" | "htm" => String::from("HTML"),
            "css" => String::from("CSS"),
            "json" => String::from("JSON"),
            "toml" => String::from("TOML"),
            "yaml" | "yml" => String::from("YAML"),
            "xml" => String::from("XML"),
            "sh" | "bash" => String::from("Shell Script"),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" => String::from("Image"),
            "pdf" => String::from("PDF"),
            "zip" | "tar" | "gz" | "bz2" | "xz" => String::from("Archive"),
            "o" | "so" | "a" => String::from("Object"),
            "elf" | "bin" => String::from("Executable"),
            _ => format!("{} file", ext.to_uppercase()),
        }
    } else {
        String::from("File")
    }
}

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
                super::notifications::info(
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
fn open_file_in_editor(path: &str) {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    let title = format!("Editor - {}", file_name);

    // Create a text editor window
    let (sw, sh) = super::cached_screen_size();
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
    super::editor::open_file(wid, path);

    super::taskbar::add_entry(wid, &title);

    crate::serial_println!("[KnoxOS] Opened file in editor: {}", path);
}

// ═════════════════════════════════════════════════════════════════════════
// CLICK HANDLING
// ═════════════════════════════════════════════════════════════════════════

/// Layout constants (must match draw_file_explorer_content in window.rs)
const NAV_H: i32 = 35;
const HEADER_H: i32 = 20;
const STATUS_H: i32 = 23;
const ITEM_H: i32 = 28;

/// Handle a single click inside the file explorer content area
/// Returns true if the click was consumed
pub fn handle_click(wid: WindowId, x: i32, y: i32) -> bool {
    let mut wm = window::WINDOW_MANAGER.lock();
    let win = match wm.windows.iter_mut().find(|w| w.id == wid) {
        Some(w) => w,
        None => return false,
    };

    if win.content_type != WindowContentType::FileExplorer {
        return false;
    }

    // Close context menu if open
    if win.explorer_ctx_menu.is_some() {
        // Check if clicking inside the context menu
        if let Some(ref ctx) = win.explorer_ctx_menu {
            let menu_w = 180;
            let menu_h = ctx.items.len() as i32 * 28 + 8;
            let menu_rect = Rect::new(ctx.x, ctx.y, menu_w as u32, menu_h as u32);
            if menu_rect.contains(x, y) {
                // Click on a menu item
                let item_idx = ((y - ctx.y - 4) / 28) as usize;
                if item_idx < ctx.items.len() && ctx.items[item_idx].enabled {
                    let action = ctx.items[item_idx].action;
                    drop(wm);
                    execute_action(wid, action);
                    return true;
                }
                return true;
            }
        }
        win.explorer_ctx_menu = None;
        return true;
    }

    // Exit rename mode if clicking elsewhere
    if win.explorer_renaming.is_some() {
        drop(wm);
        confirm_rename(wid);
        return true;
    }

    let content = win.content_rect();

    // Check navigation buttons (back/forward/up)
    let nav_y = content.y;
    if y >= nav_y && y < nav_y + NAV_H {
        // Back button area: x in [content.x+4 .. content.x+22]
        if x >= content.x + 4 && x < content.x + 22 {
            drop(wm);
            navigate_back(wid);
            return true;
        }
        // Forward button area: x in [content.x+24 .. content.x+42]
        if x >= content.x + 24 && x < content.x + 42 {
            drop(wm);
            navigate_forward(wid);
            return true;
        }
        // Up button area: x in [content.x+46 .. content.x+72]
        if x >= content.x + 46 && x < content.x + 72 {
            drop(wm);
            navigate_up(wid);
            return true;
        }
        // Address bar area: click to edit
        if x >= content.x + 80 {
            drop(wm);
            start_address_edit(wid);
            return true;
        }
        return false;
    }

    // Check sidebar bookmark clicks
    let sidebar_w: i32 = if win.explorer_sidebar_visible { 160 } else { 0 };
    let sidebar_y = content.y + NAV_H;

    if win.explorer_sidebar_visible && x >= content.x && x < content.x + sidebar_w && y >= sidebar_y
    {
        // Bookmark items start at sidebar_y + 24, each 26px tall
        let bookmark_paths = [
            "/home/user",
            "/home/user/Desktop",
            "/home/user/Documents",
            "/home/user/Downloads",
            "/home/user/Pictures",
            "/home/user/Music",
            "/",
        ];
        let item_start_y = sidebar_y + 24;
        for (i, path) in bookmark_paths.iter().enumerate() {
            let by = item_start_y + (i as i32 * 26);
            if y >= by && y < by + 24 {
                drop(wm);
                navigate_to(wid, path);
                return true;
            }
        }
        return true;
    }

    // Adjust click coordinates for sidebar offset
    let list_offset_x = sidebar_w;

    // Check column header clicks for sorting
    let header_y = content.y + NAV_H;
    if y >= header_y && y < header_y + HEADER_H {
        let name_col = content.x + list_offset_x + 12;
        let perms_col = content.x + content.width as i32 - 270;
        let size_col = content.x + content.width as i32 - 180;
        let type_col = content.x + content.width as i32 - 100;

        let sort_col = if x >= type_col {
            ExplorerSort::Type
        } else if x >= size_col && content.width > 250 {
            ExplorerSort::Size
        } else {
            ExplorerSort::Name
        };

        drop(wm);
        toggle_sort(wid, sort_col);
        return true;
    }

    // Check file/folder entry clicks
    let entries_y = content.y + NAV_H + HEADER_H + 4;
    let list_h = content.height as i32 - NAV_H - HEADER_H - 4 - STATUS_H;
    let scroll_item_offset = (win.scroll_y / ITEM_H.max(1)) as usize;

    if y >= entries_y && y < entries_y + list_h {
        let clicked_vi = ((y - entries_y) / ITEM_H) as usize + scroll_item_offset;
        win.explorer_selected = clicked_vi as i32;
        return true;
    }

    false
}

/// Handle a double-click inside the file explorer content area
/// Returns true if the click was consumed
pub fn handle_double_click(wid: WindowId, x: i32, y: i32) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    let win = match wm.windows.iter().find(|w| w.id == wid) {
        Some(w) => w,
        None => return false,
    };

    if win.content_type != WindowContentType::FileExplorer {
        return false;
    }

    let content = win.content_rect();
    let entries_y = content.y + NAV_H + HEADER_H + 4;
    let list_h = content.height as i32 - NAV_H - HEADER_H - 4 - STATUS_H;
    let scroll_item_offset = (win.scroll_y / ITEM_H.max(1)) as usize;
    let cur_path = current_path(&win.title);
    let show_hidden = win.explorer_show_hidden;
    let sort = win.explorer_sort;
    let sort_asc = win.explorer_sort_asc;
    drop(wm);

    if y >= entries_y && y < entries_y + list_h {
        let clicked_vi = ((y - entries_y) / ITEM_H) as usize + scroll_item_offset;

        // Read entries and find the clicked one
        let mut entries = read_entries(&cur_path, show_hidden);
        sort_entries(&mut entries, sort, sort_asc);

        if let Some(entry) = entries.get(clicked_vi) {
            let full_path = if cur_path == "/" {
                format!("/{}", entry.name)
            } else {
                format!("{}/{}", cur_path, entry.name)
            };

            if entry.is_dir {
                navigate_to(wid, &full_path);
            } else {
                open_file_in_editor(&full_path);
            }
            return true;
        }
    }

    false
}

/// Handle a right-click inside the file explorer content area
/// Returns true if the click was consumed
pub fn handle_right_click(wid: WindowId, x: i32, y: i32) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    let win = match wm.windows.iter().find(|w| w.id == wid) {
        Some(w) => w,
        None => return false,
    };

    if win.content_type != WindowContentType::FileExplorer {
        return false;
    }

    let content = win.content_rect();
    let entries_y = content.y + NAV_H + HEADER_H + 4;
    let list_h = content.height as i32 - NAV_H - HEADER_H - 4 - STATUS_H;
    let scroll_item_offset = (win.scroll_y / ITEM_H.max(1)) as usize;
    let cur_path = current_path(&win.title);
    let show_hidden = win.explorer_show_hidden;
    let sort = win.explorer_sort;
    let sort_asc = win.explorer_sort_asc;
    drop(wm);

    let has_clipboard = !crate::clipboard::is_empty();

    // Determine which entry was right-clicked (if any)
    let mut target_index = None;
    let mut target_path = cur_path.clone();

    if y >= entries_y && y < entries_y + list_h {
        let clicked_vi = ((y - entries_y) / ITEM_H) as usize + scroll_item_offset;

        let mut entries = read_entries(&cur_path, show_hidden);
        sort_entries(&mut entries, sort, sort_asc);

        if let Some(entry) = entries.get(clicked_vi) {
            target_index = Some(clicked_vi);
            target_path = if cur_path == "/" {
                format!("/{}", entry.name)
            } else {
                format!("{}/{}", cur_path, entry.name)
            };
        }
    }

    let ctx = build_context_menu(x, y, target_index, target_path, has_clipboard);

    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.explorer_selected = target_index.map(|i| i as i32).unwrap_or(-1);
        win.explorer_ctx_menu = Some(ctx);
    }

    true
}

/// Handle keyboard shortcuts for the explorer
pub fn handle_key(
    wid: WindowId,
    key: super::event_types::KeyCode,
    ctrl: bool,
    shift: bool,
) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    let win = match wm.windows.iter().find(|w| w.id == wid) {
        Some(w) => w,
        None => return false,
    };

    if win.content_type != WindowContentType::FileExplorer {
        return false;
    }

    // If editing address bar, route keys there
    if win.explorer_editing_path {
        drop(wm);
        match key {
            super::event_types::KeyCode::Enter => confirm_address_edit(wid),
            super::event_types::KeyCode::Escape => cancel_address_edit(wid),
            super::event_types::KeyCode::Backspace => address_bar_backspace(wid),
            _ => {}
        }
        return true;
    }

    // If search mode is active, route keys to search bar
    if win.explorer_search_active {
        drop(wm);
        match key {
            super::event_types::KeyCode::Escape => cancel_search(wid),
            super::event_types::KeyCode::Backspace => search_backspace(wid),
            super::event_types::KeyCode::Enter => {
                // Enter while searching: close search and open selected entry
                cancel_search(wid);
            }
            _ => {}
        }
        return true;
    }

    // If renaming, route keys there
    if win.explorer_renaming.is_some() {
        drop(wm);
        match key {
            super::event_types::KeyCode::Enter => confirm_rename(wid),
            super::event_types::KeyCode::Escape => cancel_rename(wid),
            super::event_types::KeyCode::Backspace => rename_backspace(wid),
            _ => {}
        }
        return true;
    }

    let selected = win.explorer_selected;
    let cur_path = current_path(&win.title);
    let show_hidden = win.explorer_show_hidden;
    let sort = win.explorer_sort;
    let sort_asc = win.explorer_sort_asc;
    drop(wm);

    use super::event_types::KeyCode;

    match key {
        KeyCode::ArrowUp => {
            let mut wm = window::WINDOW_MANAGER.lock();
            if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                if win.explorer_selected > 0 {
                    win.explorer_selected -= 1;
                }
                // Auto-load preview if visible
                if win.explorer_preview_visible {
                    let new_sel = win.explorer_selected;
                    drop(wm);
                    let mut entries = read_entries(&cur_path, show_hidden);
                    sort_entries(&mut entries, sort, sort_asc);
                    if new_sel >= 0 {
                        if let Some(entry) = entries.get(new_sel as usize) {
                            if !entry.is_dir && is_previewable(&entry.name) {
                                let full = if cur_path == "/" {
                                    format!("/{}", entry.name)
                                } else {
                                    format!("{}/{}", cur_path, entry.name)
                                };
                                load_preview(wid, &full);
                            } else {
                                let mut wm2 = window::WINDOW_MANAGER.lock();
                                if let Some(w) = wm2.windows.iter_mut().find(|w| w.id == wid) {
                                    w.explorer_preview_content.clear();
                                    w.explorer_preview_path.clear();
                                }
                            }
                        }
                    }
                }
            }
            true
        }
        KeyCode::ArrowDown => {
            let entries = read_entries(&cur_path, show_hidden);
            let mut wm = window::WINDOW_MANAGER.lock();
            if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                if (win.explorer_selected + 1) < entries.len() as i32 {
                    win.explorer_selected += 1;
                }
                // Auto-load preview if visible
                if win.explorer_preview_visible {
                    let new_sel = win.explorer_selected;
                    drop(wm);
                    let mut sorted = read_entries(&cur_path, show_hidden);
                    sort_entries(&mut sorted, sort, sort_asc);
                    if new_sel >= 0 {
                        if let Some(entry) = sorted.get(new_sel as usize) {
                            if !entry.is_dir && is_previewable(&entry.name) {
                                let full = if cur_path == "/" {
                                    format!("/{}", entry.name)
                                } else {
                                    format!("{}/{}", cur_path, entry.name)
                                };
                                load_preview(wid, &full);
                            } else {
                                let mut wm2 = window::WINDOW_MANAGER.lock();
                                if let Some(w) = wm2.windows.iter_mut().find(|w| w.id == wid) {
                                    w.explorer_preview_content.clear();
                                    w.explorer_preview_path.clear();
                                }
                            }
                        }
                    }
                }
            }
            true
        }
        KeyCode::Enter => {
            if selected >= 0 {
                let mut entries = read_entries(&cur_path, show_hidden);
                sort_entries(&mut entries, sort, sort_asc);
                if let Some(entry) = entries.get(selected as usize) {
                    let full_path = if cur_path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", cur_path, entry.name)
                    };
                    if entry.is_dir {
                        navigate_to(wid, &full_path);
                    } else {
                        open_file_in_editor(&full_path);
                    }
                }
            }
            true
        }
        KeyCode::Backspace => {
            navigate_up(wid);
            true
        }
        KeyCode::Delete => {
            if selected >= 0 {
                let mut entries = read_entries(&cur_path, show_hidden);
                sort_entries(&mut entries, sort, sort_asc);
                if let Some(entry) = entries.get(selected as usize) {
                    let full_path = if cur_path == "/" {
                        format!("/{}", entry.name)
                    } else {
                        format!("{}/{}", cur_path, entry.name)
                    };
                    let mut vfs = crate::vfs::VFS.lock();
                    if entry.is_dir {
                        let _ = vfs.rmdir(&full_path);
                    } else {
                        let _ = vfs.unlink(&full_path);
                    }
                }
            }
            true
        }
        KeyCode::F2 => {
            // Rename
            if selected >= 0 {
                let mut entries = read_entries(&cur_path, show_hidden);
                sort_entries(&mut entries, sort, sort_asc);
                if let Some(entry) = entries.get(selected as usize) {
                    let mut wm = window::WINDOW_MANAGER.lock();
                    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                        win.explorer_renaming = Some(selected as usize);
                        win.explorer_rename_buf = entry.name.clone();
                    }
                }
            }
            true
        }
        _ if ctrl => {
            match key {
                KeyCode::C => {
                    // Copy
                    if selected >= 0 {
                        let mut entries = read_entries(&cur_path, show_hidden);
                        sort_entries(&mut entries, sort, sort_asc);
                        if let Some(entry) = entries.get(selected as usize) {
                            let full_path = if cur_path == "/" {
                                format!("/{}", entry.name)
                            } else {
                                format!("{}/{}", cur_path, entry.name)
                            };
                            crate::clipboard::copy_files(&[full_path.as_str()]);
                        }
                    }
                    true
                }
                KeyCode::X => {
                    // Cut
                    if selected >= 0 {
                        let mut entries = read_entries(&cur_path, show_hidden);
                        sort_entries(&mut entries, sort, sort_asc);
                        if let Some(entry) = entries.get(selected as usize) {
                            let full_path = if cur_path == "/" {
                                format!("/{}", entry.name)
                            } else {
                                format!("{}/{}", cur_path, entry.name)
                            };
                            crate::clipboard::cut_files(&[full_path.as_str()]);
                        }
                    }
                    true
                }
                KeyCode::V => {
                    // Paste
                    execute_action(wid, ExplorerAction::Paste);
                    true
                }
                KeyCode::H => {
                    // Toggle hidden files
                    let mut wm = window::WINDOW_MANAGER.lock();
                    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                        win.explorer_show_hidden = !win.explorer_show_hidden;
                    }
                    true
                }
                KeyCode::L => {
                    // Focus address bar
                    start_address_edit(wid);
                    true
                }
                KeyCode::F => {
                    // Open search bar (Ctrl+F)
                    start_search(wid);
                    true
                }
                KeyCode::P => {
                    // Toggle preview panel (Ctrl+P)
                    toggle_preview(wid);
                    true
                }
                KeyCode::G => {
                    // Toggle grid/list view (Ctrl+G)
                    toggle_grid_view(wid);
                    true
                }
                KeyCode::B => {
                    // Toggle sidebar (Ctrl+B)
                    let mut wm = super::window::WINDOW_MANAGER.lock();
                    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                        win.explorer_sidebar_visible = !win.explorer_sidebar_visible;
                    }
                    true
                }
                _ => false,
            }
        }
        _ => false,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// FILE PREVIEW PANEL (9.65)
// ═══════════════════════════════════════════════════════════════════════════

/// Toggle the file preview panel on/off
pub fn toggle_preview(wid: WindowId) {
    let mut wm = super::window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.explorer_preview_visible = !win.explorer_preview_visible;
        if !win.explorer_preview_visible {
            win.explorer_preview_content.clear();
            win.explorer_preview_path.clear();
        }
    }
}

/// Load preview content for the selected file
pub fn load_preview(wid: WindowId, file_path: &str) {
    let vfs = crate::vfs::VFS.lock();
    let content = if let Some(data) = vfs.read_file(file_path) {
        // Try to interpret as UTF-8 text, show first ~40 lines
        match core::str::from_utf8(data) {
            Ok(text) => {
                let mut preview = alloc::string::String::new();
                for (line_count, line) in text.lines().enumerate() {
                    if line_count >= 40 {
                        preview.push_str("\n... (truncated)");
                        break;
                    }
                    // Truncate long lines
                    if line.len() > 80 {
                        preview.push_str(&line[..80]);
                        preview.push_str("...");
                    } else {
                        preview.push_str(line);
                    }
                    preview.push('\n');
                }
                preview
            }
            Err(_) => {
                // Binary file — show hex dump of first 128 bytes
                let len = data.len().min(128);
                let mut hex = alloc::format!("[Binary file — {} bytes]\n\n", data.len());
                for (i, byte) in data[..len].iter().enumerate() {
                    if i > 0 && i % 16 == 0 {
                        hex.push('\n');
                    }
                    hex.push_str(&alloc::format!("{:02x} ", byte));
                }
                hex
            }
        }
    } else {
        alloc::string::String::from("(unable to read file)")
    };
    drop(vfs);

    let mut wm = super::window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.explorer_preview_content = content;
        win.explorer_preview_path = alloc::string::String::from(file_path);
    }
}

/// Check if a path looks like a previewable text file
pub fn is_previewable(name: &str) -> bool {
    let text_exts = [
        ".txt",
        ".rs",
        ".md",
        ".toml",
        ".json",
        ".yaml",
        ".yml",
        ".py",
        ".js",
        ".ts",
        ".c",
        ".h",
        ".cpp",
        ".sh",
        ".bash",
        ".conf",
        ".cfg",
        ".ini",
        ".log",
        ".csv",
        ".xml",
        ".html",
        ".css",
        ".sql",
        ".env",
        ".gitignore",
        ".makefile",
    ];
    let lower = name.to_ascii_lowercase();
    text_exts.iter().any(|ext| lower.ends_with(ext)) || !lower.contains('.') // extensionless files are often text
}

// ═══════════════════════════════════════════════════════════════════════════
// GRID / LIST VIEW TOGGLE (9.68)
// ═══════════════════════════════════════════════════════════════════════════

/// Toggle between grid view and list view
pub fn toggle_grid_view(wid: WindowId) {
    let mut wm = super::window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.explorer_grid_view = !win.explorer_grid_view;
        win.scroll_y = 0; // reset scroll on view change
        crate::serial_println!(
            "[Explorer] View mode: {}",
            if win.explorer_grid_view {
                "grid"
            } else {
                "list"
            }
        );
    }
}

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
