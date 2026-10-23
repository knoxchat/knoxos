/// Right-click menu on the legacy dock.
use spin::Mutex;

use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::state::{is_auto_hide, set_auto_hide};

// ═══════════════════════════════════════════════════════════════════════════
// TASKBAR CONTEXT MENU (right-click on taskbar)
// ═══════════════════════════════════════════════════════════════════════════

/// Taskbar context menu state
pub struct TaskbarContextMenu {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
    pub hovered_item: Option<usize>,
}

lazy_static::lazy_static! {
    pub static ref TASKBAR_CTX_MENU: Mutex<TaskbarContextMenu> = Mutex::new(TaskbarContextMenu {
        visible: false,
        x: 0,
        y: 0,
        hovered_item: None,
    });
}

/// Context menu items for the taskbar
const TASKBAR_CTX_ITEMS: &[&str] = &[
    "Taskbar Settings",
    "Toggle Auto-hide",
    "Lock Screen",
    "Log Out",
];

/// Show taskbar context menu at the given position
pub fn show_context_menu(x: i32, y: i32) {
    let item_h = 28i32;
    let menu_h = TASKBAR_CTX_ITEMS.len() as i32 * item_h + 8;
    let mut ctx = TASKBAR_CTX_MENU.lock();
    ctx.visible = true;
    ctx.x = x;
    ctx.y = y - menu_h; // Show above the click point
    ctx.hovered_item = None;
}

/// Close the taskbar context menu
pub fn close_context_menu() {
    TASKBAR_CTX_MENU.lock().visible = false;
}

/// Check if context menu is visible
pub fn is_context_menu_open() -> bool {
    TASKBAR_CTX_MENU.lock().visible
}

/// Draw the taskbar context menu
pub fn draw_context_menu(fb: &mut FrameBuffer) {
    let ctx = TASKBAR_CTX_MENU.lock();
    if !ctx.visible {
        return;
    }

    let item_h = 28i32;
    let menu_w: u32 = 180;
    let menu_h = TASKBAR_CTX_ITEMS.len() as u32 * item_h as u32 + 8;
    let menu_x = ctx.x;
    let menu_y = ctx.y;

    // Background with rounded corners
    let bg = Pixel::rgb(36, 36, 42);
    fb.fill_rounded_rect_aa(Rect::new(menu_x, menu_y, menu_w, menu_h), bg, 8);
    fb.draw_rounded_rect(
        Rect::new(menu_x, menu_y, menu_w, menu_h),
        Pixel::rgb(60, 60, 66),
        8,
        1,
    );

    // Menu items
    for (i, label) in TASKBAR_CTX_ITEMS.iter().enumerate() {
        let item_y = menu_y + 4 + i as i32 * item_h;
        let is_hovered = ctx.hovered_item == Some(i);

        if is_hovered {
            fb.fill_rounded_rect_aa(
                Rect::new(menu_x + 4, item_y, menu_w - 8, item_h as u32),
                Pixel::rgb(55, 55, 62),
                4,
            );
        }

        let text_color = if is_hovered {
            colors::WHITE
        } else {
            Pixel::rgb(200, 200, 210)
        };

        font_engine::draw_ui_text(fb, menu_x + 14, item_y + 7, label, 12, text_color);
    }
}

/// Handle click on taskbar context menu. Returns true if handled.
pub fn handle_context_menu_click(x: i32, y: i32) -> bool {
    let ctx = TASKBAR_CTX_MENU.lock();
    if !ctx.visible {
        return false;
    }

    let item_h = 28i32;
    let menu_w = 180i32;
    let menu_h = TASKBAR_CTX_ITEMS.len() as i32 * item_h + 8;
    let menu_x = ctx.x;
    let menu_y = ctx.y;

    // Check if click is outside menu
    if x < menu_x || x > menu_x + menu_w || y < menu_y || y > menu_y + menu_h {
        drop(ctx);
        close_context_menu();
        return true;
    }

    // Determine which item was clicked
    let rel_y = y - menu_y - 4;
    let item_idx = rel_y / item_h;
    drop(ctx);
    close_context_menu();

    if item_idx >= 0 && (item_idx as usize) < TASKBAR_CTX_ITEMS.len() {
        match item_idx as usize {
            0 => {
                // Taskbar Settings: open settings window on Personalization tab
                crate::gui::desktop::open_application(
                    "Settings",
                    crate::gui::desktop::IconType::Settings,
                );
            }
            1 => {
                // Toggle auto-hide
                let current = is_auto_hide();
                set_auto_hide(!current);
                crate::serial_println!("[KnoxOS] Taskbar auto-hide: {}", !current);
            }
            2 => {
                // Lock screen
                crate::gui::lock_screen::lock();
            }
            3 => {
                // Log out
                crate::gui::sounds::logout();
                crate::gui::login::set_logged_in(false);
                crate::gui::login::reset();
            }
            _ => {}
        }
    }

    true
}
