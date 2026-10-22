// SPDX-License-Identifier: MIT
//! Badge (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::format;

use knox_gpui::{Div, Hsla, IntoElement, Styled, div, px, text};

use crate::icon::{Icon, IconName};
use crate::size::{ControlSize, Sizable};
use crate::theme::Theme;

enum BadgeMark {
    Number,
    Dot,
    Icon(IconName),
}

/// Count / dot / icon overlay on a child (kit `Badge`).
pub struct Badge {
    theme: Theme,
    count: usize,
    max: usize,
    mark: BadgeMark,
    color: Option<Hsla>,
    size: ControlSize,
    child: Div,
}

impl Badge {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            count: 0,
            max: 99,
            mark: BadgeMark::Number,
            color: None,
            size: ControlSize::Medium,
            child: div(),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn dot(mut self) -> Self {
        self.mark = BadgeMark::Dot;
        self
    }

    pub fn count(mut self, count: usize) -> Self {
        self.count = count;
        self.mark = BadgeMark::Number;
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.mark = BadgeMark::Icon(icon);
        self
    }

    pub fn max(mut self, max: usize) -> Self {
        self.max = max;
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = self.child.child(child);
        self
    }

    pub fn is_visible(&self) -> bool {
        match self.mark {
            BadgeMark::Number => self.count > 0,
            BadgeMark::Dot | BadgeMark::Icon(_) => true,
        }
    }
}

impl Default for Badge {
    fn default() -> Self {
        Self::new()
    }
}

impl Sizable for Badge {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl IntoElement for Badge {
    type Element = Div;

    fn into_element(self) -> Div {
        let visible = self.is_visible();
        let bg = self.color.unwrap_or(self.theme.colors.destructive);
        let fg = self.theme.colors.destructive_foreground;
        let (mark_h, font) = match self.size {
            ControlSize::Large => (px(20), 12u16),
            ControlSize::Medium => (px(16), 10),
            ControlSize::Small => (px(12), 8),
        };
        let overlay = match self.mark {
            BadgeMark::Dot => div()
                .absolute()
                .top(px(0))
                .right(px(0))
                .w(px(6))
                .h(px(6))
                .rounded(self.theme.radius.full)
                .bg(bg),
            BadgeMark::Number => {
                let label = if self.count > self.max {
                    format!("{}+", self.max)
                } else {
                    format!("{}", self.count)
                };
                div()
                    .absolute()
                    .top(px(-4))
                    .right(px(-6))
                    .h(mark_h)
                    .min_w(mark_h)
                    .pad_x(px(4))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(self.theme.radius.full)
                    .bg(bg)
                    .child(text(label).color(fg).size(font))
            }
            BadgeMark::Icon(name) => div()
                .absolute()
                .right(px(0))
                .bottom(px(0))
                .w(mark_h)
                .h(mark_h)
                .flex()
                .items_center()
                .justify_center()
                .rounded(self.theme.radius.full)
                .bg(bg)
                .border_1()
                .border_color(self.theme.colors.background)
                .child(
                    Icon::new(name)
                        .themed(&self.theme)
                        .color(fg)
                        .with_size(ControlSize::Small),
                ),
        };

        let mut root = div().relative().child(self.child);
        if visible {
            root = root.child(overlay);
        }
        root
    }
}
