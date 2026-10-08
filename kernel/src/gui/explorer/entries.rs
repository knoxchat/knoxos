/// Directory listing, sorting, and display formatting
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::gui::window::{self, ExplorerSort, WindowId};

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

pub(super) fn format_size(bytes: u64) -> String {
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
pub(super) fn format_date(timestamp: i64) -> String {
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

pub(super) fn file_type_from_ext(name: &str) -> String {
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
