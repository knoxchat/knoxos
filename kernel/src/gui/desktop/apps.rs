/// Application launch routing from desktop icons / start menu / shortcuts
use crate::gui::framebuffer::Pixel;
use crate::gui::taskbar;
use crate::gui::window;

use super::folder::TRASH_DIR;
use super::types::IconType;

/// Open an application by creating a new window
pub fn open_application(name: &str, icon_type: IconType) {
    // Route Empty launcher stubs to a Ring 3 SHM client (Gate F4).
    match name {
        "Paint" | "Video Player" | "Webamp" | "Doom" | "ClassiCube" | "Quake III" => {
            crate::user_task::spawn_launcher_app(name);
            return;
        }
        "Task Manager" => {
            crate::gui::task_manager::open();
            return;
        }
        "Calculator" => {
            crate::gui::calculator::open();
            return;
        }
        "Image Viewer" => {
            crate::gui::image_viewer::open();
            return;
        }
        "Log Viewer" | "Logs" => {
            crate::gui::log_viewer::open();
            return;
        }
        "Calendar" => {
            crate::gui::calendar_app::open();
            return;
        }
        "Bluetooth" | "Bluetooth Manager" => {
            crate::gui::bt_manager::open();
            return;
        }
        "Software Updater" | "Updates" => {
            crate::gui::software_updater::open();
            return;
        }
        "Software Center" | "App Store" => {
            crate::gui::software_center::open();
            return;
        }
        "Disk Utility" | "Disks" => {
            crate::gui::disk_utility::open();
            return;
        }
        "Setup Wizard" => {
            crate::gui::setup_wizard::open();
            return;
        }
        _ => {}
    }

    let mut wm = window::WINDOW_MANAGER.lock();

    // Cascade offset: 26px per window (matches browser)
    let cascade = wm.windows.len() as i32;
    let offset = cascade * 26;

    let win = match icon_type {
        IconType::MyPC => {
            let mut w = window::Window::new("Files", 200 + offset, 80 + offset, 900, 640);
            w.content_color = Pixel::rgb(32, 32, 32);
            w
        }
        IconType::Terminal => {
            let mut w = window::Window::new("Terminal", 280 + offset, 100 + offset, 860, 540);
            w.content_color = Pixel::rgb(12, 12, 12);
            w
        }
        IconType::AIBrain => {
            let mut w =
                window::Window::new("AI Assistant - KnoxOS", 260 + offset, 70 + offset, 920, 620);
            w.content_color = Pixel::rgb(20, 20, 30);
            w
        }
        IconType::Folder => {
            // Open folder with Unix path in title
            let folder_path = match name {
                "Documents" => "/home/user/Documents",
                "Downloads" => "/home/user/Downloads",
                "Music" => "/home/user/Music",
                "Pictures" => "/home/user/Pictures",
                "Videos" => "/home/user/Videos",
                "Desktop" => "/home/user/Desktop",
                _ => "/home/user",
            };
            let title = alloc::format!("Files - {}", folder_path);
            let mut w = window::Window::new(&title, 220 + offset, 90 + offset, 900, 640);
            w.content_color = Pixel::rgb(32, 32, 32);
            w
        }
        IconType::Globe => {
            // If Vivaldi is installed, launch Vivaldi processes and brand the window
            let vivaldi_state = crate::vivaldi::status();
            let title = if vivaldi_state == crate::vivaldi::VivaldiState::Installed
                || vivaldi_state == crate::vivaldi::VivaldiState::Running
                || vivaldi_state == crate::vivaldi::VivaldiState::Stopped
            {
                // Launch Vivaldi if not already running
                if vivaldi_state != crate::vivaldi::VivaldiState::Running {
                    let _ = crate::vivaldi::launch(None);
                }
                "Vivaldi Browser"
            } else {
                "Browser"
            };
            let mut w = window::Window::new(title, 160 + offset, 50 + offset, 1100, 720);
            w.content_color = Pixel::rgb(255, 255, 255);
            w
        }
        IconType::Settings => {
            let mut w = window::Window::new("Settings", 240 + offset, 70 + offset, 900, 620);
            w.content_color = Pixel::rgb(24, 24, 28);
            w
        }
        IconType::Trash => {
            let title = alloc::format!("Files - {}", TRASH_DIR);
            let mut w = window::Window::new(&title, 220 + offset, 90 + offset, 900, 640);
            w.content_color = Pixel::rgb(32, 32, 32);
            w
        }
        _ => {
            let mut w = window::Window::new(name, 220 + offset, 90 + offset, 900, 640);
            w.content_color = Pixel::rgb(32, 32, 32);
            w
        }
    };

    let wid = win.id;
    let is_terminal = win.content_type == window::WindowContentType::Terminal;
    let is_browser = win.content_type == window::WindowContentType::Browser;
    let is_editor = win.content_type == window::WindowContentType::TextEditor;
    let is_ai = win.content_type == window::WindowContentType::AIAssistant;
    let vivaldi_branded = win.title.contains("Vivaldi");
    let title = alloc::string::String::from(name);
    crate::gui::window_events::register_window(wid);
    wm.add_window(win);
    drop(wm);

    // Create a per-window terminal instance if this is a terminal window
    if is_terminal {
        crate::terminal::create_for_window(wid);
        // Initialize with a single tab
        let mut wm2 = window::WINDOW_MANAGER.lock();
        if let Some(win) = wm2.windows.iter_mut().find(|w| w.id == wid) {
            win.terminal_tabs = alloc::vec![wid];
            win.terminal_active_tab = 0;
        }
        drop(wm2);
    }

    // Create a per-window browser instance if this is a browser window
    if is_browser {
        crate::gui::browser::create_for_window(wid, vivaldi_branded);
    }

    // Create a per-window editor state if this is a text editor window
    if is_editor {
        crate::gui::editor::new_empty(wid);
    }

    // Create a per-window AI assistant state if this is an AI window
    if is_ai {
        crate::gui::ai_assistant::create_for_window(wid);
    }

    taskbar::add_entry(wid, &title);
    taskbar::set_active(wid);

    // Play window open sound
    crate::gui::sounds::window_open();
}
