use crate::gui::colors;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

// ═══════════════════════════════════════════════════════════════════════════
// SLIDER WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct Slider {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub value: u8, // 0-100
    pub track_color: Pixel,
    pub fill_color: Pixel,
    pub dragging: bool,
}

impl Slider {
    pub fn new(x: i32, y: i32, width: u32, value: u8) -> Self {
        Self {
            x,
            y,
            width,
            value,
            track_color: Pixel::rgb(50, 50, 55),
            fill_color: Pixel::rgb(82, 139, 255),
            dragging: false,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let track_h: u32 = 6;
        let track_y = self.y + 4;

        // Track (AA)
        fb.fill_rounded_rect_aa(
            Rect::new(self.x, track_y, self.width, track_h),
            self.track_color,
            3,
        );

        // Fill (AA)
        let fill_w = (self.width * self.value as u32) / 100;
        if fill_w > 0 {
            fb.fill_rounded_rect_aa(
                Rect::new(self.x, track_y, fill_w, track_h),
                self.fill_color,
                3,
            );
        }

        // Thumb (AA for smooth edges)
        let thumb_x = self.x + fill_w as i32;
        let thumb_y = track_y + 3;
        fb.fill_circle_aa(thumb_x, thumb_y, 8, self.fill_color);
        fb.fill_circle_aa(thumb_x, thumb_y, 5, colors::WHITE);
    }

    pub fn hit_test(&self, mx: i32, my: i32) -> bool {
        mx >= self.x - 8
            && mx <= self.x + self.width as i32 + 8
            && my >= self.y - 4
            && my <= self.y + 16
    }

    pub fn update_from_mouse(&mut self, mx: i32) {
        let relative = (mx - self.x).clamp(0, self.width as i32);
        self.value = (relative * 100 / self.width as i32) as u8;
    }
}
