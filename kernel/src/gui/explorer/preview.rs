/// File preview panel (9.65) and grid/list view toggle (9.68)
use crate::gui::window::{self, WindowId};

// ═══════════════════════════════════════════════════════════════════════════
// FILE PREVIEW PANEL (9.65)
// ═══════════════════════════════════════════════════════════════════════════

/// Toggle the file preview panel on/off
pub fn toggle_preview(wid: WindowId) {
    let mut wm = window::WINDOW_MANAGER.lock();
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

    let mut wm = window::WINDOW_MANAGER.lock();
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
    let mut wm = window::WINDOW_MANAGER.lock();
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
