// SPDX-License-Identifier: MIT
//! Tooltip (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;

use knox_gpui::{
    Bounds, ClickEvent, Div, Element, IntoElement, KeyDownEvent, LayoutHints, MouseEvent, PaintEnv,
    Point, Scene, Size, Styled, WindowInput, div, px, text,
};

use crate::kbd::Kbd;
use crate::theme::Theme;

/// Hover bubble over a trigger (kit `Tooltip`).
pub struct Tooltip {
    theme: Theme,
    text: String,
    kbd: Option<Kbd>,
    trigger: Div,
}

impl Tooltip {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            theme: Theme::default(),
            text: text.into(),
            kbd: None,
            trigger: div(),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn key_binding(mut self, kbd: Kbd) -> Self {
        self.kbd = Some(kbd);
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.trigger = self.trigger.child(child);
        self
    }
}

impl IntoElement for Tooltip {
    type Element = TooltipEl;

    fn into_element(self) -> TooltipEl {
        let c = self.theme.colors;
        let mut bubble = div()
            .flex()
            .items_center()
            .gap_1()
            .pad_x(px(8))
            .pad_y(px(4))
            .rounded(self.theme.radius.sm)
            .bg(c.foreground)
            .child(text(self.text).color(c.background).size(12));
        if let Some(kbd) = self.kbd {
            bubble = bubble.child(kbd.themed(&self.theme).appearance(false));
        }
        TooltipEl {
            trigger: self.trigger,
            bubble,
            hit: None,
            computed: Bounds::ZERO,
            bubble_origin: Point::ZERO,
        }
    }
}

pub struct TooltipEl {
    trigger: Div,
    bubble: Div,
    hit: Option<knox_gpui::HitboxId>,
    computed: Bounds,
    bubble_origin: Point,
}

impl Element for TooltipEl {
    fn request_layout(&mut self, origin: Point, available: Size) -> Bounds {
        self.computed = self.trigger.request_layout(origin, available);
        self.bubble_origin = Point::xy(
            self.computed.x(),
            self.computed.y() + self.computed.height() + 4,
        );
        let _ = self
            .bubble
            .request_layout(self.bubble_origin, Size::wh(240, 40));
        self.computed
    }

    fn layout_hints(&self) -> LayoutHints {
        self.trigger.layout_hints()
    }

    fn prepaint(&mut self, bounds: Bounds, cx: &mut WindowInput) {
        self.hit = Some(cx.insert_hitbox(self.computed));
        self.trigger.prepaint(bounds, cx);
    }

    fn paint(
        &mut self,
        bounds: Bounds,
        scene: &mut Scene,
        input: &WindowInput,
        env: &mut PaintEnv,
    ) {
        let bounds = if bounds.is_empty() {
            self.computed
        } else {
            bounds
        };
        self.trigger.paint(bounds, scene, input, env);
        let hovered =
            input.element_state(self.hit).hovered || self.computed.contains(input.pointer());
        if hovered {
            let b = self.bubble.computed();
            self.bubble.paint(b, scene, input, env);
        }
    }

    fn mouse_down(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        self.trigger.mouse_down(event, cx);
    }
    fn mouse_up(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        self.trigger.mouse_up(event, cx);
    }
    fn mouse_move(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        self.trigger.mouse_move(event, cx);
    }
    fn click(&mut self, event: &ClickEvent, cx: &mut WindowInput) {
        self.trigger.click(event, cx);
    }
    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut WindowInput) -> bool {
        self.trigger.key_down(event, cx)
    }
}
