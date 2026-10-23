// SPDX-License-Identifier: MIT
//! Sidebar, Toolbar, StatusBar, GroupBox, DescriptionList
//! (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, FontId, Hsla, IntoElement, Pixels, Styled, div, px};

use crate::icon::{Icon, IconName};
use crate::interact::{IndexHandler, bind_activate};
use crate::label::Label;
use crate::scroll::ScrollArea;
use crate::separator::Separator;
use crate::size::Sizable;
use crate::theme::Theme;

/// Vertical nav (kit `Sidebar`).
pub struct Sidebar {
    theme: Theme,
    items: Vec<String>,
    selected: usize,
    width: Pixels,
    height: Option<Pixels>,
    header_title: String,
    header_hint: String,
    search: Option<Div>,
    scroll: i32,
    on_scroll: Option<alloc::sync::Arc<dyn Fn(i32) + Send + Sync>>,
    on_select: Option<IndexHandler>,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            items: Vec::new(),
            selected: 0,
            width: px(240),
            height: None,
            header_title: String::new(),
            header_hint: String::new(),
            search: None,
            scroll: 0,
            on_scroll: None,
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

    pub fn header(mut self, title: impl Into<String>, hint: impl Into<String>) -> Self {
        self.header_title = title.into();
        self.header_hint = hint.into();
        self
    }

    pub fn search(mut self, field: impl IntoElement) -> Self {
        self.search = Some(div().child(field));
        self
    }

    pub fn offset(mut self, offset: i32) -> Self {
        self.scroll = offset.max(0);
        self
    }

    pub fn on_offset(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_scroll = Some(alloc::sync::Arc::new(f));
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
        let c = self.theme.colors;
        let total_h = self.height.unwrap_or(px(280));
        let mut col = div()
            .flex_col()
            .w(self.width)
            .h(total_h)
            .p(self.theme.spacing.sm)
            .gap_2()
            .bg(c.surface)
            .border_1()
            .border_color(c.border);
        let mut chrome_h = self.theme.spacing.sm.0 * 2;
        if !self.header_title.is_empty() {
            chrome_h += 40;
            col = col.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(32))
                            .h(px(32))
                            .rounded(self.theme.radius.lg)
                            .bg(c.primary)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(IconName::LayoutGrid)
                                    .themed(&self.theme)
                                    .color(c.primary_foreground)
                                    .small(),
                            ),
                    )
                    .child(
                        div()
                            .flex_col()
                            .gap(px(1))
                            .child(
                                Label::new(self.header_title)
                                    .themed(&self.theme)
                                    .font(FontId::UiBold)
                                    .size(self.theme.typo.body),
                            )
                            .child(
                                Label::new(self.header_hint)
                                    .themed(&self.theme)
                                    .muted(true)
                                    .size(self.theme.typo.caption),
                            ),
                    ),
            );
        }
        if let Some(search) = self.search {
            chrome_h += 36;
            col = col.child(
                div()
                    .bg(c.accent)
                    .rounded(self.theme.radius.full)
                    .pad_x(self.theme.spacing.xs)
                    .child(search),
            );
        }
        let item_w = px((self.width.0 - self.theme.spacing.sm.0 * 2).max(80));
        let mut menu = div().flex_col().gap_1().w(item_w);
        for (i, label) in self.items.into_iter().enumerate() {
            let pick = self.on_select.clone();
            let selected = i == self.selected;
            let bg = if selected {
                c.accent
            } else {
                Hsla::TRANSPARENT
            };
            let hover = c.accent.opacity(0.55);
            let mut item = div()
                .w(item_w)
                .h(px(28))
                .pad_x(px(8))
                .flex()
                .items_center()
                .rounded(self.theme.radius.md)
                .bg(bg)
                .child(
                    Label::new(label)
                        .themed(&self.theme)
                        .size(self.theme.typo.body),
                );
            if !selected {
                item = item.hover(move |s| s.bg(hover));
            }
            if let Some(p) = pick {
                item = bind_activate(item, alloc::sync::Arc::new(move || p(i)));
            }
            menu = menu.child(item);
        }
        let menu_h = (total_h.0 - chrome_h - 8).max(80);
        let mut scroller = ScrollArea::new()
            .themed(&self.theme)
            .size(item_w, px(menu_h))
            .offset(self.scroll)
            .child(menu);
        if let Some(on) = self.on_scroll {
            scroller = scroller.on_offset(move |o| on(o));
        }
        col.child(scroller)
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
    prefix: Option<Div>,
    extra: Option<String>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            left: String::new(),
            right: String::new(),
            prefix: None,
            extra: None,
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

    pub fn prefix(mut self, child: impl IntoElement) -> Self {
        self.prefix = Some(div().child(child));
        self
    }

    pub fn extra(mut self, text: impl Into<String>) -> Self {
        self.extra = Some(text.into());
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
        let mut left = div().flex().items_center().gap_2();
        if let Some(prefix) = self.prefix {
            left = left.child(prefix);
        }
        if !self.left.is_empty() {
            left = left.child(
                Label::new(self.left)
                    .themed(&self.theme)
                    .muted(true)
                    .size(self.theme.typo.caption),
            );
        }
        if let Some(extra) = self.extra {
            left = left
                .child(
                    div()
                        .h(px(14))
                        .child(Separator::vertical().themed(&self.theme)),
                )
                .child(
                    Label::new(extra)
                        .themed(&self.theme)
                        .muted(true)
                        .size(self.theme.typo.caption),
                );
        }
        div()
            .flex()
            .justify_between()
            .items_center()
            .grow()
            .h(px(28))
            .pad_x(self.theme.spacing.sm)
            .bg(self.theme.colors.muted.opacity(0.4))
            .child(left)
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
    description: String,
    children: Vec<Div>,
    outline: bool,
    fill: bool,
    width: Option<Pixels>,
}

impl GroupBox {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            title: title.into(),
            description: String::new(),
            children: Vec::new(),
            outline: true,
            fill: false,
            width: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.description = text.into();
        self
    }

    pub fn outline(mut self) -> Self {
        self.outline = true;
        self.fill = false;
        self
    }

    pub fn fill(mut self) -> Self {
        self.fill = true;
        self.outline = false;
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(div().child(child));
        self
    }
}

impl IntoElement for GroupBox {
    type Element = Div;

    fn into_element(self) -> Div {
        let c = self.theme.colors;
        let mut title_col = div().flex_col().gap(px(2)).grow().child(
            Label::new(self.title)
                .themed(&self.theme)
                .font(FontId::UiBold)
                .size(self.theme.typo.body),
        );
        if !self.description.is_empty() {
            title_col = title_col.child(
                Label::new(self.description)
                    .themed(&self.theme)
                    .muted(true)
                    .size(self.theme.typo.caption),
            );
        }

        let mut content = div()
            .flex()
            .flex_wrap()
            .justify_center()
            .items_center()
            .gap(self.theme.spacing.lg)
            .p(self.theme.spacing.lg)
            .rounded(self.theme.radius.lg);
        if self.fill {
            content = content.bg(c.muted.opacity(0.35));
        }
        if self.outline {
            content = content.border_1().border_color(c.border);
        }
        for child in self.children {
            content = content.child(child);
        }

        let mut box_ = div()
            .flex_col()
            .gap_2()
            .mb(self.theme.spacing.lg)
            .child(title_col)
            .child(content);
        if let Some(w) = self.width {
            box_ = box_.w(w);
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
