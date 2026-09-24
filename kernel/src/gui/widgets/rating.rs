use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::theme::ThemeColors;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Rating Widget — Star-based rating selector
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct RatingWidget {
    pub x: i32,
    pub y: i32,
    pub max_stars: u8,
    pub current: u8,
    pub star_size: i32,
    pub gap: i32,
}

impl RatingWidget {
    pub fn new(x: i32, y: i32, max_stars: u8, current: u8) -> Self {
        Self {
            x,
            y,
            max_stars: max_stars.min(10),
            current: current.min(max_stars),
            star_size: 16,
            gap: 4,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer, theme: &ThemeColors) {
        for i in 0..self.max_stars {
            let sx = self.x + i as i32 * (self.star_size + self.gap);
            let filled = i < self.current;
            let color = if filled {
                Pixel::rgb(255, 200, 50)
            } else {
                Pixel::new(
                    theme.text_primary.r,
                    theme.text_primary.g,
                    theme.text_primary.b,
                    60,
                )
            };
            let cx = sx + self.star_size / 2;
            let cy = self.y + self.star_size / 2;
            let r = self.star_size / 2;
            if filled {
                fb.fill_circle_aa(cx, cy, (r - 1) as u32, color);
                // Star points
                fb.fill_rect(Rect::new(cx - 1, cy - r, 2, (r / 2) as u32), color);
                fb.fill_rect(Rect::new(cx - r, cy - 2, (r / 2) as u32, 3), color);
                fb.fill_rect(Rect::new(cx + r / 2, cy - 2, (r / 2) as u32, 3), color);
                fb.fill_rect(Rect::new(cx - r / 2, cy + r / 2, 2, (r / 2) as u32), color);
                fb.fill_rect(
                    Rect::new(cx + r / 2 - 1, cy + r / 2, 2, (r / 2) as u32),
                    color,
                );
            } else {
                fb.draw_rounded_rect(
                    Rect::new(
                        sx + 2,
                        self.y + 2,
                        (self.star_size - 4) as u32,
                        (self.star_size - 4) as u32,
                    ),
                    color,
                    (r - 2) as u32,
                    1,
                );
            }
        }
    }

    /// Hit test — returns Some(new_rating) if clicked
    pub fn hit_test(&self, mx: i32, my: i32) -> Option<u8> {
        if my < self.y || my > self.y + self.star_size {
            return None;
        }
        let total_w = self.max_stars as i32 * (self.star_size + self.gap) - self.gap;
        if mx < self.x || mx > self.x + total_w {
            return None;
        }
        let rel_x = mx - self.x;
        let star_idx = (rel_x / (self.star_size + self.gap)) as u8;
        Some((star_idx + 1).min(self.max_stars))
    }
}
