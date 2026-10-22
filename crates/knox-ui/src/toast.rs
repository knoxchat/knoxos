// SPDX-License-Identifier: MIT
//! Toast and Notification list (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. The host clock calls [`tick_toasts`].

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::interact::IndexHandler;
use crate::theme::Theme;

/// One ephemeral notice.
#[derive(Clone, Debug)]
pub struct ToastData {
    pub title: String,
    pub message: String,
    pub remaining_ms: u32,
}

impl ToastData {
    pub fn new(title: impl Into<String>, message: impl Into<String>, remaining_ms: u32) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            remaining_ms,
        }
    }
}

/// Advance remaining time. Expired entries are dropped (timed dismiss).
pub fn tick_toasts(items: &mut Vec<ToastData>, dt_ms: u32) {
    for item in items.iter_mut() {
        item.remaining_ms = item.remaining_ms.saturating_sub(dt_ms);
    }
    items.retain(|t| t.remaining_ms > 0);
}

/// Single toast card (kit `Toast`).
pub struct Toast {
    theme: Theme,
    data: ToastData,
    index: usize,
    on_dismiss: Option<IndexHandler>,
}

impl Toast {
    pub fn new(data: ToastData) -> Self {
        Self {
            theme: Theme::default(),
            data,
            index: 0,
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn index(mut self, index: usize) -> Self {
        self.index = index;
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }
}

impl IntoElement for Toast {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut card = div()
            .flex()
            .justify_between()
            .items_center()
            .w(px(280))
            .p(self.theme.spacing.sm)
            .gap_2()
            .bg(self.theme.colors.surface)
            .border_1()
            .border_color(self.theme.colors.border)
            .rounded(self.theme.radius.md)
            .child(
                div()
                    .flex_col()
                    .gap_1()
                    .child(
                        text(self.data.title)
                            .color(self.theme.colors.foreground)
                            .size(13),
                    )
                    .child(
                        text(self.data.message)
                            .color(self.theme.colors.muted_foreground)
                            .size(11),
                    ),
            );
        if let Some(dismiss) = self.on_dismiss {
            let i = self.index;
            card = card.child(
                Button::new("toast-x")
                    .themed(&self.theme)
                    .ghost()
                    .label("x")
                    .on_click(move || dismiss(i)),
            );
        }
        card
    }
}

/// Stacked notifications (kit notification list). Newest last.
pub struct NotificationList {
    theme: Theme,
    items: Vec<ToastData>,
    on_dismiss: Option<IndexHandler>,
}

impl NotificationList {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            items: Vec::new(),
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = ToastData>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }
}

impl Default for NotificationList {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for NotificationList {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut stack = div().flex_col().gap_2();
        for (i, data) in self.items.into_iter().enumerate() {
            let mut t = Toast::new(data).themed(&self.theme).index(i);
            if let Some(d) = self.on_dismiss.clone() {
                t = t.on_dismiss(move |idx| d(idx));
            }
            stack = stack.child(t);
        }
        stack
    }
}

#[cfg(test)]
mod tests {
    use super::{NotificationList, Toast, ToastData, tick_toasts};
    use crate::theme::Theme;
    use alloc::vec;
    use knox_gpui::IntoElement;

    #[test]
    fn timed_dismiss_drops_expired() {
        let mut items = vec![
            ToastData::new("A", "one", 40),
            ToastData::new("B", "two", 120),
        ];
        tick_toasts(&mut items, 50);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "B");
        tick_toasts(&mut items, 80);
        assert!(items.is_empty());
    }

    #[test]
    fn stack_constructs() {
        let theme = Theme::aurora_dark();
        let _ = Toast::new(ToastData::new("Hi", "body", 1000))
            .themed(&theme)
            .into_element();
        let _ = NotificationList::new()
            .themed(&theme)
            .items([ToastData::new("A", "", 10), ToastData::new("B", "", 10)])
            .into_element();
    }
}
