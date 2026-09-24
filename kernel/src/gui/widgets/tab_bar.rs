use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;
use alloc::vec::Vec;

// ═══════════════════════════════════════════════════════════════════════════
// TABS WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct TabBar {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub tabs: Vec<String>,
    pub active_index: usize,
}

impl TabBar {
    pub fn new(x: i32, y: i32, width: u32, tabs: Vec<String>) -> Self {
        Self {
            x,
            y,
            width,
            tabs,
            active_index: 0,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let tab_h: u32 = 32;
        let tab_count = self.tabs.len().max(1);
        let tab_w = self.width / tab_count as u32;

        // Background
        fb.fill_rect(
            Rect::new(self.x, self.y, self.width, tab_h),
            Pixel::rgb(30, 30, 34),
        );

        for (i, tab_label) in self.tabs.iter().enumerate() {
            let tx = self.x + (i as u32 * tab_w) as i32;
            let is_active = i == self.active_index;

            if is_active {
                // Subtle active tab background with AA rounded corners
                fb.fill_rounded_rect_aa(
                    Rect::new(tx + 2, self.y + 2, tab_w - 4, tab_h - 4),
                    Pixel::rgb(40, 40, 44),
                    4,
                );
                // Rounded capsule indicator at bottom (pill shape)
                let indicator_w = tab_w.clamp(16, 48);
                let indicator_x = tx + (tab_w as i32 - indicator_w as i32) / 2;
                fb.fill_rounded_rect_aa(
                    Rect::new(indicator_x, self.y + tab_h as i32 - 3, indicator_w, 3),
                    Pixel::rgb(82, 139, 255),
                    2,
                );
            }

            let color = if is_active {
                colors::WHITE
            } else {
                Pixel::rgb(140, 140, 140)
            };
            if is_active {
                fonts::draw_string_centered_bold_compact(
                    fb, tx, self.y, tab_w, tab_h, tab_label, color, 1,
                );
            } else {
                fonts::draw_string_centered_compact(
                    fb, tx, self.y, tab_w, tab_h, tab_label, color, 1,
                );
            }
        }

        // Bottom border
        fb.draw_hline(
            self.x,
            self.y + tab_h as i32,
            self.width,
            Pixel::rgb(50, 50, 55),
        );
    }

    pub fn handle_click(&mut self, mx: i32, my: i32) -> bool {
        let tab_h = 32;
        if my < self.y || my > self.y + tab_h || mx < self.x || mx > self.x + self.width as i32 {
            return false;
        }
        let tab_count = self.tabs.len().max(1) as u32;
        let tab_w = self.width / tab_count;
        let idx = ((mx - self.x) as u32 / tab_w) as usize;
        if idx < self.tabs.len() {
            self.active_index = idx;
            return true;
        }
        false
    }
}
