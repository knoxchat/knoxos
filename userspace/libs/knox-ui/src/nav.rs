// SPDX-License-Identifier: MIT
//! Tabs, TabBar, Accordion, Collapsible, Breadcrumb, Pagination, Stepper,
//! NavStack (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, FontId, IntoElement, Key, Styled, div};

use crate::button::{Button, ButtonVariants};
use crate::icon::{Icon, IconName};
use crate::interact::{Handler, IndexHandler};
use crate::label::Label;
use crate::size::{Selectable, Sizable};
use crate::theme::Theme;

/// Tab strip (kit `TabBar`).
pub struct TabBar {
    id: String,
    theme: Theme,
    labels: Vec<String>,
    selected: usize,
    on_select: Option<IndexHandler>,
}

impl TabBar {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            labels: Vec::new(),
            selected: 0,
            on_select: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn labels<I, S>(mut self, labels: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.labels = labels.into_iter().map(Into::into).collect();
        self
    }

    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }
}

impl IntoElement for TabBar {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut bar = div().flex().gap_1().items_center();
        let n = self.labels.len();
        let selected = self.selected.min(n.saturating_sub(1));
        let pick = self.on_select.clone();
        for (i, label) in self.labels.into_iter().enumerate() {
            let fire = pick.clone();
            let arrows = pick.clone();
            bar = bar.child(
                Button::new(format!("{}-{i}", self.id))
                    .themed(&self.theme)
                    .ghost()
                    .compact()
                    .label(label)
                    .selected(i == selected)
                    .on_press(move || {
                        if let Some(p) = &fire {
                            p(i);
                        }
                    })
                    .into_element()
                    .on_key_down(move |e| {
                        let next = match e.key {
                            Key::Left | Key::Up => selected.saturating_sub(1),
                            Key::Right | Key::Down => (selected + 1).min(n.saturating_sub(1)),
                            _ => return false,
                        };
                        if let Some(p) = &arrows {
                            p(next);
                        }
                        true
                    }),
            );
        }
        bar.on_key_down(move |e| {
            let next = match e.key {
                Key::Left | Key::Up => selected.saturating_sub(1),
                Key::Right | Key::Down => (selected + 1).min(n.saturating_sub(1)),
                _ => return false,
            };
            if let Some(p) = &pick {
                p(next);
            }
            true
        })
        .focusable()
    }
}

/// Tabs = bar + selected page (kit `Tabs`).
pub struct Tabs {
    bar: TabBar,
    pages: Vec<Div>,
    selected: usize,
}

impl Tabs {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            bar: TabBar::new(id),
            pages: Vec::new(),
            selected: 0,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.bar = self.bar.themed(theme);
        self
    }

    pub fn labels<I, S>(mut self, labels: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.bar = self.bar.labels(labels);
        self
    }

    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self.bar = self.bar.selected(selected);
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.bar = self.bar.on_select(f);
        self
    }

    pub fn page(mut self, child: impl IntoElement) -> Self {
        self.pages.push(div().child(child));
        self
    }
}

impl IntoElement for Tabs {
    type Element = Div;

    fn into_element(self) -> Div {
        let page = self
            .pages
            .into_iter()
            .nth(self.selected)
            .unwrap_or_else(div);
        div().flex_col().gap_2().child(self.bar).child(page)
    }
}

/// Expandable section (kit `Collapsible`).
pub struct Collapsible {
    theme: Theme,
    title: String,
    open: bool,
    child: Option<Div>,
    on_toggle: Option<Handler>,
}

impl Collapsible {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            title: title.into(),
            open: false,
            child: None,
            on_toggle: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(div().child(child));
        self
    }

    pub fn on_toggle(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_toggle = Some(Arc::new(f));
        self
    }
}

impl IntoElement for Collapsible {
    type Element = Div;

    fn into_element(self) -> Div {
        let caret = if self.open {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        };
        let mut head = Button::new("coll-h")
            .themed(&self.theme)
            .ghost()
            .icon(caret)
            .label(self.title);
        if let Some(t) = self.on_toggle {
            head = head.on_press(move || t());
        }
        let mut root = div().flex_col().child(head);
        if self.open {
            if let Some(child) = self.child {
                root = root.child(child);
            }
        }
        root
    }
}

