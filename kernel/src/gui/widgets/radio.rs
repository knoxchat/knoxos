use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel};
use alloc::string::String;
use alloc::vec::Vec;

// ═══════════════════════════════════════════════════════════════════════════
// RADIO BUTTON WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct RadioButton {
    pub x: i32,
    pub y: i32,
    pub selected: bool,
    pub label: String,
    pub hovered: bool,
}

impl RadioButton {
    pub fn new(x: i32, y: i32, label: &str, selected: bool) -> Self {
        Self {
            x,
            y,
            selected,
            label: String::from(label),
            hovered: false,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let radius: u32 = 8;
        let cx = self.x + radius as i32;
        let cy = self.y + radius as i32;

        // Outer circle
        let outer_color = if self.selected {
            Pixel::rgb(82, 139, 255)
        } else if self.hovered {
            Pixel::rgb(80, 80, 84)
        } else {
            Pixel::rgb(60, 60, 64)
        };
        fb.fill_circle_aa(cx, cy, radius, outer_color);

        // Inner background (2px ring)
        let inner_bg = if self.selected {
            Pixel::rgb(82, 139, 255)
        } else {
            Pixel::rgb(30, 30, 34)
        };
        fb.fill_circle_aa(cx, cy, radius - 2, inner_bg);

        // Selected dot (inner filled circle)
        if self.selected {
            fb.fill_circle_aa(cx, cy, 4, colors::WHITE);
        }

        // Label
        fonts::draw_string_compact(
            fb,
            self.x + radius as i32 * 2 + 8,
            self.y + 4,
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

/// Radio button group — renders a vertical list of radio options.
/// Only one can be selected at a time.
pub struct RadioGroup {
    pub x: i32,
    pub y: i32,
    pub options: Vec<String>,
    pub selected: usize,
    pub spacing: i32,
}

impl RadioGroup {
    pub fn new(x: i32, y: i32, options: &[&str], selected: usize) -> Self {
        Self {
            x,
            y,
            options: options.iter().map(|s| String::from(*s)).collect(),
            selected,
            spacing: 24,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        for (i, opt) in self.options.iter().enumerate() {
            let ry = self.y + i as i32 * self.spacing;
            let rb = RadioButton::new(self.x, ry, opt, i == self.selected);
            rb.draw(fb);
        }
    }

    /// Returns Some(index) if clicked on a radio option, None otherwise
    pub fn hit_test(&self, mx: i32, my: i32) -> Option<usize> {
        for (i, opt) in self.options.iter().enumerate() {
            let ry = self.y + i as i32 * self.spacing;
            let rb = RadioButton::new(self.x, ry, opt, i == self.selected);
            if rb.hit_test(mx, my) {
                return Some(i);
            }
        }
        None
    }
}
