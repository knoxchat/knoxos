// SPDX-License-Identifier: MIT
//! Image (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code: pre-raster BGRA (kernel `gui/image.rs` / host buffers), not
//! runtime resvg.

use alloc::vec;
use alloc::vec::Vec;

use knox_gpui::{
    Bounds, Corners, Element, Hsla, LayoutHints, PaintEnv, Pixels, Point, PolychromeSprite, Quad,
    Scene, Size, WindowInput, px,
};

use crate::theme::Theme;

/// How a source bitmap maps into the widget box (kit `ObjectFit` role).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ObjectFit {
    #[default]
    Fill,
    Contain,
    Cover,
}

/// Pre-raster color image painted as a polychrome sprite.
pub struct Image {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    src_w: u32,
    src_h: u32,
    pixels: Option<Vec<u8>>,
    fit: ObjectFit,
    corners: Corners,
    cache_key: u64,
    computed: Bounds,
}

impl Image {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(48),
            height: px(32),
            src_w: 0,
            src_h: 0,
            pixels: None,
            fit: ObjectFit::Fill,
            corners: Corners::ZERO,
            cache_key: 0,
            computed: Bounds::ZERO,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn object_fit(mut self, fit: ObjectFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn rounded(mut self, radius: Pixels) -> Self {
        self.corners = Corners::all(radius);
        self
    }

    /// Reuse the atlas tile across frames when the bitmap is stable.
    pub fn cache_key(mut self, key: u64) -> Self {
        self.cache_key = key;
        self
    }

    /// Packed BGRA8888, `src_w * src_h * 4` bytes.
    pub fn pixels(mut self, src_w: u32, src_h: u32, pixels: Vec<u8>) -> Self {
        self.src_w = src_w;
        self.src_h = src_h;
        self.pixels = Some(pixels);
        self
    }

    pub fn solid(self, color: Hsla) -> Self {
        let w = self.width.0.max(1) as u32;
        let h = self.height.0.max(1) as u32;
        let bgra = color.to_bgra();
        let packed = bgra.to_bytes();
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&packed);
        }
        self.pixels(w, h, pixels)
    }

    /// Coral→slate gradient used by the D8.2 gallery.
    pub fn demo_gradient(width: i32, height: i32) -> Self {
        let w = width.max(1) as u32;
        let h = height.max(1) as u32;
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let t = x as f32 / w.max(1) as f32;
                let r = (232.0 * (1.0 - t) + 70.0 * t) as u8;
                let g = (121.0 * (1.0 - t) + 65.0 * t) as u8;
                let b = (100.0 * (1.0 - t) + 78.0 * t) as u8;
                let i = ((y * w + x) * 4) as usize;
                pixels[i] = b;
                pixels[i + 1] = g;
                pixels[i + 2] = r;
                pixels[i + 3] = 255;
            }
        }
        Self::new()
            .size(px(width), px(height))
            .pixels(w, h, pixels)
            .cache_key(0xD8_02_01)
    }

    pub fn has_pixels(&self) -> bool {
        self.pixels
            .as_ref()
            .map(|p| !p.is_empty() && self.src_w > 0 && self.src_h > 0)
            .unwrap_or(false)
    }
}

impl Default for Image {
    fn default() -> Self {
        Self::new()
    }
}

