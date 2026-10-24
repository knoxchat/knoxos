// SPDX-License-Identifier: MIT
//! TitleBar + WindowBorder client-side decoration (GPUI kit catalog).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. Drag / min / max / close are callbacks into the WM.

use alloc::string::String;
use alloc::sync::Arc;

use knox_gpui::{Div, FontId, IntoElement, Pixels, Point, Styled, div, px};

use crate::button::{Button, ButtonVariants};
use crate::icon::{Icon, IconName};
use crate::interact::Handler;
use crate::label::Label;
use crate::size::Sizable;
use crate::theme::Theme;

type DragHandler = Arc<dyn Fn(Point) + Send + Sync>;

/// CSD caption (kit `TitleBar`).
pub struct TitleBar {
    theme: Theme,
    title: String,
    icon: IconName,
    maximized: bool,
    on_drag: Option<DragHandler>,
    on_min: Option<Handler>,
    on_max: Option<Handler>,
    on_close: Option<Handler>,
}

impl TitleBar {
    pub fn new(title: impl Into<String>) -> Self {
        let title = title.into();
        Self {
            theme: Theme::default(),
            icon: IconName::for_label(&title),
            title,
            maximized: false,
            on_drag: None,
            on_min: None,
            on_max: None,
            on_close: None,
        }
    }

    pub fn icon(mut self, name: IconName) -> Self {
        self.icon = name;
        self
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }

    pub fn on_drag(mut self, f: impl Fn(Point) + Send + Sync + 'static) -> Self {
        self.on_drag = Some(Arc::new(f));
        self
    }

    pub fn on_min(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_min = Some(Arc::new(f));
        self
    }

    pub fn on_max(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_max = Some(Arc::new(f));
        self
    }

    pub fn on_close(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_close = Some(Arc::new(f));
        self
    }
}

impl IntoElement for TitleBar {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut caption = div()
            .flex()
            .items_center()
            .gap_2()
            .grow()
            .h(px(36))
            .child(
                Icon::new(self.icon)
                    .themed(&self.theme)
                    .color(self.theme.colors.foreground),
            )
            .child(
                Label::new(self.title)
                    .themed(&self.theme)
                    .font(FontId::UiBold)
                    .size(self.theme.typo.title),
            );
        if let Some(drag) = self.on_drag {
            caption = caption.on_mouse_down(move |e| drag(e.position));
        }
        let mut buttons = div().flex().gap_1();
        if let Some(min) = self.on_min {
            buttons = buttons.child(
                Button::new("csd-min")
                    .themed(&self.theme)
                    .ghost()
                    .small()
                    .compact()
                    .icon(IconName::Minus)
                    .on_click(move || min()),
            );
        }
        if let Some(max) = self.on_max {
            let glyph = if self.maximized {
                IconName::Minimize2
            } else {
                IconName::Maximize2
            };
            buttons = buttons.child(
                Button::new("csd-max")
                    .themed(&self.theme)
                    .ghost()
                    .small()
                    .compact()
                    .icon(glyph)
                    .on_click(move || max()),
            );
        }
        if let Some(close) = self.on_close {
            buttons = buttons.child(
                Button::new("csd-x")
                    .themed(&self.theme)
                    .danger()
                    .small()
                    .compact()
                    .icon(IconName::Close)
                    .on_click(move || close()),
            );
        }
        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(36))
            .pad_x(self.theme.spacing.sm)
            .pad_y(self.theme.spacing.xs)
            .bg(self.theme.colors.surface)
            .child(caption)
            .child(buttons)
    }
}

/// Rounded window chrome wrapping a titlebar + body (kit `WindowBorder`).
pub struct WindowBorder {
    theme: Theme,
    titlebar: TitleBar,
    body: Option<Div>,
    width: Pixels,
    height: Pixels,
}

impl WindowBorder {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            titlebar: TitleBar::new(title),
            body: None,
            width: px(400),
            height: px(240),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self.titlebar = self.titlebar.themed(theme);
        self
    }

    pub fn titlebar(mut self, titlebar: TitleBar) -> Self {
        self.titlebar = titlebar;
        self
    }

    pub fn body(mut self, child: impl IntoElement) -> Self {
        self.body = Some(div().child(child));
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }
}

impl IntoElement for WindowBorder {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut col = div()
            .flex_col()
            .w(self.width)
            .h(self.height)
            .bg(self.theme.colors.background)
            .border_1()
            .border_color(self.theme.colors.border)
            .rounded(self.theme.radius.lg)
            .child(self.titlebar);
        if let Some(body) = self.body {
            col = col.child(body.grow());
        }
        col
    }
}

#[cfg(test)]
mod tests {
    use super::{TitleBar, WindowBorder};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, prepare_element, text,
    };

    #[test]
    fn titlebar_drag_reports_position() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let mut el = TitleBar::new("Kit Gallery")
            .themed(&Theme::aurora_dark())
            .on_drag(move |_| flag.store(true, Ordering::SeqCst))
            .on_min(|| {})
            .on_max(|| {})
            .on_close(|| {})
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(400, 36), &mut input);
        dispatch_mouse_down(&mut el, &mut input, Point::xy(20, 8));
        assert!(hit.load(Ordering::SeqCst));
        let _ = WindowBorder::new("W")
            .themed(&Theme::aurora_dark())
            .body(text("body"))
            .into_element();
    }
}
