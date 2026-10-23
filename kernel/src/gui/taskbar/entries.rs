/// Running-app entries, clock text, hover, and dock click handling.
use alloc::string::String;

use crate::gui::framebuffer::Rect;
use crate::gui::window;

use super::layout::compute_dock_layout;
use super::state::{DOCK_BAR_HEIGHT, DOCK_ITEM_SIZE, MAX_DOCK_ENTRIES, TASKBAR, TaskbarEntry};

/// Update the clock display (called from timer interrupt)
pub fn update_clock(_ticks: u64) {
    let mut taskbar = TASKBAR.lock();

    // Read real wall-clock time from CMOS RTC (works correctly in QEMU)
    let dt = crate::rtc::local_now();
    taskbar.clock_text = crate::gui::locale::format_time(dt.hour, dt.minute);
}

/// Get the current clock text (for change detection)
pub fn get_clock_text() -> String {
    TASKBAR.lock().clock_text.clone()
}

/// Add an entry to the taskbar when a window opens
pub fn add_entry(window_id: window::WindowId, title: &str) {
    // Determine content type from the window manager
    let content_type = {
        let wm = window::WINDOW_MANAGER.lock();
        wm.windows
            .iter()
            .find(|w| w.id == window_id)
            .map(|w| w.content_type)
            .unwrap_or(window::WindowContentType::ShmEmpty)
    };
    let mut taskbar = TASKBAR.lock();
    taskbar.entries.push(TaskbarEntry {
        window_id,
        title: String::from(title),
        active: true,
        content_type,
        badge_count: 0,
    });
}

/// Remove an entry from the taskbar when a window closes
pub fn remove_entry(window_id: window::WindowId) {
    let mut taskbar = TASKBAR.lock();
    taskbar.entries.retain(|e| e.window_id != window_id);
}

/// Set notification badge count for a specific dock entry (window)
pub fn set_badge(window_id: window::WindowId, count: u32) {
    let mut taskbar = TASKBAR.lock();
    if let Some(entry) = taskbar
        .entries
        .iter_mut()
        .find(|e| e.window_id == window_id)
    {
        entry.badge_count = count;
    }
}

/// Clear notification badge for a specific dock entry
pub fn clear_badge(window_id: window::WindowId) {
    set_badge(window_id, 0);
}

/// Set the active entry (focused window)
pub fn set_active(window_id: window::WindowId) {
    let mut taskbar = TASKBAR.lock();
    for entry in taskbar.entries.iter_mut() {
        entry.active = entry.window_id == window_id;
    }
}

/// Update hover state based on current mouse position in floating dock
pub fn update_hover(mouse_x: i32, mouse_y: i32, screen_width: u32, screen_height: u32) {
    let mut taskbar = TASKBAR.lock();
    let entry_count = taskbar.entries.len().min(MAX_DOCK_ENTRIES);
    let layout = compute_dock_layout(entry_count, screen_width, screen_height);

    // Check if mouse is even in dock area
    if mouse_y < layout.dock_y
        || mouse_y > layout.dock_y + DOCK_BAR_HEIGHT as i32
        || mouse_x < layout.dock_x
        || mouse_x > layout.dock_x + layout.dock_w as i32
    {
        taskbar.hovered_entry = None;
        return;
    }

    // Start button + workspace area + spacing
    let mut cursor_x =
        layout.content_x + layout.start_btn_w + layout.entry_spacing + layout.workspace_area_w;

    // Check each entry
    let mut hovered = None;
    for (i, _entry) in taskbar.entries.iter().enumerate() {
        if i >= MAX_DOCK_ENTRIES {
            break;
        }
        let item_rect = Rect::new(
            cursor_x,
            layout.dock_cy - DOCK_ITEM_SIZE as i32 / 2,
            DOCK_ITEM_SIZE,
            DOCK_ITEM_SIZE,
        );
        if item_rect.contains(mouse_x, mouse_y) {
            hovered = Some(i);
            break;
        }
        cursor_x += DOCK_ITEM_SIZE as i32;
    }
    taskbar.hovered_entry = hovered;

    let _ = cursor_x;
}

/// Clear hover state (called when mouse leaves taskbar area)
pub fn clear_hover() {
    TASKBAR.lock().hovered_entry = None;
}

