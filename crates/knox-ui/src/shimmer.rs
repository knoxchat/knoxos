// SPDX-License-Identifier: MIT
//! Shimmer (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. Sweep is a highlight quad, not a GPU gradient.

use alloc::string::String;

use knox_gpui::{
    Bounds, Div, Element, Hsla, IntoElement, LayoutHints, PaintEnv, Point, Quad, Scene, Size,
    WindowInput, div, px, text,
};

use crate::theme::Theme;

/// Sweeping highlight over a label. Reduced-motion paints the label only.
pub struct Shimmer {
    theme: Theme,
    label: String,
    phase: f32,
    size_px: u16,
    reduced_motion: bool,
}

impl Shimmer {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            label: label.into(),
            phase: 0.0,
            size_px: 13,
            reduced_motion: false,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn phase(mut self, phase: f32) -> Self {
        self.phase = phase.clamp(0.0, 1.0);
        self
    }

    pub fn size(mut self, size_px: u16) -> Self {
        self.size_px = size_px.max(10);
        self
    }

    pub fn reduced_motion(mut self, on: bool) -> Self {
        self.reduced_motion = on;
        self
    }
}

impl IntoElement for Shimmer {
    type Element = ShimmerEl;

    fn into_element(self) -> ShimmerEl {
        let label = text(self.label.clone())
            .color(self.theme.colors.muted_foreground)
            .size(self.size_px);
        ShimmerEl {
            inner: div().child(label),
            highlight: self.theme.colors.foreground.opacity(0.16),
            phase: self.phase,
            reduced_motion: self.reduced_motion,
            computed: Bounds::ZERO,
        }
    }
}

pub struct ShimmerEl {
    inner: Div,
    highlight: Hsla,
    phase: f32,
    reduced_motion: bool,
    computed: Bounds,
}

impl Element for ShimmerEl {
    fn request_layout(&mut self, origin: Point, available: Size) -> Bounds {
        self.computed = self.inner.request_layout(origin, available);
        self.computed
    }

    fn layout_hints(&self) -> LayoutHints {
        self.inner.layout_hints()
    }

    fn prepaint(&mut self, bounds: Bounds, cx: &mut WindowInput) {
        self.inner.prepaint(bounds, cx);
    }

    fn paint(
        &mut self,
        bounds: Bounds,
        scene: &mut Scene,
        input: &WindowInput,
        env: &mut PaintEnv,
    ) {
        let bounds = if bounds.is_empty() {
            self.computed
        } else {
            bounds
        };
        scene.start_layer(bounds);
        self.inner.paint(bounds, scene, input, env);
        if !self.reduced_motion && bounds.width() > 1 {
            let band = (bounds.width() / 3).max(8);
            let travel = bounds.width() + band;
            let x = bounds.x() - band + (self.phase * travel as f32) as i32;
            scene.push_quad(
                Quad::new(
                    Bounds::from_xywh(x, bounds.y(), band, bounds.height().max(1)),
                    self.highlight,
                )
                .rounded(px(1)),
            );
        }
        scene.end_layer();
    }

    fn mouse_down(&mut self, event: &knox_gpui::MouseEvent, cx: &mut WindowInput) {
        self.inner.mouse_down(event, cx);
    }
    fn mouse_up(&mut self, event: &knox_gpui::MouseEvent, cx: &mut WindowInput) {
        self.inner.mouse_up(event, cx);
    }
    fn click(&mut self, event: &knox_gpui::ClickEvent, cx: &mut WindowInput) {
        self.inner.click(event, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::Shimmer;
    use crate::theme::Theme;
    use knox_gpui::{
        HostTextSystem, IntoElement, PaintEnv, Point, RasterBuffer, Scene, Size, SpriteAtlas,
        WindowInput, paint_element_with_input,
    };

    fn paint(phase: f32, reduced: bool) -> RasterBuffer {
        let theme = Theme::aurora_dark();
        let mut el = Shimmer::new("Loading")
            .themed(&theme)
            .phase(phase)
            .reduced_motion(reduced)
            .into_element();
        let mut atlas = SpriteAtlas::new();
        let sys = HostTextSystem;
        let mut env = PaintEnv {
            atlas: &mut atlas,
            text: Some(&sys),
        };
        let mut scene = Scene::new();
        let mut input = WindowInput::new();
        paint_element_with_input(
            &mut el,
            Point::ZERO,
            Size::wh(120, 20),
            &mut scene,
            &mut input,
            &mut env,
        );
        let mut buf = RasterBuffer::new(120, 20);
        buf.draw_with_atlas(&scene, Some(&atlas));
        buf
    }

    fn fingerprint(buf: &RasterBuffer) -> u32 {
        let mut h = 2166136261u32;
        for y in 0..buf.height() as i32 {
            for x in 0..buf.width() as i32 {
                if let Some(p) = buf.pixel(x, y) {
                    h ^= u32::from(p.r);
                    h = h.wrapping_mul(16777619);
                    h ^= u32::from(p.a);
                    h = h.wrapping_mul(16777619);
                }
            }
        }
        h
    }

    #[test]
    fn shimmer_moves_unless_reduced() {
        let a = paint(0.1, false);
        let b = paint(0.7, false);
        assert_ne!(
            fingerprint(&a),
            fingerprint(&b),
            "sweep should move with phase"
        );
        let c = paint(0.1, true);
        let d = paint(0.7, true);
        assert_eq!(
            fingerprint(&c),
            fingerprint(&d),
            "reduced-motion must not sweep"
        );
    }
}
