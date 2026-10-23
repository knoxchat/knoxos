/// Popup listing all windows for an app when its dock icon is used.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::window;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Running App Window List — popup showing all windows for an app
// when clicking its dock icon. Groups by content_type.
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

struct AppWindowList {
    visible: bool,
    content_type: window::WindowContentType,
    windows: Vec<(window::WindowId, String, bool)>, // (id, title, focused)
    popup_x: i32,
    popup_y: i32,
    hovered_idx: Option<usize>,
}

lazy_static::lazy_static! {
    static ref APP_WINDOW_LIST: Mutex<AppWindowList> = Mutex::new(AppWindowList {
        visible: false,
        content_type: window::WindowContentType::ShmEmpty,
        windows: Vec::new(),
        popup_x: 0,
        popup_y: 0,
        hovered_idx: None,
    });
}

/// Show the running app window list popup for a given content type
pub fn show_app_window_list(ct: window::WindowContentType, center_x: i32, dock_top_y: i32) {
    let wm = window::WINDOW_MANAGER.lock();
    let mut windows: Vec<(window::WindowId, String, bool)> = wm
        .windows
        .iter()
        .filter(|w| w.content_type == ct && w.visible)
        .map(|w| (w.id, w.title.clone(), w.focused))
        .collect();
    drop(wm);

    if windows.len() <= 1 {
        // No list needed for single window
        close_app_window_list();
        return;
    }

    // Sort focused first, then alphabetical
    windows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));

    let item_h = 28i32;
    let popup_w = 220i32;
    let popup_h = windows.len() as i32 * item_h + 12; // 6px padding top+bottom
    let popup_x = (center_x - popup_w / 2).max(8);
    let popup_y = dock_top_y - popup_h - 8;

    let mut list = APP_WINDOW_LIST.lock();
    list.visible = true;
    list.content_type = ct;
    list.windows = windows;
    list.popup_x = popup_x;
    list.popup_y = popup_y;
    list.hovered_idx = None;
    crate::gui::request_redraw();
}

/// Close the app window list popup
pub fn close_app_window_list() {
    let mut list = APP_WINDOW_LIST.lock();
    if list.visible {
        list.visible = false;
        crate::gui::request_redraw();
    }
}

/// Check if the app window list is visible
pub fn is_app_window_list_visible() -> bool {
    APP_WINDOW_LIST.lock().visible
}

/// Update hover in the app window list
pub fn update_app_window_list_hover(mx: i32, my: i32) {
    let mut list = APP_WINDOW_LIST.lock();
    if !list.visible {
        return;
    }

    let item_h = 28i32;
    let padding = 6i32;
    let px = list.popup_x;
    let py = list.popup_y;
    let pw = 220i32;
    let ph = list.windows.len() as i32 * item_h + padding * 2;

    if mx < px || mx > px + pw || my < py || my > py + ph {
        list.hovered_idx = None;
        return;
    }

    let rel_y = my - py - padding;
    if rel_y >= 0 {
        let idx = (rel_y / item_h) as usize;
        if idx < list.windows.len() {
            list.hovered_idx = Some(idx);
        } else {
            list.hovered_idx = None;
        }
    }
}

/// Handle click in the app window list (returns true if consumed)
pub fn handle_app_window_list_click(mx: i32, my: i32) -> bool {
    let list = APP_WINDOW_LIST.lock();
    if !list.visible {
        return false;
    }

    let item_h = 28i32;
    let padding = 6i32;
    let px = list.popup_x;
    let py = list.popup_y;
    let pw = 220i32;
    let ph = list.windows.len() as i32 * item_h + padding * 2;

    if mx < px || mx > px + pw || my < py || my > py + ph {
        drop(list);
        close_app_window_list();
        return false;
    }

    let rel_y = my - py - padding;
    if rel_y >= 0 {
        let idx = (rel_y / item_h) as usize;
        if let Some((wid, _, _)) = list.windows.get(idx) {
            let wid = *wid;
            drop(list);
            close_app_window_list();
            window::WINDOW_MANAGER.lock().focus_window(wid);
            crate::gui::request_redraw();
            return true;
        }
    }
    drop(list);
    close_app_window_list();
    true
}

/// Draw the running app window list popup
pub fn draw_app_window_list(fb: &mut FrameBuffer) {
    let list = APP_WINDOW_LIST.lock();
    if !list.visible || list.windows.is_empty() {
        return;
    }

    let item_h = 28i32;
    let padding = 6i32;
    let popup_w = 220u32;
    let popup_h = (list.windows.len() as i32 * item_h + padding * 2) as u32;

    let rect = Rect::new(list.popup_x, list.popup_y, popup_w, popup_h);

    // Glass background
    fb.fill_rounded_rect_aa(rect, Pixel::new(20, 25, 38, 235), 10);
    fb.draw_rounded_rect(rect, Pixel::new(80, 140, 240, 50), 10, 1);

    // Arrow pointing down
    let arrow_cx = list.popup_x + popup_w as i32 / 2;
    let arrow_base = list.popup_y + popup_h as i32;
    for i in 0..5i32 {
        let half = 5 - i;
        fb.fill_rect(
            Rect::new(arrow_cx - half, arrow_base + i, (half * 2) as u32, 1),
            Pixel::new(20, 25, 38, 235),
        );
    }

    for (i, (_, title, focused)) in list.windows.iter().enumerate() {
        let iy = list.popup_y + padding + i as i32 * item_h;
        let hovered = list.hovered_idx == Some(i);

        // Highlight
        if hovered || *focused {
            let bg = if hovered {
                Pixel::new(60, 120, 220, 60)
            } else {
                Pixel::new(40, 80, 160, 30)
            };
            fb.fill_rounded_rect_aa(
                Rect::new(list.popup_x + 4, iy, popup_w - 8, item_h as u32),
                bg,
                6,
            );
        }

        // Type icon
        let icon_cx = list.popup_x + 18;
        let icon_cy = iy + item_h / 2;
        crate::gui::alt_tab::draw_content_type_icon(fb, icon_cx, icon_cy, list.content_type);

        // Title
        let max_chars = 26usize;
        let display = if title.len() > max_chars {
            let mut t = String::from(&title[..max_chars - 2]);
            t.push_str("..");
            t
        } else {
            title.clone()
        };
        let text_color = if *focused {
            Pixel::new(120, 200, 255, 255)
        } else {
            Pixel::new(190, 200, 220, 230)
        };
        font_engine::draw_ui_text(fb, list.popup_x + 34, iy + 8, &display, 12, text_color);

        // Focused indicator dot
        if *focused {
            let dot_x = list.popup_x + popup_w as i32 - 14;
            let dot_y = iy + item_h / 2;
            fb.fill_circle_aa(dot_x, dot_y, 3, Pixel::new(0, 200, 255, 200));
        }
    }
}
