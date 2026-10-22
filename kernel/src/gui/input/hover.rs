/// Overlay hover tracking, title-bar button hit tests, and in-window widget hover
use core::sync::atomic::{AtomicU64, Ordering};

use crate::gui::desktop;
use crate::gui::startmenu;
use crate::gui::taskbar;
use crate::gui::window;
use crate::gui::window::{ViewKind, WindowContentType};

/// Last widget hover token. When it changes, that window needs a composite so
/// buttons/tabs actually highlight under the pointer.
static LAST_HOVER_TOKEN: AtomicU64 = AtomicU64::new(0);

/// Check if the mouse is over an interactive OVERLAY element that has visible
/// hover-highlight state changes. Only these truly need a full desktop redraw.
///
/// Window chrome and in-window widgets push *window-local* damage instead.
pub(super) fn needs_overlay_hover_redraw(x: i32, y: i32, screen_w: i32, screen_h: i32) -> bool {
    if desktop::CONTEXT_MENU.lock().visible {
        return true;
    }
    if startmenu::is_visible() {
        return true;
    }
    if crate::gui::popups::any_popup_open() {
        return true;
    }
    if crate::gui::notifications::is_panel_open() {
        return true;
    }
    {
        let wm = window::WINDOW_MANAGER.lock();
        if wm.any_dragging() || wm.any_resizing() {
            return false;
        }
    }
    let taskbar_y = screen_h - crate::gui::scale::taskbar_height() as i32;
    if y >= taskbar_y {
        if crate::gui::gpui_platform::panel_is_live() {
            return false;
        }
        let old_hover = taskbar::TASKBAR.lock().hovered_entry;
        taskbar::update_hover(x, y, screen_w as u32, screen_h as u32);
        taskbar::update_window_preview(screen_w as u32, screen_h as u32);
        let new_hover = taskbar::TASKBAR.lock().hovered_entry;
        if old_hover != new_hover {
            crate::gui::request_taskbar_redraw();
        }
        return false;
    }

    // Title-bar buttons and in-window widgets: damage that window only.
    if hover_window_widgets(x, y) {
        return false;
    }

    false
}

/// Update in-window hover (chrome buttons, calculator keys, settings tabs…).
/// Returns true if a window-local redraw was requested.
fn hover_window_widgets(x: i32, y: i32) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    let Some(wid) = wm.window_at(x, y) else {
        LAST_HOVER_TOKEN.store(0, Ordering::Relaxed);
        return false;
    };
    let Some(win) = wm.windows.iter().rev().find(|w| w.id == wid) else {
        return false;
    };

    let chrome = (win.closeable && win.close_button_rect().contains(x, y))
        || (win.maximizable && win.maximize_button_rect().contains(x, y))
        || (win.minimizable && win.minimize_button_rect().contains(x, y));

    let token = hover_token(wid, win.content_type, win.content_rect(), x, y, chrome);
    let rect = win.rect;
    let calc_live = win.content_type == WindowContentType::Gpui(ViewKind::Calculator)
        && crate::gui::gpui_platform::calc_is_live();
    let settings_live = win.content_type == WindowContentType::Gpui(ViewKind::Settings)
        && crate::gui::gpui_platform::settings_is_live();
    let files_live = win.content_type == WindowContentType::Gpui(ViewKind::Files)
        && crate::gui::gpui_platform::files_is_live();
    let tasks_live = win.content_type == WindowContentType::Gpui(ViewKind::Tasks)
        && crate::gui::gpui_platform::tasks_is_live();
    let editor_live = win.content_type == WindowContentType::Gpui(ViewKind::Editor)
        && crate::gui::gpui_platform::editor_is_live();
    let chat_live = win.content_type == WindowContentType::Gpui(ViewKind::Chat)
        && crate::gui::gpui_platform::chat_is_live();
    let browse_live = win.content_type == WindowContentType::Gpui(ViewKind::Browser)
        && crate::gui::gpui_platform::browse_is_live();
    let term_live = win.content_type == WindowContentType::TerminalSurface
        && crate::gui::gpui_platform::term_is_live();
    let wizard_live = win.content_type == WindowContentType::Gpui(ViewKind::Wizard)
        && crate::gui::gpui_platform::dialogs_is_live();
    let apps_live = crate::gui::gpui_platform::apps_is_live()
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
        );
    drop(wm);

    if calc_live && crate::gui::gpui_platform::calc_pointer_moved(x, y) {
        crate::gui::request_window_redraw(rect);
        return true;
    }
    if settings_live && crate::gui::gpui_platform::settings_pointer_moved(x, y) {
        crate::gui::request_window_redraw(rect);
        return true;
    }
    if files_live && crate::gui::gpui_platform::files_pointer_moved(x, y) {
        crate::gui::request_window_redraw(rect);
        return true;
    }
    if tasks_live && crate::gui::gpui_platform::tasks_pointer_moved(x, y) {
        crate::gui::request_window_redraw(rect);
        return true;
    }
    if editor_live && crate::gui::gpui_platform::editor_pointer_moved(x, y) {
        crate::gui::request_window_redraw(rect);
        return true;
    }
    if chat_live && crate::gui::gpui_platform::chat_pointer_moved(x, y) {
        crate::gui::request_window_redraw(rect);
        return true;
    }
    if browse_live && crate::gui::gpui_platform::browse_pointer_moved(x, y) {
        crate::gui::request_window_redraw(rect);
        return true;
    }
    if term_live && crate::gui::gpui_platform::term_pointer_moved(x, y) {
        crate::gui::request_window_redraw(rect);
        return true;
    }
    if wizard_live && crate::gui::gpui_platform::dialogs_pointer_moved(x, y) {
        crate::gui::request_window_redraw(rect);
        return true;
    }
    if apps_live && crate::gui::gpui_platform::apps_pointer_moved(x, y) {
        crate::gui::request_window_redraw(rect);
        return true;
    }

    let old = LAST_HOVER_TOKEN.swap(token, Ordering::Relaxed);
    if old != token {
        crate::gui::request_window_redraw(rect);
        return true;
    }
    chrome
}

