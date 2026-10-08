/// Mouse scroll-wheel routing
use crate::gui::startmenu;
use crate::gui::window;
use crate::gui::window::WindowContentType;

use super::config::MOUSE_SETTINGS;

/// Handle mouse scroll wheel events
/// scroll_z: positive = scroll up, negative = scroll down
pub(super) fn handle_scroll(x: i32, y: i32, scroll_z: i8, screen_w: u32, screen_h: u32) {
    // File picker modal — intercept scroll when visible
    if crate::gui::file_picker::handle_scroll(x, y, scroll_z) {
        return;
    }

    let settings = MOUSE_SETTINGS.lock();
    let lines = settings.scroll_lines as usize;
    drop(settings);

    // Check if scrolling over a terminal window
    let wm = window::WINDOW_MANAGER.lock();
    if let Some(wid) = wm.window_at_inner(x, y) {
        if let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) {
            if win.content_type == WindowContentType::Terminal {
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
            if win.content_type == WindowContentType::FileExplorer
                || win.content_type == WindowContentType::Browser
                || win.content_type == WindowContentType::Settings
                || win.content_type == WindowContentType::AIAssistant
                || win.content_type == WindowContentType::TextEditor
                || win.content_type == WindowContentType::ArchiveViewer
                || win.content_type == WindowContentType::DiskUtility
                || win.content_type == WindowContentType::BluetoothManager
                || win.content_type == WindowContentType::CalendarApp
                || win.content_type == WindowContentType::LogViewer
                || win.content_type == WindowContentType::SoftwareUpdater
                || win.content_type == WindowContentType::SoftwareCenter
                || win.content_type == WindowContentType::SetupWizard
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

    // Check if scrolling over start menu
    if startmenu::is_visible() {
        startmenu::handle_scroll(scroll_z);
    }
}
