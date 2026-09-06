use alloc::vec;
/// Animated Cursor — Loading spinner and animated cursor types
///
/// Provides:
///   - Multi-frame cursor animation
///   - Loading spinner cursor (rotating arc)
///   - Smooth frame interpolation
///   - Timer-driven animation updates
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use crate::gui::framebuffer::Pixel;
use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// ANIMATION STATE
// ═══════════════════════════════════════════════════════════════════════

/// Current animation frame
static ANIMATION_FRAME: AtomicU8 = AtomicU8::new(0);
/// Last frame time (TSC)
static LAST_FRAME_TIME: AtomicU64 = AtomicU64::new(0);
/// Animation speed (ms per frame)
static FRAME_INTERVAL_MS: AtomicU8 = AtomicU8::new(80);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimatedCursorType {
    /// Normal cursor (not animated)
    Static,
    /// Spinning wait/busy cursor
    Spinner,
    /// Progress cursor (arrow + hourglass)
    Progress,
}

/// A single frame of an animated cursor
#[derive(Clone)]
pub struct CursorFrame {
    pub bitmap: Vec<Pixel>,
    pub width: u32,
    pub height: u32,
    pub hotspot_x: i32,
    pub hotspot_y: i32,
}

/// An animated cursor with multiple frames
pub struct AnimatedCursor {
    pub cursor_type: AnimatedCursorType,
    pub frames: Vec<CursorFrame>,
    pub frame_count: u8,
}

impl AnimatedCursor {
    pub fn new_spinner() -> Self {
        let size = 24u32;
        let center = size as f32 / 2.0;
        let radius = 8.0f32;
        let num_frames = 8u8;

        let mut frames = Vec::with_capacity(num_frames as usize);

        for frame in 0..num_frames {
            let angle_offset = (frame as f32 / num_frames as f32) * 2.0 * core::f32::consts::PI;
            let mut bitmap = vec![Pixel::new(0, 0, 0, 0); (size * size) as usize];

            // Draw spinning arc
            for deg in 0..360 {
                let angle = (deg as f32 * core::f32::consts::PI / 180.0) + angle_offset;

                // Arc covers 270 degrees of the circle
                let arc_pos = (deg as f32 / 360.0 * 2.0 * core::f32::consts::PI) - angle_offset;
                let arc_frac = if arc_pos < 0.0 {
                    arc_pos + 2.0 * core::f32::consts::PI
                } else {
                    arc_pos
                };
                let arc_frac_norm = arc_frac / (2.0 * core::f32::consts::PI);

                // Only draw 75% of the circle
                if arc_frac_norm > 0.75 {
                    continue;
                }

                // Compute alpha based on position in arc (fade out at tail)
                let alpha = if arc_frac_norm < 0.5 {
                    255
                } else {
                    (255.0 * (1.0 - (arc_frac_norm - 0.5) / 0.25)) as u8
                };

                let x = center + radius * libm::cosf(angle);
                let y = center + radius * libm::sinf(angle);

                let px = x as i32;
                let py = y as i32;
                if px >= 0 && px < size as i32 && py >= 0 && py < size as i32 {
                    let idx = (py as u32 * size + px as u32) as usize;
                    bitmap[idx] = Pixel::new(0, 180, 255, alpha);
                }

                // Draw thicker by filling neighbors
                for &(dx, dy) in &[(1i32, 0), (0, 1), (-1, 0), (0, -1)] {
                    let nx = px + dx;
                    let ny = py + dy;
                    if nx >= 0 && nx < size as i32 && ny >= 0 && ny < size as i32 {
                        let idx = (ny as u32 * size + nx as u32) as usize;
                        if bitmap[idx].a < alpha / 2 {
                            bitmap[idx] = Pixel::new(0, 180, 255, alpha / 2);
                        }
                    }
                }
            }

            // Draw center dot
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let px = (center as i32 + dx) as u32;
                    let py = (center as i32 + dy) as u32;
                    if px < size && py < size {
                        bitmap[(py * size + px) as usize] = Pixel::new(255, 255, 255, 200);
                    }
                }
            }

            frames.push(CursorFrame {
                bitmap,
                width: size,
                height: size,
                hotspot_x: center as i32,
                hotspot_y: center as i32,
            });
        }

        Self {
            cursor_type: AnimatedCursorType::Spinner,
            frames,
            frame_count: num_frames,
        }
    }

    /// Get the current frame based on time
    pub fn current_frame(&self) -> &CursorFrame {
        let idx = ANIMATION_FRAME.load(Ordering::Relaxed) as usize % self.frames.len();
        &self.frames[idx]
    }
}

/// Advance the animation frame (called periodically)
pub fn tick(tsc_now: u64, tsc_freq_khz: u64) {
    let last = LAST_FRAME_TIME.load(Ordering::Relaxed);
    let interval_ms = FRAME_INTERVAL_MS.load(Ordering::Relaxed) as u64;

    if tsc_freq_khz == 0 {
        return;
    }

    let elapsed_ms = (tsc_now - last) / (tsc_freq_khz);

    if elapsed_ms >= interval_ms {
        ANIMATION_FRAME.fetch_add(1, Ordering::Relaxed);
        LAST_FRAME_TIME.store(tsc_now, Ordering::Relaxed);
    }
}

/// Get current animation frame index
pub fn current_frame_index() -> u8 {
    ANIMATION_FRAME.load(Ordering::Relaxed)
}

/// Set animation speed
pub fn set_frame_interval(ms: u8) {
    FRAME_INTERVAL_MS.store(ms, Ordering::Relaxed);
}

/// Initialize animated cursor
pub fn init() {
    serial_println!("[KnoxOS] Animated cursor initialized");
}
