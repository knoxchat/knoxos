// SPDX-License-Identifier: MIT
//! Progress and ProgressCircle (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::vec;
use alloc::vec::Vec;

use knox_gpui::{
    Bounds, Div, Element, Hsla, IntoElement, LayoutHints, MonochromeSprite, PaintEnv, Pixels,
    Point, Scene, Size, Styled, WindowInput, div, px,
};

use crate::size::{ControlSize, Sizable};
use crate::theme::Theme;

/// Linear 0..=100 bar.
pub struct Progress {
    theme: Theme,
    value: f32,
    color: Option<Hsla>,
    width: Pixels,
    size: ControlSize,
}

impl Progress {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            value: 0.0,
            color: None,
            width: px(160),
            size: ControlSize::Medium,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn value(mut self, value: f32) -> Self {
        self.value = value.clamp(0.0, 100.0);
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn value_of(&self) -> f32 {
        self.value
    }
}

impl Default for Progress {
    fn default() -> Self {
        Self::new()
    }
}

impl Sizable for Progress {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl IntoElement for Progress {
    type Element = Div;

    fn into_element(self) -> Div {
        let h = match self.size {
            ControlSize::XSmall | ControlSize::Small => px(4),
            ControlSize::Medium => px(6),
            ControlSize::Large => px(8),
        };
        let fill_w = px(((self.width.0 as f32) * self.value / 100.0) as i32);
        let color = self.color.unwrap_or(self.theme.colors.primary);
        div()
            .w(self.width)
            .h(h)
            .rounded(self.theme.radius.full)
            .bg(self.theme.colors.muted)
            .overflow_hidden()
            .child(
                div()
                    .w(fill_w)
                    .h(h)
                    .rounded(self.theme.radius.full)
                    .bg(color),
            )
    }
}

const GRID: i32 = 32;

/// Circular 0..=100 indicator.
pub struct ProgressCircle {
    theme: Theme,
    value: f32,
    color: Option<Hsla>,
    size: ControlSize,
    computed: Bounds,
}

impl ProgressCircle {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            value: 0.0,
            color: None,
            size: ControlSize::Medium,
            computed: Bounds::ZERO,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn value(mut self, value: f32) -> Self {
        self.value = value.clamp(0.0, 100.0);
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn value_of(&self) -> f32 {
        self.value
    }
}

impl Default for ProgressCircle {
    fn default() -> Self {
        Self::new()
    }
}

impl Sizable for ProgressCircle {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Element for ProgressCircle {
    fn request_layout(&mut self, origin: Point, _available: Size) -> Bounds {
        let s = match self.size {
            ControlSize::XSmall => 16,
            ControlSize::Small => 20,
            ControlSize::Medium => 28,
            ControlSize::Large => 36,
        };
        self.computed = Bounds::from_xywh(origin.x.0, origin.y.0, s, s);
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
        let track = raster_ring(1.0);
        let fill = raster_ring(self.value / 100.0);
        let track_tile = env
            .atlas
            .intern_alpha(0xD8_03_30, GRID as u32, GRID as u32, track);
        let fill_key = 0xD8_03_40 + (self.value as u64);
        let fill_tile = env
            .atlas
            .intern_alpha(fill_key, GRID as u32, GRID as u32, fill);
        scene.push_mono_sprite(MonochromeSprite::new(
            bounds,
            track_tile,
            self.theme.colors.muted,
        ));
        if self.value > 0.0 {
            scene.push_mono_sprite(MonochromeSprite::new(
                bounds,
                fill_tile,
                self.color.unwrap_or(self.theme.colors.primary),
            ));
        }
    }
}

fn raster_ring(frac: f32) -> Vec<u8> {
    let mut buf = vec![0u8; (GRID * GRID) as usize];
    let cx = GRID as f32 * 0.5;
    let cy = cx;
    let outer = 14.0f32;
    let inner = 10.5f32;
    let tau = core::f32::consts::TAU;
    let span = frac.clamp(0.0, 1.0) * tau;
    for y in 0..GRID {
        for x in 0..GRID {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let r = libm::sqrtf(dx * dx + dy * dy);
            if r < inner || r > outer {
                continue;
            }
            let mut ang = libm::atan2f(dx, -dy);
            if ang < 0.0 {
                ang += tau;
            }
            if ang <= span {
                buf[(y * GRID + x) as usize] = 255;
            }
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::Progress;

    #[test]
    fn value_clamps() {
        assert_eq!(Progress::new().value(150.0).value_of(), 100.0);
        assert_eq!(Progress::new().value(-4.0).value_of(), 0.0);
        assert_eq!(Progress::new().value(42.0).value_of(), 42.0);
    }
}
