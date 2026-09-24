use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Avatar Widget — Circular user icon with initials
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct AvatarWidget {
    pub x: i32,
    pub y: i32,
    pub radius: u32,
    pub initials: [u8; 2],
    pub bg_color: Pixel,
}

impl AvatarWidget {
    pub fn new(x: i32, y: i32, radius: u32, name: &str, bg_color: Pixel) -> Self {
        let bytes = name.as_bytes();
        let first = if !bytes.is_empty() {
            bytes[0].to_ascii_uppercase()
        } else {
            b'?'
        };
        let second = name
            .split_whitespace()
            .nth(1)
            .and_then(|w| w.as_bytes().first().copied())
            .map(|b| b.to_ascii_uppercase())
            .unwrap_or(0);
        Self {
            x,
            y,
            radius,
            initials: [first, second],
            bg_color,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let cx = self.x + self.radius as i32;
        let cy = self.y + self.radius as i32;

        fb.fill_circle_aa(cx, cy, self.radius, self.bg_color);
        fb.draw_rounded_rect(
            Rect::new(self.x, self.y, self.radius * 2, self.radius * 2),
            Pixel::new(255, 255, 255, 40),
            self.radius,
            1,
        );

        let mut text = [0u8; 2];
        let mut len = 0;
        text[0] = self.initials[0];
        len += 1;
        if self.initials[1] != 0 {
            text[1] = self.initials[1];
            len += 1;
        }
        let s = core::str::from_utf8(&text[..len]).unwrap_or("?");
        let text_w = fonts::measure_string_width_compact(s, 1) as i32;
        let text_x = cx - text_w / 2;
        let text_y = cy - 5;
        fonts::draw_string_compact(fb, text_x, text_y, s, Pixel::rgb(255, 255, 255), 1);
    }
}
