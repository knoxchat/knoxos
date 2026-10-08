/// Click, double-click, and right-click handling
use alloc::format;

use crate::gui::framebuffer::Rect;
use crate::gui::window::{self, ExplorerSort, WindowContentType, WindowId};

use super::actions::{build_context_menu, confirm_rename, execute_action, open_file_in_editor};
use super::address::start_address_edit;
use super::entries::{read_entries, sort_entries, toggle_sort};
use super::nav::{navigate_back, navigate_forward, navigate_to, navigate_up};
use super::path::current_path;

// ═════════════════════════════════════════════════════════════════════════
// CLICK HANDLING
// ═════════════════════════════════════════════════════════════════════════

/// Layout constants (must match draw_file_explorer_content in window.rs)
const NAV_H: i32 = 35;
const HEADER_H: i32 = 20;
const STATUS_H: i32 = 23;
const ITEM_H: i32 = 28;

/// Handle a single click inside the file explorer content area
/// Returns true if the click was consumed
pub fn handle_click(wid: WindowId, x: i32, y: i32) -> bool {
    let mut wm = window::WINDOW_MANAGER.lock();
    let win = match wm.windows.iter_mut().find(|w| w.id == wid) {
        Some(w) => w,
        None => return false,
    };

    if win.content_type != WindowContentType::FileExplorer {
        return false;
    }

    // Close context menu if open
    if win.explorer_ctx_menu.is_some() {
        // Check if clicking inside the context menu
        if let Some(ref ctx) = win.explorer_ctx_menu {
            let menu_w = 180;
            let menu_h = ctx.items.len() as i32 * 28 + 8;
            let menu_rect = Rect::new(ctx.x, ctx.y, menu_w as u32, menu_h as u32);
            if menu_rect.contains(x, y) {
                // Click on a menu item
                let item_idx = ((y - ctx.y - 4) / 28) as usize;
                if item_idx < ctx.items.len() && ctx.items[item_idx].enabled {
                    let action = ctx.items[item_idx].action;
                    drop(wm);
                    execute_action(wid, action);
                    return true;
                }
                return true;
            }
        }
        win.explorer_ctx_menu = None;
        return true;
    }

    // Exit rename mode if clicking elsewhere
    if win.explorer_renaming.is_some() {
        drop(wm);
        confirm_rename(wid);
        return true;
    }

    let content = win.content_rect();

    // Check navigation buttons (back/forward/up)
    let nav_y = content.y;
    if y >= nav_y && y < nav_y + NAV_H {
        // Back button area: x in [content.x+4 .. content.x+22]
        if x >= content.x + 4 && x < content.x + 22 {
            drop(wm);
            navigate_back(wid);
            return true;
        }
        // Forward button area: x in [content.x+24 .. content.x+42]
        if x >= content.x + 24 && x < content.x + 42 {
            drop(wm);
            navigate_forward(wid);
            return true;
        }
        // Up button area: x in [content.x+46 .. content.x+72]
        if x >= content.x + 46 && x < content.x + 72 {
            drop(wm);
            navigate_up(wid);
            return true;
        }
        // Address bar area: click to edit
        if x >= content.x + 80 {
            drop(wm);
            start_address_edit(wid);
            return true;
        }
        return false;
    }

    // Check sidebar bookmark clicks
    let sidebar_w: i32 = if win.explorer_sidebar_visible { 160 } else { 0 };
    let sidebar_y = content.y + NAV_H;

    if win.explorer_sidebar_visible && x >= content.x && x < content.x + sidebar_w && y >= sidebar_y
    {
        // Bookmark items start at sidebar_y + 24, each 26px tall
        let bookmark_paths = [
            "/home/user",
            "/home/user/Desktop",
            "/home/user/Documents",
            "/home/user/Downloads",
            "/home/user/Pictures",
            "/home/user/Music",
            "/",
        ];
        let item_start_y = sidebar_y + 24;
        for (i, path) in bookmark_paths.iter().enumerate() {
            let by = item_start_y + (i as i32 * 26);
            if y >= by && y < by + 24 {
                drop(wm);
                navigate_to(wid, path);
                return true;
            }
        }
        return true;
    }

    // Adjust click coordinates for sidebar offset
    let list_offset_x = sidebar_w;

    // Check column header clicks for sorting
    let header_y = content.y + NAV_H;
    if y >= header_y && y < header_y + HEADER_H {
        let name_col = content.x + list_offset_x + 12;
        let perms_col = content.x + content.width as i32 - 270;
        let size_col = content.x + content.width as i32 - 180;
        let type_col = content.x + content.width as i32 - 100;

        let sort_col = if x >= type_col {
            ExplorerSort::Type
        } else if x >= size_col && content.width > 250 {
            ExplorerSort::Size
        } else {
            ExplorerSort::Name
        };

        drop(wm);
        toggle_sort(wid, sort_col);
        return true;
    }

    // Check file/folder entry clicks
    let entries_y = content.y + NAV_H + HEADER_H + 4;
    let list_h = content.height as i32 - NAV_H - HEADER_H - 4 - STATUS_H;
    let scroll_item_offset = (win.scroll_y / ITEM_H.max(1)) as usize;

    if y >= entries_y && y < entries_y + list_h {
        let clicked_vi = ((y - entries_y) / ITEM_H) as usize + scroll_item_offset;
        win.explorer_selected = clicked_vi as i32;
        return true;
    }

    false
}

