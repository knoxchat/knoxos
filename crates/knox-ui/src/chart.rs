// SPDX-License-Identifier: MIT
//! Charts + plot primitives: Line, Bar, Area, Pie, Candlestick, Radar, Sankey.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code: Scene quads only, no extra crates.

use alloc::vec::Vec;

use knox_gpui::{
    Bounds, Element, Hsla, LayoutHints, PaintEnv, Pixels, Point, Quad, Scene, Size, WindowInput, px,
};

use crate::theme::Theme;

/// Chart kind painted from a small data vec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartKind {
    Line,
    Bar,
    Area,
    Pie,
    Candlestick,
    Radar,
    Sankey,
}

/// One OHLC candle / radar / sankey sample. Unused slots stay 0.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
}

impl Sample {
    pub fn v(v: f32) -> Self {
        Self {
            a: v,
            ..Self::default()
        }
    }

    pub fn ohlc(open: f32, high: f32, low: f32, close: f32) -> Self {
        Self {
            a: open,
            b: high,
            c: low,
            d: close,
        }
    }
}

/// Chart widget (kit plot primitives).
pub struct Chart {
    theme: Theme,
    kind: ChartKind,
    data: Vec<Sample>,
    width: Pixels,
    height: Pixels,
    computed: Bounds,
}

