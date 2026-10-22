// SPDX-License-Identifier: MIT
//! Select, Combobox, SearchableList (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Key, KeyDownEvent, Pixels, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::input::{FieldState, Input};
use crate::interact::{Handler, IndexHandler};
use crate::size::{ControlSize, Disableable, Selectable, Sizable};
use crate::theme::Theme;

type ToggleHandler = Handler;
type QueryHandler = Arc<dyn Fn(FieldState) + Send + Sync>;

fn matches_query(item: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let q: String = query.chars().flat_map(char::to_lowercase).collect();
    let hay: String = item.chars().flat_map(char::to_lowercase).collect();
    hay.contains(&q)
}

fn filtered_indices(items: &[String], query: &str) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, s)| matches_query(s, query))
        .map(|(i, _)| i)
        .collect()
}

fn item_row(
    theme: &Theme,
    id: String,
    label: String,
    selected: bool,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
) -> Button {
    let mut b = Button::new(id)
        .themed(theme)
        .ghost()
        .small()
        .label(label)
        .selected(selected);
    if let Some(handler) = on_click {
        b = b.on_click(move || handler());
    }
    b
}

/// Closed-choice dropdown (kit `Select`).
pub struct Select {
    id: String,
    theme: Theme,
    items: Vec<String>,
    selected: Option<usize>,
    open: bool,
    disabled: bool,
    size: ControlSize,
    width: Pixels,
    on_toggle: Option<ToggleHandler>,
    on_select: Option<IndexHandler>,
}

impl Select {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            items: Vec::new(),
            selected: None,
            open: false,
            disabled: false,
            size: ControlSize::Medium,
            width: px(160),
            on_toggle: None,
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

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn on_toggle(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_toggle = Some(Arc::new(f));
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }
}

impl Sizable for Select {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for Select {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for Select {
    type Element = Div;

    fn into_element(self) -> Div {
        let label = self
            .selected
            .and_then(|i| self.items.get(i).cloned())
            .unwrap_or_else(|| String::from("Select"));
        let mut trigger = Button::new(format!("{}-trig", self.id))
            .themed(&self.theme)
            .secondary()
            .with_size(self.size)
            .label(format!("{label} v"))
            .min_w(self.width)
            .disabled(self.disabled)
            .selected(self.open);
        if let Some(toggle) = self.on_toggle {
            trigger = trigger.on_click(move || toggle());
        }
        let mut root = div().flex_col().child(trigger);
        if self.open && !self.items.is_empty() {
            let mut menu = div()
                .flex_col()
                .bg(self.theme.colors.surface)
                .border_1()
                .border_color(self.theme.colors.border)
                .rounded(self.theme.radius.md)
                .p(self.theme.spacing.xs);
            for (i, item) in self.items.into_iter().enumerate() {
                let pick = self.on_select.clone();
                menu = menu.child(item_row(
                    &self.theme,
                    format!("{}-item-{i}", self.id),
                    item,
                    self.selected == Some(i),
                    pick.map(|p| Arc::new(move || p(i)) as Arc<dyn Fn() + Send + Sync>),
                ));
            }
            root = root.child(menu);
        }
        root
    }
}

/// Filterable list (kit `SearchableList`).
pub struct SearchableList {
    id: String,
    theme: Theme,
    items: Vec<String>,
    query: FieldState,
    selected: Option<usize>,
    highlight: usize,
    disabled: bool,
    width: Pixels,
    query_bar: bool,
    on_query: Option<QueryHandler>,
    on_select: Option<IndexHandler>,
    on_highlight: Option<IndexHandler>,
}

impl SearchableList {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            items: Vec::new(),
            query: FieldState::empty(),
            selected: None,
            highlight: 0,
            disabled: false,
            width: px(200),
            query_bar: true,
            on_query: None,
            on_select: None,
            on_highlight: None,
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

    pub fn query(mut self, query: FieldState) -> Self {
        self.query = query;
        self
    }

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn highlight(mut self, highlight: usize) -> Self {
        self.highlight = highlight;
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn query_bar(mut self, visible: bool) -> Self {
        self.query_bar = visible;
        self
    }

    pub fn on_query(mut self, f: impl Fn(FieldState) + Send + Sync + 'static) -> Self {
        self.on_query = Some(Arc::new(f));
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }

    pub fn on_highlight(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_highlight = Some(Arc::new(f));
        self
    }
}

impl Disableable for SearchableList {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for SearchableList {
    type Element = Div;

