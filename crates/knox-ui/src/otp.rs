// SPDX-License-Identifier: MIT
//! OtpInput (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{
    Bounds, CursorStyle, Element, HitboxId, Key, KeyDownEvent, LayoutHints, MouseEvent, PaintEnv,
    Point, Quad, Scene, Size, WindowInput, px, ring_color, text,
};

use crate::size::{ControlSize, Disableable, Sizable};
use crate::theme::Theme;

type StringHandler = Arc<dyn Fn(&str) + Send + Sync>;

/// Fixed-length one-time-password boxes.
pub struct OtpInput {
    theme: Theme,
    value: String,
    length: usize,
    disabled: bool,
    size: ControlSize,
    on_change: Option<StringHandler>,
    computed: Bounds,
    hitbox_id: Option<HitboxId>,
}

impl OtpInput {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            value: String::new(),
            length: 6,
            disabled: false,
            size: ControlSize::Medium,
            on_change: None,
            computed: Bounds::ZERO,
            hitbox_id: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self.trim();
        self
    }

    pub fn length(mut self, length: usize) -> Self {
        self.length = length.clamp(1, 12);
        self.trim();
        self
    }

    pub fn on_change(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    pub fn value_ref(&self) -> &str {
        &self.value
    }

    fn trim(&mut self) {
        if self.value.chars().count() > self.length {
            self.value = self.value.chars().take(self.length).collect();
        }
    }

    fn box_side(&self) -> i32 {
        self.size.height()
    }

    fn emit(&mut self) {
        self.trim();
        if let Some(handler) = &self.on_change {
            handler(&self.value);
        }
    }

    fn insert(&mut self, ch: char) {
        if !ch.is_ascii_alphanumeric() {
            return;
        }
        if self.value.chars().count() >= self.length {
            return;
        }
        self.value.push(ch.to_ascii_uppercase());
        self.emit();
    }
}

impl Default for OtpInput {
    fn default() -> Self {
        Self::new()
    }
}

impl Sizable for OtpInput {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for OtpInput {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Element for OtpInput {
    fn request_layout(&mut self, origin: Point, _available: Size) -> Bounds {
        let side = self.box_side();
        let gap = 8;
        let w = self.length as i32 * side + (self.length as i32 - 1) * gap;
        self.computed = Bounds::from_xywh(origin.x.0, origin.y.0, w, side);
        self.computed
    }

    fn layout_hints(&self) -> LayoutHints {
        LayoutHints::default()
    }

    fn prepaint(&mut self, _bounds: Bounds, cx: &mut WindowInput) {
        self.hitbox_id = Some(cx.insert_hitbox_ex(
            self.computed,
            CursorStyle::IBeam,
            !self.disabled,
            !self.disabled,
        ));
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
        let focused = input.focused() == self.hitbox_id && self.hitbox_id.is_some();
        let side = self.box_side();
        let gap = 8;
        let chars: Vec<char> = self.value.chars().collect();
        let caret = chars.len().min(self.length.saturating_sub(1));
        for i in 0..self.length {
            let x = bounds.x() + i as i32 * (side + gap);
            let box_b = Bounds::from_xywh(x, bounds.y(), side, side);
            if focused && i == caret {
                scene.push_quad(
                    Quad::new(box_b.dilate(px(2)), ring_color()).rounded(self.theme.radius.md),
                );
            }
            let mut quad = Quad::new(box_b, self.theme.colors.input).rounded(self.theme.radius.md);
            quad.border_widths = knox_gpui::Edges::all(px(1));
            quad.border_color = if focused && i == caret {
                self.theme.colors.ring
            } else {
                self.theme.colors.border
            };
            scene.push_quad(quad);
            if let Some(ch) = chars.get(i) {
                let mut label = text(String::from(*ch))
                    .color(self.theme.colors.foreground)
                    .size(self.size.font());
                let inner = Point::xy(box_b.x() + side / 3, box_b.y() + 8);
                let lb = label.request_layout(inner, Size::wh(side, side));
                label.paint(lb, scene, input, env);
            }
        }
    }

    fn mouse_down(&mut self, _event: &MouseEvent, _cx: &mut WindowInput) {}

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut WindowInput) -> bool {
        if self.disabled || self.hitbox_id != cx.focused() {
            return false;
        }
        match event.key {
            Key::Backspace => {
                self.value.pop();
                self.emit();
                true
            }
            Key::Char(c) => {
                self.insert(c);
                true
            }
            _ => false,
        }
    }

    fn ime_commit(&mut self, text: &str, cx: &mut WindowInput) {
        if self.disabled || self.hitbox_id != cx.focused() {
            return;
        }
        for c in text.chars() {
            self.insert(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OtpInput;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use knox_gpui::{
        Key, KeyDownEvent, Modifiers, Point, Size, WindowInput, dispatch_ime_commit,
        dispatch_key_down, prepare_element,
    };

    #[test]
    fn types_and_backspaces() {
        let len = Arc::new(AtomicUsize::new(0));
        let n = len.clone();
        let mut el = OtpInput::new().length(4).on_change(move |s| {
            n.store(s.len(), Ordering::SeqCst);
        });
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(200, 40), &mut input);
        input.focus(input.hitboxes()[0].id);
        dispatch_ime_commit(&mut el, &mut input, "a1");
        assert_eq!(el.value_ref(), "A1");
        dispatch_key_down(
            &mut el,
            &mut input,
            &KeyDownEvent::new(Key::Char('b'), Modifiers::NONE),
        );
        assert_eq!(el.value_ref(), "A1B");
        dispatch_key_down(
            &mut el,
            &mut input,
            &KeyDownEvent::new(Key::Backspace, Modifiers::NONE),
        );
        assert_eq!(el.value_ref(), "A1");
        assert_eq!(len.load(Ordering::SeqCst), 2);
    }
}
