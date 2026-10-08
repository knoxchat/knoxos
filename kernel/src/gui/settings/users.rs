/// Users tab — account list, lock screen, log out.
use alloc::string::String;

use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::widgets::{draw_focus_indicator, draw_section_header, ttf, ttf_b, ttf_h};

// ═══════════════════════════════════════════════════════════════════════════
// USERS TAB — User account management
// ═══════════════════════════════════════════════════════════════════════════
pub(super) fn draw_users_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, focus_idx: i32) -> i32 {
    let mut cy = y + 20;
    let pad = 20i32;

    // Section header: Current user
    draw_section_header(fb, x + pad, cy, w - pad * 2, "Current User");
    cy += 30;

    let current_uid = crate::users::get_current_uid();
    if let Some(user) = crate::users::get_user(current_uid) {
        // User avatar circle
        let avatar_cx = x + pad + 28;
        let avatar_cy = cy + 28;
        fb.fill_circle_aa(avatar_cx, avatar_cy, 24u32, Pixel::new(0, 140, 220, 200));
        fb.draw_rounded_rect(
            Rect::new(avatar_cx - 24, avatar_cy - 24, 48, 48),
            Pixel::new(80, 180, 255, 80),
            24,
            1,
        );
        // Initial letter
        let initial = user.username.chars().next().unwrap_or('U');
        let mut ibuf = [0u8; 4];
        let istr = initial
            .to_uppercase()
            .next()
            .unwrap_or('U')
            .encode_utf8(&mut ibuf);
        let iw = font_engine::measure_ui_text(istr, 15) as i32;
        ttf_h(fb, avatar_cx - iw / 2, avatar_cy - 10, istr, colors::WHITE);

        // Username and full name
        ttf_b(
            fb,
            x + pad + 64,
            cy + 10,
            &user.username,
            Pixel::new(220, 240, 255, 240),
        );
        ttf(
            fb,
            x + pad + 64,
            cy + 28,
            &user.gecos,
            Pixel::new(140, 160, 200, 180),
        );

        // User details
        let uid_str = {
            let mut s = String::from("UID: ");
            use core::fmt::Write;
            let _ = write!(s, "{}", user.uid);
            s
        };
        ttf(
            fb,
            x + pad + 64,
            cy + 44,
            &uid_str,
            Pixel::new(100, 130, 170, 160),
        );
        cy += 64;
    }
    cy += 16;

    // Section: All Users
    draw_section_header(fb, x + pad, cy, w - pad * 2, "System Users");
    cy += 30;

    // List all non-system users (uid >= 1000) and root
    let users = crate::users::list_users();
    for user in &users {
        if user.uid > 0 && user.uid < 1000 {
            continue; // Skip system service accounts
        }

        let row_h = 40i32;
        let row_rect = Rect::new(x + pad, cy, (w - pad * 2) as u32, row_h as u32);

        // Row background
        let is_current = user.uid == current_uid;
        if is_current {
            fb.fill_rounded_rect_aa(row_rect, Pixel::new(0, 100, 180, 40), 6);
        }

        // Small avatar circle
        let small_cx = x + pad + 16;
        let small_cy = cy + row_h / 2;
        let avatar_color = if user.uid == 0 {
            Pixel::new(200, 60, 60, 200) // Red for root
        } else if user.disabled {
            Pixel::new(80, 80, 80, 150)
        } else {
            Pixel::new(0, 140, 200, 180)
        };
        fb.fill_circle_aa(small_cx, small_cy, 12u32, avatar_color);

        // Username
        ttf_b(
            fb,
            x + pad + 36,
            cy + 6,
            &user.username,
            Pixel::new(220, 240, 255, 230),
        );

        // Role / info
        let role = if user.uid == 0 {
            "Administrator (root)"
        } else if user.disabled {
            "Disabled"
        } else {
            "Standard User"
        };
        ttf(
            fb,
            x + pad + 36,
            cy + 22,
            role,
            Pixel::new(120, 150, 190, 160),
        );

        // UID badge on right
        let uid_str = {
            let mut s = String::new();
            use core::fmt::Write;
            let _ = write!(s, "uid:{}", user.uid);
            s
        };
        let uid_w = font_engine::measure_ui_text(&uid_str, 13) as i32;
        ttf(
            fb,
            x + w - pad - uid_w - 8,
            cy + (row_h - 10) / 2,
            &uid_str,
            Pixel::new(80, 120, 160, 140),
        );

        cy += row_h + 4;
    }

    cy += 16;

    // Section: Account Actions
    draw_section_header(fb, x + pad, cy, w - pad * 2, "Account Actions");
    cy += 30;

    // Lock screen button
    let btn_w = 160i32;
    let btn_h = 30i32;
    let btn_rect = Rect::new(x + pad, cy, btn_w as u32, btn_h as u32);
    fb.fill_rounded_rect_aa(btn_rect, Pixel::new(0, 140, 220, 180), 6);
    fb.draw_rounded_rect(btn_rect, Pixel::new(60, 180, 255, 60), 6, 1);
    ttf_b(
        fb,
        x + pad + 12,
        cy + (btn_h - 10) / 2,
        "Lock Screen",
        colors::WHITE,
    );
    // Focus: item 0 = Lock Screen button
    draw_focus_indicator(fb, btn_rect, focus_idx == 0);
    cy += btn_h + 10;

    // Logout button
    let btn_rect2 = Rect::new(x + pad, cy, btn_w as u32, btn_h as u32);
    fb.fill_rounded_rect_aa(btn_rect2, Pixel::new(180, 60, 60, 180), 6);
    fb.draw_rounded_rect(btn_rect2, Pixel::new(220, 100, 100, 60), 6, 1);
    ttf_b(
        fb,
        x + pad + 12,
        cy + (btn_h - 10) / 2,
        "Log Out",
        colors::WHITE,
    );
    // Focus: item 1 = Log Out button
    draw_focus_indicator(fb, btn_rect2, focus_idx == 1);
    cy += btn_h + 10;

    // Total content height
    cy - y + 20
}
