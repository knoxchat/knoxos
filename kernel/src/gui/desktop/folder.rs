/// Desktop folder integration (~/Desktop) and recycle bin / trash
use alloc::string::String;
use alloc::vec::Vec;

use super::types::{
    DESKTOP, DesktopIcon, ICON_GRID_SPACING_X, ICON_GRID_SPACING_Y, ICON_GRID_X, ICON_GRID_Y,
    IconType,
};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Desktop Folder Integration — ~/Desktop mapped to desktop icons
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub(crate) const DESKTOP_DIR: &str = "/home/user/Desktop";
pub(crate) const TRASH_DIR: &str = "/home/user/.trash";

/// Determine icon type from a filesystem entry name
fn icon_type_for_file(name: &str) -> IconType {
    if name.ends_with('/') {
        return IconType::Folder;
    }
    let lower = {
        let mut s = String::new();
        for c in name.chars() {
            s.push(if c.is_ascii_uppercase() {
                (c as u8 + 32) as char
            } else {
                c
            });
        }
        s
    };
    if lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".bmp")
        || lower.ends_with(".jpeg")
    {
        IconType::Image
    } else if lower.ends_with(".zip")
        || lower.ends_with(".tar")
        || lower.ends_with(".gz")
        || lower.ends_with(".7z")
    {
        IconType::Archive
    } else if lower.ends_with(".sh") || lower.ends_with(".py") || lower.ends_with(".rs") {
        IconType::Script
    } else {
        IconType::Document
    }
}

/// Sync desktop icons with the ~/Desktop directory.
/// Preserves pinned app shortcuts, adds filesystem entries.
pub fn sync_desktop_folder() {
    let entries = match crate::vfs::list_directory(DESKTOP_DIR) {
        Some(e) => e,
        None => {
            // Try to create the Desktop directory
            crate::vfs::ensure_directory(DESKTOP_DIR);
            return;
        }
    };

    let mut desktop = DESKTOP.lock();

    // Separate pinned shortcuts from filesystem icons
    let pinned: Vec<DesktopIcon> = desktop
        .icons
        .iter()
        .filter(|i| {
            i.is_shortcut || i.icon_type == IconType::MyPC || i.icon_type == IconType::Trash
        })
        .cloned()
        .collect();

    // Determine grid positions for filesystem entries (start after pinned icons)
    let pinned_count = pinned.len() as i32;
    let screen_h = crate::gui::screen_size().1 as i32;
    let max_rows = (screen_h - ICON_GRID_Y) / ICON_GRID_SPACING_Y;
    let max_rows = max_rows.max(4);

    let mut icons = pinned;
    for (i, entry_name) in entries.iter().enumerate() {
        // Skip . and ..
        if entry_name == "." || entry_name == ".." {
            continue;
        }
        // Check if already represented by a pinned icon
        if icons.iter().any(|ic| ic.name == *entry_name) {
            continue;
        }

        let slot = pinned_count + i as i32;
        let col = slot / max_rows;
        let row = slot % max_rows;

        let mut full_path = String::from(DESKTOP_DIR);
        full_path.push('/');
        full_path.push_str(entry_name);

        icons.push(DesktopIcon {
            name: String::from(entry_name.as_str()),
            icon_type: icon_type_for_file(entry_name),
            x: ICON_GRID_X + col * ICON_GRID_SPACING_X,
            y: ICON_GRID_Y + row * ICON_GRID_SPACING_Y,
            selected: false,
            is_shortcut: false,
            path: full_path,
        });
    }

    desktop.icons = icons;
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Recycle Bin / Trash
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Move a file to the trash directory instead of deleting permanently
pub fn move_to_trash(path: &str) -> Result<(), &'static str> {
    // Ensure trash dir exists
    crate::vfs::ensure_directory(TRASH_DIR);

    // Extract filename from path
    let filename = path.rsplit('/').next().unwrap_or(path);
    let mut trash_path = String::from(TRASH_DIR);
    trash_path.push('/');
    trash_path.push_str(filename);

    // Move file to trash
    crate::file_manager::rename(path, &trash_path).map_err(|_| "Failed to move to trash")
}

/// Restore a file from trash to the desktop directory
pub fn restore_from_trash(filename: &str) -> Result<(), &'static str> {
    let mut trash_path = String::from(TRASH_DIR);
    trash_path.push('/');
    trash_path.push_str(filename);

    let mut dest_path = String::from(DESKTOP_DIR);
    dest_path.push('/');
    dest_path.push_str(filename);

    crate::file_manager::rename(&trash_path, &dest_path).map_err(|_| "Failed to restore from trash")
}

/// Empty the trash (permanently delete all files in trash)
pub fn empty_trash() -> Result<u32, &'static str> {
    let entries = crate::vfs::list_directory(TRASH_DIR).ok_or("Trash not found")?;
    let mut count = 0u32;
    for entry in &entries {
        if entry == "." || entry == ".." {
            continue;
        }
        let mut path = String::from(TRASH_DIR);
        path.push('/');
        path.push_str(entry);
        if crate::vfs::remove_dispatch(&path).is_ok() {
            count += 1;
        }
    }
    Ok(count)
}

/// Get the number of items in trash
pub fn trash_count() -> u32 {
    match crate::vfs::list_directory(TRASH_DIR) {
        Some(entries) => entries.iter().filter(|e| *e != "." && *e != "..").count() as u32,
        None => 0,
    }
}
