/// Window manager state and helpers to open browser/explorer windows
use alloc::vec::Vec;
use spin::Mutex;

use super::types::{Window, WindowContentType, WindowId};

/// Auto-tiling layout mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TilingMode {
    /// Floating mode — windows are freely positioned (default)
    Floating,
    /// Master-stack: one large master pane on left, stack of windows on right (60/40 split)
    MasterStack,
    /// Grid: all windows arranged in an even grid
    Grid,
    /// Monocle: one maximized window at a time, cycle through them
    Monocle,
    /// Columns: windows arranged in equal-width vertical columns
    Columns,
}

pub struct WindowManager {
    pub windows: Vec<Window>,
    pub focused_window: Option<WindowId>,
    /// Currently active workspace (0..NUM_WORKSPACES-1)
    pub current_workspace: u8,
    /// Automatic tiling layout mode
    pub tiling_mode: TilingMode,
    /// Master-stack ratio (0.0–1.0, default 0.6 = 60% master)
    pub master_ratio: f32,
}

// WindowManager::new() and Default impl moved to wm_core.rs
// WindowManager methods moved to wm_core.rs and wm_layout.rs

lazy_static::lazy_static! {
    pub static ref WINDOW_MANAGER: Mutex<WindowManager> = Mutex::new(WindowManager::new());
}

/// Open a browser window navigated to the given URL
pub fn open_browser_window(url: &str) {
    let mut win = Window::new("Browser", 200, 100, 900, 650);
    win.content_type = WindowContentType::Browser;
    // Store the URL in the title so the browser renderer can use it
    win.title = alloc::format!("Browser — {}", url);
    let mut wm = WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);
    crate::gui::request_redraw();
}

/// Open a file explorer window at a given path
pub fn open_file_explorer_at(path: &str) {
    let title = alloc::format!("Files — {}", path);
    let mut win = Window::new(&title, 180, 80, 800, 550);
    win.content_type = WindowContentType::FileExplorer;
    let mut wm = WINDOW_MANAGER.lock();
    wm.add_window(win);
    drop(wm);
    crate::gui::request_redraw();
}
