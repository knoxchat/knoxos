/// Application launch routing from desktop icons / start menu / shortcuts.
///
/// Boot gates still spawn Ring 3 SHM clients (`spawn_*_app`). Desktop clicks
/// open the in-kernel apps so Files/Terminal/Browser/Settings actually work.
use alloc::format;
use alloc::string::String;
use alloc::vec;

use crate::gui::framebuffer::Pixel;
use crate::gui::taskbar;
use crate::gui::window::{self, Window, WindowContentType, WindowId};

use super::folder::TRASH_DIR;
use super::types::IconType;

/// Open an application by creating a new in-kernel window.
pub fn open_application(name: &str, icon_type: IconType) {
    match name {
        "Terminal" => {
            open_terminal();
            return;
        }
        "Files" => {
            open_explorer_at("/home/user");
            return;
        }
        "Documents" => {
            open_explorer_at("/home/user/Documents");
            return;
        }
        "Trash" => {
            open_explorer_at(TRASH_DIR);
            return;
        }
        "Browser" => {
            open_browser_at("knoxos://home");
            return;
        }
        "Settings" => {
            open_settings();
            return;
        }
        "AI Assistant" => {
            open_ai();
            return;
        }
        "Calculator" => {
            crate::gui::calculator::open();
            return;
        }
        "Task Manager" => {
            crate::gui::task_manager::open();
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
        "Archive Manager" => {
            crate::gui::archive_manager::open();
            return;
        }
        "Monaco Editor" | "Text Editor" | "Editor" | "Vim" => {
            open_editor_at("");
            return;
        }
        "Paint" => {
            crate::gui::image_viewer::open();
            return;
        }
        "Video Player" => {
            open_explorer_at("/home/user/Videos");
            return;
        }
        _ => {}
    }

    match icon_type {
        IconType::Terminal => open_terminal(),
        IconType::MyPC | IconType::Folder => open_explorer_at("/home/user"),
        IconType::Trash => open_explorer_at(TRASH_DIR),
        IconType::Globe => open_browser_at("knoxos://home"),
        IconType::Settings => open_settings(),
        IconType::AIBrain => open_ai(),
        IconType::Document | IconType::Script => open_editor_at(""),
        IconType::Image => crate::gui::image_viewer::open(),
        IconType::Archive => crate::gui::archive_manager::open(),
        IconType::MediaPlayer => open_explorer_at("/home/user/Videos"),
        IconType::Game => {
            crate::gui::notifications::info(name, "This app is not installed yet.");
        }
    }
}

/// Open Files at `path` (creates the directory if it is missing).
pub fn open_explorer_at(path: &str) {
    ensure_dir(path);
    let offset = cascade_offset();
    let title = explorer_title(path);
    let mut win = Window::new(&title, 220 + offset, 90 + offset, 900, 640);
    win.content_type = WindowContentType::FileExplorer;
    win.content_color = Pixel::rgb(32, 32, 32);
    win.explorer_history = vec![String::from(path)];
    win.explorer_history_idx = 0;
    let _ = present_window(win, "Files");
}

/// Open the browser, optionally navigating to `url`.
pub fn open_browser_at(url: &str) {
    let offset = cascade_offset();
    let mut win = Window::new("Browser", 180 + offset, 70 + offset, 1000, 680);
    win.content_type = WindowContentType::Browser;
    let wid = present_window(win, "Browser");
    crate::gui::browser::create_for_window(wid, false);
    if !url.is_empty() && url != "knoxos://home" {
        crate::gui::browser::with_browser_mut(wid, |state| {
            state.navigate(url);
        });
    }
}

/// Open a text editor, loading `path` when it is not empty.
pub fn open_editor_at(path: &str) {
    let offset = cascade_offset();
    let title = if path.is_empty() {
        String::from("Editor")
    } else {
        format!("Editor — {}", path)
    };
    let mut win = Window::new(&title, 200 + offset, 80 + offset, 880, 620);
    win.content_type = WindowContentType::TextEditor;
    let wid = present_window(win, "Editor");
    if path.is_empty() {
        crate::gui::editor::new_empty(wid);
    } else {
        crate::gui::editor::open_file(wid, path);
    }
}

fn open_terminal() {
    let offset = cascade_offset();
    let mut win = Window::new("Terminal", 280 + offset, 100 + offset, 860, 540);
    win.content_type = WindowContentType::Terminal;
    win.content_color = Pixel::rgb(12, 12, 12);
    let wid = present_window(win, "Terminal");
    crate::terminal::create_for_window(wid);
    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.terminal_tabs = vec![wid];
        win.terminal_active_tab = 0;
    }
}

fn open_settings() {
    if focus_existing(WindowContentType::Settings) {
        return;
    }
    let offset = cascade_offset();
    let mut win = Window::new("Settings", 200 + offset, 80 + offset, 860, 600);
    win.content_type = WindowContentType::Settings;
    let _ = present_window(win, "Settings");
}

fn open_ai() {
    let offset = cascade_offset();
    let mut win = Window::new("AI Assistant", 240 + offset, 90 + offset, 720, 560);
    win.content_type = WindowContentType::AIAssistant;
    let wid = present_window(win, "AI Assistant");
    crate::gui::ai_assistant::create_for_window(wid);
}

fn explorer_title(path: &str) -> String {
    if path == "/home/user" {
        String::from("Files")
    } else {
        format!("Files - {}", path)
    }
}

fn cascade_offset() -> i32 {
    window::WINDOW_MANAGER.lock().windows.len() as i32 * 26
}

fn present_window(win: Window, dock_title: &str) -> WindowId {
    let wid = win.id;
    crate::gui::window_events::register_window(wid);
    window::WINDOW_MANAGER.lock().add_window(win);
    taskbar::add_entry(wid, dock_title);
    taskbar::set_active(wid);
    crate::gui::sounds::window_open();
    crate::gui::request_redraw();
    crate::serial_println!("[KnoxOS] Launched {} (window {})", dock_title, wid);
    wid
}

fn focus_existing(content_type: WindowContentType) -> bool {
    let mut wm = window::WINDOW_MANAGER.lock();
    let found = wm
        .windows
        .iter()
        .rev()
        .find(|w| w.content_type == content_type)
        .map(|w| w.id);
    if let Some(id) = found {
        wm.focus_window(id);
        drop(wm);
        taskbar::set_active(id);
        crate::gui::request_redraw();
        true
    } else {
        false
    }
}

fn ensure_dir(path: &str) {
    let mut vfs = crate::vfs::VFS.lock();
    if vfs.resolve_path(path).is_some() {
        return;
    }
    let _ = vfs.mkdir(path, 0o755);
}
