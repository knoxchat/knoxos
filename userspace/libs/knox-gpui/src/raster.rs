// SPDX-License-Identifier: MIT
//! Software raster target: Scene → packed BGRA.
//!
//! Internal memory is packed BGRA8888 (`b, g, r, a` per pixel), matching the
//! KnoxOS software framebuffer. D1.3 paints opaque axis-aligned quads in
//! paint order. D1.4 paints rounded-rect quads with a 1px coverage AA fringe.
//! D1.5 paints box shadows (blurred rounded rect). D1.6 scissors to layer
//! clips / `content_mask`. Sprites and underlines wait for later IDs.

use alloc::vec;
use alloc::vec::Vec;

use crate::aa::{box_shadow_coverage, rounded_rect_coverage};
use crate::geometry::{Bounds, Corners, Edges, Pixels};
use crate::scene::{ContentMask, PaintOperation, Quad, Scene, Shadow};

/// Bytes per packed BGRA pixel.
pub const BYTES_PER_PIXEL: usize = 4;

pub use crate::geometry::Bgra;

/// CPU backing store: `width * height` packed BGRA pixels, row-major, no padding.
#[derive(Clone, Debug)]
pub struct RasterBuffer {
    width: usize,
    height: usize,
    pitch: usize,
    pixels: Vec<u8>,
}

impl RasterBuffer {
    /// Allocate a zeroed (transparent) buffer.
    pub fn new(width: usize, height: usize) -> Self {
        let pitch = width.saturating_mul(BYTES_PER_PIXEL);
        let len = pitch.saturating_mul(height);
        Self {
            width,
            height,
            pitch,
            pixels: vec![0u8; len],
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Row stride in bytes (`width * 4`).
    pub fn pitch(&self) -> usize {
        self.pitch
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.pixels
    }

    pub fn pixel(&self, x: i32, y: i32) -> Option<Bgra> {
        let (x, y) = self.xy(x, y)?;
        let i = y * self.pitch + x * BYTES_PER_PIXEL;
        Some(Bgra::from_bytes([
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]))
    }

    /// Multiply every pixel's straight alpha by `factor` (window opacity).
    pub fn modulate_opacity(&mut self, factor: f32) {
        if factor >= 1.0 {
            return;
        }
        if factor <= 0.0 {
            for chunk in self.pixels.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
                chunk[3] = 0;
            }
            return;
        }
        let f = (factor * 255.0 + 0.5) as u32;
        for chunk in self.pixels.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
            chunk[3] = ((chunk[3] as u32 * f + 127) / 255) as u8;
        }
    }

