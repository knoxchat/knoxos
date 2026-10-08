use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;

/// Button widget
pub struct Button {
    pub rect: Rect,
    pub label: String,
    pub hovered: bool,
    pub pressed: bool,
    pub bg_color: Pixel,
    pub text_color: Pixel,
}

impl Button {
    pub fn new(x: i32, y: i32, width: u32, height: u32, label: &str) -> Self {
        Self {
            rect: Rect::new(x, y, width, height),
            label: String::from(label),
            hovered: false,
            pressed: false,
            bg_color: Pixel::rgb(60, 60, 60),
            text_color: colors::WHITE,
        }
    }

    /// Hit-test the button's clickable rectangle.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.rect.contains(x, y)
    }

    /// Update hover/press from the pointer. Returns true on a click (press→release inside).
    pub fn update_pointer(&mut self, x: i32, y: i32, primary_down: bool) -> bool {
        let over = self.rect.contains(x, y);
        let was_pressed = self.pressed;
        self.hovered = over;
        self.pressed = over && primary_down;
        was_pressed && over && !primary_down
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let bg = if self.pressed {
            colors::darken(self.bg_color, 51)
        } else if self.hovered {
            colors::lighten(self.bg_color, 38)
        } else {
            self.bg_color
        };

        // AA rounded rect background with subtle radius for modern look
        fb.fill_rounded_rect_aa(self.rect, bg, 6);
        // 1px AA border highlight
        fb.draw_rounded_rect(self.rect, colors::lighten(bg, 51), 6, 1);
        fonts::draw_string_centered_compact(
            fb,
            self.rect.x,
            self.rect.y,
            self.rect.width,
            self.rect.height,
            &self.label,
            self.text_color,
            1,
        );
    }
}
