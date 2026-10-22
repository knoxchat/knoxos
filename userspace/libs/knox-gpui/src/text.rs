// SPDX-License-Identifier: MIT
//! Text API and `PlatformTextSystem` (GPUI `text_system` role).
//!
//! Rasterization is platform-owned (`font_engine` in the kernel; a bitmap
//! stub on the host). Glyphs land in [`SpriteAtlas`] as coverage tiles.

use alloc::string::String;
use alloc::vec::Vec;

use crate::atlas::SpriteAtlas;
use crate::element::{Element, PaintEnv};
use crate::geometry::{Bounds, Hsla, Pixels, Point, Size, px};
use crate::input::WindowInput;
use crate::layout::LayoutHints;
use crate::scene::{MonochromeSprite, Scene};

/// Matches kernel `font_engine::FontId` roles (Ui / UiBold / Mono).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontId {
    #[default]
    Ui,
    UiBold,
    Mono,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphId(pub u32);

/// One rasterized glyph in the atlas.
#[derive(Clone, Copy, Debug)]
pub struct GlyphRaster {
    pub tile: crate::scene::AtlasTile,
    pub bearing: Point,
    pub advance: Pixels,
    pub size: Size,
}

/// Metrics / raster seam. Implemented by KnoxOS `font_engine` and host tests.
pub trait PlatformTextSystem {
    fn glyph_id(&self, font: FontId, ch: char) -> GlyphId {
        let _ = font;
        GlyphId(ch as u32)
    }

    /// Rasterize `ch` into `atlas`. Must produce a non-empty tile for `'A'`.
    fn rasterize_glyph(
        &self,
        font: FontId,
        ch: char,
        size_px: u16,
        atlas: &mut SpriteAtlas,
    ) -> Option<GlyphRaster>;

    fn advance(&self, font: FontId, ch: char, size_px: u16) -> Pixels {
        self.rasterize_glyph(font, ch, size_px, &mut SpriteAtlas::new())
            .map(|g| g.advance)
            .unwrap_or(px(size_px as i32 / 2))
    }
}

/// Shaped run painted as monochrome sprites.
pub struct Text {
    value: String,
    color: Hsla,
    font: FontId,
    size_px: u16,
    tracking: i32,
    computed: Bounds,
}

pub fn text(value: impl Into<String>) -> Text {
    Text {
        value: value.into(),
        color: Hsla::WHITE,
        font: FontId::Ui,
        size_px: 16,
        tracking: 0,
        computed: Bounds::ZERO,
    }
}

impl Text {
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = color;
        self
    }

    pub fn font(mut self, font: FontId) -> Self {
        self.font = font;
        self
    }

    pub fn size(mut self, size_px: u16) -> Self {
        self.size_px = size_px;
        self
    }

    /// Extra pixels between glyphs (splash wordmark / tagline lockup).
    pub fn tracking(mut self, tracking: i32) -> Self {
        self.tracking = tracking.max(0);
        self
    }

    pub fn computed(&self) -> Bounds {
        self.computed
    }

    /// Inter advances in 16ths of an em.
    /// Layout may overestimate; paint uses the real fontdue advance.
    fn layout_advance(&self, ch: char) -> i32 {
        let em = self.size_px as i32;
        let sixteenths = match ch {
            ' ' => 5,
            'i' | 'j' | 'l' | 'I' | '.' | ',' | ':' | ';' | '!' | '\'' | '|' => 4,
            'f' | 't' | 'r' | 'J' => 6,
            'm' | 'M' | 'w' | 'W' => 14,
            _ => 9,
        };
        ((em * sixteenths) / 16).max(2)
    }

    /// Extra descender / tittle room so Inter Medium stems survive overflow clip.
    /// Large wordmarks keep the requested em so splash lockups do not grow.
    fn line_height(&self) -> i32 {
        let em = self.size_px as i32;
        if em <= 24 { em + (em / 4).max(4) } else { em }
    }
}

