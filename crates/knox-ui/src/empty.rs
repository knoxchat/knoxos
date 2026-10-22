// SPDX-License-Identifier: MIT
//! Empty (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;

use knox_gpui::{Div, FontId, IntoElement, Styled, div, px, text};

use crate::icon::{Icon, IconName};
use crate::size::{ControlSize, Sizable};
use crate::theme::Theme;

/// Placeholder for an empty collection or failed load.
pub struct Empty {
    theme: Theme,
    icon: IconName,
    title: String,
    description: String,
}

impl Empty {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            icon: IconName::Inbox,
            title: String::new(),
            description: String::new(),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = icon;
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}

impl Default for Empty {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Empty {
    type Element = Div;

    fn into_element(self) -> Div {
        let c = self.theme.colors;
        let mut col = div()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_1()
            .p(self.theme.spacing.md)
            .min_w(px(160))
            .rounded(self.theme.radius.lg)
            .border_1()
            .border_color(c.border)
            .child(
                Icon::new(self.icon)
                    .themed(&self.theme)
                    .color(c.muted_foreground)
                    .with_size(ControlSize::Large),
            );
        if !self.title.is_empty() {
            col = col.child(
                text(self.title)
                    .color(c.foreground)
                    .font(FontId::UiBold)
                    .size(13),
            );
        }
        if !self.description.is_empty() {
            col = col.child(text(self.description).color(c.muted_foreground).size(12));
        }
        col
    }
}