    fn into_element(self) -> Div {
        let q = self.query.value();
        let shown = filtered_indices(&self.items, &q);
        let highlight = if shown.is_empty() {
            0
        } else {
            self.highlight.min(shown.len() - 1)
        };
        let mut list = div()
            .flex_col()
            .w(self.width)
            .bg(self.theme.colors.surface)
            .border_1()
            .border_color(self.theme.colors.border)
            .rounded(self.theme.radius.md)
            .p(self.theme.spacing.xs);
        if shown.is_empty() {
            list = list.child(
                text("No matches")
                    .color(self.theme.colors.muted_foreground)
                    .size(12),
            );
        } else {
            for (row, &i) in shown.iter().enumerate() {
                let label = self.items[i].clone();
                let pick = self.on_select.clone();
                list = list.child(item_row(
                    &self.theme,
                    format!("{}-row-{i}", self.id),
                    label,
                    self.selected == Some(i) || row == highlight,
                    pick.map(|p| Arc::new(move || p(i)) as Arc<dyn Fn() + Send + Sync>),
                ));
            }
        }
        let mut root = div().flex_col().gap_1();
        if self.query_bar {
            let mut field = Input::new(format!("{}-q", self.id))
                .themed(&self.theme)
                .placeholder("Filter")
                .w(self.width)
                .disabled(self.disabled)
                .state(self.query);
            if let Some(on_query) = self.on_query {
                field = field.on_change(move |s| on_query(s));
            }
            field = bind_list_keys(
                field,
                shown,
                highlight,
                self.on_select.clone(),
                self.on_highlight.clone(),
                None,
            );
            root = root.child(field);
        }
        root.child(list)
    }
}

/// Input + filtered dropdown (kit `Combobox`).
pub struct Combobox {
    id: String,
    theme: Theme,
    items: Vec<String>,
    query: FieldState,
    selected: Option<usize>,
    open: bool,
    highlight: usize,
    disabled: bool,
    width: Pixels,
    on_query: Option<QueryHandler>,
    on_toggle: Option<ToggleHandler>,
    on_select: Option<IndexHandler>,
    on_highlight: Option<IndexHandler>,
}

impl Combobox {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            items: Vec::new(),
            query: FieldState::empty(),
            selected: None,
            open: false,
            highlight: 0,
            disabled: false,
            width: px(180),
            on_query: None,
            on_toggle: None,
            on_select: None,
            on_highlight: None,
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

    pub fn query(mut self, query: FieldState) -> Self {
        self.query = query;
        self
    }

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn highlight(mut self, highlight: usize) -> Self {
        self.highlight = highlight;
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn on_query(mut self, f: impl Fn(FieldState) + Send + Sync + 'static) -> Self {
        self.on_query = Some(Arc::new(f));
        self
    }

    pub fn on_toggle(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_toggle = Some(Arc::new(f));
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }

    pub fn on_highlight(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_highlight = Some(Arc::new(f));
        self
    }
}

impl Disableable for Combobox {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoElement for Combobox {
    type Element = Div;