impl Element for Text {
    fn request_layout(&mut self, origin: Point, _available: Size) -> Bounds {
        let line_h = px(self.line_height());
        let mut x = 0i32;
        let mut lines = 1i32;
        let mut line_w = 0i32;
        // Single-line labels (buttons, desktop names, empty titles) wrap only
        // on `\n`. Soft-wrap vs `overflow_hidden` punched holes in words.
        for ch in self.value.chars() {
            if ch == '\n' {
                lines += 1;
                x = x.max(line_w);
                line_w = 0;
                continue;
            }
            if line_w > 0 {
                line_w += self.tracking;
            }
            line_w += self.layout_advance(ch);
        }
        x = x.max(line_w);
        // Left slack for negative LSB, right slack for RSB / stem pad.
        if x > 0 {
            x += 4;
        }
        self.computed = Bounds::new(
            origin,
            Size {
                width: px(x),
                height: line_h * lines,
            },
        );
        self.computed
    }

    fn layout_hints(&self) -> LayoutHints {
        LayoutHints::default()
    }

    fn paint(
        &mut self,
        bounds: Bounds,
        scene: &mut Scene,
        _input: &WindowInput,
        env: &mut PaintEnv,
    ) {
        let bounds = if bounds.is_empty() {
            self.computed
        } else {
            bounds
        };
        let Some(sys) = env.text else {
            return;
        };
        let line_h = self.line_height();
        let mut pen_y = bounds.y();
        // 1px left inset matches request_layout slack so a negative LSB cannot
        // sit outside overflow clip.
        let mut line_x = 1i32;
        for ch in self.value.chars() {
            if ch == '\n' {
                line_x = 1;
                pen_y += line_h;
                continue;
            }
            let Some(glyph) = sys.rasterize_glyph(self.font, ch, self.size_px, env.atlas) else {
                line_x += self.layout_advance(ch);
                continue;
            };
            let dest = Bounds::new(
                Point {
                    x: px(bounds.x() + line_x + glyph.bearing.x.0),
                    y: px((pen_y + glyph.bearing.y.0).max(pen_y)),
                },
                glyph.size,
            );
            scene.push_mono_sprite(MonochromeSprite::new(dest, glyph.tile, self.color));
            line_x += glyph.advance.0.max(1) + self.tracking;
        }
    }
}

/// 5×7 bitmap `'A'` plus a filled cell for other ASCII, for host tests (D5.1).
pub struct HostTextSystem;

/// Coverage for a 5×7 capital A (1 = ink).
const A_BITS: [u8; 7] = [
    0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
];

