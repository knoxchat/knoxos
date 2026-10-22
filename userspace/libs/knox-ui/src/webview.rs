// SPDX-License-Identifier: MIT
//! WebView: blit a SHM / browser backing store (not `lb-wry`).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. Later D11.7 browser chrome uses this surface.

use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px};

use crate::image::Image;
use crate::theme::Theme;

/// SHM-backed web surface (kit `WebView` replacement).
pub struct WebView {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    src_w: u32,
    src_h: u32,
    pixels: Option<Vec<u8>>,
    cache_key: u64,
}

impl WebView {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(160),
            height: px(90),
            src_w: 0,
            src_h: 0,
            pixels: None,
            cache_key: 0,
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

    /// Packed BGRA8888 SHM buffer (`src_w * src_h * 4`).
    pub fn shm(mut self, src_w: u32, src_h: u32, pixels: Vec<u8>, cache_key: u64) -> Self {
        self.src_w = src_w;
        self.src_h = src_h;
        self.pixels = Some(pixels);
        self.cache_key = cache_key;
        self
    }

    /// Gallery stand-in: a solid colored SHM surface.
    pub fn demo_color(width: i32, height: i32, color: knox_gpui::Hsla) -> Self {
        let w = width.max(1) as u32;
        let h = height.max(1) as u32;
        let packed = color.to_bgra().to_bytes();
        let mut pixels = alloc::vec![0u8; (w * h * 4) as usize];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&packed);
        }
        Self::new()
            .size(px(width), px(height))
            .shm(w, h, pixels, 0xD8_28_01)
    }
}

impl Default for WebView {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for WebView {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut img = Image::new()
            .themed(&self.theme)
            .size(self.width, self.height)
            .cache_key(self.cache_key);
        if let Some(px) = self.pixels {
            img = img.pixels(self.src_w, self.src_h, px);
        }
        div()
            .w(self.width)
            .h(self.height)
            .border_1()
            .border_color(self.theme.colors.border)
            .child(img)
    }
}

#[cfg(test)]
mod tests {
    use super::WebView;
    use crate::theme::Theme;
    use knox_gpui::{Hsla, IntoElement, px};

    #[test]
    fn gallery_shows_colored_shm() {
        let _ = WebView::demo_color(64, 36, Hsla::from_rgb(40, 120, 200))
            .themed(&Theme::aurora_dark())
            .into_element();
        let _ = WebView::new()
            .themed(&Theme::aurora_dark())
            .size(px(32), px(16))
            .into_element();
    }
}
