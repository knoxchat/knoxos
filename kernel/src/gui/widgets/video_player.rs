use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};
use alloc::string::String;
use alloc::vec::Vec;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Video Player Widget
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

/// Video player widget with transport controls
pub struct VideoPlayer {
    pub rect: Rect,
    pub state: PlaybackState,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: u8,
    pub file_path: String,
    pub frame_data: Vec<u8>,
    pub frame_width: u32,
    pub frame_height: u32,
}

impl VideoPlayer {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self {
            rect: Rect::new(x, y, w, h),
            state: PlaybackState::Stopped,
            position_ms: 0,
            duration_ms: 0,
            volume: 80,
            file_path: String::new(),
            frame_data: Vec::new(),
            frame_width: 0,
            frame_height: 0,
        }
    }

    pub fn play(&mut self) {
        self.state = PlaybackState::Playing;
    }
    pub fn pause(&mut self) {
        self.state = PlaybackState::Paused;
    }
    pub fn stop(&mut self) {
        self.state = PlaybackState::Stopped;
        self.position_ms = 0;
    }

    pub fn seek(&mut self, ms: u64) {
        self.position_ms = ms.min(self.duration_ms);
    }

    fn format_time(ms: u64) -> String {
        let secs = ms / 1000;
        let m = secs / 60;
        let s = secs % 60;
        alloc::format!("{:02}:{:02}", m, s)
    }

    pub fn draw(&self, fb: &mut FrameBuffer) {
        let r = self.rect;
        // Video area (black background)
        fb.fill_rect(r, Pixel::rgb(0, 0, 0));

        // If we have frame data, blit it
        if !self.frame_data.is_empty() && self.frame_width > 0 && self.frame_height > 0 {
            let scale_x = r.width as f32 / self.frame_width as f32;
            let scale_y = (r.height - 40) as f32 / self.frame_height as f32;
            let scale = if scale_x < scale_y { scale_x } else { scale_y };
            let draw_w = (self.frame_width as f32 * scale) as u32;
            let draw_h = (self.frame_height as f32 * scale) as u32;
            let ox = r.x + (r.width as i32 - draw_w as i32) / 2;
            let oy = r.y + ((r.height - 40) as i32 - draw_h as i32) / 2;
            // Nearest-neighbor blit from RGBA data
            for py in 0..draw_h.min(r.height - 40) {
                let src_y = (py as f32 / scale) as usize;
                for px in 0..draw_w {
                    let src_x = (px as f32 / scale) as usize;
                    let idx = (src_y * self.frame_width as usize + src_x) * 4;
                    if idx + 3 < self.frame_data.len() {
                        let pixel = Pixel::new(
                            self.frame_data[idx],
                            self.frame_data[idx + 1],
                            self.frame_data[idx + 2],
                            self.frame_data[idx + 3],
                        );
                        fb.set_pixel((ox + px as i32) as usize, (oy + py as i32) as usize, pixel);
                    }
                }
            }
        } else {
            // No video — show placeholder
            let cx = r.x + r.width as i32 / 2;
            let cy = r.y + (r.height as i32 - 40) / 2;
            fb.fill_circle_aa(cx, cy, 30, Pixel::new(60, 60, 80, 200));
            // Play triangle
            if self.state != PlaybackState::Playing {
                for dy in -12..=12i32 {
                    let hw = (12 - dy.abs()) / 2;
                    for dx in -2..hw {
                        fb.set_pixel((cx + dx) as usize, (cy + dy) as usize, colors::WHITE);
                    }
                }
            }
        }

        // Transport bar
        let bar_y = r.y + r.height as i32 - 36;
        fb.fill_rect(
            Rect::new(r.x, bar_y, r.width, 36),
            Pixel::new(20, 20, 30, 220),
        );

        // Progress bar
        let prog_x = r.x + 80;
        let prog_w = r.width as i32 - 160;
        fb.fill_rect(
            Rect::new(prog_x, bar_y + 14, prog_w as u32, 4),
            Pixel::rgb(60, 60, 80),
        );
        if self.duration_ms > 0 {
            let fill = (self.position_ms as f32 / self.duration_ms as f32 * prog_w as f32) as i32;
            fb.fill_rect(
                Rect::new(prog_x, bar_y + 14, fill.max(0) as u32, 4),
                Pixel::rgb(80, 160, 255),
            );
        }

        // Time display
        let pos_str = Self::format_time(self.position_ms);
        let dur_str = Self::format_time(self.duration_ms);
        fonts::draw_string_compact(
            fb,
            r.x + 8,
            bar_y + 10,
            &pos_str,
            Pixel::rgb(200, 200, 220),
            1,
        );
        let time_right = alloc::format!("{}", dur_str);
        fonts::draw_string_compact(
            fb,
            r.x + r.width as i32 - 50,
            bar_y + 10,
            &time_right,
            Pixel::rgb(200, 200, 220),
            1,
        );

        // Play/Pause button
        let btn_cx = r.x + 50;
        let btn_cy = bar_y + 18;
        match self.state {
            PlaybackState::Playing => {
                fb.fill_rect(Rect::new(btn_cx - 6, btn_cy - 8, 4, 16), colors::WHITE);
                fb.fill_rect(Rect::new(btn_cx + 2, btn_cy - 8, 4, 16), colors::WHITE);
            }
            _ => {
                for dy in -8..=8i32 {
                    let hw = (8 - dy.abs()) / 2;
                    for dx in -2..hw {
                        fb.set_pixel(
                            (btn_cx + dx) as usize,
                            (btn_cy + dy) as usize,
                            colors::WHITE,
                        );
                    }
                }
            }
        }
    }
}
