/// Persistent window layouts — save/restore workspace arrangements
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::gui::framebuffer::Rect;
use crate::gui::window_attrs::WindowLevel;

use super::manager::WINDOW_MANAGER;
use super::types::{WindowContentType, WindowState};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Persistent Window Layouts — Save/restore workspace arrangements
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Saved state of a single window in a layout
#[derive(Clone)]
pub struct SavedWindowState {
    pub title: String,
    pub content_type: WindowContentType,
    pub rect: Rect,
    pub workspace: u8,
    pub state: WindowState,
    pub window_level: WindowLevel,
}

/// A named layout that can be saved and restored
#[derive(Clone)]
pub struct WindowLayout {
    pub name: String,
    pub windows: Vec<SavedWindowState>,
}

lazy_static::lazy_static! {
    /// Saved window layouts (up to 8)
    pub static ref SAVED_LAYOUTS: Mutex<Vec<WindowLayout>> = Mutex::new(Vec::new());
}

/// Save the current window arrangement as a named layout
pub fn save_layout(name: &str) {
    let wm = WINDOW_MANAGER.lock();
    let mut layout = WindowLayout {
        name: String::from(name),
        windows: Vec::new(),
    };
    for w in &wm.windows {
        layout.windows.push(SavedWindowState {
            title: w.title.clone(),
            content_type: w.content_type,
            rect: w.rect,
            workspace: w.workspace,
            state: w.state,
            window_level: w.window_level,
        });
    }
    drop(wm);

    let mut layouts = SAVED_LAYOUTS.lock();
    // Replace existing layout with same name, or add new
    if let Some(existing) = layouts.iter_mut().find(|l| l.name == name) {
        *existing = layout;
    } else {
        if layouts.len() >= 8 {
            layouts.remove(0); // evict oldest if at capacity
        }
        layouts.push(layout);
    }
}

/// Restore a saved layout by name. Repositions existing windows to match.
pub fn restore_layout(name: &str) -> bool {
    let layouts = SAVED_LAYOUTS.lock();
    let layout = match layouts.iter().find(|l| l.name == name) {
        Some(l) => l.clone(),
        None => return false,
    };
    drop(layouts);

    let mut wm = WINDOW_MANAGER.lock();
    // Match saved windows to actual windows by content_type + title
    for saved in &layout.windows {
        if let Some(win) = wm
            .windows
            .iter_mut()
            .find(|w| w.content_type as u8 == saved.content_type as u8 && w.title == saved.title)
        {
            win.rect = saved.rect;
            win.saved_rect = saved.rect;
            win.workspace = saved.workspace;
            win.state = saved.state;
            win.window_level = saved.window_level;
        }
    }
    true
}

/// List saved layout names
pub fn list_layouts() -> Vec<String> {
    SAVED_LAYOUTS
        .lock()
        .iter()
        .map(|l| l.name.clone())
        .collect()
}

/// Delete a saved layout by name
pub fn delete_layout(name: &str) -> bool {
    let mut layouts = SAVED_LAYOUTS.lock();
    let len_before = layouts.len();
    layouts.retain(|l| l.name != name);
    layouts.len() < len_before
}
