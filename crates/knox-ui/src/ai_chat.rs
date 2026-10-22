// SPDX-License-Identifier: MIT
//! AI assistant chat chrome: MessageScroller + Input (D11.8).
//! Domain stays in `kernel/src/gui/ai_assistant.rs`.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS app view, not a kit catalog control.

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px};

use crate::button::{Button, ButtonVariants};
use crate::input::{FieldState, Input};
use crate::interact::Handler;
use crate::message::{ChatMessage, MessageScroller};
use crate::theme::Theme;

/// AI chat window body.
pub struct AiChat {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    messages: alloc::vec::Vec<ChatMessage>,
    input: FieldState,
    offset: i32,
    on_send: Option<Handler>,
    on_input: Option<alloc::sync::Arc<dyn Fn(FieldState) + Send + Sync>>,
    on_offset: Option<alloc::sync::Arc<dyn Fn(i32) + Send + Sync>>,
}

impl AiChat {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(640),
            height: px(480),
            messages: alloc::vec::Vec::new(),
            input: FieldState::empty(),
            offset: 0,
            on_send: None,
            on_input: None,
            on_offset: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn messages(mut self, messages: impl IntoIterator<Item = ChatMessage>) -> Self {
        self.messages = messages.into_iter().collect();
        self
    }

    pub fn input(mut self, state: FieldState) -> Self {
        self.input = state;
        self
    }

    pub fn offset(mut self, offset: i32) -> Self {
        self.offset = offset.max(0);
        self
    }

    pub fn on_send(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_send = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_input(mut self, f: impl Fn(FieldState) + Send + Sync + 'static) -> Self {
        self.on_input = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_offset(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_offset = Some(alloc::sync::Arc::new(f));
        self
    }
}

impl Default for AiChat {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for AiChat {
    type Element = Div;

    fn into_element(self) -> Div {
        let body_h = px((self.height.0 - 48).max(80));
        let mut scroller = MessageScroller::new()
            .themed(&self.theme)
            .items(self.messages)
            .offset(self.offset)
            .size(self.width, body_h);
        if let Some(on) = self.on_offset {
            scroller = scroller.on_offset(move |o| on(o));
        }
        let mut composer = div().flex().gap_1().p(self.theme.spacing.xs).items_center();
        let mut input = Input::new("ai-input")
            .themed(&self.theme)
            .state(self.input)
            .placeholder("Ask Knox…")
            .w(px((self.width.0 - 80).max(80)))
            .grow();
        if let Some(on) = self.on_input {
            input = input.on_change(move |s| on(s));
        }
        composer = composer.child(input);
        if let Some(h) = self.on_send {
            composer = composer.child(
                Button::new("ai-send")
                    .themed(&self.theme)
                    .primary()
                    .label("Send")
                    .on_click(move || h()),
            );
        }
        div()
            .flex_col()
            .w(self.width)
            .h(self.height)
            .bg(self.theme.colors.background)
            .child(scroller)
            .child(composer)
    }
}

#[cfg(test)]
mod tests {
    use super::AiChat;
    use crate::message::{BubbleSide, ChatMessage};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element, px,
    };

    #[test]
    fn send_button_fires() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let mut el = AiChat::new()
            .themed(&Theme::aurora_dark())
            .size(px(400), px(240))
            .messages([ChatMessage::new(BubbleSide::Them, "Hi")])
            .on_send(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(400, 240), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.y() >= 180 && hb.bounds.height() <= 36)
            .max_by_key(|hb| hb.bounds.x())
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("send");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert!(hit.load(Ordering::SeqCst));
    }
}
