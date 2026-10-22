// SPDX-License-Identifier: MIT
//! Element tree: `request_layout` → `prepaint` → `paint`.
//!
//! Type names follow Zed GPUI (`Element`, `IntoElement`, `AnyElement`) as a
//! design reference (Apache-2.0). Layout is integer flex (D4), not Taffy.

use alloc::boxed::Box;

use crate::atlas::SpriteAtlas;
use crate::geometry::{Bounds, Point, Size};
use crate::input::{
    ClickEvent, Key, KeyDownEvent, MouseButton, MouseEvent, ScrollEvent, WindowInput,
};
use crate::layout::LayoutHints;
use crate::scene::Scene;
use crate::text::PlatformTextSystem;

/// Atlas + text system for a paint pass (sprites / glyphs).
pub struct PaintEnv<'a> {
    pub atlas: &'a mut SpriteAtlas,
    pub text: Option<&'a dyn PlatformTextSystem>,
}

/// Participates in layout and paint. Immediate description, per frame.
pub trait Element: 'static {
    /// Size and place this subtree. Returns absolute bounds.
    fn request_layout(&mut self, origin: Point, available: Size) -> Bounds;

    /// Flex / absolute hints for the parent. Default is in-flow, no grow.
    fn layout_hints(&self) -> LayoutHints {
        LayoutHints::default()
    }

    /// Register hitboxes. Default is a no-op.
    fn prepaint(&mut self, _bounds: Bounds, _cx: &mut WindowInput) {}

    /// Push primitives into `scene` in paint order.
    fn paint(&mut self, bounds: Bounds, scene: &mut Scene, input: &WindowInput, env: &mut PaintEnv);

    fn mouse_down(&mut self, _event: &MouseEvent, _cx: &mut WindowInput) {}
    fn mouse_up(&mut self, _event: &MouseEvent, _cx: &mut WindowInput) {}
    fn mouse_move(&mut self, _event: &MouseEvent, _cx: &mut WindowInput) {}
    fn click(&mut self, _event: &ClickEvent, _cx: &mut WindowInput) {}
    fn scroll(&mut self, _event: &ScrollEvent, _cx: &mut WindowInput) {}
    fn key_down(&mut self, _event: &KeyDownEvent, _cx: &mut WindowInput) -> bool {
        false
    }
    fn ime_commit(&mut self, _text: &str, _cx: &mut WindowInput) {}
}

/// Conversion into a concrete [`Element`].
pub trait IntoElement {
    type Element: Element;
    fn into_element(self) -> Self::Element;
}

impl<E: Element> IntoElement for E {
    type Element = E;
    fn into_element(self) -> E {
        self
    }
}

trait ElementObj {
    fn request_layout(&mut self, origin: Point, available: Size) -> Bounds;
    fn layout_hints(&self) -> LayoutHints;
    fn prepaint(&mut self, bounds: Bounds, cx: &mut WindowInput);
    fn paint(&mut self, bounds: Bounds, scene: &mut Scene, input: &WindowInput, env: &mut PaintEnv);
    fn mouse_down(&mut self, event: &MouseEvent, cx: &mut WindowInput);
    fn mouse_up(&mut self, event: &MouseEvent, cx: &mut WindowInput);
    fn mouse_move(&mut self, event: &MouseEvent, cx: &mut WindowInput);
    fn click(&mut self, event: &ClickEvent, cx: &mut WindowInput);
    fn scroll(&mut self, event: &ScrollEvent, cx: &mut WindowInput);
    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut WindowInput) -> bool;
    fn ime_commit(&mut self, text: &str, cx: &mut WindowInput);
}

impl<E: Element> ElementObj for E {
    fn request_layout(&mut self, origin: Point, available: Size) -> Bounds {
        Element::request_layout(self, origin, available)
    }
    fn layout_hints(&self) -> LayoutHints {
        Element::layout_hints(self)
    }
    fn prepaint(&mut self, bounds: Bounds, cx: &mut WindowInput) {
        Element::prepaint(self, bounds, cx);
    }
    fn paint(
        &mut self,
        bounds: Bounds,
        scene: &mut Scene,
        input: &WindowInput,
        env: &mut PaintEnv,
    ) {
        Element::paint(self, bounds, scene, input, env);
    }
    fn mouse_down(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        Element::mouse_down(self, event, cx);
    }
    fn mouse_up(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        Element::mouse_up(self, event, cx);
    }
    fn mouse_move(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        Element::mouse_move(self, event, cx);
    }
    fn click(&mut self, event: &ClickEvent, cx: &mut WindowInput) {
        Element::click(self, event, cx);
    }
    fn scroll(&mut self, event: &ScrollEvent, cx: &mut WindowInput) {
        Element::scroll(self, event, cx);
    }
    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut WindowInput) -> bool {
        Element::key_down(self, event, cx)
    }
    fn ime_commit(&mut self, text: &str, cx: &mut WindowInput) {
        Element::ime_commit(self, text, cx);
    }
}

/// Type-erased element (child list entries).
pub struct AnyElement {
    inner: Box<dyn ElementObj>,
    computed: Bounds,
}

impl AnyElement {
    pub fn new(element: impl Element) -> Self {
        Self {
            inner: Box::new(element),
            computed: Bounds::ZERO,
        }
    }

    pub fn computed(&self) -> Bounds {
        self.computed
    }
}

impl Element for AnyElement {
    fn request_layout(&mut self, origin: Point, available: Size) -> Bounds {
        self.computed = self.inner.request_layout(origin, available);
        self.computed
    }

