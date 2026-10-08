/// Overlay hover tracking and title-bar button hit tests
use crate::gui::desktop;
use crate::gui::startmenu;
use crate::gui::taskbar;
use crate::gui::window;

/// Check if the mouse is over an interactive OVERLAY element that has visible
/// hover-highlight state changes. Only these truly need a full desktop redraw.
///
/// Elements that just change cursor shape (resize edges, title bar, desktop icons)
/// do NOT need a full redraw — the cursor type is updated separately via
/// `update_cursor_for_position()` and the cursor-only fast path handles it.
///
/// This dramatically reduces full redraws during normal mouse movement, keeping
/// the cursor responsive even with expensive AA rendering.
pub(super) fn needs_overlay_hover_redraw(x: i32, y: i32, screen_w: i32, screen_h: i32) -> bool {
    // Context menu visible — items have hover highlight
    if desktop::CONTEXT_MENU.lock().visible {
        return true;
    }
    // Start menu visible — items have hover highlight
    if startmenu::is_visible() {
        return true;
    }
    // System popups visible — need redraw for interaction
    if crate::gui::popups::any_popup_open() {
        return true;
    }
    // Notification panel open — need redraw for interaction
    if crate::gui::notifications::is_panel_open() {
        return true;
    }
    // If dragging or resizing a window, the drag handler pushes targeted
    // damage rects — no full redraw needed on the hover path.
    // (This check is defensive: mouse.left_button should be false here
    // since we're in the 'else if moved' branch, not the drag branch.)
    {
        let wm = window::WINDOW_MANAGER.lock();
        if wm.any_dragging() || wm.any_resizing() {
            return false;
        }
    }
    // Over the taskbar — only redraw if hover state actually changed
    let taskbar_y = screen_h - crate::gui::scale::taskbar_height() as i32;
    // Update tray context menu hover even if above taskbar
    if crate::gui::system_tray::is_context_menu_open() {
        crate::gui::system_tray::update_context_menu_hover(x, y);
    }
    if y >= taskbar_y {
        // Pre-compute the new hover state and compare with current
        // If unchanged, skip the full redraw (just cursor-only)
        let old_hover = taskbar::TASKBAR.lock().hovered_entry;
        // update_hover is called in draw_desktop, but we can peek here cheaply
        // by checking if the mouse is over any entry
        // If we return true and old_hover == new_hover, it's wasted work.
        // So instead: always update hover, and only redraw if it changed.
        taskbar::update_hover(x, y, screen_w as u32, screen_h as u32);
        taskbar::update_window_preview(screen_w as u32, screen_h as u32);
        crate::gui::system_tray::update_context_menu_hover(x, y);
        let new_hover = taskbar::TASKBAR.lock().hovered_entry;
        if old_hover != new_hover {
            return true;
        }
        // Hover didn't change — cursor-only is fine
        return false;
    }

    // Window title-bar buttons have visible hover highlights (close→red,
    // max/min→subtle glow). Trigger a full redraw so these highlights render.
    {
        let wm = window::WINDOW_MANAGER.lock();
        if let Some(wid) = wm.window_at(x, y) {
            if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
                if (win.closeable && win.close_button_rect().contains(x, y))
                    || (win.maximizable && win.maximize_button_rect().contains(x, y))
                    || (win.minimizable && win.minimize_button_rect().contains(x, y))
                {
                    return true;
                }
            }
        }
    }

    false
}

/// Check if a point is on any window's title-bar button (close/max/min).
/// Used to prevent double-click detection from swallowing button clicks.
pub(super) fn is_on_window_button(x: i32, y: i32) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.window_at(x, y) {
        if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
            return (win.closeable && win.close_button_rect().contains(x, y))
                || (win.maximizable && win.maximize_button_rect().contains(x, y))
                || (win.minimizable && win.minimize_button_rect().contains(x, y));
        }
    }
    false
}