fn hover_token(
    wid: window::WindowId,
    kind: WindowContentType,
    content: crate::gui::framebuffer::Rect,
    x: i32,
    y: i32,
    chrome: bool,
) -> u64 {
    let mut cell: u64 = 0;
    if chrome {
        cell = 0x4000;
    } else if content.contains(x, y) {
        cell = match kind {
            WindowContentType::Gpui(ViewKind::Calculator) => calculator_cell(content, x, y),
            WindowContentType::Gpui(ViewKind::Settings) => settings_cell(content, x, y),
            WindowContentType::Gpui(ViewKind::Tasks) => row_cell(content, x, y, 32),
            WindowContentType::Gpui(ViewKind::Files) => row_cell(content, x, y, 28),
            WindowContentType::Gpui(ViewKind::Software)
            | WindowContentType::Gpui(ViewKind::Updater)
            | WindowContentType::Gpui(ViewKind::Wizard)
            | WindowContentType::Gpui(ViewKind::Logs)
            | WindowContentType::Gpui(ViewKind::Archive)
            | WindowContentType::Gpui(ViewKind::Disks)
            | WindowContentType::Gpui(ViewKind::Bluetooth)
            | WindowContentType::Gpui(ViewKind::Calendar)
            | WindowContentType::Gpui(ViewKind::Chat)
            | WindowContentType::Gpui(ViewKind::Image)
            | WindowContentType::Gpui(ViewKind::Browser)
            | WindowContentType::Gpui(ViewKind::Editor) => row_cell(content, x, y, 24),
            _ => 1,
        };
    }
    ((wid as u64) << 32) | cell
}

fn calculator_cell(area: crate::gui::framebuffer::Rect, x: i32, y: i32) -> u64 {
    let display_h = 80;
    let grid_y = area.y + display_h;
    if y < grid_y {
        return 1;
    }
    let grid_h = area.height as i32 - display_h;
    let btn_w = area.width as i32 / 4;
    let btn_h = grid_h / 5;
    if btn_w <= 0 || btn_h <= 0 {
        return 1;
    }
    let col = (x - area.x) / btn_w;
    let row = (y - grid_y) / btn_h;
    if !(0..5).contains(&row) || !(0..4).contains(&col) {
        return 1;
    }
    2 + (row as u64) * 4 + col as u64
}

fn settings_cell(area: crate::gui::framebuffer::Rect, x: i32, y: i32) -> u64 {
    const SIDEBAR_WIDTH: i32 = 160;
    const TAB_HEIGHT: i32 = 36;
    if x >= area.x && x < area.x + SIDEBAR_WIDTH {
        let ty = y - (area.y + 44);
        if ty >= 0 {
            return 10 + (ty / TAB_HEIGHT).max(0) as u64;
        }
        return 2;
    }
    100 + row_cell(area, x, y, 28)
}

fn row_cell(area: crate::gui::framebuffer::Rect, _x: i32, y: i32, row_h: i32) -> u64 {
    if row_h <= 0 {
        return 1;
    }
    2 + ((y - area.y).max(0) / row_h) as u64
}

/// Check if a point is on any window's title-bar button (close/max/min).
/// Used to prevent double-click detection from swallowing button clicks.
pub(super) fn is_on_window_button(x: i32, y: i32) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    for win in wm.windows.iter().rev() {
        if !win.is_visible() {
            continue;
        }
        if (win.closeable && win.close_button_rect().contains(x, y))
            || (win.maximizable && win.maximize_button_rect().contains(x, y))
            || (win.minimizable && win.minimize_button_rect().contains(x, y))
        {
            return true;
        }
    }
    false
}
