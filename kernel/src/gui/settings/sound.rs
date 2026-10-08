/// Sound tab — volume sliders and output device list.
use crate::gui::colors;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

use super::widgets::{draw_focus_indicator, draw_section_header, draw_slider_row, ttf};

// ═══════════════════════════════════════════════════════════════════════════
// SOUND TAB
// ═══════════════════════════════════════════════════════════════════════════
pub(super) fn draw_sound_tab(fb: &mut FrameBuffer, x: i32, y: i32, w: i32, focus_idx: i32) -> i32 {
    draw_section_header(fb, x, y + 12, w, "Sound");

    draw_slider_row(
        fb,
        x,
        y + 42,
        w,
        "Master Volume",
        75,
        Pixel::rgb(82, 139, 255),
    );
    // Focus: item 0 = master volume
    draw_focus_indicator(
        fb,
        Rect::new(x + 24, y + 42, (w - 52) as u32, 28),
        focus_idx == 0,
    );

    draw_slider_row(
        fb,
        x,
        y + 82,
        w,
        "System Sounds",
        50,
        Pixel::rgb(82, 139, 255),
    );
    // Focus: item 1 = system sounds
    draw_focus_indicator(
        fb,
        Rect::new(x + 24, y + 82, (w - 52) as u32, 28),
        focus_idx == 1,
    );

    fb.draw_hline(x + 16, y + 118, (w - 32) as u32, Pixel::rgb(45, 45, 50));
    draw_section_header(fb, x, y + 128, w, "Output Device");

    // Device list
    let devices = ["HD Audio Output (Default)", "HDMI Audio", "USB Headset"];
    for (i, device) in devices.iter().enumerate() {
        let dy = y + 158 + i as i32 * 30;
        let is_selected = i == 0;
        if is_selected {
            fb.fill_rounded_rect_aa(
                Rect::new(x + 20, dy - 4, (w - 40) as u32, 26),
                Pixel::rgb(45, 45, 50),
                4,
            );
        }
        let radio_color = if is_selected {
            Pixel::rgb(82, 139, 255)
        } else {
            Pixel::rgb(70, 70, 75)
        };
        fb.fill_circle_aa(x + 36, dy + 6, 6, radio_color);
        if is_selected {
            fb.fill_circle_aa(x + 36, dy + 6, 3, Pixel::rgb(82, 139, 255));
        }
        ttf(fb, x + 52, dy, device, colors::WHITE);
        // Focus: items 2..4 = output devices
        let dev_rect = Rect::new(x + 20, dy - 4, (w - 40) as u32, 26);
        draw_focus_indicator(fb, dev_rect, focus_idx == (i as i32 + 2));
    }

    280
}
