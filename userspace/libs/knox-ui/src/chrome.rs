// SPDX-License-Identifier: MIT
//! Sidebar, Toolbar, StatusBar, GroupBox, DescriptionList
//! (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, FontId, IntoElement, Pixels, Styled, div, px};

use crate::button::{Button, ButtonVariants};
use crate::interact::IndexHandler;
use crate::label::Label;
use crate::size::Selectable;
use crate::theme::Theme;

/// Vertical nav (kit `Sidebar`).
pub struct Sidebar {
    theme: Theme,
    items: Vec<String>,
    selected: usize,
    width: Pixels,
    height: Option<Pixels>,
    on_select: Option<IndexHandler>,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            items: Vec::new(),
            selected: 0,
            width: px(140),
            height: None,
            on_select: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn items<I, S>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.items = items.into_iter().map(Into::into).collect();
        self
    }

    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn h(mut self, height: Pixels) -> Self {
        self.height = Some(height);
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Sidebar {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut col = div()
            .flex_col()
            .w(self.width)
            .p(self.theme.spacing.xs)
            .gap_1()
            .bg(self.theme.colors.muted.opacity(0.35))
            .border_1()
            .border_color(self.theme.colors.border);
        col = if let Some(h) = self.height {
            col.h(h)
        } else {
            col.h(px(200))
        };
        for (i, label) in self.items.into_iter().enumerate() {
            let pick = self.on_select.clone();
            col = col.child(
                Button::new(alloc::format!("sb-{i}"))
                    .themed(&self.theme)
                    .ghost()
                    .label(label)
                    .selected(i == self.selected)
                    .on_click(move || {
                        if let Some(p) = &pick {
                            p(i);
                        }
                    }),
            );
        }
        col
    }
}

/// Horizontal action row (kit `Toolbar`).
pub struct Toolbar {
    theme: Theme,
    child: Option<Div>,
}

impl Toolbar {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            child: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(div().child(child));
        self
    }
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Toolbar {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut bar = div()
            .flex()
            .items_center()
            .gap_2()
            .grow()
            .h(px(32))
            .p(self.theme.spacing.xs)
            .bg(self.theme.colors.surface)
            .border_1()
            .border_color(self.theme.colors.border);
        if let Some(child) = self.child {
            bar = bar.child(child);
        }
        bar
    }
}

/// Bottom status strip (kit `StatusBar`).
pub struct StatusBar {
    theme: Theme,
    left: String,
    right: String,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            left: String::new(),
            right: String::new(),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn left(mut self, text: impl Into<String>) -> Self {
        self.left = text.into();
        self
    }

    pub fn right(mut self, text: impl Into<String>) -> Self {
        self.right = text.into();
        self
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for StatusBar {
    type Element = Div;

    fn into_element(self) -> Div {
        div()
            .flex()
            .justify_between()
            .items_center()
            .grow()
            .h(px(28))
            .p(self.theme.spacing.xs)
            .bg(self.theme.colors.muted.opacity(0.4))
            .child(
                Label::new(self.left)
                    .themed(&self.theme)
                    .muted(true)
                    .size(self.theme.typo.caption),
            )
            .child(
                Label::new(self.right)
                    .themed(&self.theme)
                    .muted(true)
                    .size(self.theme.typo.caption),
            )
    }
}

/// Labeled group (kit `GroupBox`).
pub struct GroupBox {
    theme: Theme,
    title: String,
    child: Option<Div>,
}

impl GroupBox {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            title: title.into(),
            child: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(div().child(child));
        self
    }
}

impl IntoElement for GroupBox {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut box_ = div()
            .flex_col()
            .gap_2()
            .w(px(280))
            .p(self.theme.spacing.sm)
            .border_1()
            .border_color(self.theme.colors.border)
            .rounded(self.theme.radius.md)
            .child(
                Label::new(self.title)
                    .themed(&self.theme)
                    .muted(true)
                    .size(self.theme.typo.caption),
            );
        if let Some(child) = self.child {
            box_ = box_.child(child);
        }
        box_
    }
}

/// Key / value rows (kit `DescriptionList`).
pub struct DescriptionList {
    theme: Theme,
    rows: Vec<(String, String)>,
}

impl DescriptionList {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            rows: Vec::new(),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn row(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.rows.push((key.into(), value.into()));
        self
    }
}

impl Default for DescriptionList {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for DescriptionList {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut col = div().flex_col().gap_1().w(px(280));
        for (k, v) in self.rows {
            col = col.child(
                div()
                    .flex()
                    .justify_between()
                    .gap_2()
                    .child(
                        Label::new(k)
                            .themed(&self.theme)
                            .muted(true)
                            .size(self.theme.typo.caption),
                    )
                    .child(
                        Label::new(v)
                            .themed(&self.theme)
                            .font(FontId::UiBold)
                            .size(self.theme.typo.caption),
                    ),
            );
        }
        col
    }
}

#[cfg(test)]
mod tests {
    use super::{DescriptionList, GroupBox, Sidebar, StatusBar, Toolbar};
    use crate::button::Button;
    use crate::theme::Theme;
    use knox_gpui::IntoElement;

    #[test]
    fn constructs_frame_catalog() {
        let theme = Theme::aurora_dark();
        let _ = Sidebar::new()
            .themed(&theme)
            .items(["Home", "Search"])
            .into_element();
        let _ = Toolbar::new()
            .themed(&theme)
            .child(Button::new("tb").themed(&theme).label("New"))
            .into_element();
        let _ = StatusBar::new()
            .themed(&theme)
            .left("Ready")
            .right("Ln 1")
            .into_element();
        let _ = GroupBox::new("General").themed(&theme).into_element();
        let _ = DescriptionList::new()
            .themed(&theme)
            .row("Name", "Knox")
            .into_element();
    }
}
