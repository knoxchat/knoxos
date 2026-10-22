// SPDX-License-Identifier: MIT
//! Carousel (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::interact::IndexHandler;
use crate::size::Selectable;
use crate::theme::Theme;

/// Slide carousel with prev/next + dots (kit `Carousel`).
pub struct Carousel {
    theme: Theme,
    slides: Vec<String>,
    index: usize,
    on_index: Option<IndexHandler>,
}

impl Carousel {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            slides: Vec::new(),
            index: 0,
            on_index: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn slides<I, S>(mut self, slides: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.slides = slides.into_iter().map(Into::into).collect();
        self
    }

    pub fn index(mut self, index: usize) -> Self {
        self.index = index;
        self
    }

    pub fn on_index(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_index = Some(Arc::new(f));
        self
    }
}

impl Default for Carousel {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Carousel {
    type Element = Div;

    fn into_element(self) -> Div {
        let n = self.slides.len().max(1);
        let index = self.index % n;
        let label = self
            .slides
            .get(index)
            .cloned()
            .unwrap_or_else(|| format!("Slide {index}"));
        let fire = self.on_index.clone();
        let prev = (index + n - 1) % n;
        let next = (index + 1) % n;
        let mut dots = div().flex().gap_1().justify_center();
        for i in 0..n {
            let f = fire.clone();
            dots = dots.child(
                Button::new(format!("dot-{i}"))
                    .themed(&self.theme)
                    .ghost()
                    .label(if i == index { "o" } else { "." })
                    .selected(i == index)
                    .on_click(move || {
                        if let Some(cb) = &f {
                            cb(i);
                        }
                    }),
            );
        }
        div()
            .flex_col()
            .gap_2()
            .w(px(240))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child({
                        let f = fire.clone();
                        Button::new("car-prev")
                            .themed(&self.theme)
                            .ghost()
                            .label("<")
                            .on_click(move || {
                                if let Some(cb) = &f {
                                    cb(prev);
                                }
                            })
                    })
                    .child(
                        div()
                            .w(px(160))
                            .h(px(64))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(self.theme.colors.muted)
                            .rounded(self.theme.radius.md)
                            .child(text(label).color(self.theme.colors.foreground).size(13)),
                    )
                    .child({
                        let f = fire;
                        Button::new("car-next")
                            .themed(&self.theme)
                            .ghost()
                            .label(">")
                            .on_click(move || {
                                if let Some(cb) = &f {
                                    cb(next);
                                }
                            })
                    }),
            )
            .child(dots)
    }
}

#[cfg(test)]
mod tests {
    use super::Carousel;
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element,
    };

    #[test]
    fn prev_next_and_dots() {
        let hit = Arc::new(AtomicUsize::new(0));
        let flag = hit.clone();
        let mut el = Carousel::new()
            .themed(&Theme::aurora_dark())
            .slides(["A", "B", "C"])
            .index(0)
            .on_index(move |i| flag.store(i, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(240, 100), &mut input);
        // Next button is on the right of the 240px row.
        dispatch_mouse_down(&mut el, &mut input, Point::xy(220, 20));
        dispatch_mouse_up(&mut el, &mut input, Point::xy(220, 20));
        assert_eq!(hit.load(Ordering::SeqCst), 1);
    }
}