/// Handle a double-click inside the file explorer content area
/// Returns true if the click was consumed
pub fn handle_double_click(wid: WindowId, x: i32, y: i32) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    let win = match wm.windows.iter().find(|w| w.id == wid) {
        Some(w) => w,
        None => return false,
    };

    if win.content_type != WindowContentType::FileExplorer {
        return false;
    }

    let content = win.content_rect();
    let entries_y = content.y + NAV_H + HEADER_H + 4;
    let list_h = content.height as i32 - NAV_H - HEADER_H - 4 - STATUS_H;
    let scroll_item_offset = (win.scroll_y / ITEM_H.max(1)) as usize;
    let cur_path = current_path(&win.title);
    let show_hidden = win.explorer_show_hidden;
    let sort = win.explorer_sort;
    let sort_asc = win.explorer_sort_asc;
    drop(wm);

    if y >= entries_y && y < entries_y + list_h {
        let clicked_vi = ((y - entries_y) / ITEM_H) as usize + scroll_item_offset;

        // Read entries and find the clicked one
        let mut entries = read_entries(&cur_path, show_hidden);
        sort_entries(&mut entries, sort, sort_asc);

        if let Some(entry) = entries.get(clicked_vi) {
            let full_path = if cur_path == "/" {
                format!("/{}", entry.name)
            } else {
                format!("{}/{}", cur_path, entry.name)
            };

            if entry.is_dir {
                navigate_to(wid, &full_path);
            } else {
                open_file_in_editor(&full_path);
            }
            return true;
        }
    }

    false
}

/// Handle a right-click inside the file explorer content area
/// Returns true if the click was consumed
pub fn handle_right_click(wid: WindowId, x: i32, y: i32) -> bool {
    let wm = window::WINDOW_MANAGER.lock();
    let win = match wm.windows.iter().find(|w| w.id == wid) {
        Some(w) => w,
        None => return false,
    };

    if win.content_type != WindowContentType::FileExplorer {
        return false;
    }

    let content = win.content_rect();
    let entries_y = content.y + NAV_H + HEADER_H + 4;
    let list_h = content.height as i32 - NAV_H - HEADER_H - 4 - STATUS_H;
    let scroll_item_offset = (win.scroll_y / ITEM_H.max(1)) as usize;
    let cur_path = current_path(&win.title);
    let show_hidden = win.explorer_show_hidden;
    let sort = win.explorer_sort;
    let sort_asc = win.explorer_sort_asc;
    drop(wm);

    let has_clipboard = !crate::clipboard::is_empty();

    // Determine which entry was right-clicked (if any)
    let mut target_index = None;
    let mut target_path = cur_path.clone();

    if y >= entries_y && y < entries_y + list_h {
        let clicked_vi = ((y - entries_y) / ITEM_H) as usize + scroll_item_offset;

        let mut entries = read_entries(&cur_path, show_hidden);
        sort_entries(&mut entries, sort, sort_asc);

        if let Some(entry) = entries.get(clicked_vi) {
            target_index = Some(clicked_vi);
            target_path = if cur_path == "/" {
                format!("/{}", entry.name)
            } else {
                format!("{}/{}", cur_path, entry.name)
            };
        }
    }

    let ctx = build_context_menu(x, y, target_index, target_path, has_clipboard);

    let mut wm = window::WINDOW_MANAGER.lock();
    if let Some(win) = wm.windows.iter_mut().find(|w| w.id == wid) {
        win.explorer_selected = target_index.map(|i| i as i32).unwrap_or(-1);
        win.explorer_ctx_menu = Some(ctx);
    }

    true
}
