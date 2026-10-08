/// System tab — OS info, performance, power, keyboard layout.
use crate::gui::colors;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::ARCH_NAME;
use super::widgets::{
    draw_focus_indicator, draw_info_row, draw_section_header, draw_toggle_row, ttf, ttf_b,
};

// ═══════════════════════════════════════════════════════════════════════════
// SYSTEM TAB
// ═══════════════════════════════════════════════════════════════════════════
pub(super) fn draw_system_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, focus_idx: i32) -> i32 {
    draw_section_header(fb, x, y + 12, w, "System");

    draw_info_row(fb, x, y + 42, "OS", "KnoxOS v0.1.0");
    draw_info_row(fb, x, y + 60, "Architecture", ARCH_NAME);
    draw_info_row(fb, x, y + 78, "Kernel", "Microkernel (Rust)");

    fb.draw_hline(x + 16, y + 98, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_section_header(fb, x, y + 108, w, "Performance");

    // CPU bar
    ttf_b(fb, x + 24, y + 136, "CPU", Pixel::rgb(140, 140, 140));
    let bar_w = (w - 100) as u32;
    fb.fill_rounded_rect_aa(
        Rect::new(x + 64, y + 138, bar_w, 8),
        Pixel::rgb(50, 50, 55),
        4,
    );
    let cpu_fill = bar_w * 23 / 100;
    fb.fill_rounded_rect_aa(
        Rect::new(x + 64, y + 138, cpu_fill, 8),
        Pixel::rgb(82, 139, 255),
        4,
    );
    ttf(fb, x + w - 48, y + 136, "23%", Pixel::rgb(82, 139, 255));

    // Memory bar
    ttf_b(fb, x + 24, y + 160, "RAM", Pixel::rgb(140, 140, 140));
    fb.fill_rounded_rect_aa(
        Rect::new(x + 64, y + 162, bar_w, 8),
        Pixel::rgb(50, 50, 55),
        4,
    );
    let mem_fill = bar_w * 42 / 100;
    fb.fill_rounded_rect_aa(
        Rect::new(x + 64, y + 162, mem_fill, 8),
        Pixel::rgb(158, 206, 106),
        4,
    );
    ttf(fb, x + w - 48, y + 160, "42%", Pixel::rgb(158, 206, 106));

    fb.draw_hline(x + 16, y + 188, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_section_header(fb, x, y + 198, w, "Power");
    draw_toggle_row(
        fb,
        x,
        y + 228,
        w,
        "Power Saving",
        "Reduce performance to save battery",
        false,
    );
    // Focus: item 0 = power saving toggle
    draw_focus_indicator(fb, Rect::new(x + w - 60, y + 228, 36, 18), focus_idx == 0);
    draw_info_row(fb, x, y + 268, "Uptime", "0d 0h 12m");

    // ── Keyboard Layout section ──
    fb.draw_hline(x + 16, y + 290, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_section_header(fb, x, y + 300, w, "Keyboard Layout");

    let current_layout = crate::task::keyboard::get_layout();
    let layouts: &[(&str, &str, crate::task::keyboard::KeyboardLayout)] = &[
        (
            "US QWERTY",
            "English (US)",
            crate::task::keyboard::KeyboardLayout::Us,
        ),
        (
            "UK QWERTY",
            "English (UK)",
            crate::task::keyboard::KeyboardLayout::Uk,
        ),
        (
            "QWERTZ",
            "German",
            crate::task::keyboard::KeyboardLayout::De,
        ),
        (
            "AZERTY",
            "French",
            crate::task::keyboard::KeyboardLayout::Fr,
        ),
        (
            "QWERTY",
            "Spanish",
            crate::task::keyboard::KeyboardLayout::Es,
        ),
        (
            "Dvorak",
            "US Dvorak",
            crate::task::keyboard::KeyboardLayout::Dvorak,
        ),
    ];

    let mut layout_y = y + 332;
    for (i, (label, desc, layout)) in layouts.iter().enumerate() {
        let is_active = current_layout == *layout;
        let row_rect = Rect::new(x + 20, layout_y, (w - 40) as u32, 30);

        if is_active {
            fb.fill_rounded_rect_aa(row_rect, Pixel::new(0, 100, 180, 60), 6);
        }

        // Radio dot
        let dot_x = x + 32;
        let dot_y = layout_y + 15;
        fb.draw_rounded_rect(
            Rect::new(dot_x - 7, dot_y - 7, 14, 14),
            Pixel::new(100, 160, 220, 160),
            7,
            1,
        );
        if is_active {
            fb.fill_circle_aa(dot_x, dot_y, 4u32, Pixel::new(0, 200, 255, 220));
        }

        // Layout name
        ttf_b(
            fb,
            x + 48,
            layout_y + 4,
            label,
            if is_active {
                Pixel::new(0, 220, 255, 255)
            } else {
                Pixel::new(200, 220, 240, 220)
            },
        );

        // Description
        ttf(
            fb,
            x + 48,
            layout_y + 18,
            desc,
            Pixel::new(120, 150, 180, 160),
        );

        // Focus: items 1..6 = keyboard layouts
        draw_focus_indicator(fb, row_rect, focus_idx == (i as i32 + 1));

        layout_y += 34;
    }

    layout_y - y + 20
}
