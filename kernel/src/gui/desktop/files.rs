/// Creating desktop files/folders and accepting explorer drops
use alloc::string::String;

use crate::gui::notifications;

use super::types::{
    DESKTOP, DesktopIcon, ICON_GRID_SPACING_X, ICON_GRID_SPACING_Y, ICON_GRID_X, ICON_GRID_Y,
    IconType,
};

// ═══════════════════════════════════════════════════════════════════════════
// DESKTOP FILE/FOLDER CREATION (9.29)
// ═══════════════════════════════════════════════════════════════════════════

/// Create a new file on the Desktop directory and add a desktop icon
pub(crate) fn create_desktop_file() {
    use alloc::format;
    let desktop_path = "/home/user/Desktop";

    // Find a unique name
    let mut name = String::from("New File.txt");
    let mut counter = 1u32;
    {
        let vfs = crate::vfs::VFS.lock();
        loop {
            let path = format!("{}/{}", desktop_path, name);
            if vfs.resolve_path(&path).is_none() {
                break;
            }
            counter += 1;
            name = format!("New File ({}).txt", counter);
        }
    }

    // Create the file in VFS
    {
        let mut vfs = crate::vfs::VFS.lock();
        let path = format!("{}/{}", desktop_path, name);
        vfs.write_file(&path, b"");
    }

    // Add a desktop icon for the new file
    {
        let mut desktop = DESKTOP.lock();
        let idx = desktop.icons.len();
        let col = idx % 6;
        let row = idx / 6;
        desktop.icons.push(DesktopIcon {
            name: name.clone(),
            icon_type: IconType::Document,
            x: ICON_GRID_X + (col as i32) * ICON_GRID_SPACING_X,
            y: ICON_GRID_Y + (row as i32) * ICON_GRID_SPACING_Y,
            selected: false,
            is_shortcut: false,
            path: String::new(),
        });
    }

    // Show a notification
    crate::gui::notifications::info("Desktop", &format!("Created: {}", name));
    crate::gui::request_redraw();
}

/// Create a new folder on the Desktop directory and add a desktop icon
pub(crate) fn create_desktop_folder() {
    use alloc::format;
    let desktop_path = "/home/user/Desktop";

    // Find a unique name
    let mut name = String::from("New Folder");
    let mut counter = 1u32;
    {
        let vfs = crate::vfs::VFS.lock();
        loop {
            let path = format!("{}/{}", desktop_path, name);
            if vfs.resolve_path(&path).is_none() {
                break;
            }
            counter += 1;
            name = format!("New Folder ({})", counter);
        }
    }

    // Create the folder in VFS
    {
        let mut vfs = crate::vfs::VFS.lock();
        let path = format!("{}/{}", desktop_path, name);
        let _ = vfs.mkdir(&path, 0o755);
    }

    // Add a desktop icon for the new folder
    {
        let mut desktop = DESKTOP.lock();
        let idx = desktop.icons.len();
        let col = idx % 6;
        let row = idx / 6;
        desktop.icons.push(DesktopIcon {
            name: name.clone(),
            icon_type: IconType::Folder,
            x: ICON_GRID_X + (col as i32) * ICON_GRID_SPACING_X,
            y: ICON_GRID_Y + (row as i32) * ICON_GRID_SPACING_Y,
            selected: false,
            is_shortcut: false,
            path: String::new(),
        });
    }

    // Show a notification
    crate::gui::notifications::info("Desktop", &format!("Created: {}", name));
    crate::gui::request_redraw();
}

// ─── File Drop from Explorer to Desktop (9.27) ──────────────────────

/// Accept a file being dropped onto the desktop from an explorer window.
/// Copies the file to /home/user/Desktop and adds a desktop icon.
pub fn accept_file_drop(src_path: &str) {
    use alloc::format;
    let desktop_path = "/home/user/Desktop";
    let name = src_path.rsplit('/').next().unwrap_or("dropped_file");

    // Copy the file to Desktop
    let dst = format!("{}/{}", desktop_path, name);
    match crate::vfs::copy_file_dispatch(src_path, &dst) {
        Ok(()) => {
            // Add a desktop icon
            let icon_type = {
                let vfs = crate::vfs::VFS.lock();
                if let Some(ino) = vfs.resolve_path(src_path) {
                    if let Some(inode) = vfs.get_inode(ino) {
                        if inode.file_type == crate::vfs::FileType::Directory {
                            IconType::Folder
                        } else {
                            IconType::Document
                        }
                    } else {
                        IconType::Document
                    }
                } else {
                    IconType::Document
                }
            };

            let mut desktop = DESKTOP.lock();
            let idx = desktop.icons.len();
            let col = idx % 6;
            let row = idx / 6;
            desktop.icons.push(DesktopIcon {
                name: String::from(name),
                icon_type,
                x: ICON_GRID_X + (col as i32) * ICON_GRID_SPACING_X,
                y: ICON_GRID_Y + (row as i32) * ICON_GRID_SPACING_Y,
                selected: false,
                is_shortcut: false,
                path: String::new(),
            });
            drop(desktop);
            crate::gui::notifications::info("Desktop", &format!("Copied {} to Desktop", name));
        }
        Err(_) => {
            crate::gui::notifications::error(
                "Desktop",
                &format!("Failed to copy {} to Desktop", name),
            );
        }
    }
    crate::gui::request_redraw();
}
