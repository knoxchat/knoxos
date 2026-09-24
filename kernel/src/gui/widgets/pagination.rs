use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::theme::ThemeColors;

use super::util::format_usize;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Pagination Widget — Page number navigation
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct PaginationWidget {
    pub x: i32,
    pub y: i32,
    pub total_pages: usize,
    pub current_page: usize,
    pub button_size: i32,
    pub gap: i32,
}

impl PaginationWidget {
    pub fn new(x: i32, y: i32, total_pages: usize, current_page: usize) -> Self {
        Self {
            x,
            y,
            total_pages: total_pages.max(1),
            current_page: current_page.min(total_pages.saturating_sub(1)),
            button_size: 28,
            gap: 4,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer, theme: &ThemeColors) {
        // Prev arrow
        let prev_rect = Rect::new(
            self.x,
            self.y,
            self.button_size as u32,
            self.button_size as u32,
        );
        fb.fill_rounded_rect_aa(
            prev_rect,
            Pixel::new(
                theme.bg_surface.r,
                theme.bg_surface.g,
                theme.bg_surface.b,
                200,
            ),
            6,
        );
        fonts::draw_string_compact(fb, self.x + 8, self.y + 8, "<", theme.text_primary, 1);

        // Page buttons (max 7 visible)
        let max_visible = 7usize.min(self.total_pages);
        let start_page = if self.total_pages <= max_visible || self.current_page < max_visible / 2 {
            0
        } else if self.current_page >= self.total_pages - max_visible / 2 {
            self.total_pages - max_visible
        } else {
            self.current_page - max_visible / 2
        };

        let mut cx = self.x + self.button_size + self.gap;
        for i in 0..max_visible {
            let page = start_page + i;
            let rect = Rect::new(cx, self.y, self.button_size as u32, self.button_size as u32);
            let is_current = page == self.current_page;
            if is_current {
                fb.fill_rounded_rect_aa(rect, Pixel::new(0, 160, 255, 200), 6);
            } else {
                fb.fill_rounded_rect_aa(
                    rect,
                    Pixel::new(
                        theme.bg_surface.r,
                        theme.bg_surface.g,
                        theme.bg_surface.b,
                        160,
                    ),
                    6,
                );
            }
            let num_str = page + 1;
            let mut buf = [0u8; 4];
            let s = format_usize(num_str, &mut buf);
            let tw = fonts::measure_string_width_compact(s, 1) as i32;
            let text_color = if is_current {
                Pixel::rgb(255, 255, 255)
            } else {
                theme.text_primary
            };
            fonts::draw_string_compact(
                fb,
                cx + (self.button_size - tw) / 2,
                self.y + 8,
                s,
                text_color,
                1,
            );
            cx += self.button_size + self.gap;
        }

        // Next arrow
        let next_rect = Rect::new(cx, self.y, self.button_size as u32, self.button_size as u32);
        fb.fill_rounded_rect_aa(
            next_rect,
            Pixel::new(
                theme.bg_surface.r,
                theme.bg_surface.g,
                theme.bg_surface.b,
                200,
            ),
            6,
        );
        fonts::draw_string_compact(fb, cx + 8, self.y + 8, ">", theme.text_primary, 1);
    }

    /// Hit test — returns Some(new_page) if a page button was clicked
    pub fn hit_test(&self, mx: i32, my: i32) -> Option<usize> {
        if my < self.y || my > self.y + self.button_size {
            return None;
        }
        if mx >= self.x && mx < self.x + self.button_size {
            return Some(self.current_page.saturating_sub(1));
        }
        let max_visible = 7usize.min(self.total_pages);
        let start_page = if self.total_pages <= max_visible || self.current_page < max_visible / 2 {
            0
        } else if self.current_page >= self.total_pages - max_visible / 2 {
            self.total_pages - max_visible
        } else {
            self.current_page - max_visible / 2
        };
        let mut cx = self.x + self.button_size + self.gap;
        for i in 0..max_visible {
            if mx >= cx && mx < cx + self.button_size {
                return Some(start_page + i);
            }
            cx += self.button_size + self.gap;
        }
        if mx >= cx && mx < cx + self.button_size {
            return Some((self.current_page + 1).min(self.total_pages - 1));
        }
        None
    }
}