fn sample(pixels: &[u8], sw: u32, sh: u32, sx: i32, sy: i32) -> [u8; 4] {
    if sw == 0 || sh == 0 {
        return [0, 0, 0, 0];
    }
    let x = sx.clamp(0, sw as i32 - 1) as u32;
    let y = sy.clamp(0, sh as i32 - 1) as u32;
    let i = ((y * sw + x) * 4) as usize;
    if i + 3 >= pixels.len() {
        return [0, 0, 0, 0];
    }
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn resample(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32, fit: ObjectFit) -> Vec<u8> {
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return out;
    }
    let (off_x, off_y, used_w, used_h) = match fit {
        ObjectFit::Fill => (0.0, 0.0, sw as f32, sh as f32),
        ObjectFit::Contain => {
            let scale = (dw as f32 / sw as f32).min(dh as f32 / sh as f32);
            let used_w = sw as f32 * scale;
            let used_h = sh as f32 * scale;
            let off_x = (dw as f32 - used_w) * 0.5;
            let off_y = (dh as f32 - used_h) * 0.5;
            (off_x, off_y, used_w, used_h)
        }
        ObjectFit::Cover => {
            let scale = (dw as f32 / sw as f32).max(dh as f32 / sh as f32);
            let used_w = sw as f32 * scale;
            let used_h = sh as f32 * scale;
            let off_x = (dw as f32 - used_w) * 0.5;
            let off_y = (dh as f32 - used_h) * 0.5;
            (off_x, off_y, used_w, used_h)
        }
    };
    for y in 0..dh {
        for x in 0..dw {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let inside = px >= off_x && px < off_x + used_w && py >= off_y && py < off_y + used_h;
            let i = ((y * dw + x) * 4) as usize;
            if !inside {
                continue;
            }
            let sx = ((px - off_x) * sw as f32 / used_w) as i32;
            let sy = ((py - off_y) * sh as f32 / used_h) as i32;
            let p = sample(src, sw, sh, sx, sy);
            out[i..i + 4].copy_from_slice(&p);
        }
    }
    out
}

impl Element for Image {
    fn request_layout(&mut self, origin: Point, _available: Size) -> Bounds {
        self.computed = Bounds::new(
            origin,
            Size {
                width: self.width,
                height: self.height,
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
        let Some(src) = self.pixels.as_ref() else {
            let mut quad = Quad::new(bounds, self.theme.colors.muted);
            quad.corner_radii = self.corners;
            quad.border_color = self.theme.colors.border;
            quad.border_widths = knox_gpui::Edges::all(px(1));
            scene.push_quad(quad);
            return;
        };
        let dw = bounds.width().max(1) as u32;
        let dh = bounds.height().max(1) as u32;
        let sampled = resample(src, self.src_w, self.src_h, dw, dh, self.fit);
        let tile = env.atlas.intern_bgra(self.cache_key, dw, dh, sampled);
        let mut sprite = PolychromeSprite::new(bounds, tile);
        sprite.corner_radii = self.corners;
        scene.push_polychrome_sprite(sprite);
    }
}

#[cfg(test)]
mod tests {
    use super::{Image, ObjectFit, resample};
    use knox_gpui::{
        HostTextSystem, IntoElement, PaintEnv, Point, RasterBuffer, Scene, Size, SpriteAtlas,
        WindowInput, paint_element_with_input, px,
    };

    #[test]
    fn contain_letterboxes() {
        let src = alloc::vec![0, 0, 255, 255, 0, 0, 255, 255];
        let out = resample(&src, 2, 1, 4, 4, ObjectFit::Contain);
        // Top-left should stay empty (letterbox); a mid row should be red.
        assert_eq!(&out[0..4], &[0, 0, 0, 0]);
        let mid = ((2 * 4 + 1) * 4) as usize;
        assert_eq!(out[mid + 2], 255);
        assert_eq!(out[mid + 3], 255);
    }

    #[test]
    fn gallery_gradient_paints() {
        let mut img = Image::demo_gradient(8, 4).into_element();
        let mut atlas = SpriteAtlas::new();
        let sys = HostTextSystem;
        let mut env = PaintEnv {
            atlas: &mut atlas,
            text: Some(&sys),
        };
        let mut scene = Scene::new();
        let mut input = WindowInput::new();
        paint_element_with_input(
            &mut img,
            Point::ZERO,
            Size::wh(8, 4),
            &mut scene,
            &mut input,
            &mut env,
        );
        let mut buf = RasterBuffer::new(8, 4);
        buf.draw_with_atlas(&scene, Some(&atlas));
        let left = buf.pixel(0, 1).unwrap();
        let right = buf.pixel(7, 1).unwrap();
        assert!(left.r > right.r, "coral should sit on the left");
        assert_eq!(left.a, 255);
        let _ = px(1);
    }
}
