/// Mouse scroll-wheel routing
use crate::gui::startmenu;
use crate::gui::window;
use crate::gui::window::{ViewKind, WindowContentType};

use super::config::MOUSE_SETTINGS;

/// Handle mouse scroll wheel events
/// scroll_z: positive = scroll up, negative = scroll down
pub(super) fn handle_scroll(x: i32, y: i32, scroll_z: i8, screen_w: u32, screen_h: u32) {
    if !crate::gui::login::is_logged_in() || crate::gui::lock_screen::is_locked() {
        return;
    }
    // File picker modal — intercept scroll when visible
    if crate::gui::file_picker::is_visible() {
        if crate::gui::gpui_platform::dialogs_is_live() {
            crate::gui::gpui_platform::dialogs_scroll(x, y, scroll_z as i32);
            crate::gui::request_redraw();
            return;
        }
        if crate::gui::file_picker::handle_scroll(x, y, scroll_z) {
            return;
        }
    }

    let settings = MOUSE_SETTINGS.lock();
    let lines = settings.scroll_lines as usize;
    drop(settings);

    // Check if scrolling over a terminal window
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.window_at_inner(x, y) {
        if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
            if win.content_type == WindowContentType::TerminalSurface {
                // Route scroll to active terminal tab
                let active_term_id = if win.terminal_tabs.is_empty() {
                    wid
                } else {
                    win.terminal_tabs
                        .get(win.terminal_active_tab)
                        .copied()
                        .unwrap_or(wid)
                };
                drop(wm);
                if scroll_z > 0 {
                    crate::terminal::scroll_window(active_term_id, true, lines);
                } else {
                    crate::terminal::scroll_window(active_term_id, false, lines);
                }
                return;
            }
            // For other window types, handle scroll_y if applicable
            if win.content_type == WindowContentType::Gpui(ViewKind::Settings)
                && crate::gui::gpui_platform::settings_is_live()
            {
                drop(wm);
                crate::gui::gpui_platform::settings_scroll(x, y, scroll_z as i32);
                crate::gui::damage_window(wid);
                return;
            }
            if win.content_type == WindowContentType::Gpui(ViewKind::Tasks)
                && crate::gui::gpui_platform::tasks_is_live()
            {
                drop(wm);
                crate::gui::gpui_platform::tasks_scroll(x, y, scroll_z as i32);
                crate::gui::damage_window(wid);
                return;
            }
            if win.content_type == WindowContentType::Gpui(ViewKind::Editor)
                && crate::gui::gpui_platform::editor_is_live()
            {
                drop(wm);
                crate::gui::gpui_platform::editor_scroll(x, y, scroll_z as i32);
                crate::gui::damage_window(wid);
                return;
            }
            if win.content_type == WindowContentType::Gpui(ViewKind::Chat)
                && crate::gui::gpui_platform::chat_is_live()
            {
                drop(wm);
                crate::gui::gpui_platform::chat_scroll(x, y, scroll_z as i32);
                crate::gui::damage_window(wid);
                return;
            }
            if win.content_type == WindowContentType::Gpui(ViewKind::Browser)
                && crate::gui::gpui_platform::browse_is_live()
            {
                drop(wm);
                crate::gui::gpui_platform::browse_scroll(x, y, scroll_z as i32);
                crate::gui::damage_window(wid);
                return;
            }
            if crate::gui::gpui_platform::apps_is_live()
                && matches!(
                    win.content_type,
                    WindowContentType::Gpui(ViewKind::Archive)
                        | WindowContentType::Gpui(ViewKind::Disks)
                        | WindowContentType::Gpui(ViewKind::Bluetooth)
                        | WindowContentType::Gpui(ViewKind::Calendar)
                        | WindowContentType::Gpui(ViewKind::Logs)
                        | WindowContentType::Gpui(ViewKind::Updater)
                        | WindowContentType::Gpui(ViewKind::Software)
                        | WindowContentType::Gpui(ViewKind::Image)
                )
            {
                drop(wm);
                crate::gui::gpui_platform::apps_scroll(x, y, scroll_z as i32);
                crate::gui::damage_window(wid);
                return;
            }
            if win.content_type == WindowContentType::Gpui(ViewKind::Files)
                || win.content_type == WindowContentType::Gpui(ViewKind::Browser)
                || win.content_type == WindowContentType::Gpui(ViewKind::Settings)
                || win.content_type == WindowContentType::Gpui(ViewKind::Chat)
                || win.content_type == WindowContentType::Gpui(ViewKind::Editor)
                || win.content_type == WindowContentType::Gpui(ViewKind::Archive)
                || win.content_type == WindowContentType::Gpui(ViewKind::Disks)
                || win.content_type == WindowContentType::Gpui(ViewKind::Bluetooth)
                || win.content_type == WindowContentType::Gpui(ViewKind::Calendar)
                || win.content_type == WindowContentType::Gpui(ViewKind::Logs)
                || win.content_type == WindowContentType::Gpui(ViewKind::Updater)
                || win.content_type == WindowContentType::Gpui(ViewKind::Software)
                || win.content_type == WindowContentType::Gpui(ViewKind::Wizard)
            {
                drop(wm);
                // Route scroll to generic window scroll state
                let mut wm = window::WINDOW_MANAGER.lock();
                if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
                    if scroll_z > 0 {
                        win.scroll_by(-(lines as i32 * 20));
                    } else {
                        win.scroll_by(lines as i32 * 20);
                    }
                }
                return;
            }
        }
    }
    drop(wm);

    if crate::gui::gpui_platform::scroll(x, y, scroll_z as i32) {
        crate::gui::gpui_platform::damage_proof();
        return;
    }

    // Check if scrolling over start menu
    if startmenu::is_visible() && !crate::gui::gpui_platform::launcher_is_live() {
        startmenu::handle_scroll(scroll_z);
    }
}