    /// Fill the whole store with `color`.
    pub fn clear(&mut self, color: Bgra) {
        if color == Bgra::TRANSPARENT {
            self.pixels.fill(0);
            return;
        }
        let packed = color.to_bytes();
        for chunk in self.pixels.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
            chunk.copy_from_slice(&packed);
        }
    }

    /// Walk `scene` in paint order.
    ///
    /// Sharp opaque quads overwrite (D1.3). Rounded quads blend a coverage AA
    /// fringe (D1.4). Box shadows paint a blurred rounded-rect halo (D1.5).
    /// `StartLayer` / `EndLayer` and `content_mask` scissor destination pixels
    /// (D1.6). Monochrome glyphs and BGRA (polychrome) sprites sample the atlas.
    /// Sharp semi-transparent quads and underlines are skipped.
    pub fn draw(&mut self, scene: &Scene) {
        self.draw_with_atlas(scene, None);
    }

    /// Walk `scene` and sample glyph tiles from `atlas` (D5).
    pub fn draw_with_atlas(&mut self, scene: &Scene, atlas: Option<&crate::atlas::SpriteAtlas>) {
        let mut layers: Vec<Bounds> = Vec::new();
        let screen = self.buffer_bounds();
        for op in scene.paint_order() {
            match op {
                PaintOperation::StartLayer { clip } => layers.push(*clip),
                PaintOperation::EndLayer => {
                    layers.pop();
                }
                PaintOperation::Quad(quad) => {
                    let clip = compose_clip(screen, layers.last().copied(), quad.content_mask);
                    self.paint_quad(quad, clip);
                }
                PaintOperation::Shadow(shadow) => {
                    let clip = compose_clip(screen, layers.last().copied(), shadow.content_mask);
                    self.paint_shadow(shadow, clip);
                }
                PaintOperation::MonochromeSprite(sprite) => {
                    let clip = compose_clip(screen, layers.last().copied(), sprite.content_mask);
                    if let Some(atlas) = atlas {
                        self.paint_mono_sprite(sprite, atlas, clip);
                    }
                }
                PaintOperation::PolychromeSprite(sprite) => {
                    let clip = compose_clip(screen, layers.last().copied(), sprite.content_mask);
                    if let Some(atlas) = atlas {
                        self.paint_poly_sprite(sprite, atlas, clip);
                    }
                }
                PaintOperation::Underline(_) => {}
            }
        }
    }

    fn buffer_bounds(&self) -> Bounds {
        Bounds::from_xywh(0, 0, self.width as i32, self.height as i32)
    }

    fn paint_quad(&mut self, quad: &Quad, clip: Bounds) {
        if quad.bounds.is_empty() || clip.is_empty() {
            return;
        }
        if quad.bounds.intersect(clip).is_empty() {
            return;
        }
        let radii = quad.corner_radii.clamp_to(quad.bounds.size);
        if !quad.background.is_transparent() {
            if radii.is_sharp() {
                // Opaque axis-aligned fill (D1.3). Translucent sharp quads are
                // skipped — the rounded path is the blend path catalog controls use.
                self.fill_opaque_quad(quad, clip);
            } else {
                self.fill_rounded_quad(quad, radii, clip);
            }
        }
        if !quad.border_color.is_transparent() && !quad.border_widths.is_zero() {
            self.paint_quad_border(quad, radii, clip);
        }
    }

    fn paint_quad_border(&mut self, quad: &Quad, radii: Corners, clip: Bounds) {
        let dest = quad.bounds.intersect(clip);
        let Some((x0, x1)) = clip_span(dest.x(), dest.width(), self.width) else {
            return;
        };
        let Some((y0, y1)) = clip_span(dest.y(), dest.height(), self.height) else {
            return;
        };
        let color = quad.border_color.to_bgra();
        let inner = inset_edges(quad.bounds, quad.border_widths);
        let inner_radii = shrink_radii(radii, quad.border_widths.max());
        for y in y0..y1 {
            for x in x0..x1 {
                let px = x as i32;
                let py = y as i32;
                let outer = rounded_rect_coverage(px, py, quad.bounds, radii);
                if outer == 0 {
                    continue;
                }
                let inner_cov = if inner.is_empty() {
                    0
                } else {
                    rounded_rect_coverage(px, py, inner, inner_radii)
                };
                let ring = (outer as u32 * (255 - inner_cov as u32) + 127) / 255;
                if ring == 0 {
                    continue;
                }
                self.blend_pixel(x, y, color.modulate_alpha(ring as u8));
            }
        }
    }

    fn fill_opaque_quad(&mut self, quad: &Quad, clip: Bounds) {
        if !quad.background.is_opaque() {
            return;
        }
        let b = quad.bounds.intersect(clip);
        if b.is_empty() {
            return;
        }
        self.fill_quad(
            b.x(),
            b.y(),
            b.width(),
            b.height(),
            quad.background.to_bgra(),
        );
    }

    /// Rounded fill with coverage AA. Opaque interior pixels overwrite via
    /// source-over; corner squares get a 1px fringe (kernel `fill_rounded_rect_aa`
    /// idea, standalone buffer). Clip is a scissor — coverage still uses the
    /// original rounded bounds.
    fn fill_rounded_quad(&mut self, quad: &Quad, radii: Corners, clip: Bounds) {
        let dest = quad.bounds.intersect(clip);
        let Some((x0, x1)) = clip_span(dest.x(), dest.width(), self.width) else {
            return;
        };
        let Some((y0, y1)) = clip_span(dest.y(), dest.height(), self.height) else {
            return;
        };
        let color = quad.background.to_bgra();
        for y in y0..y1 {
            for x in x0..x1 {
                let cov = rounded_rect_coverage(x as i32, y as i32, quad.bounds, radii);
                if cov == 0 {
                    continue;
                }
                self.blend_pixel(x, y, color.modulate_alpha(cov));
            }
        }
    }

    fn paint_shadow(&mut self, shadow: &Shadow, clip: Bounds) {
        if shadow.color.is_transparent() || clip.is_empty() {
            return;
        }
        let shape = shadow.shape_bounds();
        if shape.is_empty() {
            return;
        }
        let dest = if shadow.inset {
            shadow.bounds.intersect(clip)
        } else {
            shadow.paint_bounds().intersect(clip)
        };
        if dest.is_empty() {
            return;
        }
        let Some((x0, x1)) = clip_span(dest.x(), dest.width(), self.width) else {
            return;
        };
        let Some((y0, y1)) = clip_span(dest.y(), dest.height(), self.height) else {
            return;
        };
        let radii = shadow.shape_radii();
        let color = shadow.color.to_bgra();
        let blur = shadow.blur_radius.get();
        for y in y0..y1 {
            for x in x0..x1 {
                let cov = box_shadow_coverage(x as i32, y as i32, shape, radii, blur, shadow.inset);
                if cov == 0 {
                    continue;
                }
                self.blend_pixel(x, y, color.modulate_alpha(cov));
            }
        }
    }

    fn paint_mono_sprite(
        &mut self,
        sprite: &crate::scene::MonochromeSprite,
        atlas: &crate::atlas::SpriteAtlas,
        clip: Bounds,
    ) {
        let dest = sprite.bounds.intersect(clip);
        if dest.is_empty() || sprite.color.is_transparent() {
            return;
        }
        let Some((tw, th)) = atlas.tile_size(sprite.tile) else {
            return;
        };
        if tw == 0 || th == 0 {
            return;
        }
        let Some((x0, x1)) = clip_span(dest.x(), dest.width(), self.width) else {
            return;
        };
        let Some((y0, y1)) = clip_span(dest.y(), dest.height(), self.height) else {
            return;
        };
        let color = sprite.color.to_bgra();
        let dw = sprite.bounds.width().max(1) as u32;
        let dh = sprite.bounds.height().max(1) as u32;
        let one_to_one = dw == tw && dh == th;
        for y in y0..y1 {
            for x in x0..x1 {
                let lx = (x as i32 - sprite.bounds.x()).max(0) as u32;
                let ly = (y as i32 - sprite.bounds.y()).max(0) as u32;
                let (tx, ty) = if one_to_one {
                    (lx, ly)
                } else {
                    (lx * tw / dw, ly * th / dh)
                };
                let cov = atlas.coverage(sprite.tile, tx, ty);
                if cov == 0 {
                    continue;
                }
                self.blend_glyph_pixel(x, y, color, cov);
            }
        }
    }

    fn paint_poly_sprite(
        &mut self,
        sprite: &crate::scene::PolychromeSprite,
        atlas: &crate::atlas::SpriteAtlas,
        clip: Bounds,
    ) {
        let dest = sprite.bounds.intersect(clip);
        if dest.is_empty() || sprite.opacity <= 0.0 {
            return;
        }
        let Some((tw, th)) = atlas.tile_size(sprite.tile) else {
            return;
        };
        if tw == 0 || th == 0 {
            return;
        }
        let Some((x0, x1)) = clip_span(dest.x(), dest.width(), self.width) else {
            return;
        };
        let Some((y0, y1)) = clip_span(dest.y(), dest.height(), self.height) else {
            return;
        };
        let dw = sprite.bounds.width().max(1) as u32;
        let dh = sprite.bounds.height().max(1) as u32;
        let opac = if sprite.opacity >= 1.0 {
            255u8
        } else {
            (sprite.opacity * 255.0 + 0.5) as u8
        };
        let radii = sprite.corner_radii.clamp_to(sprite.bounds.size);
        let round = !radii.is_sharp();
        for y in y0..y1 {
            for x in x0..x1 {
                let px = x as i32;
                let py = y as i32;
                let mut cov = 255u8;
                if round {
                    cov = rounded_rect_coverage(px, py, sprite.bounds, radii);
                    if cov == 0 {
                        continue;
                    }
                }
                let lx = (px - sprite.bounds.x()).max(0) as u32;
                let ly = (py - sprite.bounds.y()).max(0) as u32;
                let tx = lx * tw / dw;
                let ty = ly * th / dh;
                let mut color = atlas.bgra(sprite.tile, tx, ty);
                if sprite.grayscale {
                    color = grayscale(color);
                }
                let mut a = (color.a as u32 * opac as u32 * cov as u32 + 127 * 255) / (255 * 255);
                if a > 255 {
                    a = 255;
                }
                color.a = a as u8;
                self.blend_pixel(x, y, color);
            }
        }
    }

    fn blend_pixel(&mut self, x: usize, y: usize, src: Bgra) {
        if src.a == 0 {
            return;
        }
        let i = y * self.pitch + x * BYTES_PER_PIXEL;
        let dst = Bgra::from_bytes([
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]);
        let out = src.over(dst);
        self.pixels[i..i + BYTES_PER_PIXEL].copy_from_slice(&out.to_bytes());
    }

    /// Glyph coverage is linear light. Blending it as sRGB washed out
    /// caption-size stems on dark chrome.
    fn blend_glyph_pixel(&mut self, x: usize, y: usize, color: Bgra, cov: u8) {
        if cov == 0 {
            return;
        }
        let a = ((color.a as u32 * cov as u32 + 127) / 255) as u8;
        if a == 0 {
            return;
        }
        let i = y * self.pitch + x * BYTES_PER_PIXEL;
        let dst = Bgra::from_bytes([
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]);
        let out = if a >= 250 {
            Bgra {
                b: color.b,
                g: color.g,
                r: color.r,
                a: 255,
            }
        } else if dst.a == 0 {
            Bgra {
                b: color.b,
                g: color.g,
                r: color.r,
                a,
            }
        } else {
            let sa = a as u32;
            let inv = 255 - sa;
            let ch = |sc: u8, dc: u8| -> u8 {
                let src_lin = srgb_to_lin(sc);
                let dst_lin = srgb_to_lin(dc);
                lin_to_srgb((src_lin * sa + dst_lin * inv + 127) / 255)
            };
            let out_a = sa + (dst.a as u32 * inv + 127) / 255;
            Bgra {
                b: ch(color.b, dst.b),
                g: ch(color.g, dst.g),
                r: ch(color.r, dst.r),
                a: out_a.min(255) as u8,
            }
        };
        self.pixels[i..i + BYTES_PER_PIXEL].copy_from_slice(&out.to_bytes());
    }

    /// Opaque axis-aligned quad. Out-of-bounds pixels are clipped, not written.
    /// On x86_64 the row fill uses SSE2 16-byte stores (D13.4).
    pub fn fill_quad(&mut self, x: i32, y: i32, width: i32, height: i32, color: Bgra) {
        let Some((x0, x1)) = clip_span(x, width, self.width) else {
            return;
        };
        let Some((y0, y1)) = clip_span(y, height, self.height) else {
            return;
        };
        let packed = u32::from_le_bytes(color.to_bytes());
        for row in y0..y1 {
            let start = row * self.pitch + x0 * BYTES_PER_PIXEL;
            let end = row * self.pitch + x1 * BYTES_PER_PIXEL;
            fill_opaque_row(&mut self.pixels[start..end], packed);
        }
    }

    fn xy(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        if x < 0 || y < 0 {
            return None;
        }
        let x = x as usize;
        let y = y as usize;
        if x >= self.width || y >= self.height {
            None
        } else {
            Some((x, y))
        }
    }
}