/// Stack of collapsibles (kit `Accordion`). One open index.
pub struct Accordion {
    theme: Theme,
    titles: Vec<String>,
    bodies: Vec<Div>,
    open: Option<usize>,
    on_open: Option<IndexHandler>,
}

impl Accordion {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            titles: Vec::new(),
            bodies: Vec::new(),
            open: None,
            on_open: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn item(mut self, title: impl Into<String>, body: impl IntoElement) -> Self {
        self.titles.push(title.into());
        self.bodies.push(div().child(body));
        self
    }

    pub fn open(mut self, open: Option<usize>) -> Self {
        self.open = open;
        self
    }

    pub fn on_open(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_open = Some(Arc::new(f));
        self
    }
}

impl Default for Accordion {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Accordion {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut col = div().flex_col().gap_1();
        let mut bodies = self.bodies.into_iter();
        for (i, title) in self.titles.into_iter().enumerate() {
            let body = bodies.next().unwrap_or_else(div);
            let fire = self.on_open.clone();
            col = col.child(
                Collapsible::new(title)
                    .themed(&self.theme)
                    .open(self.open == Some(i))
                    .child(body)
                    .on_toggle(move || {
                        if let Some(p) = &fire {
                            p(i);
                        }
                    }),
            );
        }
        col
    }
}

/// Path crumbs (kit `Breadcrumb`).
pub struct Breadcrumb {
    theme: Theme,
    parts: Vec<String>,
    on_select: Option<IndexHandler>,
}

impl Breadcrumb {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            parts: Vec::new(),
            on_select: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn parts<I, S>(mut self, parts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.parts = parts.into_iter().map(Into::into).collect();
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }
}

impl Default for Breadcrumb {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Breadcrumb {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut row = div().flex().items_center().gap_1();
        let last = self.parts.len().saturating_sub(1);
        for (i, part) in self.parts.into_iter().enumerate() {
            if i > 0 {
                row = row.child(
                    Icon::new(IconName::ChevronRight)
                        .themed(&self.theme)
                        .color(self.theme.colors.muted_foreground)
                        .small(),
                );
            }
            let pick = self.on_select.clone();
            row = row.child(
                Button::new(format!("crumb-{i}"))
                    .themed(&self.theme)
                    .ghost()
                    .label(part)
                    .selected(i == last)
                    .on_press(move || {
                        if let Some(p) = &pick {
                            p(i);
                        }
                    }),
            );
        }
        row
    }
}

/// Page numbers (kit `Pagination`).
pub struct Pagination {
    theme: Theme,
    page: usize,
    pages: usize,
    on_page: Option<IndexHandler>,
}

impl Pagination {
    pub fn new(pages: usize) -> Self {
        Self {
            theme: Theme::default(),
            page: 0,
            pages: pages.max(1),
            on_page: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }

    pub fn on_page(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_page = Some(Arc::new(f));
        self
    }
}

impl IntoElement for Pagination {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut row = div().flex().gap_1().items_center();
        let page = self.page.min(self.pages - 1);
        let fire = self.on_page.clone();
        row = row.child({
            let f = fire.clone();
            let prev = page.saturating_sub(1);
            Button::new("pg-prev")
                .themed(&self.theme)
                .ghost()
                .icon(IconName::ChevronLeft)
                .compact()
                .on_press(move || {
                    if let Some(p) = &f {
                        p(prev);
                    }
                })
        });
        for i in 0..self.pages.min(7) {
            let f = fire.clone();
            row = row.child(
                Button::new(format!("pg-{i}"))
                    .themed(&self.theme)
                    .ghost()
                    .label(format!("{}", i + 1))
                    .selected(i == page)
                    .on_press(move || {
                        if let Some(p) = &f {
                            p(i);
                        }
                    }),
            );
        }
        row.child({
            let f = fire;
            let next = (page + 1).min(self.pages - 1);
            Button::new("pg-next")
                .themed(&self.theme)
                .ghost()
                .icon(IconName::ChevronRight)
                .compact()
                .on_press(move || {
                    if let Some(p) = &f {
                        p(next);
                    }
                })
        })
    }
}

/// Linear steps (kit `Stepper`).
pub struct Stepper {
    theme: Theme,
    labels: Vec<String>,
    step: usize,
    on_step: Option<IndexHandler>,
}