    fn layout_hints(&self) -> LayoutHints {
        self.inner.layout_hints()
    }

    fn prepaint(&mut self, bounds: Bounds, cx: &mut WindowInput) {
        self.inner.prepaint(bounds, cx);
    }

    fn paint(
        &mut self,
        bounds: Bounds,
        scene: &mut Scene,
        input: &WindowInput,
        env: &mut PaintEnv,
    ) {
        self.inner.paint(bounds, scene, input, env);
    }

    fn mouse_down(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        self.inner.mouse_down(event, cx);
    }

    fn mouse_up(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        self.inner.mouse_up(event, cx);
    }

    fn mouse_move(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        self.inner.mouse_move(event, cx);
    }

    fn click(&mut self, event: &ClickEvent, cx: &mut WindowInput) {
        self.inner.click(event, cx);
    }

    fn scroll(&mut self, event: &ScrollEvent, cx: &mut WindowInput) {
        self.inner.scroll(event, cx);
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut WindowInput) -> bool {
        self.inner.key_down(event, cx)
    }

    fn ime_commit(&mut self, text: &str, cx: &mut WindowInput) {
        self.inner.ime_commit(text, cx);
    }
}

/// Layout + hitboxes. Does not paint.
pub fn prepare_element(
    element: &mut impl Element,
    origin: Point,
    available: Size,
    input: &mut WindowInput,
) -> Bounds {
    input.begin_hitboxes();
    let bounds = element.request_layout(origin, available);
    element.prepaint(bounds, input);
    bounds
}

/// Layout → prepaint → paint a root element into `scene`.
pub fn paint_element(
    element: &mut impl Element,
    origin: Point,
    available: Size,
    scene: &mut Scene,
) -> Bounds {
    let mut input = WindowInput::new();
    let mut atlas = SpriteAtlas::new();
    let mut env = PaintEnv {
        atlas: &mut atlas,
        text: None,
    };
    paint_element_with_input(element, origin, available, scene, &mut input, &mut env)
}

/// Like [`paint_element`], using caller pointer / focus / text state.
pub fn paint_element_with_input(
    element: &mut impl Element,
    origin: Point,
    available: Size,
    scene: &mut Scene,
    input: &mut WindowInput,
    env: &mut PaintEnv,
) -> Bounds {
    let bounds = prepare_element(element, origin, available, input);
    element.paint(bounds, scene, input, env);
    bounds
}

/// Mouse down on the topmost hitbox. Call after [`prepare_element`].
pub fn dispatch_mouse_down(element: &mut impl Element, input: &mut WindowInput, position: Point) {
    dispatch_mouse_down_button(element, input, position, MouseButton::Left);
}

pub fn dispatch_mouse_down_button(
    element: &mut impl Element,
    input: &mut WindowInput,
    position: Point,
    button: MouseButton,
) {
    input.arm_press(position);
    element.mouse_down(
        &MouseEvent {
            position,
            button,
            modifiers: input.modifiers(),
        },
        input,
    );
}

/// Mouse up. Returns whether a click fired (down+up on the same hitbox).
pub fn dispatch_mouse_up(
    element: &mut impl Element,
    input: &mut WindowInput,
    position: Point,
) -> bool {
    let clicked = input.arm_release(position);
    element.mouse_up(
        &MouseEvent {
            position,
            button: MouseButton::Left,
            modifiers: input.modifiers(),
        },
        input,
    );
    if let Some(_id) = clicked {
        element.click(&ClickEvent { position }, input);
        true
    } else {
        false
    }
}

pub fn dispatch_mouse_move(element: &mut impl Element, input: &mut WindowInput, position: Point) {
    input.arm_move(position);
    element.mouse_move(
        &MouseEvent {
            position,
            button: MouseButton::Left,
            modifiers: input.modifiers(),
        },
        input,
    );
}

pub fn dispatch_scroll(element: &mut impl Element, input: &mut WindowInput, delta: Point) {
    input.arm_move(input.pointer());
    element.scroll(
        &ScrollEvent {
            position: input.pointer(),
            delta,
            modifiers: input.modifiers(),
        },
        input,
    );
}

/// Tab cycles focus; other keys go to the focused element.
pub fn dispatch_key_down(
    element: &mut impl Element,
    input: &mut WindowInput,
    event: &KeyDownEvent,
) -> bool {
    input.set_modifiers(event.keystroke.modifiers);
    if event.key == Key::Tab {
        return input.tab(event.keystroke.modifiers.shift);
    }
    element.key_down(event, input)
}

pub fn dispatch_ime_commit(element: &mut impl Element, input: &mut WindowInput, text: &str) {
    element.ime_commit(text, input);
}

#[cfg(test)]
mod tests {
    use super::paint_element;
    use crate::Div;
    use crate::geometry::{Bounds, Hsla, Point, Size, px};
    use crate::raster::RasterBuffer;
    use crate::scene::Scene;
    use crate::style::Styled;

    /// D3.1 done-when: host test paints a single element into a Scene.
    #[test]
    fn paints_a_single_element_into_scene() {
        let mut el = Div::new().w(px(8)).h(px(8)).bg(Hsla::RED);
        let mut scene = Scene::new();
        let bounds = paint_element(&mut el, Point::ZERO, Size::wh(8, 8), &mut scene);
        assert_eq!(bounds, Bounds::from_xywh(0, 0, 8, 8));
        assert_eq!(scene.len(), 1);

        let mut buf = RasterBuffer::new(8, 8);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(7, 7), Some(Hsla::RED.to_bgra()));
    }
}
