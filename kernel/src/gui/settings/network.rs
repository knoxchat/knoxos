/// Network tab — Wi-Fi toggle and available networks.
use crate::gui::colors;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::widgets::{
    draw_focus_indicator, draw_info_row, draw_section_header, draw_toggle_row, ttf,
};

// ═══════════════════════════════════════════════════════════════════════════
// NETWORK TAB
// ═══════════════════════════════════════════════════════════════════════════
pub(super) fn draw_network_tab(
    fb: &mut FrameBuffer,
    x: i32,
    y: i32,
    w: i32,
    focus_idx: i32,
) -> i32 {
    draw_section_header(fb, x, y + 12, w, "Network");

    draw_toggle_row(fb, x, y + 42, w, "Wi-Fi", "Connected to KnoxOS-Net", true);
    // Focus: item 0 = Wi-Fi toggle
    draw_focus_indicator(fb, Rect::new(x + w - 60, y + 42, 36, 18), focus_idx == 0);

    fb.draw_hline(x + 16, y + 82, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_section_header(fb, x, y + 92, w, "Available Networks");

    let networks = [
        ("KnoxOS-Net", "Connected", true),
        ("Office-5G", "Secured", false),
        ("Guest", "Open", false),
    ];

    for (i, (name, status, connected)) in networks.iter().enumerate() {
        let ny = y + 120 + i as i32 * 40;
        if *connected {
            fb.fill_rounded_rect_aa(
                Rect::new(x + 20, ny - 2, (w - 40) as u32, 34),
                Pixel::rgb(45, 45, 50),
                4,
            );
        }

        // WiFi signal bars
        let icon_x = x + 30;
        for bar in 0..4u32 {
            let bh = 4 + bar * 3;
            let color = if *connected || bar < 2 {
                Pixel::rgb(158, 206, 106)
            } else {
                Pixel::rgb(60, 60, 65)
            };
            fb.fill_rounded_rect_aa(
                Rect::new(icon_x + bar as i32 * 5, ny + 16 - bh as i32, 3, bh),
                color,
                1,
            );
        }

        ttf(fb, x + 56, ny + 2, name, colors::WHITE);
        ttf(fb, x + 56, ny + 16, status, Pixel::rgb(120, 120, 120));
        // Focus: items 1..3 = network entries
        let net_rect = Rect::new(x + 20, ny - 2, (w - 40) as u32, 34);
        draw_focus_indicator(fb, net_rect, focus_idx == (i as i32 + 1));
    }

    fb.draw_hline(x + 16, y + 248, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_info_row(fb, x, y + 260, "IP Address", "10.0.2.15");
    draw_info_row(fb, x, y + 278, "MAC Address", "52:54:00:12:34:56");

    310
}
