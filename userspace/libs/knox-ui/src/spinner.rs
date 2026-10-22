// SPDX-License-Identifier: MIT
//! Spinner (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code: a rotating arc sprite, not SVG transform.

use alloc::vec;
use alloc::vec::Vec;

use knox_gpui::{
    Bounds, Element, Hsla, LayoutHints, MonochromeSprite, PaintEnv, Point, Scene, Size, WindowInput,
};

use crate::size::{ControlSize, Sizable};
use crate::theme::Theme;

const GRID: i32 = 24;

/// Cycling loading indicator.
pub struct Spinner {
    theme: Theme,
    size: ControlSize,
    color: Option<Hsla>,
    phase: f32,
    reduced_motion: bool,
    computed: Bounds,
}

impl Spinner {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            size: ControlSize::Medium,
            color: None,
            phase: 0.0,
            reduced_motion: false,
            computed: Bounds::ZERO,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn phase(mut self, phase: f32) -> Self {
        self.phase = phase.clamp(0.0, 1.0);
        self
    }

    pub fn reduced_motion(mut self, on: bool) -> Self {
        self.reduced_motion = on;
        self
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Sizable for Spinner {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Element for Spinner {
    fn request_layout(&mut self, origin: Point, _available: Size) -> Bounds {
        let s = self.size.icon().max(14);
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
        let start = if self.reduced_motion { 0.0 } else { self.phase };
        let span = if self.reduced_motion { 1.0 } else { 0.28 };
        let alpha = raster_arc(start, span);
        let key = 0xD8_03_80 + ((start * 24.0) as u64);
        let tile = env.atlas.intern_alpha(key, GRID as u32, GRID as u32, alpha);
        let color = self.color.unwrap_or(self.theme.colors.primary);
        scene.push_mono_sprite(MonochromeSprite::new(bounds, tile, color));
    }
}

fn raster_arc(start: f32, span: f32) -> Vec<u8> {
    let mut buf = vec![0u8; (GRID * GRID) as usize];
    let cx = (GRID as f32) * 0.5;
    let cy = cx;
    let outer = 9.0f32;
    let inner = 6.5f32;
    let tau = core::f32::consts::TAU;
    let a0 = start * tau;
    let a1 = a0 + span * tau;
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
            let mut t = ang;
            while t < a0 {
                t += tau;
            }
            if t <= a1 {
                buf[(y * GRID + x) as usize] = 255;
            }
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::raster_arc;

    #[test]
    fn arc_has_ink() {
        let a = raster_arc(0.0, 0.28);
        let b = raster_arc(0.5, 0.28);
        let ink_a = a.iter().filter(|p| **p > 0).count();
        let ink_b = b.iter().filter(|p| **p > 0).count();
        assert!(ink_a > 10 && ink_b > 10);
        assert_ne!(a, b, "rotated spinner arcs must differ");
    }
}
