use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

// ═══════════════════════════════════════════════════════════════════════════
// DATE/TIME PICKER WIDGET
// ═══════════════════════════════════════════════════════════════════════════

pub struct DateTimePicker {
    pub x: i32,
    pub y: i32,
    pub year: u16,
    pub month: u8,  // 1-12
    pub day: u8,    // 1-31
    pub hour: u8,   // 0-23
    pub minute: u8, // 0-59
    pub show_time: bool,
}

impl DateTimePicker {
    pub fn new(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            year: 2026,
            month: 3,
            day: 7,
            hour: 12,
            minute: 0,
            show_time: true,
        }
    }

    fn days_in_month(&self) -> u8 {
        match self.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if (self.year % 4 == 0 && self.year % 100 != 0) || self.year % 400 == 0 {
                    29
                } else {
                    28
                }
            }
            _ => 30,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        use alloc::format;

        let bg = Pixel::rgb(36, 36, 42);
        let w: u32 = if self.show_time { 280 } else { 220 };
        let h: u32 = 36;

        // Background
        fb.fill_rounded_rect_aa(Rect::new(self.x, self.y, w, h), bg, 6);
        fb.draw_rounded_rect(
            Rect::new(self.x, self.y, w, h),
            Pixel::rgb(60, 60, 66),
            6,
            1,
        );

        // Date display
        let month_names = [
            "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let month_name = month_names.get(self.month as usize).unwrap_or(&"???");
        let date_str = format!("{} {:02}, {:04}", month_name, self.day, self.year);
        fonts::draw_string_compact(fb, self.x + 10, self.y + 11, &date_str, colors::WHITE, 1);

        if self.show_time {
            let time_str = format!("{:02}:{:02}", self.hour, self.minute);
            fonts::draw_string_compact(
                fb,
                self.x + 170,
                self.y + 11,
                &time_str,
                Pixel::rgb(140, 200, 255),
                1,
            );
        }

        // Navigation arrows (< >)
        fonts::draw_string_compact(
            fb,
            self.x - 16,
            self.y + 11,
            "<",
            Pixel::rgb(120, 120, 130),
            1,
        );
        fonts::draw_string_compact(
            fb,
            self.x + w as i32 + 6,
            self.y + 11,
            ">",
            Pixel::rgb(120, 120, 130),
            1,
        );
    }

    /// Handle click. Returns true if the arrow buttons were clicked (to change date).
    pub fn handle_click(&mut self, mx: i32, my: i32) -> bool {
        let w: i32 = if self.show_time { 280 } else { 220 };
        let h: i32 = 36;

        // Left arrow (previous day)
        if mx >= self.x - 20 && mx < self.x && my >= self.y && my < self.y + h {
            if self.day > 1 {
                self.day -= 1;
            } else {
                if self.month > 1 {
                    self.month -= 1;
                } else {
                    self.month = 12;
                    self.year -= 1;
                }
                self.day = self.days_in_month();
            }
            return true;
        }

        // Right arrow (next day)
        if mx >= self.x + w && mx < self.x + w + 20 && my >= self.y && my < self.y + h {
            if self.day < self.days_in_month() {
                self.day += 1;
            } else {
                self.day = 1;
                if self.month < 12 {
                    self.month += 1;
                } else {
                    self.month = 1;
                    self.year += 1;
                }
            }
            return true;
        }

        false
    }
}
