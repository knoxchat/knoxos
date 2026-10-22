// SPDX-License-Identifier: MIT
//! ColorPicker (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::sync::Arc;

use knox_gpui::{Div, Hsla, IntoElement, Styled, div, hsla, px};

use crate::slider::Slider;
use crate::theme::Theme;

type ColorHandler = Arc<dyn Fn(Hsla) + Send + Sync>;

/// HSLA sliders plus a live swatch.
pub struct ColorPicker {
    theme: Theme,
    color: Hsla,
    on_change: Option<ColorHandler>,
}

impl ColorPicker {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            color: Hsla::from_rgb(232, 121, 100),
            on_change: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = color;
        self
    }

    pub fn on_change(mut self, f: impl Fn(Hsla) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    pub fn color_of(&self) -> Hsla {
        self.color
    }
}

impl Default for ColorPicker {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for ColorPicker {
    type Element = Div;

    fn into_element(self) -> Div {
        let color = self.color;
        let on = self.on_change;
        let hue = {
            let on = on.clone();
            Slider::new()
                .themed(&self.theme)
                .min(0.0)
                .max(100.0)
                .step(1.0)
                .value(color.h * 100.0)
                .w(px(140))
                .on_change(move |v| {
                    if let Some(handler) = &on {
                        handler(hsla(v / 100.0, color.s, color.l, color.a));
                    }
                })
        };
        let sat = {
            let on = on.clone();
            Slider::new()
                .themed(&self.theme)
                .min(0.0)
                .max(100.0)
                .step(1.0)
                .value(color.s * 100.0)
                .w(px(140))
                .on_change(move |v| {
                    if let Some(handler) = &on {
                        handler(hsla(color.h, v / 100.0, color.l, color.a));
                    }
                })
        };
        let light = {
            let on = on.clone();
            Slider::new()
                .themed(&self.theme)
                .min(0.0)
                .max(100.0)
                .step(1.0)
                .value(color.l * 100.0)
                .w(px(140))
                .on_change(move |v| {
                    if let Some(handler) = &on {
                        handler(hsla(color.h, color.s, v / 100.0, color.a));
                    }
                })
        };

        div()
            .flex()
            .gap_2()
            .items_center()
            .child(
                div()
                    .w(px(28))
                    .h(px(28))
                    .rounded(self.theme.radius.md)
                    .bg(color),
            )
            .child(div().flex_col().gap_1().child(hue).child(sat).child(light))
    }
}

#[cfg(test)]
mod tests {
    use super::ColorPicker;
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};
    use knox_gpui::{
        Hsla, IntoElement, Point, Size, WindowInput, dispatch_mouse_down, prepare_element,
    };

    #[test]
    fn slider_commits_color() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let mut el = ColorPicker::new()
            .themed(&Theme::aurora_dark())
            .color(Hsla::RED)
            .on_change(move |_| flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(200, 80), &mut input);
        let thumb = input.hitboxes()[0];
        dispatch_mouse_down(
            &mut el,
            &mut input,
            Point::xy(thumb.bounds.x() + 40, thumb.bounds.y() + 8),
        );
        assert!(hit.load(Ordering::SeqCst));
    }
}
