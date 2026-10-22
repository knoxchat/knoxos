// SPDX-License-Identifier: MIT
//! Message, MessageScroller, Bubble, Attachment, Marker, Rating, Questionnaire.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::interact::IndexHandler;
use crate::scroll::ScrollArea;
use crate::size::Selectable;
use crate::theme::Theme;

/// Chat bubble alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BubbleSide {
    Them,
    Us,
}

/// Speech bubble (kit `Bubble`).
pub struct Bubble {
    theme: Theme,
    side: BubbleSide,
    body: String,
}

impl Bubble {
    pub fn new(body: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            side: BubbleSide::Them,
            body: body.into(),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn side(mut self, side: BubbleSide) -> Self {
        self.side = side;
        self
    }
}

impl IntoElement for Bubble {
    type Element = Div;

    fn into_element(self) -> Div {
        let us = self.side == BubbleSide::Us;
        let bg = if us {
            self.theme.colors.primary.opacity(0.25)
        } else {
            self.theme.colors.muted
        };
        let row = if us {
            div().flex().justify_end()
        } else {
            div().flex()
        };
        row.w(px(320)).child(
            div()
                .p(self.theme.spacing.xs)
                .bg(bg)
                .rounded(self.theme.radius.md)
                .child(text(self.body).color(self.theme.colors.foreground).size(12)),
        )
    }
}

/// File chip (kit `Attachment`).
pub struct Attachment {
    theme: Theme,
    name: String,
}

impl Attachment {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            name: name.into(),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }
}

impl IntoElement for Attachment {
    type Element = Div;

    fn into_element(self) -> Div {
        div()
            .p(self.theme.spacing.xs)
            .bg(self.theme.colors.muted)
            .rounded(self.theme.radius.sm)
            .child(
                text(alloc::format!("[{}]", self.name))
                    .color(self.theme.colors.foreground)
                    .size(12),
            )
    }
}

/// Inline annotation (kit `Marker`).
pub struct Marker {
    theme: Theme,
    label: String,
}

impl Marker {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            label: label.into(),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }
}

impl IntoElement for Marker {
    type Element = Div;

    fn into_element(self) -> Div {
        div().child(text(self.label).color(self.theme.colors.primary).size(12))
    }
}

/// Star rating (kit `Rating`).
pub struct Rating {
    theme: Theme,
    value: u8,
    max: u8,
    on_change: Option<IndexHandler>,
}

impl Rating {
    pub fn new(value: u8) -> Self {
        Self {
            theme: Theme::default(),
            value,
            max: 5,
            on_change: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn max(mut self, max: u8) -> Self {
        self.max = max.max(1);
        self
    }

    pub fn on_change(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }
}

impl IntoElement for Rating {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut row = div().flex().gap_1();
        for i in 1..=self.max {
            let on = i <= self.value;
            let pick = self.on_change.clone();
            row = row.child(
                Button::new(alloc::format!("star-{i}"))
                    .themed(&self.theme)
                    .ghost()
                    .label(if on { "*" } else { "." })
                    .selected(on)
                    .on_click(move || {
                        if let Some(p) = &pick {
                            p(i as usize);
                        }
                    }),
            );
        }
        row
    }
}

/// Multi-choice prompt (kit `Questionnaire`).
pub struct Questionnaire {
    theme: Theme,
    prompt: String,
    choices: Vec<String>,
    selected: Option<usize>,
    on_select: Option<IndexHandler>,
}

impl Questionnaire {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            prompt: prompt.into(),
            choices: Vec::new(),
            selected: None,
            on_select: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn choices<I, S>(mut self, choices: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.choices = choices.into_iter().map(Into::into).collect();
        self
    }

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }
}

impl IntoElement for Questionnaire {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut col = div().flex_col().gap_1().child(
            text(self.prompt)
                .color(self.theme.colors.foreground)
                .size(13),
        );
        for (i, c) in self.choices.into_iter().enumerate() {
            let pick = self.on_select.clone();
            col = col.child(
                Button::new(alloc::format!("q-{i}"))
                    .themed(&self.theme)
                    .ghost()
                    .label(c)
                    .selected(self.selected == Some(i))
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

/// One chat row.
#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub side: BubbleSide,
    pub body: String,
    pub attachment: Option<String>,
}

impl ChatMessage {
    pub fn new(side: BubbleSide, body: impl Into<String>) -> Self {
        Self {
            side,
            body: body.into(),
            attachment: None,
        }
    }
}

/// Message row (kit `Message`).
pub struct Message {
    theme: Theme,
    data: ChatMessage,
}

impl Message {
    pub fn new(data: ChatMessage) -> Self {
        Self {
            theme: Theme::default(),
            data,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }
}

impl IntoElement for Message {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut col = div().flex_col().gap_1().child(
            Bubble::new(self.data.body)
                .themed(&self.theme)
                .side(self.data.side),
        );
        if let Some(name) = self.data.attachment {
            col = col.child(Attachment::new(name).themed(&self.theme));
        }
        col
    }
}

/// Chat history that pins to the latest row (kit `MessageScroller`).
pub struct MessageScroller {
    theme: Theme,
    items: Vec<ChatMessage>,
    offset: i32,
    width: Pixels,
    height: Pixels,
    on_offset: Option<Arc<dyn Fn(i32) + Send + Sync>>,
}

impl MessageScroller {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            items: Vec::new(),
            offset: 0,
            width: px(340),
            height: px(160),
            on_offset: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = ChatMessage>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    pub fn offset(mut self, offset: i32) -> Self {
        self.offset = offset.max(0);
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn on_offset(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_offset = Some(Arc::new(f));
        self
    }

    /// Offset that shows the last messages in a `viewport` of `row_h` pixels.
    pub fn offset_to_latest(count: usize, row_h: i32, viewport: i32) -> i32 {
        ((count as i32) * row_h - viewport).max(0)
    }
}

impl Default for MessageScroller {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for MessageScroller {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut col = div().flex_col().gap_2();
        for item in self.items {
            col = col.child(Message::new(item).themed(&self.theme));
        }
        let mut area = ScrollArea::new()
            .themed(&self.theme)
            .size(self.width, self.height)
            .offset(self.offset)
            .child(col);
        if let Some(on) = self.on_offset {
            area = area.on_offset(move |o| on(o));
        }
        area.into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Attachment, Bubble, BubbleSide, ChatMessage, Marker, Message, MessageScroller,
        Questionnaire, Rating,
    };
    use crate::theme::Theme;
    use knox_gpui::IntoElement;

    #[test]
    fn scroller_pins_to_latest() {
        assert_eq!(MessageScroller::offset_to_latest(10, 24, 80), 160);
        let theme = Theme::aurora_dark();
        let items = [
            ChatMessage::new(BubbleSide::Them, "Hi"),
            ChatMessage::new(BubbleSide::Us, "Hello"),
        ];
        let _ = MessageScroller::new()
            .themed(&theme)
            .items(items)
            .offset(MessageScroller::offset_to_latest(2, 40, 80))
            .into_element();
        let _ = Bubble::new("x").themed(&theme).into_element();
        let _ = Attachment::new("a.txt").themed(&theme).into_element();
        let _ = Marker::new("note").themed(&theme).into_element();
        let _ = Rating::new(3).themed(&theme).into_element();
        let _ = Questionnaire::new("Ok?")
            .themed(&theme)
            .choices(["Yes", "No"]);
        let _ = Message::new(ChatMessage::new(BubbleSide::Us, "m")).themed(&theme);
    }
}
