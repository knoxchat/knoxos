use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;

// ═══════════════════════════════════════════════════════════════════════════
// BADGE / TAG WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct Badge;

impl Badge {
    /// Draw a small count badge (like notification count)
    pub fn draw_count(fb: &mut FrameBuffer, x: i32, y: i32, count: u32) {
        if count == 0 {
            return;
        }

        let text = if count > 99 {
            String::from("99+")
        } else {
            alloc::format!("{}", count)
        };

        let w = (text.len() as u32 * 8 + 8).max(16);
        let h: u32 = 16;

        fb.fill_rounded_rect_aa(Rect::new(x, y, w, h), Pixel::rgb(247, 118, 142), 8);

        fonts::draw_string_compact(
            fb,
            x + (w as i32 - text.len() as i32 * 8) / 2,
            y + 2,
            &text,
            colors::WHITE,
            1,
        );
    }

    /// Draw a status dot (AA smooth)
    pub fn draw_dot(fb: &mut FrameBuffer, x: i32, y: i32, color: Pixel) {
        fb.fill_circle_aa(x, y, 4, color);
    }
}
