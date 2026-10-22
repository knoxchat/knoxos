// SPDX-License-Identifier: MIT
//! `PlatformTextSystem` on `font_engine` (D5.1).

use knox_gpui::{FontId, GlyphRaster, PlatformTextSystem, Point, Size, SpriteAtlas, px};

use crate::gui::font_engine;

pub struct KnoxosTextSystem;

fn map_font(font: FontId) -> font_engine::FontId {
    match font {
        FontId::Ui => font_engine::FontId::Ui,
        FontId::UiBold => font_engine::FontId::UiBold,
        FontId::Mono => font_engine::FontId::Mono,
    }
}

impl PlatformTextSystem for KnoxosTextSystem {
    fn rasterize_glyph(
        &self,
        font: FontId,
        ch: char,
        size_px: u16,
        atlas: &mut SpriteAtlas,
    ) -> Option<GlyphRaster> {
        let g = font_engine::glyph_coverage(map_font(font), ch, size_px)?;
        // Intern by (font, size, codepoint). Allocating every paint fills the
        // atlas mid-frame and evicts glyphs already in the scene.
        let key = 0xD501_0000_0000u64
            | ((font as u64) << 40)
            | ((size_px as u64) << 24)
            | (ch as u32 as u64);
        let (w, h, coverage, bearing, size) = if g.width == 0 || g.height == 0 {
            if ch == ' ' {
                (1u32, 1u32, alloc::vec![0u8], Point::ZERO, Size::wh(1, 1))
            } else {
                let stem_h = (size_px as i32 * 3 / 4).max(4);
                let stem_w = 2i32;
                (
                    stem_w as u32,
                    stem_h as u32,
                    alloc::vec![255u8; (stem_w * stem_h) as usize],
                    Point::xy(0, size_px as i32 - stem_h),
                    Size::wh(stem_w, stem_h),
                )
            }
        } else {
            // Place from the fontdue ascent, not `size_px`. Inter Medium
            // bitmaps are taller than the em; pinning to `size_px` clipped
            // tittles and punched holes (`Primary` → `Pr ary`).
            let ascent = font_engine::ascent(map_font(font), size_px);
            let y_off = (ascent - g.bearing_y as i32).max(0);
            (
                g.width as u32,
                g.height as u32,
                g.coverage,
                Point::xy(g.bearing_x as i32, y_off),
                Size::wh(g.width as i32, g.height as i32),
            )
        };
        let tile = atlas.intern_alpha(key, w, h, coverage);
        Some(GlyphRaster {
            tile,
            bearing,
            advance: px((g.advance as i32).max(1)),
            size,
        })
    }
}
