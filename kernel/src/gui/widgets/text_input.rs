use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;

/// Text input widget
pub struct TextInput {
    pub rect: Rect,
    pub text: String,
    pub placeholder: String,
    pub focused: bool,
    pub cursor_pos: usize,
    pub blink_tick: u32,
}

impl TextInput {
    pub fn new(x: i32, y: i32, width: u32, height: u32, placeholder: &str) -> Self {
        Self {
            rect: Rect::new(x, y, width, height),
            text: String::new(),
            placeholder: String::from(placeholder),
            focused: false,
            cursor_pos: 0,
            blink_tick: 0,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        // AA rounded background
        fb.fill_rounded_rect_aa(self.rect, Pixel::rgb(40, 40, 40), 4);

        // Border with AA
        let border_color = if self.focused {
            colors::HIGHLIGHT
        } else {
            Pixel::rgb(80, 80, 80)
        };
        fb.draw_rounded_rect(self.rect, border_color, 4, 1);

        // Text or placeholder
        let (text, color) = if self.text.is_empty() {
            (&self.placeholder, Pixel::rgb(120, 120, 120))
        } else {
            (&self.text, colors::WHITE)
        };

        fonts::draw_string_compact(
            fb,
            self.rect.x + 6,
            self.rect.y + (self.rect.height as i32 - 12) / 2,
            text,
            color,
            1,
        );

        // Cursor — smooth blinking with rounded shape
        if self.focused {
            let cursor_x = self.rect.x + 6 + (self.cursor_pos as i32 * 8);
            // Smooth blink: sinusoidal fade using tick counter
            // Period = 36 ticks (~2 sec at 18Hz). Use triangle wave for smooth on/off.
            let phase = self.blink_tick % 36;
            let alpha = if phase < 18 {
                // Fade in: 0→255 over 18 ticks
                (phase * 255 / 18) as u8
            } else {
                // Fade out: 255→0 over 18 ticks
                ((36 - phase) * 255 / 18) as u8
            };
            // Always visible for first second after focus/keystroke
            let alpha = if self.blink_tick < 18 { 255 } else { alpha };
            if alpha > 10 {
                let cursor_color = Pixel::new(255, 255, 255, alpha);
                fb.fill_rounded_rect_aa(
                    Rect::new(cursor_x, self.rect.y + 4, 2, self.rect.height - 8),
                    cursor_color,
                    1,
                );
            }
        }
    }

    /// Advance blink animation (call once per frame tick)
    pub fn tick(&mut self) {
        if self.focused {
            self.blink_tick += 1;
        }
    }

    /// Reset blink to fully visible (call on keystroke or focus)
    pub fn reset_blink(&mut self) {
        self.blink_tick = 0;
    }

    pub fn insert_char(&mut self, ch: char) {
        self.text.insert(self.cursor_pos, ch);
        self.cursor_pos += 1;
        self.blink_tick = 0; // Reset blink on input
    }

    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.text.remove(self.cursor_pos);
            self.blink_tick = 0; // Reset blink on input
        }
    }
}
