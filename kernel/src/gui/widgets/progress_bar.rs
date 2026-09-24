use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

// ═══════════════════════════════════════════════════════════════════════════
// PROGRESS BAR WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct ProgressBar {
    pub rect: Rect,
    pub progress: u8, // 0-100
    pub color: Pixel,
    pub show_text: bool,
    pub indeterminate: bool,
    pub animation_tick: u32,
}

impl ProgressBar {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            rect: Rect::new(x, y, width, height),
            progress: 0,
            color: Pixel::rgb(82, 139, 255),
            show_text: true,
            indeterminate: false,
            animation_tick: 0,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        // Track (AA)
        fb.fill_rounded_rect_aa(self.rect, Pixel::rgb(40, 40, 44), 3);

        if self.indeterminate {
            // Bouncing bar animation
            let bar_w = self.rect.width / 3;
            let travel = self.rect.width - bar_w;
            let pos = (self.animation_tick % (travel * 2)) as i32;
            let actual_pos = if pos < travel as i32 {
                pos
            } else {
                travel as i32 * 2 - pos
            };
            fb.fill_rounded_rect_aa(
                Rect::new(
                    self.rect.x + actual_pos,
                    self.rect.y,
                    bar_w,
                    self.rect.height,
                ),
                self.color,
                3,
            );
        } else {
            // Determinate fill
            let fill_w = (self.rect.width * self.progress as u32) / 100;
            if fill_w > 0 {
                fb.fill_rounded_rect_aa(
                    Rect::new(self.rect.x, self.rect.y, fill_w, self.rect.height),
                    self.color,
                    3,
                );
            }

            // Percentage text
            if self.show_text && self.rect.height >= 12 {
                let pct_str = alloc::format!("{}%", self.progress);
                fonts::draw_string_compact(
                    fb,
                    self.rect.x + (self.rect.width as i32 - pct_str.len() as i32 * 8) / 2,
                    self.rect.y + (self.rect.height as i32 - 12) / 2,
                    &pct_str,
                    colors::WHITE,
                    1,
                );
            }
        }
    }
}
