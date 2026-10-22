use crate::serial_println;
/// Lazy Window Rendering
///
/// Only render visible portions of windows, skip occluded regions,
/// damage-driven partial updates.
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone)]
pub struct VisibilityInfo {
    pub window_id: u32,
    pub visible_rects: Vec<Rect>,
    pub fully_occluded: bool,
    pub off_screen: bool,
}

pub struct LazyRenderer {
    pub visibility: Vec<VisibilityInfo>,
    pub screen_width: u32,
    pub screen_height: u32,
    pub skipped_frames: u64,
    pub rendered_frames: u64,
}

lazy_static::lazy_static! {
    static ref RENDERER: Mutex<LazyRenderer> = Mutex::new(LazyRenderer {
        visibility: Vec::new(),
        screen_width: 1920,
        screen_height: 1080,
        skipped_frames: 0,
        rendered_frames: 0,
    });
}

impl LazyRenderer {
    pub fn compute_visibility(&mut self, windows: &[(u32, Rect)]) {
        self.visibility.clear();
        let screen = Rect {
            x: 0,
            y: 0,
            w: self.screen_width,
            h: self.screen_height,
        };

        for (i, (id, rect)) in windows.iter().enumerate() {
            let off_screen = rect.x + rect.w as i32 <= 0
                || rect.y + rect.h as i32 <= 0
                || rect.x >= screen.w as i32
                || rect.y >= screen.h as i32;

            let occluded = windows[i + 1..].iter().any(|(_, above)| {
                above.x <= rect.x
                    && above.y <= rect.y
                    && above.x + above.w as i32 >= rect.x + rect.w as i32
                    && above.y + above.h as i32 >= rect.y + rect.h as i32
            });

            self.visibility.push(VisibilityInfo {
                window_id: *id,
                visible_rects: if off_screen || occluded {
                    Vec::new()
                } else {
                    alloc::vec![*rect]
                },
                fully_occluded: occluded,
                off_screen,
            });
        }
    }

    pub fn should_render(&self, window_id: u32) -> bool {
        self.visibility
            .iter()
            .find(|v| v.window_id == window_id)
            .map(|v| !v.fully_occluded && !v.off_screen)
            .unwrap_or(true)
    }

    pub fn stats(&self) -> (u64, u64) {
        (self.rendered_frames, self.skipped_frames)
    }
}

pub fn init() {
    serial_println!("[LAZY_RENDER] Lazy window renderer initialized");
}