/// True when the opaque fill path uses SIMD stores.
///
/// Host tests (std `x86_64`) use SSE2. The kernel is `x86_64-unknown-none`
/// with a soft-float / no-SSE ABI — XMM ops there #UD unless CR4.OSFXSR is
/// on, and that exception on the boot stack is a double fault. Kernel fills
/// stay scalar (D13.4 GATE still reports CPUID SSE2).
pub fn opaque_fill_uses_simd() -> bool {
    cfg!(all(target_arch = "x86_64", not(target_os = "none")))
}

fn fill_opaque_row(dst: &mut [u8], px: u32) {
    #[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
    {
        fill_opaque_row_sse2(dst, px);
    }
    #[cfg(not(all(target_arch = "x86_64", not(target_os = "none"))))]
    {
        fill_opaque_row_scalar(dst, px);
    }
}

fn fill_opaque_row_scalar(dst: &mut [u8], px: u32) {
    let bytes = px.to_le_bytes();
    for chunk in dst.chunks_exact_mut(4) {
        chunk.copy_from_slice(&bytes);
    }
}

#[cfg(all(target_arch = "x86_64", not(target_os = "none")))]
fn fill_opaque_row_sse2(dst: &mut [u8], px: u32) {
    // SAFETY: x86_64 always has SSE2; `dst` is a unique packed BGRA row.
    unsafe {
        use core::arch::x86_64::{__m128i, _mm_set1_epi32, _mm_storeu_si128};
        let mut p = dst.as_mut_ptr();
        let end = p.add(dst.len());
        let v: __m128i = _mm_set1_epi32(px as i32);
        while p.add(16) <= end {
            _mm_storeu_si128(p as *mut __m128i, v);
            p = p.add(16);
        }
        while p.add(4) <= end {
            (p as *mut u32).write_unaligned(px);
            p = p.add(4);
        }
    }
}

