// SPDX-License-Identifier: MIT
//! Alert (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;

use knox_gpui::{Div, FontId, Hsla, IntoElement, Styled, div, px, text};

use crate::icon::{Icon, IconName};
use crate::interact::{Handler, bind_activate};
use crate::size::{ControlSize, Sizable};
use crate::theme::Theme;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertVariant {
    #[default]
    Default,
    Info,
    Success,
    Warning,
    Error,
}

impl AlertVariant {
    fn face(self, theme: &Theme) -> (Hsla, Hsla, IconName) {
        let c = theme.colors;
        let success = Hsla::from_rgb(34, 160, 110);
        let warning = Hsla::from_rgb(210, 150, 50);
        let info = Hsla::from_rgb(70, 140, 210);
        match self {
            Self::Default => (c.surface, c.border, IconName::Info),
            Self::Info => (info.opacity(0.16), info, IconName::Info),
            Self::Success => (success.opacity(0.16), success, IconName::Check),
            Self::Warning => (warning.opacity(0.16), warning, IconName::Alert),
            Self::Error => (c.destructive.opacity(0.16), c.destructive, IconName::Close),
        }
    }
}

/// Status banner with icon, title, message, optional close.
pub struct Alert {
    theme: Theme,
    variant: AlertVariant,
    title: Option<String>,
    message: String,
    visible: bool,
    on_close: Option<Handler>,
}

impl Alert {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            variant: AlertVariant::Default,
            title: None,
            message: message.into(),
            visible: true,
            on_close: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message).with_variant(AlertVariant::Info)
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self::new(message).with_variant(AlertVariant::Success)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message).with_variant(AlertVariant::Warning)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(message).with_variant(AlertVariant::Error)
    }

    pub fn with_variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn on_close(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_close = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

impl IntoElement for Alert {
    type Element = Div;

    fn into_element(self) -> Div {
        if !self.visible {
            return div();
        }
        let (bg, accent, icon) = self.variant.face(&self.theme);
        let fg = self.theme.colors.foreground;
        let mut body = div().flex_col().gap_1().grow();
        if let Some(title) = self.title {
            body = body.child(
                text(title)
                    .color(fg)
                    .font(FontId::UiBold)
                    .size(self.theme.typo.ui),
            );
        }
        body = body.child(text(self.message).color(fg).size(self.theme.typo.body));

        let mut row = div()
            .flex()
            .items_start()
            .gap_2()
            .p(self.theme.spacing.sm)
            .rounded(self.theme.radius.md)
            .bg(bg)
            .border_1()
            .border_color(accent)
            .child(
                Icon::new(icon)
                    .themed(&self.theme)
                    .color(accent)
                    .with_size(ControlSize::Medium),
            )
            .child(body);

        if let Some(close) = self.on_close {
            let x = bind_activate(
                div()
                    .w(px(18))
                    .h(px(18))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(self.theme.radius.sm)
                    .hover(move |s| s.bg(accent.opacity(0.2)))
                    .child(
                        Icon::new(IconName::Close)
                            .themed(&self.theme)
                            .color(accent)
                            .with_size(ControlSize::Small),
                    ),
                close,
            );
            row = row.child(x);
        }
        row
    }
}
