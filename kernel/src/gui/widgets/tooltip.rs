use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;

// ═══════════════════════════════════════════════════════════════════════════
// TOOLTIP WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct Tooltip {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub visible: bool,
    pub show_delay_ticks: u32,
    pub current_ticks: u32,
}

impl Tooltip {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            x: 0,
            y: 0,
            visible: false,
            show_delay_ticks: 18, // ~1 second at 18 Hz
            current_ticks: 0,
        }
    }

    pub fn show(&mut self, x: i32, y: i32, text: &str) {
        self.x = x;
        self.y = y;
        self.text = String::from(text);
        self.current_ticks = 0;
        self.visible = false;
    }

    pub fn tick(&mut self) {
        if !self.text.is_empty() {
            self.current_ticks += 1;
            if self.current_ticks >= self.show_delay_ticks {
                self.visible = true;
            }
        }
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.text.clear();
        self.current_ticks = 0;
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        if !self.visible || self.text.is_empty() {
            return;
        }

        let text_w = self.text.len() as u32 * 8 + 12;
        let text_h: u32 = 22;

        // Shadow (AA rounded)
        fb.fill_rounded_rect_aa(
            Rect::new(self.x + 2, self.y + 2, text_w, text_h),
            Pixel::new(0, 0, 0, 120),
            6,
        );

        // Background (AA rounded)
        fb.fill_rounded_rect_aa(
            Rect::new(self.x, self.y, text_w, text_h),
            Pixel::rgb(50, 50, 55),
            6,
        );
        fb.draw_rounded_rect(
            Rect::new(self.x, self.y, text_w, text_h),
            Pixel::rgb(80, 80, 84),
            6,
            1,
        );

        fonts::draw_string_compact(fb, self.x + 6, self.y + 5, &self.text, colors::WHITE, 1);
    }
}