fn srgb_to_lin(c: u8) -> u32 {
    (c as u32 * c as u32 + 127) / 255
}

fn lin_to_srgb(l: u32) -> u8 {
    let v = libm::sqrtf((l.min(255) * 255) as f32);
    v as u32 as u8
}

fn grayscale(c: Bgra) -> Bgra {
    let y = ((c.r as u32 * 77) + (c.g as u32 * 150) + (c.b as u32 * 29)) / 256;
    Bgra {
        r: y as u8,
        g: y as u8,
        b: y as u8,
        a: c.a,
    }
}

fn compose_clip(screen: Bounds, layer: Option<Bounds>, mask: ContentMask) -> Bounds {
    let mut clip = screen;
    if let Some(layer) = layer {
        clip = clip.intersect(layer);
    }
    if let Some(mask) = mask.bounds {
        clip = clip.intersect(mask);
    }
    clip
}

fn inset_edges(bounds: Bounds, edges: Edges) -> Bounds {
    let origin = crate::geometry::Point {
        x: bounds.origin.x + edges.left,
        y: bounds.origin.y + edges.top,
    };
    let width = Pixels((bounds.size.width.0 - edges.left.0 - edges.right.0).max(0));
    let height = Pixels((bounds.size.height.0 - edges.top.0 - edges.bottom.0).max(0));
    Bounds::new(origin, crate::geometry::Size { width, height })
}

fn shrink_radii(radii: Corners, by: Pixels) -> Corners {
    let shrink = |c: Pixels| Pixels((c.0 - by.0).max(0));
    Corners {
        top_left: shrink(radii.top_left),
        top_right: shrink(radii.top_right),
        bottom_right: shrink(radii.bottom_right),
        bottom_left: shrink(radii.bottom_left),
    }
}

fn clip_span(start: i32, size: i32, limit: usize) -> Option<(usize, usize)> {
    if size <= 0 || limit == 0 {
        return None;
    }
    let end = start.saturating_add(size);
    if end <= 0 {
        return None;
    }
    let x0 = start.max(0) as usize;
    let x1 = (end as usize).min(limit);
    if x0 >= x1 { None } else { Some((x0, x1)) }
}

#[cfg(test)]
mod tests {
    use super::{BYTES_PER_PIXEL, Bgra, RasterBuffer};
    use crate::geometry::{Bounds, Hsla, Point, px};
    use crate::scene::{ContentMask, Quad, Scene, Shadow, Underline};
    use alloc::vec;
    use alloc::vec::Vec;

    /// Distinct RGB so a BGRA/RGBA swap cannot match the golden.
    const GOLDEN: Bgra = Bgra::rgb(0xE8, 0x6A, 0x4A);
    const GOLDEN_BGRA: [u8; 4] = [0x4A, 0x6A, 0xE8, 0xFF];

