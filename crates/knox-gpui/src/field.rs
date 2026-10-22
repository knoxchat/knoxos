// SPDX-License-Identifier: MIT
//! Minimal single-line text field (D6.4 / D6.5).
//!
//! Full kit `Input` lives in `knox-ui` (D8.4). This is the GPUI-level
//! target that can take IME commits and request an ibeam.

use alloc::boxed::Box;
use alloc::string::String;

use crate::element::{Element, PaintEnv};
use crate::geometry::{Bounds, Hsla, Pixels, Point, Size, px};
use crate::input::{
    ClickEvent, CursorStyle, Key, KeyDownEvent, MouseEvent, ScrollEvent, WindowInput,
};
use crate::layout::LayoutHints;
use crate::scene::{Quad, Scene};
use crate::style::Display;
use crate::text::text;

type ChangeHandler = Box<dyn FnMut(&str) + Send>;

/// Single-line field. State lives in `value`; the parent persists it across
/// frames (elements are immediate).
pub struct TextInput {
    value: String,
    color: Hsla,
    background: Hsla,
    width: Pixels,
    height: Pixels,
    computed: Bounds,
    hitbox_id: Option<crate::input::HitboxId>,
    on_change: Option<ChangeHandler>,
}

pub fn text_input(value: impl Into<String>) -> TextInput {
    TextInput {
        value: value.into(),
        color: Hsla::WHITE,
        background: Hsla::from_rgb(28, 26, 32),
        width: px(120),
        height: px(28),
        computed: Bounds::ZERO,
        hitbox_id: None,
        on_change: None,
    }
}

impl TextInput {
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = color;
        self
    }

    pub fn bg(mut self, color: Hsla) -> Self {
        self.background = color;
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn h(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    pub fn on_change(mut self, f: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn computed(&self) -> Bounds {
        self.computed
    }

    fn fire_change(&mut self) {
        if let Some(handler) = &mut self.on_change {
            handler(&self.value);
        }
    }

    fn is_target(&self, cx: &WindowInput) -> bool {
        self.hitbox_id.is_some() && self.hitbox_id == cx.focused()
    }
}

impl Element for TextInput {
    fn request_layout(&mut self, origin: Point, _available: Size) -> Bounds {
        self.computed = Bounds::new(
            origin,
            Size {
                width: self.width,
                height: self.height,
            },
        );
        self.computed
    }

    fn layout_hints(&self) -> LayoutHints {
        LayoutHints {
            display: Display::Flex,
            ..LayoutHints::default()
        }
    }

    fn prepaint(&mut self, _bounds: Bounds, cx: &mut WindowInput) {
        self.hitbox_id = Some(cx.insert_hitbox_ex(self.computed, CursorStyle::IBeam, true, true));
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
        if input.focused() == self.hitbox_id {
            let ring = bounds.dilate(px(2));
            scene.push_quad(Quad::new(ring, crate::style::ring_color()).rounded(px(8)));
        }
        scene.push_quad(Quad::new(bounds, self.background).rounded(px(6)));
        if self.value.is_empty() {
            return;
        }
        let mut label = text(self.value.clone()).color(self.color).size(13);
        let inner = Point::xy(bounds.x() + 8, bounds.y() + 6);
        let avail = Size::wh((bounds.width() - 12).max(1), (bounds.height() - 8).max(1));
        let lb = label.request_layout(inner, avail);
        label.paint(lb, scene, input, env);
    }

    fn mouse_down(&mut self, _event: &MouseEvent, _cx: &mut WindowInput) {}
    fn mouse_up(&mut self, _event: &MouseEvent, _cx: &mut WindowInput) {}
    fn mouse_move(&mut self, _event: &MouseEvent, _cx: &mut WindowInput) {}
    fn click(&mut self, _event: &ClickEvent, _cx: &mut WindowInput) {}
    fn scroll(&mut self, _event: &ScrollEvent, _cx: &mut WindowInput) {}

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut WindowInput) -> bool {
        if !self.is_target(cx) {
            return false;
        }
        match event.key {
            Key::Backspace => {
                self.value.pop();
                self.fire_change();
                true
            }
            Key::Char(c) if !c.is_control() => {
                self.value.push(c);
                self.fire_change();
                true
            }
            Key::Space => {
                self.value.push(' ');
                self.fire_change();
                true
            }
            _ => false,
        }
    }

    fn ime_commit(&mut self, text: &str, cx: &mut WindowInput) {
        if !self.is_target(cx) || text.is_empty() {
            return;
        }
        self.value.push_str(text);
        self.fire_change();
    }
}

#[cfg(test)]
mod tests {
    use super::text_input;
    use crate::element::{dispatch_ime_commit, dispatch_key_down, prepare_element};
    use crate::geometry::{Point, Size};
    use crate::input::{Key, KeyDownEvent, Modifiers, WindowInput};

    /// D6.4: a focused field receives composed characters.
    #[test]
    fn focused_input_receives_composed_chars() {
        let mut field = text_input("");
        let mut input = WindowInput::new();
        prepare_element(&mut field, Point::ZERO, Size::wh(80, 24), &mut input);
        let id = input.hitboxes()[0].id;
        input.focus(id);
        dispatch_ime_commit(&mut field, &mut input, "你");
        assert_eq!(field.value(), "你");
        assert!(dispatch_key_down(
            &mut field,
            &mut input,
            &KeyDownEvent::new(Key::Char('a'), Modifiers::NONE)
        ));
        assert_eq!(field.value(), "你a");
        assert!(dispatch_key_down(
            &mut field,
            &mut input,
            &KeyDownEvent::new(Key::Backspace, Modifiers::NONE)
        ));
        assert_eq!(field.value(), "你");
    }
}
