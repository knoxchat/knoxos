use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use crate::gui::theme::ThemeColors;
use alloc::string::String;
use alloc::vec::Vec;

use super::util::format_usize;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Stepper / Wizard Widget — Multi-step progress indicator
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct StepperWidget {
    pub x: i32,
    pub y: i32,
    pub steps: Vec<String>,
    pub current_step: usize,
    pub step_width: i32,
}

impl StepperWidget {
    pub fn new(x: i32, y: i32, steps: Vec<String>) -> Self {
        Self {
            x,
            y,
            step_width: 120,
            current_step: 0,
            steps,
        }
    }

    pub fn draw(&self, fb: &mut FrameBuffer, theme: &ThemeColors) {
        let circle_r = 14u32;
        let line_y = self.y + circle_r as i32;

        for (i, label) in self.steps.iter().enumerate() {
            let cx = self.x + i as i32 * self.step_width + self.step_width / 2;
            let cy = line_y;

            let is_done = i < self.current_step;
            let is_current = i == self.current_step;

            // Connecting line to next step
            if i + 1 < self.steps.len() {
                let next_cx = self.x + (i + 1) as i32 * self.step_width + self.step_width / 2;
                let line_color = if is_done {
                    Pixel::new(0, 180, 120, 200)
                } else {
                    Pixel::new(
                        theme.text_primary.r,
                        theme.text_primary.g,
                        theme.text_primary.b,
                        40,
                    )
                };
                fb.fill_rect(
                    Rect::new(
                        cx + circle_r as i32,
                        cy - 1,
                        (next_cx - cx - circle_r as i32 * 2) as u32,
                        2,
                    ),
                    line_color,
                );
            }

            // Step circle
            if is_done {
                fb.fill_circle_aa(cx, cy, circle_r, Pixel::new(0, 180, 120, 220));
                fonts::draw_string_compact(fb, cx - 4, cy - 5, "v", Pixel::rgb(255, 255, 255), 1);
            } else if is_current {
                fb.fill_circle_aa(cx, cy, circle_r, Pixel::new(0, 140, 255, 220));
                let mut buf = [0u8; 4];
                let s = format_usize(i + 1, &mut buf);
                let tw = fonts::measure_string_width_compact(s, 1) as i32;
                fonts::draw_string_compact(
                    fb,
                    cx - tw / 2,
                    cy - 5,
                    s,
                    Pixel::rgb(255, 255, 255),
                    1,
                );
            } else {
                fb.fill_circle_aa(
                    cx,
                    cy,
                    circle_r,
                    Pixel::new(
                        theme.bg_surface.r,
                        theme.bg_surface.g,
                        theme.bg_surface.b,
                        200,
                    ),
                );
                fb.draw_rounded_rect(
                    Rect::new(
                        cx - circle_r as i32,
                        cy - circle_r as i32,
                        circle_r * 2,
                        circle_r * 2,
                    ),
                    Pixel::new(
                        theme.text_primary.r,
                        theme.text_primary.g,
                        theme.text_primary.b,
                        60,
                    ),
                    circle_r,
                    1,
                );
                let mut buf = [0u8; 4];
                let s = format_usize(i + 1, &mut buf);
                let tw = fonts::measure_string_width_compact(s, 1) as i32;
                fonts::draw_string_compact(
                    fb,
                    cx - tw / 2,
                    cy - 5,
                    s,
                    Pixel::new(
                        theme.text_primary.r,
                        theme.text_primary.g,
                        theme.text_primary.b,
                        120,
                    ),
                    1,
                );
            }

            // Label below circle
            let label_w = fonts::measure_string_width_compact(label, 1) as i32;
            let label_x = cx - label_w / 2;
            let label_y = cy + circle_r as i32 + 6;
            let label_color = if is_done || is_current {
                theme.text_primary
            } else {
                Pixel::new(
                    theme.text_primary.r,
                    theme.text_primary.g,
                    theme.text_primary.b,
                    100,
                )
            };
            fonts::draw_string_compact(fb, label_x, label_y, label, label_color, 1);
        }
    }

    pub fn next(&mut self) -> bool {
        if self.current_step + 1 < self.steps.len() {
            self.current_step += 1;
            true
        } else {
            false
        }
    }

    pub fn prev(&mut self) -> bool {
        if self.current_step > 0 {
            self.current_step -= 1;
            true
        } else {
            false
        }
    }
}
