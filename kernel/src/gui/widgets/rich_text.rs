use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;
use alloc::vec::Vec;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Rich Text Editor Widget
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Text formatting span
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RichTextStyle {
    Normal,
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Code,
    Heading1,
    Heading2,
}

/// A span of styled text
#[derive(Debug, Clone)]
pub struct RichSpan {
    pub text: String,
    pub style: RichTextStyle,
    pub color: Option<Pixel>,
}

/// Rich text editor widget
pub struct RichTextEditor {
    pub rect: Rect,
    pub spans: Vec<RichSpan>,
    pub cursor_pos: usize,
    pub scroll_y: i32,
    pub current_style: RichTextStyle,
    pub editable: bool,
    pub bg_color: Pixel,
}

impl RichTextEditor {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self {
            rect: Rect::new(x, y, w, h),
            spans: Vec::new(),
            cursor_pos: 0,
            scroll_y: 0,
            current_style: RichTextStyle::Normal,
            editable: true,
            bg_color: Pixel::new(25, 25, 35, 240),
        }
    }

    pub fn insert_text(&mut self, text: &str) {
        self.spans.push(RichSpan {
            text: String::from(text),
            style: self.current_style,
            color: None,
        });
    }

    pub fn set_style(&mut self, style: RichTextStyle) {
        self.current_style = style;
    }

    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for span in &self.spans {
            out.push_str(&span.text);
        }
        out
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        fb.fill_rounded_rect_aa(self.rect, self.bg_color, 6);

        let mut x = self.rect.x + 8;
        let mut y = self.rect.y + 8 - self.scroll_y;
        let max_x = self.rect.x + self.rect.width as i32 - 8;

        for span in &self.spans {
            let color = span.color.unwrap_or(Pixel::rgb(220, 220, 240));
            let scale = match span.style {
                RichTextStyle::Heading1 => 2,
                RichTextStyle::Heading2 => 1,
                _ => 1,
            };

            for ch in span.text.chars() {
                if ch == '\n' || x >= max_x {
                    x = self.rect.x + 8;
                    y += 16 * scale;
                    if ch == '\n' {
                        continue;
                    }
                }
                if y >= self.rect.y && y < self.rect.y + self.rect.height as i32 - 8 {
                    fonts::draw_char(fb, x, y, ch, color, scale as u32);
                    if span.style == RichTextStyle::Underline {
                        for ux in x..x + 8 * scale {
                            fb.set_pixel(ux as usize, (y + 12 * scale) as usize, color);
                        }
                    }
                    if span.style == RichTextStyle::Strikethrough {
                        for sx in x..x + 8 * scale {
                            fb.set_pixel(sx as usize, (y + 6 * scale) as usize, color);
                        }
                    }
                }
                x += 8 * scale;
            }
        }
    }
}
