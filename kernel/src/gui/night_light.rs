/// Night Light — Blue light filter for eye comfort
/// Applies a warm color temperature shift to the framebuffer.
/// Controlled by the quick settings toggle in popups.rs.
use super::framebuffer::{FrameBuffer, Rect};
use core::sync::atomic::{AtomicU8, Ordering};

/// Color temperature intensity (0 = off, 1-100 = warm tint strength)
/// Default is 40 when enabled, giving a comfortable warm shift.
static INTENSITY: AtomicU8 = AtomicU8::new(40);

/// Check if night light is currently active
pub fn is_active() -> bool {
    super::popups::QUICK_SETTINGS.lock().night_light
}

/// Set the filter intensity (0-100). Higher = warmer / more orange.
pub fn set_intensity(val: u8) {
    INTENSITY.store(val.min(100), Ordering::Relaxed);
}

pub fn intensity() -> u8 {
    INTENSITY.load(Ordering::Relaxed)
}

/// Apply the night light filter to a region of the framebuffer.
/// This reduces blue channel and slightly boosts red, simulating
/// a warm color temperature (~3400K).
///
/// Called from draw_desktop_damaged() after all rendering, before present.
pub fn apply(fb: &mut FrameBuffer, region: Rect) {
    let strength = INTENSITY.load(Ordering::Relaxed) as u16;
    if strength == 0 {
        return;
    }

    let bpp = fb.bytes_per_pixel;
    let pitch = fb.pitch;
    let w = fb.width;
    let h = fb.height;

    let x0 = region.x.max(0) as usize;
    let y0 = region.y.max(0) as usize;
    let x1 = ((region.x + region.width as i32) as usize).min(w);
    let y1 = ((region.y + region.height as i32) as usize).min(h);

    // Precompute channel multipliers (fixed-point 8.8):
    // Red:   boost slightly — multiply by (256 + strength*0.3)
    // Green: slight reduction — multiply by (256 - strength*0.15)
    // Blue:  strong reduction — multiply by (256 - strength*1.8)
    let r_mul: u16 = 256 + (strength * 77 / 256); // ~+30% at max
    let g_mul: u16 = 256u16.saturating_sub(strength * 38 / 256); // ~-15% at max
    let b_mul: u16 = 256u16.saturating_sub(strength * 460 / 256); // ~-180% at max → clamped

    for row in y0..y1 {
        let row_off = row * pitch;
        for col in x0..x1 {
            let off = row_off + col * bpp;
            if off + 2 < fb.buffer.len() {
                // Buffer is BGRA format
                let b = fb.buffer[off] as u16;
                let g = fb.buffer[off + 1] as u16;
                let r = fb.buffer[off + 2] as u16;

                fb.buffer[off] = ((b * b_mul) >> 8).min(255) as u8;
                fb.buffer[off + 1] = ((g * g_mul) >> 8).min(255) as u8;
                fb.buffer[off + 2] = ((r * r_mul) >> 8).min(255) as u8;
            }
        }
    }
}
