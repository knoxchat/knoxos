/// Display tab — resolution picker, brightness, scale info.
use crate::gui::colors;
use crate::gui::font_engine;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::widgets::{draw_focus_indicator, draw_info_row, draw_section_header, draw_slider_row};

// ═══════════════════════════════════════════════════════════════════════════
// DISPLAY TAB
// ═══════════════════════════════════════════════════════════════════════════

/// Height of each resolution option row
pub(super) const RES_ROW_HEIGHT: i32 = 28;
/// X offset for the resolution list within the display tab
pub(super) const RES_LIST_X_PAD: i32 = 24;
/// Starting Y offset for the resolution list (relative to content_y)
pub const RES_LIST_Y_START: i32 = 112;

pub(super) fn draw_display_tab(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: i32,
    focus_idx: i32,
) -> i32 {
    draw_section_header(fb, x, y + 12, w, "Display");

    // Current resolution info
    let res_str = alloc::format!("{}x{}", fb.width, fb.height);
    draw_info_row(fb, x, y + 42, "Resolution", &res_str);
    draw_info_row(fb, x, y + 60, "Color Depth", "32-bit (BGRA)");
    draw_info_row(fb, x, y + 78, "Refresh", "Software Rendered");

    // ── Resolution picker ────────────────
    draw_section_header(fb, x, y + 92, w, "Change Resolution");

    let current_w = fb.width;
    let current_h = fb.height;

    for (i, &(rw, rh, label)) in crate::gui::RESOLUTIONS.iter().enumerate() {
        let row_y = y + RES_LIST_Y_START + i as i32 * RES_ROW_HEIGHT;
        let is_active = rw == current_w && rh == current_h;

        // Background highlight for active / hover-zone
        let btn_rect = Rect::new(
            x + RES_LIST_X_PAD,
            row_y,
            (w - RES_LIST_X_PAD * 2) as u32,
            RES_ROW_HEIGHT as u32 - 4,
        );
        if is_active {
            fb.fill_rounded_rect_aa(btn_rect, Pixel::rgb(40, 60, 100), 4);
        } else {
            fb.fill_rounded_rect_aa(btn_rect, Pixel::rgb(36, 36, 40), 4);
        }

        // Radio button circle
        let radio_x = x + RES_LIST_X_PAD + 14;
        let radio_y = row_y + RES_ROW_HEIGHT / 2 - 2;
        fb.fill_circle_aa(radio_x, radio_y, 7, Pixel::rgb(70, 70, 75));
        if is_active {
            fb.fill_circle_aa(radio_x, radio_y, 7, Pixel::rgb(82, 139, 255));
            fb.fill_circle_aa(radio_x, radio_y, 4, colors::WHITE);
        }

        // Label text
        let text_color = if is_active {
            colors::WHITE
        } else {
            Pixel::rgb(180, 180, 180)
        };
        font_engine::draw_ui_text(
            fb,
            x + RES_LIST_X_PAD + 30,
            row_y + 5,
            label,
            13,
            text_color,
        );

        // Focus ring for this resolution option
        draw_focus_indicator(fb, btn_rect, focus_idx == i as i32);
    }

    // Brightness slider (below the list)
    let res_count = crate::gui::RESOLUTIONS.len() as i32;
    let after_list_y = y + RES_LIST_Y_START + res_count * RES_ROW_HEIGHT + 10;
    fb.draw_hline(
        x + 16,
        after_list_y,
        (w - 32) as u32,
        Pixel::rgb(45, 45, 50),
    );
    draw_slider_row(
        fb,
        x,
        after_list_y + 12,
        w,
        "Brightness",
        80,
        Pixel::rgb(224, 175, 104),
    );

    // Focus ring for brightness slider
    let slider_rect = Rect::new(x + 24, after_list_y + 12, (w - 52) as u32, 28);
    draw_focus_indicator(fb, slider_rect, focus_idx == res_count);

    // Scale info
    draw_section_header(fb, x, after_list_y + 52, w, "Scale & Layout");
    draw_info_row(fb, x, after_list_y + 82, "Scale", "100% (Recommended)");
    draw_info_row(fb, x, after_list_y + 100, "Orientation", "Landscape");

    // Return total content height
    after_list_y - y + 130
}
