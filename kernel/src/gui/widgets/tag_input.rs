use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::theme::ThemeColors;
use alloc::string::String;
use alloc::vec::Vec;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tag Input Widget — Chips with removable tags
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct TagInput {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub tags: Vec<String>,
}

impl TagInput {
    pub fn new(x: i32, y: i32, width: u32) -> Self {
        Self {
            x,
            y,
            width,
            tags: Vec::new(),
        }
    }

    pub fn add_tag(&mut self, tag: &str) {
        if !tag.is_empty() && !self.tags.iter().any(|t| t == tag) {
            self.tags.push(String::from(tag));
        }
    }

    pub fn remove_tag(&mut self, index: usize) {
        if index < self.tags.len() {
            self.tags.remove(index);
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer, theme: &ThemeColors) {
        let chip_h = 24i32;
        let chip_gap = 6i32;
        let chip_pad = 8i32;
        let mut cx = self.x;
        let cy = self.y;

        let container_h = chip_h + 8;
        fb.fill_rounded_rect_aa(
            Rect::new(
                self.x - 4,
                self.y - 4,
                self.width + 8,
                container_h as u32 + 8,
            ),
            Pixel::new(
                theme.bg_surface.r,
                theme.bg_surface.g,
                theme.bg_surface.b,
                180,
            ),
            8,
        );

        for tag in &self.tags {
            let text_w = fonts::measure_string_width_compact(tag, 1) as i32;
            let chip_w = text_w + chip_pad * 2 + 16;
            if cx + chip_w > self.x + self.width as i32 {
                break;
            }
            let chip_rect = Rect::new(cx, cy, chip_w as u32, chip_h as u32);
            fb.fill_rounded_rect_aa(chip_rect, Pixel::new(0, 140, 220, 180), (chip_h / 2) as u32);
            fonts::draw_string_compact(
                fb,
                cx + chip_pad,
                cy + 6,
                tag,
                Pixel::rgb(255, 255, 255),
                1,
            );
            let x_btn = cx + chip_w - 14;
            fonts::draw_string_compact(fb, x_btn, cy + 6, "x", Pixel::new(255, 255, 255, 180), 1);
            cx += chip_w + chip_gap;
        }
    }

    /// Hit test — returns Some(tag_index) if the × of a tag was clicked
    pub fn hit_test_remove(&self, mx: i32, my: i32) -> Option<usize> {
        let chip_h = 24i32;
        let chip_gap = 6i32;
        let chip_pad = 8i32;
        if my < self.y || my > self.y + chip_h {
            return None;
        }
        let mut cx = self.x;
        for (i, tag) in self.tags.iter().enumerate() {
            let text_w = fonts::measure_string_width_compact(tag, 1) as i32;
            let chip_w = text_w + chip_pad * 2 + 16;
            if cx + chip_w > self.x + self.width as i32 {
                break;
            }
            let x_area_start = cx + chip_w - 16;
            if mx >= x_area_start && mx <= cx + chip_w {
                return Some(i);
            }
            cx += chip_w + chip_gap;
        }
        None
    }
}
