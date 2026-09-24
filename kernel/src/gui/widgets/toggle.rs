use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;

// ═══════════════════════════════════════════════════════════════════════════
// TOGGLE SWITCH WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct ToggleSwitch {
    pub x: i32,
    pub y: i32,
    pub on: bool,
    pub label: String,
    pub hovered: bool,
}

impl ToggleSwitch {
    pub fn new(x: i32, y: i32, label: &str, on: bool) -> Self {
        Self {
            x,
            y,
            on,
            label: String::from(label),
            hovered: false,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let track_w: u32 = 36;
        let track_h: u32 = 20;
        let thumb_r: u32 = 8;

        // Track background
        let track_color = if self.on {
            Pixel::rgb(82, 139, 255)
        } else if self.hovered {
            Pixel::rgb(70, 70, 76)
        } else {
            Pixel::rgb(50, 50, 55)
        };
        fb.fill_rounded_rect_aa(
            Rect::new(self.x, self.y, track_w, track_h),
            track_color,
            track_h / 2,
        );

        // Thumb (sliding circle)
        let thumb_x = if self.on {
            self.x + track_w as i32 - thumb_r as i32 - 2
        } else {
            self.x + thumb_r as i32 + 2
        };
        let thumb_y = self.y + track_h as i32 / 2;
        fb.fill_circle_aa(thumb_x, thumb_y, thumb_r, colors::WHITE);

        // Label
        fonts::draw_string_compact(
            fb,
            self.x + track_w as i32 + 10,
            self.y + 4,
            &self.label,
            colors::WHITE,
            1,
        );
    }

    pub fn hit_test(&self, mx: i32, my: i32) -> bool {
        mx >= self.x
            && mx < self.x + 36 + 10 + self.label.len() as i32 * 8
            && my >= self.y
            && my < self.y + 20
    }
}
