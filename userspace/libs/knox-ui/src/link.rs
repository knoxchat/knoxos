// SPDX-License-Identifier: MIT
//! Link (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::sync::Arc;

use knox_gpui::{CursorStyle, Div, IntoElement, Styled, div, px, text};

use crate::interact::{Handler, bind_activate};
use crate::size::Disableable;
use crate::theme::Theme;

/// Text hyperlink (kit `Link`). `href` is recorded; the host decides whether
/// to open it. Clicks always fire `on_click`.
pub struct Link {
    id: String,
    theme: Theme,
    label: String,
    href: Option<String>,
    disabled: bool,
    on_click: Option<Handler>,
}

impl Link {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            label: String::new(),
            href: None,
            disabled: false,
            on_click: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn href(mut self, href: impl Into<String>) -> Self {
        self.href = Some(href.into());
        self
    }

    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn href_ref(&self) -> Option<&str> {
        self.href.as_deref()
    }
}

impl Disableable for Link {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for Link {
    type Element = Div;

    fn into_element(self) -> Div {
        let c = self.theme.colors;
        let (fg, hover, active) = if self.disabled {
            (c.muted_foreground, c.muted_foreground, c.muted_foreground)
        } else {
            (c.primary, c.primary.opacity(0.80), c.primary.opacity(0.60))
        };
        let mut el = div()
            .flex_col()
            .child(text(self.label).color(fg).size(self.theme.typo.body))
            .child(
                div()
                    .h(px(1))
                    .bg(fg.opacity(if self.disabled { 0.3 } else { 0.55 })),
            );
        if self.disabled {
            return el;
        }
        el = el
            .cursor(CursorStyle::PointingHand)
            .hover(move |s| s.bg(hover.opacity(0.12)))
            .active(move |s| s.bg(active.opacity(0.18)))
            .focus(move |s| s.border_color(c.ring));
        if let Some(handler) = self.on_click {
            el = bind_activate(el, handler);
        } else {
            el = el.focusable();
        }
        el
    }
}
