/// Application launch routing from desktop icons / start menu / shortcuts
use crate::gui::framebuffer::Pixel;
use crate::gui::taskbar;
use crate::gui::window;

use super::types::IconType;

/// Open an application by creating a new window
pub fn open_application(name: &str, icon_type: IconType) {
    // Route Empty launcher stubs to a Ring 3 SHM client (Gate F4).
    match name {
        "Paint" | "Video Player" | "Webamp" | "Doom" | "ClassiCube" | "Quake III"
        | "Image Viewer" | "Log Viewer" | "Logs" | "Calendar" | "Bluetooth"
        | "Bluetooth Manager" | "Software Updater" | "Updates" | "Software Center"
        | "App Store" | "Disk Utility" | "Disks" | "Setup Wizard" | "Archive Manager" => {
            crate::user_task::spawn_launcher_app(name);
            return;
        }
        "Task Manager" => {
            crate::user_task::spawn_taskmgr_app();
            return;
        }
        "Calculator" => {
            crate::user_task::spawn_calculator_app();
            return;
        }
        "Terminal" => {
            crate::user_task::spawn_pty_terminal_app();
            return;
        }
        "Monaco Editor" | "Text Editor" | "Editor" => {
            crate::user_task::spawn_editor_app();
            return;
        }
        _ => {}
    }

    match icon_type {
        IconType::MyPC | IconType::Folder | IconType::Trash => {
            crate::user_task::spawn_files_app();
            return;
        }
        IconType::Globe => {
            crate::user_task::spawn_browser_app();
            return;
        }
        IconType::Settings => {
            crate::user_task::spawn_settings_app();
            return;
        }
        IconType::AIBrain => {
            crate::user_task::spawn_ai_app();
            return;
        }
        _ => {}
    }

    let mut wm = window::WINDOW_MANAGER.lock();

    // Cascade offset: 26px per window (matches browser)
    let cascade = wm.windows.len() as i32;
    let offset = cascade * 26;

    // Start-menu Terminal is Ring 3 (`spawn_pty_terminal_app`). Vim still
    // uses IconType::Terminal and opens the leftover in-kernel emulator.
    let win = if icon_type == IconType::Terminal {
        let mut w = window::Window::new("Terminal", 280 + offset, 100 + offset, 860, 540);
        w.content_color = Pixel::rgb(12, 12, 12);
        w
    } else {
        let mut w = window::Window::new(name, 220 + offset, 90 + offset, 900, 640);
        w.content_color = Pixel::rgb(32, 32, 32);
        w
    };

    let wid = win.id;
    let is_terminal = win.content_type == window::WindowContentType::Terminal;
    let title = alloc::string::String::from(name);
    crate::gui::window_events::register_window(wid);
    wm.add_window(win);
    drop(wm);

    if is_terminal {
        crate::terminal::create_for_window(wid);
        let mut wm2 = window::WINDOW_MANAGER.lock();
        if let Some(win) = wm2.windows.iter_mut().find(|w| w.id == wid) {
            win.terminal_tabs = alloc::vec![wid];
            win.terminal_active_tab = 0;
        }
        drop(wm2);
    }

    taskbar::add_entry(wid, &title);
    taskbar::set_active(wid);

    crate::gui::sounds::window_open();
}