impl Chart {
    pub fn new(kind: ChartKind) -> Self {
        Self {
            theme: Theme::default(),
            kind,
            data: Vec::new(),
            width: px(160),
            height: px(80),
            computed: Bounds::ZERO,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn data(mut self, data: impl IntoIterator<Item = Sample>) -> Self {
        self.data = data.into_iter().collect();
        self
    }

    pub fn values(mut self, values: impl IntoIterator<Item = f32>) -> Self {
        self.data = values.into_iter().map(Sample::v).collect();
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }
}

fn max_a(data: &[Sample]) -> f32 {
    data.iter()
        .map(|s| s.a.max(s.b).max(s.c).max(s.d))
        .fold(1.0, f32::max)
}

fn push_rect(scene: &mut Scene, x: i32, y: i32, w: i32, h: i32, color: Hsla) {
    if w <= 0 || h <= 0 {
        return;
    }
    scene.push_quad(Quad::new(Bounds::from_xywh(x, y, w, h), color));
}

fn paint_line(scene: &mut Scene, bounds: Bounds, data: &[Sample], color: Hsla, fill: bool) {
    if data.is_empty() {
        return;
    }
    let max = max_a(data);
    let n = data.len() as i32;
    let w = bounds.width().max(1);
    let h = bounds.height().max(1);
    let mut prev: Option<(i32, i32)> = None;
    for (i, s) in data.iter().enumerate() {
        let x = bounds.x() + (i as i32 * w) / n.max(1);
        let y = bounds.y() + h - ((s.a / max) * (h as f32)) as i32;
        if fill {
            push_rect(
                scene,
                x,
                y,
                (w / n.max(1)).max(1),
                bounds.y() + h - y,
                color.opacity(0.35),
            );
        }
        if let Some((px, py)) = prev {
            let dx = x - px;
            let dy = y - py;
            let steps = dx.abs().max(dy.abs()).max(1);
            for t in 0..=steps {
                let ix = px + (dx * t) / steps;
                let iy = py + (dy * t) / steps;
                push_rect(scene, ix, iy, 2, 2, color);
            }
        }
        prev = Some((x, y));
    }
}

fn paint_bar(scene: &mut Scene, bounds: Bounds, data: &[Sample], color: Hsla) {
    let max = max_a(data);
    let n = data.len().max(1) as i32;
    let slot = bounds.width() / n;
    let gap = 2;
    for (i, s) in data.iter().enumerate() {
        let bh = ((s.a / max) * (bounds.height() as f32)) as i32;
        let x = bounds.x() + i as i32 * slot + gap;
        let y = bounds.y() + bounds.height() - bh;
        push_rect(scene, x, y, (slot - gap * 2).max(1), bh.max(1), color);
    }
}

fn paint_pie(scene: &mut Scene, bounds: Bounds, data: &[Sample], colors: &[Hsla]) {
    let total: f32 = data.iter().map(|s| s.a.max(0.0)).sum();
    if total <= 0.0 {
        return;
    }
    let cx = bounds.x() + bounds.width() / 2;
    let cy = bounds.y() + bounds.height() / 2;
    let r = (bounds.width().min(bounds.height()) / 2).max(4);
    let mut acc = 0.0f32;
    for (i, s) in data.iter().enumerate() {
        let share = s.a.max(0.0) / total;
        let start = acc * core::f32::consts::TAU;
        acc += share;
        let end = acc * core::f32::consts::TAU;
        let color = colors[i % colors.len()];
        let steps = ((share * 48.0) as i32).max(3);
        for t in 0..steps {
            let a0 = start + (end - start) * (t as f32) / (steps as f32);
            let a1 = start + (end - start) * ((t + 1) as f32) / (steps as f32);
            let mid = (a0 + a1) * 0.5;
            let rr = (r as f32) * 0.7;
            let x = cx + (libm::cosf(mid) * rr) as i32;
            let y = cy + (libm::sinf(mid) * rr) as i32;
            push_rect(scene, x, y, 3, 3, color);
        }
    }
}

fn paint_candle(scene: &mut Scene, bounds: Bounds, data: &[Sample], up: Hsla, down: Hsla) {
    let max = max_a(data);
    let n = data.len().max(1) as i32;
    let slot = bounds.width() / n;
    let h = bounds.height() as f32;
    for (i, s) in data.iter().enumerate() {
        let x = bounds.x() + i as i32 * slot + slot / 2;
        let y = |v: f32| bounds.y() + bounds.height() - ((v / max) * h) as i32;
        let high = y(s.b);
        let low = y(s.c);
        let open = y(s.a);
        let close = y(s.d);
        let color = if s.d >= s.a { up } else { down };
        push_rect(scene, x, high.min(low), 1, (low - high).abs().max(1), color);
        let top = open.min(close);
        let body = (open - close).abs().max(2);
        push_rect(scene, x - 3, top, 6, body, color);
    }
}

fn paint_radar(scene: &mut Scene, bounds: Bounds, data: &[Sample], color: Hsla) {
    let cx = bounds.x() + bounds.width() / 2;
    let cy = bounds.y() + bounds.height() / 2;
    let r = (bounds.width().min(bounds.height()) / 2).max(4) as f32;
    let n = data.len().max(1);
    let max = max_a(data);
    let mut prev = None;
    for (i, s) in data.iter().enumerate() {
        let ang = (i as f32) * core::f32::consts::TAU / (n as f32) - core::f32::consts::FRAC_PI_2;
        let rr = r * (s.a / max);
        let x = cx + (libm::cosf(ang) * rr) as i32;
        let y = cy + (libm::sinf(ang) * rr) as i32;
        let ax = cx + (libm::cosf(ang) * r) as i32;
        let ay = cy + (libm::sinf(ang) * r) as i32;
        push_rect(scene, ax, ay, 2, 2, color.opacity(0.4));
        if let Some((px, py)) = prev {
            push_rect(scene, (px + x) / 2, (py + y) / 2, 2, 2, color);
        }
        prev = Some((x, y));
    }
}

fn paint_sankey(scene: &mut Scene, bounds: Bounds, data: &[Sample], colors: &[Hsla]) {
    let n = data.len().max(1) as i32;
    let slot = bounds.height() / n;
    for (i, s) in data.iter().enumerate() {
        let color = colors[i % colors.len()];
        let y = bounds.y() + i as i32 * slot + 2;
        let thick = ((s.a.max(1.0) / max_a(data)) * 10.0) as i32 + 2;
        push_rect(scene, bounds.x() + 4, y, 16, thick, color);
        push_rect(
            scene,
            bounds.x() + 20,
            y + (i as i32 % 3),
            bounds.width() - 40,
            thick.max(2),
            color.opacity(0.5),
        );
        push_rect(
            scene,
            bounds.x() + bounds.width() - 20,
            y + ((s.b as i32) % slot.max(1)),
            16,
            thick,
            color,
        );
    }
}

impl Element for Chart {
    fn request_layout(&mut self, origin: Point, _available: Size) -> Bounds {
        self.computed = Bounds::new(origin, Size::wh(self.width.0, self.height.0));
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
        _env: &mut PaintEnv,
    ) {
        scene.push_quad(Quad::new(bounds, self.theme.colors.surface));
        let palette = [
            self.theme.colors.primary,
            self.theme.colors.accent,
            Hsla::from_rgb(80, 160, 200),
            Hsla::from_rgb(200, 160, 80),
            Hsla::from_rgb(160, 80, 180),
        ];
        match self.kind {
            ChartKind::Line => paint_line(scene, bounds, &self.data, palette[0], false),
            ChartKind::Area => paint_line(scene, bounds, &self.data, palette[0], true),
            ChartKind::Bar => paint_bar(scene, bounds, &self.data, palette[0]),
            ChartKind::Pie => paint_pie(scene, bounds, &self.data, &palette),
            ChartKind::Candlestick => paint_candle(
                scene,
                bounds,
                &self.data,
                palette[0],
                self.theme.colors.destructive,
            ),
            ChartKind::Radar => paint_radar(scene, bounds, &self.data, palette[1]),
            ChartKind::Sankey => paint_sankey(scene, bounds, &self.data, &palette),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Chart, ChartKind, Sample};
    use crate::theme::Theme;
    use knox_gpui::{Point, Scene, Size, paint_element, px};

    #[test]
    fn each_kind_paints_from_small_vec() {
        let theme = Theme::aurora_dark();
        let values = [
            Sample::v(1.0),
            Sample::v(3.0),
            Sample::v(2.0),
            Sample::v(4.0),
        ];
        let candles = [
            Sample::ohlc(2.0, 4.0, 1.0, 3.0),
            Sample::ohlc(3.0, 3.5, 2.0, 2.2),
        ];
        for kind in [
            ChartKind::Line,
            ChartKind::Bar,
            ChartKind::Area,
            ChartKind::Pie,
            ChartKind::Candlestick,
            ChartKind::Radar,
            ChartKind::Sankey,
        ] {
            let data = if kind == ChartKind::Candlestick {
                candles.to_vec()
            } else {
                values.to_vec()
            };
            let mut el = Chart::new(kind)
                .themed(&theme)
                .data(data)
                .size(px(80), px(40));
            let mut scene = Scene::new();
            paint_element(&mut el, Point::ZERO, Size::wh(80, 40), &mut scene);
            assert!(scene.len() > 1, "{kind:?} painted nothing");
        }
    }
}
