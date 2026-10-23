use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel};

use super::types::{BUTTON_HEIGHT, BUTTON_WIDTH};

// ─── Local Color Aliases ─────────────────────────────────────────────

pub(super) const ACCENT_CYAN: Pixel = Pixel::new(20, 180, 255, 255);
pub(super) const TEXT_WHITE: Pixel = Pixel::new(220, 235, 255, 240);
pub(super) const TEXT_DIM: Pixel = Pixel::new(100, 120, 160, 170);

// ─── Local Draw Helper (usize coords, default scale) ────────────────

#[inline(always)]
pub(super) fn draw_text(fb: &mut FrameBuffer, x: usize, y: usize, text: &str, color: Pixel) {
    fonts::draw_string(fb, x as i32, y as i32, text, color, 1);
}

// ─── Utility ─────────────────────────────────────────────────────────

pub(super) fn char_to_str(c: char, buf: &mut [u8; 4]) -> &str {
    let len = c.encode_utf8(buf).len();
    core::str::from_utf8(&buf[..len]).unwrap_or("?")
}

pub(super) fn format_u64(mut val: u64, buf: &mut [u8; 16]) -> &str {
    if val == 0 {
        buf[0] = b'0';
        return core::str::from_utf8(&buf[..1]).unwrap_or("0");
    }
    let mut pos = buf.len();
    while val > 0 && pos > 0 {
        pos -= 1;
        buf[pos] = b'0' + (val % 10) as u8;
        val /= 10;
    }
    core::str::from_utf8(&buf[pos..]).unwrap_or("?")
}

pub(super) fn draw_button(fb: &mut FrameBuffer, x: usize, y: usize, label: &str, primary: bool) {
    let bg = if primary {
        Pixel::new(0, 160, 200, 230)
    } else {
        Pixel::new(50, 50, 70, 220)
    };
    let border = if primary {
        ACCENT_CYAN
    } else {
        Pixel::new(80, 80, 100, 255)
    };

    for yy in y..y + BUTTON_HEIGHT {
        for xx in x..x + BUTTON_WIDTH {
            if yy == y || yy == y + BUTTON_HEIGHT - 1 || xx == x || xx == x + BUTTON_WIDTH - 1 {
                fb.set_pixel(xx, yy, border);
            } else {
                fb.set_pixel(xx, yy, bg);
            }
        }
    }

    let text_w = label.len() * 8;
    let text_x = x + BUTTON_WIDTH.saturating_sub(text_w) / 2;
    let text_y = y + (BUTTON_HEIGHT - 14) / 2;
    draw_text(fb, text_x, text_y, label, TEXT_WHITE);
}
