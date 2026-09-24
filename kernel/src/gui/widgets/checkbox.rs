use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;

// ═══════════════════════════════════════════════════════════════════════════
// CHECKBOX WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct Checkbox {
    pub x: i32,
    pub y: i32,
    pub checked: bool,
    pub label: String,
    pub hovered: bool,
}

impl Checkbox {
    pub fn new(x: i32, y: i32, label: &str, checked: bool) -> Self {
        Self {
            x,
            y,
            checked,
            label: String::from(label),
            hovered: false,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let box_size: u32 = 16;
        let bg = if self.checked {
            Pixel::rgb(82, 139, 255)
        } else if self.hovered {
            Pixel::rgb(55, 55, 60)
        } else {
            Pixel::rgb(40, 40, 44)
        };

        fb.fill_rounded_rect_aa(Rect::new(self.x, self.y, box_size, box_size), bg, 3);
        fb.draw_rounded_rect(
            Rect::new(self.x, self.y, box_size, box_size),
            if self.checked {
                Pixel::rgb(82, 139, 255)
            } else {
                Pixel::rgb(80, 80, 84)
            },
            3,
            1,
        );

        if self.checked {
            // Checkmark — anti-aliased
            fb.draw_line_aa(
                self.x + 3,
                self.y + 8,
                self.x + 6,
                self.y + 12,
                colors::WHITE,
            );
            fb.draw_line_aa(
                self.x + 6,
                self.y + 12,
                self.x + 12,
                self.y + 4,
                colors::WHITE,
            );
        }

        // Label
        fonts::draw_string_compact(
            fb,
            self.x + box_size as i32 + 8,
            self.y + 2,
            &self.label,
            colors::WHITE,
            1,
        );
    }

    pub fn hit_test(&self, mx: i32, my: i32) -> bool {
        mx >= self.x
            && mx < self.x + 16 + 8 + self.label.len() as i32 * 8
            && my >= self.y
            && my < self.y + 16
    }
}
