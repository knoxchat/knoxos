/// Cached Aurora wallpaper rendering
use libm::sinf;
use spin::Mutex;

use crate::gui::framebuffer::{FrameBuffer, Pixel, Rect};

/// Cached wallpaper buffer — rendered once, then blit for subsequent frames
pub(crate) static WALLPAPER_CACHE: Mutex<Option<alloc::vec::Vec<u8>>> = Mutex::new(None);

/// Invalidate the wallpaper cache (call when wallpaper/resolution changes)
pub fn invalidate_wallpaper_cache() {
    *WALLPAPER_CACHE.lock() = None;
}

/// Draw the desktop wallpaper — "Aurora" design
/// Warm gradient with soft, organic blob shapes and gentle color washes
/// for a sophisticated, human-centered feel
pub(crate) fn draw_wallpaper(fb: &mut FrameBuffer) {
    let w = fb.width;
    let h = fb.height;

    // ══════════════════════════════════════════════════════════
    // BASE: Warm vertical gradient (dark charcoal to deep plum)
    // ══════════════════════════════════════════════════════════
    let top = Pixel::rgb(22, 20, 26); // Warm charcoal with purple tint
    let bottom = Pixel::rgb(14, 12, 16); // Deep warm black
    fb.fill_gradient_v(Rect::new(0, 0, w as u32, h as u32), top, bottom);

    // Subtle warm diagonal wash for depth
    for py in 0..h {
        for px in 0..w {
            let diag = ((px + py) as f32 / ((w + h) as f32)) * 255.0;
            let alpha = (diag * 0.035) as u8;
            fb.blend_pixel(px, py, Pixel::new(40, 28, 32, alpha));
        }
    }

    // ══════════════════════════════════════════════════════════
    // ORGANIC BLOBS — Soft color washes with warm tones
    // ══════════════════════════════════════════════════════════

    // Blob 1: Large coral glow (center-right)
    {
        let cx = (w * 7 / 10) as i32;
        let cy = (h * 3 / 10) as i32;
        let radius = (w / 5) as i32;

        for py in (cy - radius).max(0)..(cy + radius).min(h as i32) {
            for px in (cx - radius).max(0)..(cx + radius).min(w as i32) {
                let dx = (px - cx) as f32;
                let dy = (py - cy) as f32;
                let dist_sq = dx * dx + dy * dy;
                let r_sq = (radius * radius) as f32;
                if dist_sq < r_sq {
                    let t = 1.0 - (dist_sq / r_sq);
                    let alpha = (t * t * t * 28.0) as u8;
                    if alpha > 0 {
                        fb.blend_pixel(px as usize, py as usize, Pixel::new(232, 121, 100, alpha));
                    }
                }
            }
        }
    }

    // Blob 2: Soft lavender glow (upper-left)
    {
        let cx = (w / 5) as i32;
        let cy = (h / 4) as i32;
        let radius = (w / 7) as i32;

        for py in (cy - radius).max(0)..(cy + radius).min(h as i32) {
            for px in (cx - radius).max(0)..(cx + radius).min(w as i32) {
                let dx = (px - cx) as f32;
                let dy = (py - cy) as f32;
                let dist_sq = dx * dx + dy * dy;
                let r_sq = (radius * radius) as f32;
                if dist_sq < r_sq {
                    let t = 1.0 - (dist_sq / r_sq);
                    let alpha = (t * t * t * 22.0) as u8;
                    if alpha > 0 {
                        fb.blend_pixel(px as usize, py as usize, Pixel::new(160, 130, 200, alpha));
                    }
                }
            }
        }
    }

    // Blob 3: Warm peach wash (bottom-left, large and diffuse)
    {
        let cx = (w / 4) as i32;
        let cy = (h * 3 / 4) as i32;
        let radius_x = (w / 3) as i32;
        let radius_y = (h / 3) as i32;
        let x0 = (cx - radius_x).max(0);
        let y0 = (cy - radius_y).max(0);
        let x1 = (cx + radius_x).min(w as i32);
        let y1 = (cy + radius_y).min(h as i32);
        for py in y0..y1 {
            for px in x0..x1 {
                let dx = (px - cx) as f32 / radius_x as f32;
                let dy = (py - cy) as f32 / radius_y as f32;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq >= 1.0 {
                    continue;
                }
                let t = 1.0 - dist_sq;
                let alpha = (t * t * 10.0) as u8;
                if alpha > 0 {
                    fb.blend_pixel(px as usize, py as usize, Pixel::new(200, 130, 100, alpha));
                }
            }
        }
    }

    // Blob 4: Faint sage green (right side, mid-height)
    {
        let cx = (w * 4 / 5) as i32;
        let cy = (h * 6 / 10) as i32;
        let radius = (w / 10) as i32;

        for py in (cy - radius).max(0)..(cy + radius).min(h as i32) {
            for px in (cx - radius).max(0)..(cx + radius).min(w as i32) {
                let dx = (px - cx) as f32;
                let dy = (py - cy) as f32;
                let dist_sq = dx * dx + dy * dy;
                let r_sq = (radius * radius) as f32;
                if dist_sq < r_sq {
                    let t = 1.0 - (dist_sq / r_sq);
                    let alpha = (t * t * t * 16.0) as u8;
                    if alpha > 0 {
                        fb.blend_pixel(px as usize, py as usize, Pixel::new(130, 180, 150, alpha));
                    }
                }
            }
        }
    }

    // ── Subtle warm noise texture for organic feel ──────────────────
    // Sparse warm-tinted dots for a gentle grain
    {
        let warm_dot = Pixel::new(180, 160, 140, 12);
        let dim_dot = Pixel::new(140, 120, 110, 8);
        let mut seed: u32 = 0xCAFE_BABE;
        for _ in 0..90 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let sx = (seed % w as u32) as usize;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let sy = (seed % h as u32) as usize;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let bright = (seed % 3) == 0;
            if sx < w && sy < h {
                fb.blend_pixel(sx, sy, if bright { warm_dot } else { dim_dot });
            }
        }
    }

    // ── Aurora wave — a soft horizontal color wave across the top ────
    {
        let wave_h = h / 3;
        for py in 0..wave_h {
            let t = py as f32 / wave_h as f32;
            let alpha = ((1.0 - t) * (1.0 - t) * 12.0) as u8;
            if alpha == 0 {
                continue;
            }
            for px in 0..w {
                let wave_offset = sinf(px as f32 * 0.005 + py as f32 * 0.008) * 0.5 + 0.5;
                let r = (200.0 + wave_offset * 32.0) as u8;
                let g = (110.0 + wave_offset * 30.0) as u8;
                let b = (130.0 + wave_offset * 50.0) as u8;
                fb.blend_pixel(px, py, Pixel::new(r, g, b, alpha));
            }
        }
    }
}
