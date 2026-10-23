// SPDX-License-Identifier: MIT
//! Separator (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. Dashed rules are Scene quads (no lyon path).

use alloc::string::String;

use knox_gpui::{
    Bounds, Div, Element, Hsla, IntoElement, LayoutHints, PaintEnv, Point, Quad, Scene, Size,
    Styled, WindowInput, div, px,
};

use crate::label::Label;
use crate::theme::Theme;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SeparatorStyle {
    #[default]
    Solid,
    Dashed,
}

/// Horizontal or vertical rule, optional label.
pub struct Separator {
    theme: Theme,
    label: Option<String>,
    vertical: bool,
    color: Option<Hsla>,
    line_style: SeparatorStyle,
}

impl Separator {
    pub fn horizontal() -> Self {
        Self {
            theme: Theme::default(),
            label: None,
            vertical: false,
            color: None,
            line_style: SeparatorStyle::Solid,
        }
    }

    pub fn vertical() -> Self {
        Self {
            theme: Theme::default(),
            label: None,
            vertical: true,
            color: None,
            line_style: SeparatorStyle::Solid,
        }
    }

    pub fn horizontal_dashed() -> Self {
        Self::horizontal().dashed()
    }

    pub fn vertical_dashed() -> Self {
        Self::vertical().dashed()
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn dashed(mut self) -> Self {
        self.line_style = SeparatorStyle::Dashed;
        self
    }
}

struct Rule {
    vertical: bool,
    dashed: bool,
    color: Hsla,
    grow: u32,
    computed: Bounds,
}

impl Element for Rule {
    fn request_layout(&mut self, origin: Point, available: Size) -> Bounds {
        let size = if self.vertical {
            Size::wh(1, available.height.0.max(8))
        } else {
            Size::wh(available.width.0.max(8), 1)
        };
        self.computed = Bounds::new(origin, size);
        self.computed
    }

    fn layout_hints(&self) -> LayoutHints {
        LayoutHints {
            grow: self.grow,
            ..LayoutHints::default()
        }
    }

    fn paint(
        &mut self,
        bounds: Bounds,
        scene: &mut Scene,
        _input: &WindowInput,
        _env: &mut PaintEnv,
    ) {
        let bounds = if bounds.is_empty() {
            self.computed
        } else {
            bounds
        };
        if self.dashed {
            const DASH: i32 = 4;
            const GAP: i32 = 2;
            let mut pos = 0;
            let span = if self.vertical {
                bounds.height()
            } else {
                bounds.width()
            };
            while pos < span {
                let len = (span - pos).min(DASH);
                let quad = if self.vertical {
                    Bounds::from_xywh(bounds.x(), bounds.y() + pos, 1, len)
                } else {
                    Bounds::from_xywh(bounds.x() + pos, bounds.y(), len, 1)
                };
                scene.push_quad(Quad::new(quad, self.color));
                pos += DASH + GAP;
            }
        } else {
            scene.push_quad(Quad::new(bounds, self.color));
        }
    }
}

impl IntoElement for Separator {
    type Element = Div;

    fn into_element(self) -> Div {
        let color = self.color.unwrap_or(self.theme.colors.border);
        let dashed = self.line_style == SeparatorStyle::Dashed;
        let rule = |grow: u32| Rule {
            vertical: self.vertical,
            dashed,
            color,
            grow,
            computed: Bounds::ZERO,
        };

        match (self.vertical, self.label) {
            (false, None) => div()
                .flex()
                .items_center()
                .h(px(1))
                .min_w(px(48))
                .child(rule(0)),
            (true, None) => div().flex_col().w(px(1)).min_h(px(24)).child(rule(0)),
            (false, Some(label)) => div()
                .flex()
                .items_center()
                .gap_1()
                .child(rule(1))
                .child(
                    Label::new(label)
                        .themed(&self.theme)
                        .muted(true)
                        .size(self.theme.typo.caption),
                )
                .child(rule(1)),
            (true, Some(label)) => div()
                .flex_col()
                .items_center()
                .gap_1()
                .w(px(16))
                .child(rule(1))
                .child(
                    Label::new(label)
                        .themed(&self.theme)
                        .muted(true)
                        .size(self.theme.typo.caption),
                )
                .child(rule(1)),
        }
    }
}