impl PlatformTextSystem for HostTextSystem {
    fn rasterize_glyph(
        &self,
        _font: FontId,
        ch: char,
        size_px: u16,
        atlas: &mut SpriteAtlas,
    ) -> Option<GlyphRaster> {
        let (w, h, alpha) = if ch == 'A' || ch == 'a' {
            let mut pxls = Vec::with_capacity(35);
            for row in A_BITS {
                for col in (0..5).rev() {
                    pxls.push(if (row >> col) & 1 == 1 { 255 } else { 0 });
                }
            }
            (5u32, 7u32, pxls)
        } else if ch == ' ' {
            let key = 0xD502_0000_0000u64 | ((size_px as u64) << 16) | (b' ' as u64);
            return Some(GlyphRaster {
                tile: atlas.intern_alpha(key, 1, 1, alloc::vec![0]),
                bearing: Point::ZERO,
                advance: px((size_px as i32 / 3).max(3)),
                size: Size::wh(1, 1),
            });
        } else {
            let side = (size_px as u32 / 2).max(3);
            // Distinct coverage per character so display "0" vs "7" changes SHM.
            let ink = if ch.is_ascii() {
                (ch as u8).max(32)
            } else {
                200
            };
            let pxls = alloc::vec![ink; (side * side) as usize];
            (side, side, pxls)
        };
        let key = 0xD502_0000_0000u64 | ((size_px as u64) << 16) | (ch as u32 as u64);
        let tile = atlas.intern_alpha(key, w, h, alpha);
        Some(GlyphRaster {
            tile,
            bearing: Point::ZERO,
            advance: px(w as i32 + 1),
            size: Size::wh(w as i32, h as i32),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{FontId, HostTextSystem, PlatformTextSystem, text};
    use crate::atlas::SpriteAtlas;
    use crate::element::{Element, PaintEnv, paint_element_with_input};
    use crate::geometry::{Hsla, Point, Size};
    use crate::input::WindowInput;
    use crate::raster::RasterBuffer;
    use crate::scene::Scene;
    /// D5.1 done-when: host test rasterizes `'A'` non-empty.
    #[test]
    fn rasterizes_a_non_empty() {
        let mut atlas = SpriteAtlas::new();
        let g = HostTextSystem
            .rasterize_glyph(FontId::Ui, 'A', 8, &mut atlas)
            .expect("A");
        assert!(g.size.width.0 > 0 && g.size.height.0 > 0);
        let mut ink = 0u32;
        for y in 0..g.size.height.0 as u32 {
            for x in 0..g.size.width.0 as u32 {
                ink += atlas.coverage(g.tile, x, y) as u32;
            }
        }
        assert!(ink > 0, "glyph A coverage was empty");
    }

    /// D5.2: text element paints a TTF-equivalent label as mono sprites.
    #[test]
    fn text_element_paints_a() {
        let mut atlas = SpriteAtlas::new();
        let sys = HostTextSystem;
        let mut env = PaintEnv {
            atlas: &mut atlas,
            text: Some(&sys),
        };
        let mut el = text("A").color(Hsla::WHITE);
        let mut scene = Scene::new();
        let mut input = WindowInput::new();
        paint_element_with_input(
            &mut el,
            Point::ZERO,
            Size::wh(16, 16),
            &mut scene,
            &mut input,
            &mut env,
        );
        let mut buf = RasterBuffer::new(16, 16);
        buf.draw_with_atlas(&scene, Some(&atlas));
        let mut ink = 0u32;
        for y in 0..16 {
            for x in 0..16 {
                if let Some(p) = buf.pixel(x, y) {
                    if p.a > 0 {
                        ink += 1;
                    }
                }
            }
        }
        assert!(ink > 0, "expected painted glyph pixels");
    }

    /// D5.3: grayscale coverage is not a 1-bit stencil on a dark background.
    #[test]
    fn gray_coverage_is_not_stencil() {
        use crate::geometry::Bounds;
        use crate::scene::{AtlasTile, MonochromeSprite};
        let mut atlas = SpriteAtlas::new();
        let tile = atlas.alloc_alpha(1, 1, alloc::vec![128]);
        let mut scene = Scene::new();
        scene.push_quad(crate::scene::Quad::new(
            Bounds::from_xywh(0, 0, 2, 2),
            Hsla::BLACK,
        ));
        scene.push_mono_sprite(MonochromeSprite::new(
            Bounds::from_xywh(0, 0, 1, 1),
            tile,
            Hsla::WHITE,
        ));
        let mut buf = RasterBuffer::new(2, 2);
        buf.draw_with_atlas(&scene, Some(&atlas));
        let p = buf.pixel(0, 0).unwrap();
        assert!(p.r > 0 && p.r < 255, "expected mid gray, got {p:?}");
        let _ = AtlasTile::new(0, Bounds::ZERO);
    }

    #[test]
    fn labels_do_not_soft_wrap() {
        let mut el = text("Secondary").size(13);
        let bounds = el.request_layout(Point::ZERO, Size::wh(36, 32));
        assert_eq!(
            bounds.height(),
            17,
            "button/desktop labels wrap only on newline"
        );
        assert!(
            bounds.width() >= 50,
            "Secondary must keep Inter cell room, got {}",
            bounds.width()
        );
        assert!(
            bounds.width() > 36,
            "intrinsic width must not shrink to the proposal"
        );
        let mut wrapped = text("One\nTwo").size(13);
        let two = wrapped.request_layout(Point::ZERO, Size::wh(200, 40));
        assert_eq!(two.height(), 34);
        let mut files = text("Files").size(13);
        let files_b = files.request_layout(Point::ZERO, Size::wh(200, 40));
        assert!(
            files_b.width() >= 25,
            "Files must not clip the last letter, got {}",
            files_b.width()
        );
    }

    #[test]
    fn tracking_widens_wordmark() {
        let mut tight = text("KnoxOS").size(64);
        let a = tight.request_layout(Point::ZERO, Size::wh(400, 80));
        let mut airy = text("KnoxOS").size(64).tracking(2);
        let b = airy.request_layout(Point::ZERO, Size::wh(400, 80));
        assert!(
            b.width() > a.width(),
            "tracking must add space between letters"
        );
        assert_eq!(b.height(), 64);
    }
}
