/// Desktop right-click context menu
use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::icon_theme;
use crate::gui::icon_theme::IconCategory;

use super::apps::open_application;
use super::files::{create_desktop_file, create_desktop_folder};
use super::types::{CONTEXT_MENU, ContextAction, IconType};

// ═══════════════════════════════════════════════════════════════════════════
// CONTEXT MENU — Right-click desktop menu
// ═══════════════════════════════════════════════════════════════════════════

pub(crate) const CONTEXT_MENU_WIDTH: u32 = 200;
pub(crate) const CONTEXT_ITEM_HEIGHT: u32 = 28;
pub(crate) const CONTEXT_SEPARATOR_HEIGHT: u32 = 12;

/// Show the desktop context menu at the given position
pub fn show_context_menu(x: i32, y: i32) {
    let mut menu = CONTEXT_MENU.lock();
    menu.visible = true;

    // Calculate menu height
    let mut h: u32 = 8; // padding
    for item in &menu.items {
        h += if item.separator {
            CONTEXT_SEPARATOR_HEIGHT
        } else {
            CONTEXT_ITEM_HEIGHT
        };
    }

    // Clamp position to screen bounds
    let (sw, sh) = crate::gui::cached_screen_size();
    let (sw, sh) = (sw as usize, sh as usize);
    let mx = if x + CONTEXT_MENU_WIDTH as i32 > sw as i32 {
        sw as i32 - CONTEXT_MENU_WIDTH as i32 - 4
    } else {
        x
    };
    let my = if y + h as i32 > sh as i32 - crate::gui::scale::taskbar_height() as i32 {
        sh as i32 - crate::gui::scale::taskbar_height() as i32 - h as i32 - 4
    } else {
        y
    };

    menu.x = mx;
    menu.y = my;
    drop(menu);
    crate::gui::request_redraw();
}

/// Close the desktop context menu
pub fn close_context_menu() {
    let mut menu = CONTEXT_MENU.lock();
    if menu.visible {
        menu.visible = false;
        drop(menu);
        crate::gui::request_redraw();
    }
}

/// Draw the context menu if visible
pub(crate) fn draw_context_menu(fb: &mut FrameBuffer) {
    let menu = CONTEXT_MENU.lock();
    if !menu.visible {
        return;
    }

    // Calculate menu height
    let mut h: u32 = 8;
    for item in &menu.items {
        h += if item.separator {
            CONTEXT_SEPARATOR_HEIGHT
        } else {
            CONTEXT_ITEM_HEIGHT
        };
    }

    let mx = menu.x;
    let my = menu.y;

    // Shadow (rounded) — AA
    fb.fill_rounded_rect_aa(
        Rect::new(mx + 3, my + 3, CONTEXT_MENU_WIDTH, h),
        Pixel::new(0, 0, 0, 90),
        6,
    );

    // Background (rounded) — AA
    fb.fill_rounded_rect_aa(
        Rect::new(mx, my, CONTEXT_MENU_WIDTH, h),
        Pixel::new(38, 38, 38, 250),
        6,
    );

    // Border (rounded AA)
    fb.draw_rounded_rect(
        Rect::new(mx, my, CONTEXT_MENU_WIDTH, h),
        Pixel::rgb(70, 70, 70),
        6,
        1,
    );

    // Items
    let mut iy = my + 4;
    let mouse = crate::gui::input::MOUSE.lock();
    let mouse_x = mouse.x;
    let mouse_y = mouse.y;
    drop(mouse);

    for item in &menu.items {
        if item.separator {
            fb.draw_hline(
                mx + 12,
                iy + CONTEXT_SEPARATOR_HEIGHT as i32 / 2,
                CONTEXT_MENU_WIDTH - 24,
                Pixel::rgb(60, 60, 60),
            );
            iy += CONTEXT_SEPARATOR_HEIGHT as i32;
        } else {
            // Hover highlight with rounded corners
            let item_rect = Rect::new(mx + 4, iy, CONTEXT_MENU_WIDTH - 8, CONTEXT_ITEM_HEIGHT);
            if item_rect.contains(mouse_x, mouse_y) {
                fb.fill_rounded_rect_aa(item_rect, colors::SELECTION, 4);
            }

            // Small icon for each menu item via icon_theme (16×16)
            let icon_x = mx + 8;
            let icon_y = iy + (CONTEXT_ITEM_HEIGHT as i32 - 16) / 2;
            match item.action {
                ContextAction::OpenTerminal => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Apps,
                        "utilities-x-terminal",
                    );
                }
                ContextAction::OpenExplorer => {
                    icon_theme::draw_tiny_icon(fb, icon_x, icon_y, IconCategory::Places, "folder");
                }
                ContextAction::OpenBrowser => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Apps,
                        "web-browser",
                    );
                }
                ContextAction::Refresh => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Actions,
                        "view-refresh",
                    );
                }
                ContextAction::NewFile => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Actions,
                        "document-new",
                    );
                }
                ContextAction::NewFolder => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Actions,
                        "folder-new",
                    );
                }
                ContextAction::TakeScreenshot | ContextAction::TakeScreenshotRegion => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Apps,
                        "accessories-screenshot",
                    );
                }
                ContextAction::ChangeWallpaper => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Apps,
                        "accessories-painting",
                    );
                }
                ContextAction::DisplaySettings => {
                    icon_theme::draw_tiny_icon(
                        fb,
                        icon_x,
                        icon_y,
                        IconCategory::Apps,
                        "org.gnome.Settings",
                    );
                }
                _ => {}
            }

            font_engine::draw_ui_text(
                fb,
                mx + 30,
                iy + (CONTEXT_ITEM_HEIGHT as i32 - 12) / 2,
                &item.label,
                12,
                colors::WHITE,
            );
            iy += CONTEXT_ITEM_HEIGHT as i32;
        }
    }
}

