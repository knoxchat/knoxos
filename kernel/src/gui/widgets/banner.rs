use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Notification Banner Widget — Inline alert/message bar
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub enum BannerType {
    Info,
    Success,
    Warning,
    Error,
}

pub struct NotificationBanner {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub message: String,
    pub banner_type: BannerType,
}

impl NotificationBanner {
    pub fn new(x: i32, y: i32, width: u32, message: &str, banner_type: BannerType) -> Self {
        Self {
            x,
            y,
            width,
            message: String::from(message),
            banner_type,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let h = 36u32;
        let rect = Rect::new(self.x, self.y, self.width, h);

        let (bg, icon_color, icon_char) = match self.banner_type {
            BannerType::Info => (Pixel::new(20, 60, 120, 220), Pixel::rgb(80, 180, 255), "i"),
            BannerType::Success => (Pixel::new(20, 80, 40, 220), Pixel::rgb(60, 220, 100), "v"),
            BannerType::Warning => (Pixel::new(100, 80, 10, 220), Pixel::rgb(255, 200, 50), "!"),
            BannerType::Error => (Pixel::new(100, 20, 20, 220), Pixel::rgb(255, 80, 80), "x"),
        };

        fb.fill_rounded_rect_aa(rect, bg, 8);

        fb.fill_circle_aa(self.x + 20, self.y + h as i32 / 2, 10, icon_color);
        fonts::draw_string_compact(
            fb,
            self.x + 16,
            self.y + h as i32 / 2 - 5,
            icon_char,
            Pixel::rgb(255, 255, 255),
            1,
        );

        let text_x = self.x + 38;
        let text_y = self.y + (h as i32 - 10) / 2;
        fonts::draw_string_compact(
            fb,
            text_x,
            text_y,
            &self.message,
            Pixel::new(230, 235, 245, 240),
            1,
        );
    }
}