    fn packed_repeat(color: Bgra, pixels: usize) -> Vec<u8> {
        let packed = color.to_bytes();
        let mut out = vec![0u8; pixels * BYTES_PER_PIXEL];
        for chunk in out.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
            chunk.copy_from_slice(&packed);
        }
        out
    }

    #[test]
    fn d134_opaque_fill_selects_simd_on_x86_64() {
        assert_eq!(super::opaque_fill_uses_simd(), cfg!(target_arch = "x86_64"));
    }

    #[test]
    fn bgra_memory_order_is_b_g_r_a() {
        assert_eq!(GOLDEN.to_bytes(), GOLDEN_BGRA);
        assert_eq!(Bgra::from_bytes(GOLDEN_BGRA), GOLDEN);
        assert_ne!(GOLDEN.to_bytes(), [GOLDEN.r, GOLDEN.g, GOLDEN.b, GOLDEN.a]);
    }

    #[test]
    fn fill_32x32_quad_matches_bgra_bytes() {
        let mut buf = RasterBuffer::new(32, 32);
        buf.fill_quad(0, 0, 32, 32, GOLDEN);

        assert_eq!(buf.width(), 32);
        assert_eq!(buf.height(), 32);
        assert_eq!(buf.pitch(), 32 * BYTES_PER_PIXEL);
        assert_eq!(buf.as_bytes().len(), 32 * 32 * BYTES_PER_PIXEL);
        assert_eq!(&buf.as_bytes()[..4], &GOLDEN_BGRA);
        assert_eq!(buf.pixel(0, 0), Some(GOLDEN));
        assert_eq!(buf.pixel(31, 31), Some(GOLDEN));
        assert_eq!(buf.pixel(16, 16), Some(GOLDEN));
        assert_eq!(buf.as_bytes(), packed_repeat(GOLDEN, 32 * 32).as_slice());
    }

    #[test]
    fn layout_box_paints_only_its_bounds() {
        let mut buf = RasterBuffer::new(32, 32);
        buf.fill_quad(8, 8, 16, 16, GOLDEN);

        assert_eq!(buf.pixel(0, 0), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(7, 8), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(8, 8), Some(GOLDEN));
        assert_eq!(buf.pixel(23, 23), Some(GOLDEN));
        assert_eq!(buf.pixel(24, 8), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(8, 24), Some(Bgra::TRANSPARENT));
    }

    #[test]
    fn fill_quad_clips_out_of_bounds() {
        let mut buf = RasterBuffer::new(8, 8);
        buf.fill_quad(-4, -4, 8, 8, GOLDEN);
        assert_eq!(buf.pixel(0, 0), Some(GOLDEN));
        assert_eq!(buf.pixel(3, 3), Some(GOLDEN));
        assert_eq!(buf.pixel(4, 4), Some(Bgra::TRANSPARENT));

        buf.fill_quad(6, 6, 16, 16, GOLDEN);
        assert_eq!(buf.pixel(6, 6), Some(GOLDEN));
        assert_eq!(buf.pixel(7, 7), Some(GOLDEN));
        assert!(buf.pixel(8, 8).is_none());

        buf.clear(Bgra::TRANSPARENT);
        buf.fill_quad(0, 0, 0, 8, GOLDEN);
        buf.fill_quad(0, 0, 8, -1, GOLDEN);
        assert_eq!(buf.as_bytes(), [0u8; 8 * 8 * 4].as_slice());
    }

    /// D1.1 goldens: HSLA primaries → packed BGRA (`b, g, r, a`).
    const RED_BGRA: [u8; 4] = [0, 0, 255, 255];
    const BLUE_BGRA: [u8; 4] = [255, 0, 0, 255];
    const GREEN_BGRA: [u8; 4] = [0, 255, 0, 255];

    fn put_px(buf: &mut [u8], x: usize, y: usize, color: [u8; 4]) {
        let i = (y * 8 + x) * BYTES_PER_PIXEL;
        buf[i..i + 4].copy_from_slice(&color);
    }

    /// Independent of `RasterBuffer::fill_quad`: left 4×8 red, top-right 4×4
    /// blue, 2×2 green overlay at (3,3), bottom-right transparent.
    fn eight_by_eight_golden() -> Vec<u8> {
        let mut out = vec![0u8; 8 * 8 * BYTES_PER_PIXEL];
        for y in 0..8 {
            for x in 0..4 {
                put_px(&mut out, x, y, RED_BGRA);
            }
        }
        for y in 0..4 {
            for x in 4..8 {
                put_px(&mut out, x, y, BLUE_BGRA);
            }
        }
        for y in 3..5 {
            for x in 3..5 {
                put_px(&mut out, x, y, GREEN_BGRA);
            }
        }
        out
    }

    #[test]
    fn modulate_opacity_scales_alpha() {
        let mut buf = RasterBuffer::new(1, 1);
        buf.clear(Bgra::rgb(10, 20, 30));
        buf.modulate_opacity(0.5);
        let p = buf.pixel(0, 0).unwrap();
        assert_eq!(p.r, 10);
        assert_eq!(p.a, 128);
        buf.modulate_opacity(0.0);
        assert_eq!(buf.pixel(0, 0).unwrap().a, 0);
    }

    /// D1.3 done-when: an 8×8 Scene rasters to exact packed BGRA bytes.
    #[test]
    fn eight_by_eight_scene_matches_bgra_bytes() {
        let mut scene = Scene::new();
        // Underlines are not rasterized yet (later text ID).
        scene.push_underline(Underline::new(
            Bounds::from_xywh(0, 7, 8, 1),
            Hsla::WHITE,
            px(1),
        ));
        scene.push_quad(Quad::new(Bounds::from_xywh(0, 0, 4, 8), Hsla::RED));
        scene.push_quad(Quad::new(Bounds::from_xywh(4, 0, 4, 4), Hsla::BLUE));
        scene.push_quad(Quad::new(Bounds::from_xywh(3, 3, 2, 2), Hsla::GREEN));
        // Semi-transparent is not opaque fill.
        scene.push_quad(Quad::new(
            Bounds::from_xywh(0, 0, 8, 8),
            Hsla::RED.alpha(0.5),
        ));

        let mut buf = RasterBuffer::new(8, 8);
        buf.draw(&scene);

        let golden = eight_by_eight_golden();
        assert_eq!(buf.width(), 8);
        assert_eq!(buf.height(), 8);
        assert_eq!(buf.as_bytes().len(), 8 * 8 * BYTES_PER_PIXEL);
        assert_eq!(buf.as_bytes(), golden.as_slice());
        assert_eq!(buf.pixel(0, 0).unwrap().to_bytes(), RED_BGRA);
        assert_eq!(buf.pixel(7, 0).unwrap().to_bytes(), BLUE_BGRA);
        assert_eq!(buf.pixel(3, 3).unwrap().to_bytes(), GREEN_BGRA);
        assert_eq!(buf.pixel(4, 4).unwrap().to_bytes(), GREEN_BGRA);
        assert_eq!(buf.pixel(7, 7).unwrap().to_bytes(), [0, 0, 0, 0]);
        // RGBA memory order cannot match the red golden.
        assert_ne!(&buf.as_bytes()[..4], [255, 0, 0, 255]);
    }

    /// D1.4 fixture: 16×16 red card, radius 6, on a transparent store.
    fn rounded_card_scene() -> Scene {
        let mut scene = Scene::new();
        scene.push_quad(Quad::new(Bounds::from_xywh(0, 0, 16, 16), Hsla::RED).rounded(px(6)));
        scene
    }

    fn raster_rounded_card() -> RasterBuffer {
        let mut buf = RasterBuffer::new(16, 16);
        buf.draw(&rounded_card_scene());
        buf
    }

    fn fnv1a64(data: &[u8]) -> u64 {
        const OFFSET: u64 = 0xcbf29ce484222325;
        const PRIME: u64 = 0x00000100000001b3;
        let mut hash = OFFSET;
        for &b in data {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(PRIME);
        }
        hash
    }

    fn corner_region_alphas(buf: &RasterBuffer, x0: i32, y0: i32, r: i32) -> (bool, bool, bool) {
        let mut saw_out = false;
        let mut saw_aa = false;
        let mut saw_in = false;
        for y in y0..y0 + r {
            for x in x0..x0 + r {
                match buf.pixel(x, y).map(|p| p.a).unwrap_or(0) {
                    0 => saw_out = true,
                    255 => saw_in = true,
                    _ => saw_aa = true,
                }
            }
        }
        (saw_out, saw_aa, saw_in)
    }

    /// D1.4 done-when: corner pixels are neither fully transparent nor a hard stair.
    #[test]
    fn rounded_quad_corners_are_antialiased() {
        let buf = raster_rounded_card();
        let red = Hsla::RED.to_bgra();

        assert_eq!(buf.pixel(8, 8), Some(red));
        assert_eq!(buf.pixel(8, 0), Some(red));
        assert_eq!(buf.pixel(0, 8), Some(red));

        // AABB corners sit outside the quarter-circles (not a stair of opaque pixels).
        assert_eq!(buf.pixel(0, 0), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(15, 0), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(15, 15), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(0, 15), Some(Bgra::TRANSPARENT));

        // Locked fringe samples: straight-alpha red, coverage 35 / 204.
        assert_eq!(buf.pixel(1, 1), Some(Bgra::rgba(255, 0, 0, 35)));
        assert_eq!(buf.pixel(2, 1), Some(Bgra::rgba(255, 0, 0, 204)));
        assert_eq!(buf.pixel(1, 2), Some(Bgra::rgba(255, 0, 0, 204)));
        assert_eq!(buf.pixel(14, 1), Some(Bgra::rgba(255, 0, 0, 35)));
        assert_eq!(buf.pixel(14, 14), Some(Bgra::rgba(255, 0, 0, 35)));
        assert_eq!(buf.pixel(1, 14), Some(Bgra::rgba(255, 0, 0, 35)));

        for (x0, y0) in [(0, 0), (10, 0), (10, 10), (0, 10)] {
            let (out, aa, inside) = corner_region_alphas(&buf, x0, y0, 6);
            assert!(out, "corner ({x0},{y0}) missing outside pixels");
            assert!(inside, "corner ({x0},{y0}) missing opaque interior");
            assert!(aa, "corner ({x0},{y0}) is a hard stair (no AA coverage)");
        }

        // Diagonal of the top-left corner is a coverage ramp, not 255→0.
        let diag: Vec<u8> = (0..6).map(|i| buf.pixel(i, i).unwrap().a).collect();
        assert!(
            diag.iter().any(|&a| a > 0 && a < 255),
            "top-left diagonal is a hard stair: {diag:?}"
        );
        assert!(diag.contains(&0));
        assert!(diag.contains(&255));
    }

    /// D1.4 done-when: a hash/golden of the rounded card exists.
    #[test]
    fn rounded_quad_hash_golden() {
        let buf = raster_rounded_card();
        assert_eq!(buf.as_bytes().len(), 16 * 16 * BYTES_PER_PIXEL);
        // FNV-1a 64 of packed BGRA. Flip any channel / drop AA and this moves.
        const GOLDEN: u64 = 0x38bce146c2f01abd;
        let hash = fnv1a64(buf.as_bytes());
        assert_eq!(
            hash, GOLDEN,
            "D1.4 rounded-quad golden hash changed: {hash:#018x}"
        );
        // RGBA memory order cannot match: first pixel is transparent, but a
        // non-zero fringe sample is BGRA (b=0,g=0,r=255,a=35), not RGBA.
        assert_eq!(buf.pixel(1, 1).unwrap().to_bytes(), [0, 0, 255, 35]);
        assert_ne!(buf.pixel(1, 1).unwrap().to_bytes(), [255, 0, 0, 35]);
    }

    #[test]
    fn sharp_quad_keeps_hard_corners() {
        let mut scene = Scene::new();
        scene.push_quad(Quad::new(Bounds::from_xywh(0, 0, 16, 16), Hsla::RED));
        let mut buf = RasterBuffer::new(16, 16);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(15, 15), Some(Hsla::RED.to_bgra()));
        let has_aa = buf.as_bytes().chunks(4).any(|px| px[3] > 0 && px[3] < 255);
        assert!(!has_aa);
    }

    fn shadow_behind_card() -> RasterBuffer {
        let mut scene = Scene::new();
        let card = Bounds::from_xywh(8, 8, 16, 16);
        scene.push_shadow(Shadow::new(card, Hsla::BLACK).blur(px(4)).rounded(px(0)));
        scene.push_quad(Quad::new(card, Hsla::RED));
        let mut buf = RasterBuffer::new(32, 32);
        buf.draw(&scene);
        buf
    }

    /// D1.5 done-when: host test shows alpha falloff outside the quad bounds.
    #[test]
    fn box_shadow_alpha_falloff_outside_quad() {
        let buf = shadow_behind_card();
        let red = Hsla::RED.to_bgra();

        // Interior is the opaque card, not the shadow.
        assert_eq!(buf.pixel(16, 16), Some(red));
        assert_eq!(buf.pixel(8, 16), Some(red));

        // Far corner is outside the blur halo.
        assert_eq!(buf.pixel(0, 0), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(3, 16), Some(Bgra::TRANSPARENT));

        // Locked samples on the left edge (sharp, blur=4, black).
        assert_eq!(buf.pixel(7, 16), Some(Bgra::rgba(0, 0, 0, 112)));
        assert_eq!(buf.pixel(6, 16), Some(Bgra::rgba(0, 0, 0, 80)));
        assert_eq!(buf.pixel(4, 16), Some(Bgra::rgba(0, 0, 0, 16)));

        let halo = buf.pixel(7, 16).unwrap();
        assert!(
            halo.a > 0 && halo.a < 255,
            "expected falloff outside the quad, got {halo:?}"
        );
        // Halo is outside the card AABB.
        assert!(!Bounds::from_xywh(8, 8, 16, 16).contains(Point::xy(7, 16)));
    }

    #[test]
    fn box_shadow_offset_shifts_halo() {
        let mut scene = Scene::new();
        let card = Bounds::from_xywh(8, 8, 16, 16);
        scene.push_shadow(
            Shadow::new(card, Hsla::BLACK)
                .blur(px(4))
                .offset(Point::xy(4, 0)),
        );
        let mut buf = RasterBuffer::new(32, 32);
        buf.draw(&scene);
        // Same SDF as (7,16) on the un-shifted shadow, now at x=11.
        assert_eq!(buf.pixel(11, 16), Some(Bgra::rgba(0, 0, 0, 112)));
        assert_eq!(buf.pixel(7, 16), Some(Bgra::TRANSPARENT));
    }

    #[test]
    fn inset_shadow_stays_inside_quad() {
        let mut scene = Scene::new();
        let card = Bounds::from_xywh(8, 8, 16, 16);
        scene.push_shadow(Shadow::new(card, Hsla::BLACK).blur(px(4)).inset());
        let mut buf = RasterBuffer::new(32, 32);
        buf.draw(&scene);
        assert_eq!(buf.pixel(7, 16), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(0, 0), Some(Bgra::TRANSPARENT));
        let inner = buf.pixel(8, 16).unwrap();
        assert!(inner.a > 0, "inset edge should be shaded, got {inner:?}");
        // Center is beyond the blur, so it stays clear.
        assert_eq!(buf.pixel(16, 16), Some(Bgra::TRANSPARENT));
    }

    /// D1.6 done-when: a child quad outside the layer is discarded.
    #[test]
    fn layer_discards_child_quad_outside_clip() {
        let mut scene = Scene::new();
        scene.start_layer(Bounds::from_xywh(4, 4, 8, 8));
        scene.push_quad(Quad::new(Bounds::from_xywh(0, 0, 2, 2), Hsla::RED));
        scene.push_quad(Quad::new(Bounds::from_xywh(6, 6, 2, 2), Hsla::BLUE));
        scene.end_layer();

        let mut buf = RasterBuffer::new(16, 16);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(1, 1), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(6, 6), Some(Hsla::BLUE.to_bgra()));
        assert_eq!(buf.pixel(7, 7), Some(Hsla::BLUE.to_bgra()));
    }

    #[test]
    fn layer_scissors_overlapping_child() {
        let mut scene = Scene::new();
        scene.start_layer(Bounds::from_xywh(4, 4, 8, 8));
        scene.push_quad(Quad::new(Bounds::from_xywh(0, 0, 16, 16), Hsla::RED));
        scene.end_layer();

        let mut buf = RasterBuffer::new(16, 16);
        buf.draw(&scene);
        let red = Hsla::RED.to_bgra();
        assert_eq!(buf.pixel(3, 4), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(4, 3), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(4, 4), Some(red));
        assert_eq!(buf.pixel(11, 11), Some(red));
        assert_eq!(buf.pixel(12, 4), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(4, 12), Some(Bgra::TRANSPARENT));
    }

    #[test]
    fn content_mask_scissors_without_layer() {
        let mut scene = Scene::new();
        let mut quad = Quad::new(Bounds::from_xywh(0, 0, 16, 16), Hsla::RED);
        quad.content_mask = ContentMask::new(Bounds::from_xywh(4, 4, 8, 8));
        scene.push_quad(quad);
        let mut buf = RasterBuffer::new(16, 16);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(4, 4), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(12, 4), Some(Bgra::TRANSPARENT));
    }

    #[test]
    fn layer_clips_shadow_halo() {
        let mut scene = Scene::new();
        scene.start_layer(Bounds::from_xywh(8, 8, 16, 16));
        scene.push_shadow(Shadow::new(Bounds::from_xywh(8, 8, 16, 16), Hsla::BLACK).blur(px(4)));
        scene.end_layer();
        let mut buf = RasterBuffer::new(32, 32);
        buf.draw(&scene);
        // Halo pixel that D1.5 would paint is discarded by the layer scissor.
        assert_eq!(buf.pixel(7, 16), Some(Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(16, 16), Some(Hsla::BLACK.to_bgra()));
    }

    /// Mini D1.7 composition: rounded card + shadow + accent, Scene only.
    #[test]
    fn proof_card_scene_has_shadow_aa_and_accent() {
        let mut scene = Scene::new();
        let pad = 12;
        let card = Bounds::from_xywh(pad, pad, 40, 24);
        scene.push_shadow(
            Shadow::new(card, Hsla::BLACK.alpha(0.4))
                .rounded(px(6))
                .blur(px(6))
                .offset(Point::xy(0, 3)),
        );
        scene.push_quad(Quad::new(card, Hsla::from_rgb(44, 40, 50)).rounded(px(6)));
        scene.push_quad(Quad::new(
            Bounds::from_xywh(pad, pad + 6, 4, 12),
            Hsla::from_rgb(232, 121, 100),
        ));

        let mut buf = RasterBuffer::new(64, 48);
        buf.draw(&scene);

        // AABB corner sits outside the quarter-circle: not a fill_rect stair.
        // A bit of shadow halo may show through the cutout (offset+blur).
        let surface = Hsla::from_rgb(44, 40, 50).to_bgra();
        let corner = buf.pixel(pad, pad).unwrap();
        assert_ne!(corner, surface);
        assert!(
            corner.a < 255,
            "card AABB corner should not be opaque, got {corner:?}"
        );
        let fringe = buf.pixel(pad + 1, pad + 1).unwrap();
        assert!(
            fringe.a > 0 && fringe.a < 255,
            "card corner should be AA, got {fringe:?}"
        );

        // Accent on the flat left edge.
        assert_eq!(
            buf.pixel(pad, pad + 8),
            Some(Hsla::from_rgb(232, 121, 100).to_bgra())
        );

        // Shadow halo below the card (offset y=3, blur=6).
        let below = buf.pixel(pad + 20, pad + 24 + 2).unwrap();
        assert!(
            below.a > 0 && below.a < 255,
            "expected shadow falloff below the card, got {below:?}"
        );
        assert_eq!(buf.pixel(0, 0), Some(Bgra::TRANSPARENT));
    }

    #[test]
    fn quad_paints_one_pixel_border() {
        let mut scene = Scene::new();
        let mut quad = Quad::new(Bounds::from_xywh(0, 0, 8, 8), Hsla::BLUE);
        quad.border_color = Hsla::RED;
        quad.border_widths = crate::geometry::Edges::all(px(1));
        scene.push_quad(quad);
        let mut buf = RasterBuffer::new(8, 8);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(7, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(0, 7), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(4, 4), Some(Hsla::BLUE.to_bgra()));
    }

    #[test]
    fn polychrome_sprite_blits_bgra_tile() {
        use crate::atlas::SpriteAtlas;
        use crate::scene::PolychromeSprite;

        let mut atlas = SpriteAtlas::new();
        // 2×2: red, green / blue, white — packed BGRA.
        let pixels = alloc::vec![
            0, 0, 255, 255, // r
            0, 255, 0, 255, // g
            255, 0, 0, 255, // b
            255, 255, 255, 255, // white
        ];
        let tile = atlas.alloc_bgra(2, 2, pixels);
        let mut scene = Scene::new();
        scene.push_polychrome_sprite(PolychromeSprite::new(Bounds::from_xywh(0, 0, 2, 2), tile));
        let mut buf = RasterBuffer::new(2, 2);
        buf.draw_with_atlas(&scene, Some(&atlas));
        assert_eq!(buf.pixel(0, 0), Some(Bgra::rgb(255, 0, 0)));
        assert_eq!(buf.pixel(1, 0), Some(Bgra::rgb(0, 255, 0)));
        assert_eq!(buf.pixel(0, 1), Some(Bgra::rgb(0, 0, 255)));
        assert_eq!(buf.pixel(1, 1), Some(Bgra::rgb(255, 255, 255)));
    }
}
