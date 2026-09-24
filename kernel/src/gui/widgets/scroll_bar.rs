use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

/// Scrollbar widget
pub struct ScrollBar {
    pub rect: Rect,
    pub scroll_position: f32, // 0.0 - 1.0
    pub content_ratio: f32,   // visible / total
    pub vertical: bool,
}

impl ScrollBar {
    pub fn draw(&self, fb: &mut FrameBuffer) {
        // Track (AA rounded)
        fb.fill_rounded_rect_aa(self.rect, Pixel::rgb(30, 30, 30), 4);

        // Thumb
        let thumb_size = (self.content_ratio
            * if self.vertical {
                self.rect.height as f32
            } else {
                self.rect.width as f32
            })
        .max(20.0) as u32;

        let max_travel = if self.vertical {
            self.rect.height - thumb_size
        } else {
            self.rect.width - thumb_size
        };

        let thumb_pos = (self.scroll_position * max_travel as f32) as i32;

        let thumb_rect = if self.vertical {
            Rect::new(
                self.rect.x,
                self.rect.y + thumb_pos,
                self.rect.width,
                thumb_size,
            )
        } else {
            Rect::new(
                self.rect.x + thumb_pos,
                self.rect.y,
                thumb_size,
                self.rect.height,
            )
        };

        fb.fill_rounded_rect_aa(thumb_rect, Pixel::rgb(80, 80, 80), 4);
    }
}
