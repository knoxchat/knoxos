/// Screen magnification lens around the cursor (accessibility)
use crate::gui::framebuffer::{FrameBuffer, Pixel};

// ═══════════════════════════════════════════════════════════════════════════
// SCREEN MAGNIFICATION LENS (17.9)
// ═══════════════════════════════════════════════════════════════════════════

/// Apply a magnification lens around the cursor position.
/// Reads pixels from the backbuffer, scales them up, and writes back.
pub(crate) fn apply_zoom_lens(fb: &mut FrameBuffer, cx: i32, cy: i32) {
    let zoom = crate::gui::accessibility::zoom_level() as u32;
    if zoom <= 100 {
        return;
    }
    let scale = zoom as f32 / 100.0; // e.g. 150 → 1.5×
    let lens_radius = 120i32; // pixel radius of the lens circle

    let w = fb.width as i32;
    let h = fb.height as i32;

    // We iterate over every pixel in the lens area and sample from the source
    // position (closer to cursor center = more magnified).
    // To avoid reading stale data we read from backbuffer into a temp buffer
    // first, then write the magnified result.
    use alloc::vec;
    let diam = (lens_radius * 2) as usize;
    let mut buf = vec![Pixel::rgb(0, 0, 0); diam * diam];

    // Read source pixels into temp buffer
    for dy in -lens_radius..lens_radius {
        for dx in -lens_radius..lens_radius {
            let dist_sq = dx * dx + dy * dy;
            let r_sq = lens_radius * lens_radius;
            if dist_sq > r_sq {
                continue;
            }

            // Source coordinates: map back through the magnification
            let src_x = cx + (dx as f32 / scale) as i32;
            let src_y = cy + (dy as f32 / scale) as i32;

            let pixel = if src_x >= 0 && src_x < w && src_y >= 0 && src_y < h {
                fb.get_pixel(src_x as usize, src_y as usize)
            } else {
                Pixel::rgb(0, 0, 0)
            };

            let bx = (dx + lens_radius) as usize;
            let by = (dy + lens_radius) as usize;
            buf[by * diam + bx] = pixel;
        }
    }

    // Write magnified pixels back + draw circular border
    for dy in -lens_radius..lens_radius {
        for dx in -lens_radius..lens_radius {
            let dist_sq = dx * dx + dy * dy;
            let r_sq = lens_radius * lens_radius;
            if dist_sq > r_sq {
                continue;
            }

            let px = cx + dx;
            let py = cy + dy;
            if px < 0 || px >= w || py < 0 || py >= h {
                continue;
            }

            let bx = (dx + lens_radius) as usize;
            let by = (dy + lens_radius) as usize;

            // Draw border ring (2px thick)
            let outer_r = lens_radius - 1;
            let inner_r = lens_radius - 3;
            if dist_sq >= inner_r * inner_r {
                fb.set_pixel(px as usize, py as usize, Pixel::rgb(0, 200, 255));
            } else {
                fb.set_pixel(px as usize, py as usize, buf[by * diam + bx]);
            }
        }
    }
}
