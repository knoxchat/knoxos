// SPDX-License-Identifier: MIT
//! Toast and Notification list (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. The host clock calls [`tick_toasts`].

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, FontId, IntoElement, Styled, div, glyph_layout_advance, px, text};

use crate::button::{Button, ButtonVariants};
use crate::interact::IndexHandler;
use crate::theme::Theme;

/// Session toast card width (title + wrapped body + dismiss).
pub const TOAST_CARD_W: i32 = 380;
const TOAST_TEXT_W: i32 = 308;

/// Break `s` on word boundaries so a line stays within `max_px`.
pub fn wrap_words(s: &str, max_px: i32, size_px: u16) -> String {
    let mut out = String::new();
    let mut line_w = 0i32;
    let mut first = true;
    for word in s.split_whitespace() {
        let ww = word
            .chars()
            .map(|ch| glyph_layout_advance(ch, size_px))
            .sum::<i32>();
        let space = glyph_layout_advance(' ', size_px);
        if !first && line_w + space + ww > max_px && max_px > 0 {
            out.push('\n');
            line_w = 0;
        } else if !first {
            out.push(' ');
            line_w += space;
        }
        out.push_str(word);
        line_w += ww;
        first = false;
    }
    out
}

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
        let typo = self.theme.typo;
        let title = wrap_words(&self.data.title, TOAST_TEXT_W, typo.ui);
        let message = wrap_words(&self.data.message, TOAST_TEXT_W, typo.body);
        let mut card = div()
            .flex()
            .justify_between()
            .items_start()
            .w(px(TOAST_CARD_W))
            .p(px(14))
            .gap(px(10))
            .bg(self.theme.colors.surface)
            .border_1()
            .border_color(self.theme.colors.border)
            .rounded(self.theme.radius.md)
            .child(
                div()
                    .flex_col()
                    .gap_1()
                    .w(px(TOAST_TEXT_W))
                    .child(
                        text(title)
                            .color(self.theme.colors.foreground)
                            .font(FontId::UiBold)
                            .size(typo.ui),
                    )
                    .child(
                        text(message)
                            .color(self.theme.colors.muted_foreground)
                            .size(typo.body),
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
    use super::{NotificationList, TOAST_TEXT_W, Toast, ToastData, tick_toasts, wrap_words};
    use crate::theme::Theme;
    use alloc::vec;
    use knox_gpui::{IntoElement, glyph_layout_advance};

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
    fn wrap_keeps_welcome_copy_inside_card() {
        let title = wrap_words("Welcome to KnoxOS", TOAST_TEXT_W, 16);
        assert!(
            !title.contains('\n'),
            "title must stay on one line: {title}"
        );
        let body = wrap_words("Desktop environment loaded successfully", TOAST_TEXT_W, 15);
        for line in body.split('\n') {
            let w: i32 = line.chars().map(|ch| glyph_layout_advance(ch, 15)).sum();
            assert!(
                w <= TOAST_TEXT_W,
                "wrapped line {line:?} is {w}px, max {TOAST_TEXT_W}"
            );
        }
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