    fn into_element(self) -> Div {
        let placeholder = self
            .selected
            .and_then(|i| self.items.get(i).cloned())
            .unwrap_or_else(|| String::from("Search"));
        let q = self.query.value();
        let shown = filtered_indices(&self.items, &q);
        let highlight = if shown.is_empty() {
            0
        } else {
            self.highlight.min(shown.len() - 1)
        };
        let mut field = Input::new(format!("{}-in", self.id))
            .themed(&self.theme)
            .placeholder(placeholder)
            .w(self.width)
            .disabled(self.disabled)
            .state(self.query);
        if let Some(on_query) = self.on_query.clone() {
            field = field.on_change(move |s| on_query(s));
        }
        let close = if self.open {
            self.on_toggle.clone()
        } else {
            None
        };
        field = bind_list_keys(
            field,
            shown.clone(),
            highlight,
            self.on_select.clone(),
            self.on_highlight.clone(),
            close,
        );
        let mut root = div().flex_col().child(field);
        if self.open {
            let mut menu = div()
                .flex_col()
                .w(self.width)
                .bg(self.theme.colors.surface)
                .border_1()
                .border_color(self.theme.colors.border)
                .rounded(self.theme.radius.md)
                .p(self.theme.spacing.xs);
            if shown.is_empty() {
                menu = menu.child(
                    text("No matches")
                        .color(self.theme.colors.muted_foreground)
                        .size(12),
                );
            } else {
                for (row, &i) in shown.iter().enumerate() {
                    let pick = self.on_select.clone();
                    menu = menu.child(item_row(
                        &self.theme,
                        format!("{}-item-{i}", self.id),
                        self.items[i].clone(),
                        row == highlight,
                        pick.map(|p| Arc::new(move || p(i)) as Arc<dyn Fn() + Send + Sync>),
                    ));
                }
            }
            root = root.child(menu);
        }
        root
    }
}

fn bind_list_keys(
    field: Input,
    shown: Vec<usize>,
    highlight: usize,
    on_select: Option<IndexHandler>,
    on_highlight: Option<IndexHandler>,
    on_close: Option<Handler>,
) -> Input {
    field.on_key(move |e| match pick_with_keys(e, &shown, highlight) {
        Some(ListNav::Highlight(i)) => {
            if let Some(h) = &on_highlight {
                h(i);
            }
            true
        }
        Some(ListNav::Select(i)) => {
            if let Some(h) = &on_select {
                h(i);
            }
            true
        }
        Some(ListNav::Close) => {
            if let Some(h) = &on_close {
                h();
            }
            true
        }
        None => false,
    })
}

/// Apply up/down/enter against a filtered index list (host tests + parent).
pub fn pick_with_keys(event: &KeyDownEvent, shown: &[usize], highlight: usize) -> Option<ListNav> {
    if shown.is_empty() {
        return None;
    }
    match event.key {
        Key::Down => Some(ListNav::Highlight((highlight + 1) % shown.len())),
        Key::Up => Some(ListNav::Highlight(if highlight == 0 {
            shown.len() - 1
        } else {
            highlight - 1
        })),
        Key::Enter => Some(ListNav::Select(shown[highlight.min(shown.len() - 1)])),
        Key::Escape => Some(ListNav::Close),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListNav {
    Highlight(usize),
    Select(usize),
    Close,
}

#[cfg(test)]
mod tests {
    use super::{Combobox, ListNav, SearchableList, Select, filtered_indices, pick_with_keys};
    use crate::input::FieldState;
    use crate::theme::Theme;
    use alloc::string::String;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use knox_gpui::{
        IntoElement, Key, KeyDownEvent, Modifiers, Point, Size, WindowInput, dispatch_mouse_down,
        dispatch_mouse_up, prepare_element,
    };

    #[test]
    fn filter_and_keyboard_pick() {
        let items = [
            String::from("Apple"),
            String::from("Apricot"),
            String::from("Banana"),
        ];
        let shown = filtered_indices(&items, "ap");
        assert_eq!(shown, alloc::vec![0, 1]);
        let down = KeyDownEvent::new(Key::Down, Modifiers::NONE);
        assert_eq!(
            pick_with_keys(&down, &shown, 0),
            Some(ListNav::Highlight(1))
        );
        let enter = KeyDownEvent::new(Key::Enter, Modifiers::NONE);
        assert_eq!(pick_with_keys(&enter, &shown, 1), Some(ListNav::Select(1)));
    }

    #[test]
    fn select_click_commits() {
        let n = Arc::new(AtomicUsize::new(99));
        let c = n.clone();
        let mut el = Select::new("s")
            .themed(&Theme::aurora_dark())
            .items(["One", "Two"])
            .open(true)
            .on_select(move |i| {
                c.store(i, Ordering::SeqCst);
            })
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(200, 80), &mut input);
        let item = input.hitboxes()[1];
        let p = Point::xy(item.bounds.x() + 8, item.bounds.y() + 8);
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert_eq!(n.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn combobox_and_list_construct() {
        let theme = Theme::aurora_dark();
        let _ = Combobox::new("c")
            .themed(&theme)
            .items(["Ada", "Alan"])
            .query(FieldState::from("a"))
            .open(true)
            .into_element();
        let _ = SearchableList::new("l")
            .themed(&theme)
            .items(["Ada", "Alan"])
            .query(FieldState::from("al"))
            .into_element();
    }
}