/// Handle a click on the context menu, returns true if consumed
pub fn handle_context_menu_click(x: i32, y: i32) -> bool {
    let menu = CONTEXT_MENU.lock();
    if !menu.visible {
        return false;
    }

    let mx = menu.x;
    let mut iy = menu.y + 4;

    for item in &menu.items {
        if item.separator {
            iy += CONTEXT_SEPARATOR_HEIGHT as i32;
            continue;
        }

        let item_rect = Rect::new(mx + 2, iy, CONTEXT_MENU_WIDTH - 4, CONTEXT_ITEM_HEIGHT);
        if item_rect.contains(x, y) {
            let action = item.action;
            drop(menu);
            close_context_menu();

            match action {
                ContextAction::OpenTerminal => {
                    open_application("Terminal", IconType::Terminal);
                }
                ContextAction::OpenExplorer => {
                    open_application("Files", IconType::Folder);
                }
                ContextAction::OpenBrowser => {
                    open_application("Browser", IconType::Globe);
                }
                ContextAction::NewFile => {
                    create_desktop_file();
                }
                ContextAction::NewFolder => {
                    create_desktop_folder();
                }
                ContextAction::Refresh => {
                    crate::gui::request_redraw();
                }
                ContextAction::TakeScreenshot => {
                    crate::gui::screenshot::take_screenshot(
                        crate::gui::screenshot::CaptureMode::FullScreen,
                    );
                }
                ContextAction::TakeScreenshotRegion => {
                    crate::gui::screenshot::take_screenshot(
                        crate::gui::screenshot::CaptureMode::Region,
                    );
                }
                ContextAction::ChangeWallpaper => {
                    // Open Settings on Personalization tab
                    open_application("Settings", IconType::Settings);
                    crate::gui::settings::SETTINGS_STATE.lock().active_tab =
                        crate::gui::settings::SettingsTab::Personalization;
                }
                ContextAction::DisplaySettings => {
                    open_application("Settings", IconType::Settings);
                    crate::gui::settings::SETTINGS_STATE.lock().active_tab =
                        crate::gui::settings::SettingsTab::Display;
                }
                _ => {}
            }
            return true;
        }
        iy += CONTEXT_ITEM_HEIGHT as i32;
    }

    drop(menu);
    close_context_menu();
    false
}