/// Handle taskbar/dock click
pub fn handle_click(x: i32, y: i32, screen_width: u32, screen_height: u32) -> bool {
    let taskbar = TASKBAR.lock();
    let entry_count = taskbar.entries.len().min(MAX_DOCK_ENTRIES);
    let layout = compute_dock_layout(entry_count, screen_width, screen_height);
    let entry_spacing = layout.entry_spacing;
    let separator_w = layout.separator_w;
    let workspace_dot_size = layout.workspace_dot_size;
    let workspace_dot_gap = layout.workspace_dot_gap;
    let workspace_count = layout.workspace_count;
    let dock_x = layout.dock_x;
    let dock_y = layout.dock_y;
    let dock_w = layout.dock_w;
    let dock_cy = layout.dock_cy;

    // Check if click is even on the dock
    if y < dock_y || y > dock_y + DOCK_BAR_HEIGHT as i32 || x < dock_x || x > dock_x + dock_w as i32
    {
        return false;
    }

    let mut cursor_x = layout.content_x;

    // ── START LAUNCHER BUTTON ──
    let start_rect = Rect::new(
        cursor_x,
        dock_cy - DOCK_ITEM_SIZE as i32 / 2,
        DOCK_ITEM_SIZE,
        DOCK_ITEM_SIZE,
    );
    if start_rect.contains(x, y) {
        drop(taskbar);
        crate::gui::popups::close_all_popups();
        crate::gui::notifications::close_panel();
        crate::gui::startmenu::toggle();
        crate::serial_println!("[KnoxOS] Start menu toggled");
        return true;
    }
    cursor_x += DOCK_ITEM_SIZE as i32 + entry_spacing;

    // ── WORKSPACE DOTS (click to switch) ──
    for ws_i in 0..workspace_count {
        let dot_rect = Rect::new(
            cursor_x,
            dock_cy - workspace_dot_size / 2,
            workspace_dot_size as u32,
            workspace_dot_size as u32,
        );
        if dot_rect.contains(x, y) {
            drop(taskbar);
            let mut wm = window::WINDOW_MANAGER.lock();
            wm.switch_workspace(ws_i as u8);
            drop(wm);
            crate::gui::request_redraw();
            return true;
        }
        cursor_x += workspace_dot_size + workspace_dot_gap;
    }
    cursor_x += entry_spacing - workspace_dot_gap;

    // Close start menu on any other dock interaction
    let taskbar = if crate::gui::startmenu::is_visible() {
        drop(taskbar);
        crate::gui::startmenu::close();
        TASKBAR.lock()
    } else {
        taskbar
    };

    // ── APP ENTRIES ──
    for (i, entry) in taskbar.entries.iter().enumerate() {
        if i >= MAX_DOCK_ENTRIES {
            break;
        }

        let item_rect = Rect::new(
            cursor_x,
            dock_cy - DOCK_ITEM_SIZE as i32 / 2,
            DOCK_ITEM_SIZE,
            DOCK_ITEM_SIZE,
        );

        if item_rect.contains(x, y) {
            let wid = entry.window_id;
            let is_active = entry.active;
            drop(taskbar);

            crate::gui::popups::close_all_popups();
            crate::gui::notifications::close_panel();

            let mut wm = window::WINDOW_MANAGER.lock();
            if is_active {
                // Click on active: minimize
                wm.minimize_window(wid);
                let new_focus = wm.focused_window;
                drop(wm);

                let mut tb = TASKBAR.lock();
                for e in tb.entries.iter_mut() {
                    e.active = new_focus == Some(e.window_id);
                }
                crate::serial_println!("[KnoxOS] Dock: minimized window {}", wid);
            } else {
                // Click on inactive: focus/restore
                wm.focus_window(wid);
                drop(wm);
                set_active(wid);
                crate::serial_println!("[KnoxOS] Dock: focused window {}", wid);
            }
            return true;
        }

        cursor_x += DOCK_ITEM_SIZE as i32;
    }

    let _ = (cursor_x, separator_w, dock_cy);
    true
}

/// Focus or minimize the dock entry at `index` (D9.1 GPUI panel).
pub fn activate_dock_index(index: usize) {
    let (wid, is_active) = {
        let taskbar = TASKBAR.lock();
        match taskbar.entries.get(index) {
            Some(e) => (e.window_id, e.active),
            None => return,
        }
    };
    crate::gui::popups::close_all_popups();
    crate::gui::notifications::close_panel();
    let mut wm = window::WINDOW_MANAGER.lock();
    if is_active {
        wm.minimize_window(wid);
        let new_focus = wm.focused_window;
        drop(wm);
        let mut tb = TASKBAR.lock();
        for e in tb.entries.iter_mut() {
            e.active = new_focus == Some(e.window_id);
        }
        crate::serial_println!("[KnoxOS] Dock: minimized window {}", wid);
    } else {
        wm.focus_window(wid);
        drop(wm);
        set_active(wid);
        crate::serial_println!("[KnoxOS] Dock: focused window {}", wid);
    }
}
