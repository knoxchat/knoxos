//! Global loaded-font registry and string drawing.
use alloc::vec::Vec;
use spin::Mutex;

use crate::gui::framebuffer::{FrameBuffer, Pixel};

use super::font::TtfFont;
use super::raster::rasterize_outline;
use super::types::GlyphBitmap;

// ─── Global Font Registry ────────────────────────────────────────────

/// Loaded system fonts
pub(super) static FONT_REGISTRY: Mutex<Vec<TtfFont>> = Mutex::new(Vec::new());

/// Load a TTF/OTF font from raw bytes and register it
pub fn load_font(data: &[u8]) -> Result<usize, &'static str> {
    let font = TtfFont::parse(data)?;
    let mut reg = FONT_REGISTRY.lock();
    let idx = reg.len();
    reg.push(font);
    Ok(idx)
}

/// Load a font from a VFS path
pub fn load_font_from_path(path: &str) -> Result<usize, &'static str> {
    let data = crate::vfs::read_file_dispatch(path).ok_or("File not found")?;
    load_font(&data)
}

/// Get the number of loaded fonts
pub fn font_count() -> usize {
    FONT_REGISTRY.lock().len()
}

/// Render a glyph from the specified font at the given pixel size
pub fn rasterize_glyph(font_idx: usize, codepoint: char, px_size: f32) -> Option<GlyphBitmap> {
    let reg = FONT_REGISTRY.lock();
    let font = reg.get(font_idx)?;
    let glyph_id = font.cmap_lookup(codepoint as u32)?;
    let outline = font.load_glyph(glyph_id)?;
    Some(rasterize_outline(&outline, font.units_per_em, px_size))
}

/// Draw a string using a loaded TrueType font
pub fn draw_ttf_string(
    fb: &mut FrameBuffer,
    font_idx: usize,
    x: i32,
    y: i32,
    text: &str,
    color: Pixel,
    px_size: f32,
) {
    let reg = FONT_REGISTRY.lock();
    let font = match reg.get(font_idx) {
        Some(f) => f,
        None => return,
    };

    let scale = px_size / font.units_per_em as f32;
    let mut pen_x = x;

    for ch in text.chars() {
        let glyph_id = match font.cmap_lookup(ch as u32) {
            Some(id) => id,
            None => continue,
        };
        let outline = match font.load_glyph(glyph_id) {
            Some(o) => o,
            None => continue,
        };
        let bitmap = rasterize_outline(&outline, font.units_per_em, px_size);

        // Blit the glyph bitmap with gamma-correct blending
        for gy in 0..bitmap.height {
            for gx in 0..bitmap.width {
                let alpha = bitmap.pixels[gy * bitmap.width + gx];
                if alpha == 0 {
                    continue;
                }
                let px = pen_x + bitmap.bearing_x + gx as i32;
                let py = y - bitmap.bearing_y + gy as i32;
                if px >= 0 && py >= 0 && (px as usize) < fb.width && (py as usize) < fb.height {
                    if alpha == 255 {
                        fb.set_pixel(px as usize, py as usize, color);
                    } else {
                        // Gamma-correct blending via sRGB LUTs (from fonts module)
                        let bg = fb.get_pixel(px as usize, py as usize);
                        let a = alpha as u32;
                        let inv_a = 255 - a;
                        let r = crate::gui::fonts::gamma_blend_public(color.r, bg.r, a, inv_a);
                        let g = crate::gui::fonts::gamma_blend_public(color.g, bg.g, a, inv_a);
                        let b = crate::gui::fonts::gamma_blend_public(color.b, bg.b, a, inv_a);
                        fb.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
                    }
                }
            }
        }

        pen_x += (outline.advance_width as f32 * scale) as i32;
    }
}