impl Stepper {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            labels: Vec::new(),
            step: 0,
            on_step: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn labels<I, S>(mut self, labels: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.labels = labels.into_iter().map(Into::into).collect();
        self
    }

    pub fn step(mut self, step: usize) -> Self {
        self.step = step;
        self
    }

    pub fn on_step(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_step = Some(Arc::new(f));
        self
    }
}

impl Default for Stepper {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Stepper {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut row = div().flex().gap_2().items_center();
        for (i, label) in self.labels.into_iter().enumerate() {
            let pick = self.on_step.clone();
            row = row.child(
                Button::new(format!("step-{i}"))
                    .themed(&self.theme)
                    .ghost()
                    .label(format!("{}. {label}", i + 1))
                    .selected(i == self.step)
                    .on_press(move || {
                        if let Some(p) = &pick {
                            p(i);
                        }
                    }),
            );
        }
        row
    }
}

/// Push/pop navigation stack (kit `NavStack`).
#[derive(Clone, Debug, Default)]
pub struct NavStackState {
    pub pages: Vec<String>,
}

impl NavStackState {
    pub fn push(&mut self, page: impl Into<String>) {
        self.pages.push(page.into());
    }

    pub fn pop(&mut self) -> Option<String> {
        if self.pages.len() > 1 {
            self.pages.pop()
        } else {
            None
        }
    }

    pub fn current(&self) -> &str {
        self.pages.last().map(String::as_str).unwrap_or("")
    }
}

pub struct NavStack {
    theme: Theme,
    state: NavStackState,
    on_back: Option<Handler>,
}

impl NavStack {
    pub fn new(state: NavStackState) -> Self {
        Self {
            theme: Theme::default(),
            state,
            on_back: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn on_back(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_back = Some(Arc::new(f));
        self
    }
}

impl IntoElement for NavStack {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut head = div().flex().items_center().gap_2();
        if self.state.pages.len() > 1 {
            let mut back = Button::new("nav-back")
                .themed(&self.theme)
                .ghost()
                .icon(IconName::ChevronLeft)
                .compact();
            if let Some(b) = self.on_back {
                back = back.on_press(move || b());
            }
            head = head.child(back);
        }
        head = head.child(
            Label::new(String::from(self.state.current()))
                .themed(&self.theme)
                .font(FontId::UiBold)
                .size(self.theme.typo.body),
        );
        div().flex_col().child(head)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Accordion, Breadcrumb, Collapsible, NavStack, NavStackState, Pagination, Stepper, TabBar,
        Tabs,
    };
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use knox_gpui::{
        IntoElement, Key, KeyDownEvent, Modifiers, Point, Size, WindowInput, dispatch_key_down,
        prepare_element, text,
    };

    #[test]
    fn arrows_change_tab() {
        let hit = Arc::new(AtomicUsize::new(0));
        let flag = hit.clone();
        let mut el = TabBar::new("t")
            .themed(&Theme::aurora_dark())
            .labels(["A", "B", "C"])
            .selected(0)
            .on_select(move |i| flag.store(i, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(240, 40), &mut input);
        knox_gpui::dispatch_mouse_down(&mut el, &mut input, Point::xy(8, 8));
        dispatch_key_down(
            &mut el,
            &mut input,
            &KeyDownEvent::new(Key::Right, Modifiers::NONE),
        );
        assert_eq!(hit.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn stack_push_pop() {
        let mut s = NavStackState::default();
        s.push("Home");
        s.push("Child");
        assert_eq!(s.current(), "Child");
        assert_eq!(s.pop().as_deref(), Some("Child"));
        assert_eq!(s.current(), "Home");
        assert!(s.pop().is_none());
        let theme = Theme::aurora_dark();
        let _ = Tabs::new("tabs")
            .themed(&theme)
            .labels(["A"])
            .page(text("a"));
        let _ = Accordion::new().themed(&theme).item("One", text("body"));
        let _ = Collapsible::new("X").themed(&theme);
        let _ = Breadcrumb::new().themed(&theme).parts(["Home", "Docs"]);
        let _ = Pagination::new(3).themed(&theme).page(1);
        let _ = Stepper::new().themed(&theme).labels(["A", "B"]);
        let _ = NavStack::new(s).themed(&theme);
    }
}
