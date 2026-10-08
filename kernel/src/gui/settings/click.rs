/// Click handling for the Settings panel.
use crate::gui::framebuffer::Rect;

use super::display::{RES_LIST_X_PAD, RES_LIST_Y_START, RES_ROW_HEIGHT};
use super::{SIDEBAR_WIDTH, SettingsState, SettingsTab, TAB_HEIGHT, TABS};

/// Handle click on settings content.
/// `scroll_y` is the current window scroll offset for the content pane.
pub fn handle_settings_click(
    x: i32,
    y: i32,
    content_rect: Rect,
    state: &mut SettingsState,
    scroll_y: i32,
) -> bool {
    let cx = content_rect.x;
    let cy = content_rect.y;
    let cw = content_rect.width as i32;

    // Sidebar tab clicks
    if x >= cx && x < cx + SIDEBAR_WIDTH {
        for (i, (tab, _, _)) in TABS.iter().enumerate() {
            let ty = cy + 44 + i as i32 * TAB_HEIGHT;
            if y >= ty && y < ty + TAB_HEIGHT {
                state.active_tab = *tab;
                return true;
            }
        }
    }

    // Personalization tab: theme and accent color clicks
    if state.active_tab == SettingsTab::Personalization {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;

        // Theme swatches (3 themes, 82px apart, starting at x+24, y+60)
        let theme_ids = [
            crate::gui::theme::ThemeId::NebulaDark,
            crate::gui::theme::ThemeId::ArcticLight,
            crate::gui::theme::ThemeId::SunsetWarm,
        ];
        for (i, theme_id) in theme_ids.iter().enumerate() {
            let sx = content_x + 24 + i as i32 * 82;
            let sy = content_y - scroll_y + 60;
            let swatch_rect = Rect::new(sx, sy, 72, 36);
            if swatch_rect.contains(x, y) {
                crate::gui::theme::set_theme(*theme_id);
                crate::serial_println!("[Settings] Theme: {:?}", theme_id);
                crate::gui::request_redraw();
                return true;
            }
        }

        // Accent color circles (6 colors, 32px apart, starting at x+24, y+145)
        for i in 0..6u8 {
            let ax = content_x + 24 + i as i32 * 32;
            let ay = content_y - scroll_y + 145;
            // Circle hit test: 10px radius around center (ax+10, ay+10)
            let dx = x - (ax + 10);
            let dy = y - (ay + 10);
            if dx * dx + dy * dy <= 12 * 12 {
                crate::gui::theme::set_accent_index(i);
                crate::serial_println!("[Settings] Accent color: {}", i);
                return true;
            }
        }

        // Cursor theme buttons (4 buttons, 55px apart, starting at x+24, y+206)
        let cursor_themes = [
            crate::gui::desktop::CursorTheme::Default,
            crate::gui::desktop::CursorTheme::Dark,
            crate::gui::desktop::CursorTheme::Accent,
            crate::gui::desktop::CursorTheme::Large,
        ];
        for (i, theme) in cursor_themes.iter().enumerate() {
            let bx = content_x + 24 + i as i32 * 55;
            let by = content_y - scroll_y + 206;
            let btn_rect = Rect::new(bx, by, 48, 22);
            if btn_rect.contains(x, y) {
                crate::gui::desktop::set_cursor_theme(*theme);
                crate::serial_println!("[Settings] Cursor theme: {}", i);
                return true;
            }
        }
    }

    // Display tab: resolution picker clicks
    if state.active_tab == SettingsTab::Display {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;
        let content_w = cw - SIDEBAR_WIDTH;

        crate::serial_println!(
            "[Settings] Display click: mouse=({},{}) content_xy=({},{}) scroll_y={}",
            x,
            y,
            content_x,
            content_y,
            scroll_y
        );

        for (i, &(rw, rh, _label)) in crate::gui::RESOLUTIONS.iter().enumerate() {
            let row_y = content_y - scroll_y + RES_LIST_Y_START + i as i32 * RES_ROW_HEIGHT;
            let btn_rect = Rect::new(
                content_x + RES_LIST_X_PAD,
                row_y,
                (content_w - RES_LIST_X_PAD * 2) as u32,
                RES_ROW_HEIGHT as u32 - 4,
            );
            if i < 3 {
                crate::serial_println!(
                    "[Settings]   res[{}] {}x{}: btn=({},{},{}x{}) hit={}",
                    i,
                    rw,
                    rh,
                    btn_rect.x,
                    btn_rect.y,
                    btn_rect.width,
                    btn_rect.height,
                    btn_rect.contains(x, y)
                );
            }
            if btn_rect.contains(x, y) {
                // Don't switch if already at this resolution
                let (cur_w, cur_h) = crate::gui::screen_size();
                if rw != cur_w || rh != cur_h {
                    crate::serial_println!("[Settings] User selected resolution {}x{}", rw, rh);
                    // Perform the change (this drops the FB lock internally)
                    crate::gui::change_resolution(rw, rh);
                }
                return true;
            }
        }
    }

    // System tab: keyboard layout clicks
    if state.active_tab == SettingsTab::System {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;

        let layouts: &[crate::task::keyboard::KeyboardLayout] = &[
            crate::task::keyboard::KeyboardLayout::Us,
            crate::task::keyboard::KeyboardLayout::Uk,
            crate::task::keyboard::KeyboardLayout::De,
            crate::task::keyboard::KeyboardLayout::Fr,
            crate::task::keyboard::KeyboardLayout::Es,
            crate::task::keyboard::KeyboardLayout::Dvorak,
        ];

        for (i, layout) in layouts.iter().enumerate() {
            let row_y = content_y - scroll_y + 332 + i as i32 * 34;
            let row_rect = Rect::new(content_x + 20, row_y, (cw - SIDEBAR_WIDTH - 40) as u32, 30);
            if row_rect.contains(x, y) {
                crate::task::keyboard::set_layout(*layout);
                crate::serial_println!("[Settings] Keyboard layout: {}", layout.name());
                return true;
            }
        }
    }

    // Users tab: Lock screen and Logout button clicks
    if state.active_tab == SettingsTab::Users {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;
        let pad = 20i32;

        // Approximate button positions (match draw_users_tab layout)
        // Lock Screen button
        let lock_btn = Rect::new(content_x + pad, content_y - scroll_y + 460, 160, 30);
        if lock_btn.contains(x, y) {
            crate::gui::lock_screen::lock();
            return true;
        }
        // Log Out button
        let logout_btn = Rect::new(content_x + pad, content_y - scroll_y + 500, 160, 30);
        if logout_btn.contains(x, y) {
            crate::gui::lock_screen::lock();
            crate::gui::login::reset();
            return true;
        }
    }

    // DateTime tab: timezone selection + toggle clicks
    if state.active_tab == SettingsTab::DateTime {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;
        let pad = 20i32;

        // NTP toggle (approx y offset matches draw_datetime_tab layout)
        let ntp_rect = Rect::new(
            content_x + pad,
            content_y - scroll_y + 80,
            (cw - SIDEBAR_WIDTH - pad * 2) as u32,
            28,
        );
        if ntp_rect.contains(x, y) {
            crate::serial_println!("[Settings] Toggle NTP");
            return true;
        }

        // 24h format toggle
        let fmt_rect = Rect::new(
            content_x + pad,
            content_y - scroll_y + 170,
            (cw - SIDEBAR_WIDTH - pad * 2) as u32,
            28,
        );
        if fmt_rect.contains(x, y) {
            crate::serial_println!("[Settings] Toggle 24h format");
            return true;
        }

        // Show seconds toggle
        let sec_rect = Rect::new(
            content_x + pad,
            content_y - scroll_y + 206,
            (cw - SIDEBAR_WIDTH - pad * 2) as u32,
            28,
        );
        if sec_rect.contains(x, y) {
            crate::serial_println!("[Settings] Toggle show seconds");
            return true;
        }

        // Timezone list
        let timezones = crate::gui::settings_ext::common_timezones();
        for (i, tz) in timezones.iter().enumerate() {
            let row_y = content_y - scroll_y + 280 + i as i32 * 32;
            let row_rect = Rect::new(
                content_x + pad,
                row_y,
                (cw - SIDEBAR_WIDTH - pad * 2) as u32,
                28,
            );
            if row_rect.contains(x, y) {
                crate::gui::settings_ext::set_timezone(&tz.name);
                crate::serial_println!("[Settings] Timezone: {}", tz.name);
                return true;
            }
        }
    }

    // Privacy tab: toggle clicks
    if state.active_tab == SettingsTab::Privacy {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;
        let pad = 20i32;

        let settings = [
            "location",
            "analytics",
            "camera",
            "microphone",
            "firewall",
            "autoupdate",
        ];
        for (i, key) in settings.iter().enumerate() {
            let row_y = content_y - scroll_y + 72 + i as i32 * 52;
            let row_rect = Rect::new(
                content_x + pad - 4,
                row_y - 2,
                (cw - SIDEBAR_WIDTH - pad * 2 + 8) as u32,
                48,
            );
            if row_rect.contains(x, y) {
                let new_val = crate::gui::settings_ext::toggle_privacy(key);
                crate::serial_println!("[Settings] Privacy '{}' = {}", key, new_val);
                return true;
            }
        }
    }

    // Startup tab: toggle clicks
    if state.active_tab == SettingsTab::Startup {
        let content_x = cx + SIDEBAR_WIDTH;
        let content_y = cy;
        let pad = 20i32;

        let app_count = { crate::gui::settings_ext::STARTUP.lock().apps.len() };
        for i in 0..app_count {
            let row_y = content_y - scroll_y + 92 + i as i32 * 48;
            let row_rect = Rect::new(
                content_x + pad - 4,
                row_y - 2,
                (cw - SIDEBAR_WIDTH - pad * 2 + 8) as u32,
                44,
            );
            if row_rect.contains(x, y) {
                let startup = crate::gui::settings_ext::STARTUP.lock();
                if let Some(app) = startup.apps.get(i) {
                    let name = app.name.clone();
                    drop(startup);
                    let new_val = crate::gui::settings_ext::toggle_startup_app(&name);
                    crate::serial_println!("[Settings] Startup '{}' enabled = {}", name, new_val);
                }
                return true;
            }
        }
    }

    true
}
