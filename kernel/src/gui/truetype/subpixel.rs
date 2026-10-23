//! Subpixel-quantized glyph placement for smoother text.
use crate::gui::framebuffer::{FrameBuffer, Pixel};

use super::registry::{FONT_REGISTRY, rasterize_glyph};

// ═══════════════════════════════════════════════════════════════════════
// SUBPIXEL POSITIONING
// ═══════════════════════════════════════════════════════════════════════

/// Subpixel position quantized to 1/4 pixel increments
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubpixelPosition {
    /// x offset in 1/64th of a pixel (FreeType convention)
    pub x_frac: u8,
    /// y offset in 1/64th of a pixel
    pub y_frac: u8,
}

impl SubpixelPosition {
    /// Quantize a fractional pixel position to 4 sub-pixel positions (0, 16, 32, 48)
    pub fn quantize(x_frac: f32, y_frac: f32) -> Self {
        let qx = ((x_frac * 64.0) as u8) & 0x30; // Mask to 0, 16, 32, or 48
        let qy = ((y_frac * 64.0) as u8) & 0x30;
        Self {
            x_frac: qx,
            y_frac: qy,
        }
    }

    pub fn zero() -> Self {
        Self {
            x_frac: 0,
            y_frac: 0,
        }
    }
}

/// Draw a string with subpixel-positioned glyphs for smoother text
pub fn draw_string_subpixel(
    fb: &mut FrameBuffer,
    x: f32,
    y: i32,
    text: &str,
    color: Pixel,
    font_size: u32,
) -> f32 {
    let registry = FONT_REGISTRY.lock();
    if registry.is_empty() {
        drop(registry);
        crate::gui::fonts::draw_string(fb, x as i32, y, text, color, 1);
        return x + (text.len() as f32 * 10.0);
    }
    drop(registry);

    let mut pen_x = x;
    for ch in text.chars() {
        if let Some(bitmap) = rasterize_glyph(0, ch, font_size as f32) {
            let bx = pen_x + bitmap.bearing_x as f32;
            let by = y as f32 - bitmap.bearing_y as f32;

            for row in 0..bitmap.height {
                for col in 0..bitmap.width {
                    let alpha = bitmap.pixels[row * bitmap.width + col];
                    if alpha == 0 {
                        continue;
                    }

                    let px = (bx + col as f32) as usize;
                    let py = (by + row as f32) as usize;
                    if px < fb.width && py < fb.height {
                        let blended = Pixel::new(
                            color.r,
                            color.g,
                            color.b,
                            ((alpha as u32 * color.a as u32) / 255) as u8,
                        );
                        fb.blend_pixel(px, py, blended);
                    }
                }
            }
            pen_x += bitmap.advance as f32;
        } else {
            // Fall back to bitmap font
            crate::gui::fonts::draw_char(fb, pen_x as i32, y, ch, color, 1);
            pen_x += 10.0;
        }
    }

    pen_x
}
