// SPDX-License-Identifier: MIT
//! Flex container `div` (GPUI `div` / Styled / Interactivity).

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::element::{AnyElement, Element, IntoElement, PaintEnv};
use crate::geometry::{Bounds, Point, Size, px};
use crate::input::{
    ClickEvent, CursorStyle, HitboxId, KeyDownEvent, MouseEvent, ScrollEvent, WindowInput,
};
use crate::layout::{LayoutHints, flex_layout};
use crate::scene::{Quad, Scene};
use crate::style::{Overflow, Style, StyleRefinement, Styled};

type MouseHandler = Box<dyn FnMut(&MouseEvent) + Send>;
type ClickHandler = Box<dyn FnMut(&ClickEvent) + Send>;
type KeyHandler = Box<dyn FnMut(&KeyDownEvent) -> bool + Send>;
type ScrollHandler = Box<dyn FnMut(&ScrollEvent) + Send>;

/// Flex parent with a [`Style`], pointer refinements, and children.
pub struct Div {
    style: Style,
    hover: StyleRefinement,
    active: StyleRefinement,
    focus: StyleRefinement,
    children: Vec<AnyElement>,
    computed: Bounds,
    hitbox_id: Option<HitboxId>,
    on_mouse_down: Option<MouseHandler>,
    on_mouse_up: Option<MouseHandler>,
    on_mouse_move: Option<MouseHandler>,
    on_click: Option<ClickHandler>,
    on_key_down: Option<KeyHandler>,
    on_scroll: Option<ScrollHandler>,
    tab_stop: bool,
    cursor: CursorStyle,
}

impl Div {
    pub fn new() -> Self {
        Self {
            style: Style::default(),
            hover: StyleRefinement::EMPTY,
            active: StyleRefinement::EMPTY,
            focus: StyleRefinement::EMPTY,
            children: Vec::new(),
            computed: Bounds::ZERO,
            hitbox_id: None,
            on_mouse_down: None,
            on_mouse_up: None,
            on_mouse_move: None,
            on_click: None,
            on_key_down: None,
            on_scroll: None,
            tab_stop: false,
            cursor: CursorStyle::Arrow,
        }
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(AnyElement::new(child.into_element()));
        self
    }

    pub fn children<I, E>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: IntoElement,
    {
        for item in items {
            self.children.push(AnyElement::new(item.into_element()));
        }
        self
    }

    pub fn computed(&self) -> Bounds {
        self.computed
    }

    /// `:hover` overlay. GPUI owns pointer pseudo-states (kit contract).
    pub fn hover(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.hover = f(self.hover);
        self
    }

    pub fn active(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.active = f(self.active);
        self
    }

    pub fn focus(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.focus = f(self.focus);
        self
    }

    pub fn on_mouse_down(mut self, f: impl FnMut(&MouseEvent) + Send + 'static) -> Self {
        self.on_mouse_down = Some(Box::new(f));
        self
    }

    pub fn on_mouse_up(mut self, f: impl FnMut(&MouseEvent) + Send + 'static) -> Self {
        self.on_mouse_up = Some(Box::new(f));
        self
    }

    pub fn on_mouse_move(mut self, f: impl FnMut(&MouseEvent) + Send + 'static) -> Self {
        self.on_mouse_move = Some(Box::new(f));
        self
    }

    /// Click = mouse down + up on the same hitbox.
    pub fn on_click(mut self, f: impl FnMut(&ClickEvent) + Send + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self.tab_stop = true;
        self
    }

    pub fn on_key_down(mut self, f: impl FnMut(&KeyDownEvent) -> bool + Send + 'static) -> Self {
        self.on_key_down = Some(Box::new(f));
        self
    }

    pub fn on_scroll(mut self, f: impl FnMut(&ScrollEvent) + Send + 'static) -> Self {
        self.on_scroll = Some(Box::new(f));
        self
    }

    pub fn focusable(mut self) -> Self {
        self.tab_stop = true;
        self
    }

    pub fn cursor(mut self, cursor: CursorStyle) -> Self {
        self.cursor = cursor;
        self
    }

    fn is_interactive(&self) -> bool {
        !self.hover.is_empty()
            || !self.active.is_empty()
            || !self.focus.is_empty()
            || self.on_mouse_down.is_some()
            || self.on_mouse_up.is_some()
            || self.on_mouse_move.is_some()
            || self.on_click.is_some()
            || self.on_key_down.is_some()
            || self.on_scroll.is_some()
            || self.tab_stop
            || self.cursor != CursorStyle::Arrow
    }

    fn resolved_style(&self, input: &WindowInput) -> Style {
        self.style.resolved(
            &self.hover,
            &self.focus,
            &self.active,
            input.element_state(self.hitbox_id),
        )
    }

    fn fire_if_target(&mut self, target: Option<HitboxId>, f: impl FnOnce(&mut Self)) {
        if self.hitbox_id.is_some() && self.hitbox_id == target {
            f(self);
        }
    }
}

pub fn div() -> Div {
    Div::new()
}

impl Default for Div {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for Div {
    fn style(&self) -> &Style {
        &self.style
    }

    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl Element for Div {
    fn request_layout(&mut self, origin: Point, available: Size) -> Bounds {
        self.computed = flex_layout(&self.style, &mut self.children, origin, available);
        self.computed
    }

    fn layout_hints(&self) -> LayoutHints {
        LayoutHints::from_style(&self.style)
    }

    fn prepaint(&mut self, bounds: Bounds, cx: &mut WindowInput) {
        self.hitbox_id = if self.is_interactive() {
            Some(cx.insert_hitbox_ex(self.computed, self.cursor, self.tab_stop, false))
        } else {
            None
        };
        for child in &mut self.children {
            child.prepaint(child.computed(), cx);
        }
        let _ = bounds;
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
        let style = self.resolved_style(input);
        if input.focused() == self.hitbox_id && self.hitbox_id.is_some() {
            let ring = bounds.dilate(px(2));
            let mut quad = Quad::new(ring, crate::style::ring_color());
            quad.corner_radii = style.corner_radii;
            scene.push_quad(quad);
        }
        let mut bg = style
            .background
            .unwrap_or(crate::geometry::Hsla::TRANSPARENT);
        if style.opacity < 1.0 {
            bg = bg.opacity(style.opacity);
        }
        let has_border = !style.border_widths.is_zero() && !style.border_color.is_transparent();
        if !bg.is_transparent() || has_border {
            let mut quad = Quad::new(bounds, bg);
            quad.corner_radii = style.corner_radii;
            if has_border {
                let mut border = style.border_color;
                if style.opacity < 1.0 {
                    border = border.opacity(style.opacity);
                }
                quad.border_color = border;
                quad.border_widths = style.border_widths;
            }
            scene.push_quad(quad);
        }
        let clip = style.overflow != Overflow::Visible;
        if clip {
            scene.start_layer(bounds);
        }
        for child in &mut self.children {
            let cb = child.computed();
            child.paint(cb, scene, input, env);
        }
        if clip {
            scene.end_layer();
        }
    }

    fn mouse_down(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        for child in &mut self.children {
            child.mouse_down(event, cx);
        }
        let target = cx.dispatch_target();
        self.fire_if_target(target, |this| {
            if let Some(handler) = &mut this.on_mouse_down {
                handler(event);
            }
        });
    }

    fn mouse_up(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        for child in &mut self.children {
            child.mouse_up(event, cx);
        }
        let target = cx.dispatch_target();
        self.fire_if_target(target, |this| {
            if let Some(handler) = &mut this.on_mouse_up {
                handler(event);
            }
        });
    }

    fn mouse_move(&mut self, event: &MouseEvent, cx: &mut WindowInput) {
        for child in &mut self.children {
            child.mouse_move(event, cx);
        }
        let target = cx.dispatch_target();
        self.fire_if_target(target, |this| {
            if let Some(handler) = &mut this.on_mouse_move {
                handler(event);
            }
        });
    }

    fn click(&mut self, event: &ClickEvent, cx: &mut WindowInput) {
        for child in &mut self.children {
            child.click(event, cx);
        }
        let target = cx.dispatch_target();
        self.fire_if_target(target, |this| {
            if let Some(handler) = &mut this.on_click {
                handler(event);
            }
        });
    }

    fn scroll(&mut self, event: &ScrollEvent, cx: &mut WindowInput) {
        for child in &mut self.children {
            child.scroll(event, cx);
        }
        let target = cx.dispatch_target();
        self.fire_if_target(target, |this| {
            if let Some(handler) = &mut this.on_scroll {
                handler(event);
            }
        });
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut WindowInput) -> bool {
        for child in &mut self.children {
            if child.key_down(event, cx) {
                return true;
            }
        }
        if self.hitbox_id.is_some() && self.hitbox_id == cx.focused() {
            if let Some(handler) = &mut self.on_key_down {
                return handler(event);
            }
        }
        false
    }

    fn ime_commit(&mut self, text: &str, cx: &mut WindowInput) {
        for child in &mut self.children {
            child.ime_commit(text, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::div;
    use crate::atlas::SpriteAtlas;
    use crate::element::{
        Element, PaintEnv, dispatch_mouse_down, dispatch_mouse_up, paint_element,
        paint_element_with_input, prepare_element,
    };
    use crate::geometry::{Bounds, Hsla, Point, Size, px};
    use crate::input::WindowInput;
    use crate::raster::RasterBuffer;
    use crate::scene::Scene;
    use crate::style::Styled;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicU32, Ordering};

    /// D3.3 done-when: two child quads layout left-to-right with gap.
    #[test]
    fn two_children_row_with_gap() {
        let mut root = div()
            .flex()
            .gap_2()
            .child(div().w(px(10)).h(px(10)).bg(Hsla::RED))
            .child(div().w(px(10)).h(px(10)).bg(Hsla::BLUE));

        let mut scene = Scene::new();
        paint_element(&mut root, Point::ZERO, Size::wh(40, 10), &mut scene);
        assert_eq!(scene.len(), 2);

        let mut buf = RasterBuffer::new(40, 10);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(9, 9), Some(Hsla::RED.to_bgra()));
        // gap_2 = 8px empty between children.
        assert_eq!(buf.pixel(10, 0), Some(crate::Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(17, 0), Some(crate::Bgra::TRANSPARENT));
        assert_eq!(buf.pixel(18, 0), Some(Hsla::BLUE.to_bgra()));
        assert_eq!(buf.pixel(27, 0), Some(Hsla::BLUE.to_bgra()));
        assert_eq!(root.computed(), Bounds::from_xywh(0, 0, 28, 10));
    }

    /// D3.4 done-when: host test pointer-in-bounds selects hover bg.
    #[test]
    fn pointer_in_bounds_selects_hover_bg() {
        let mut hovered = div()
            .w(px(8))
            .h(px(8))
            .bg(Hsla::BLUE)
            .hover(|s| s.bg(Hsla::RED));

        let mut input = WindowInput::new();
        input.set_pointer_position(Point::xy(4, 4));
        let mut scene = Scene::new();
        let mut atlas = SpriteAtlas::new();
        let mut env = PaintEnv {
            atlas: &mut atlas,
            text: None,
        };
        paint_element_with_input(
            &mut hovered,
            Point::ZERO,
            Size::wh(8, 8),
            &mut scene,
            &mut input,
            &mut env,
        );
        let mut buf = RasterBuffer::new(8, 8);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Hsla::RED.to_bgra()));
        assert_eq!(buf.pixel(7, 7), Some(Hsla::RED.to_bgra()));

        input.set_pointer_position(Point::xy(20, 20));
        scene.clear();
        paint_element_with_input(
            &mut hovered,
            Point::ZERO,
            Size::wh(8, 8),
            &mut scene,
            &mut input,
            &mut env,
        );
        buf.clear(crate::Bgra::TRANSPARENT);
        buf.draw(&scene);
        assert_eq!(buf.pixel(0, 0), Some(Hsla::BLUE.to_bgra()));
    }

    #[test]
    fn active_and_focus_refinements() {
        let mut el = div()
            .w(px(8))
            .h(px(8))
            .bg(Hsla::BLUE)
            .hover(|s| s.bg(Hsla::RED))
            .focus(|s| s.bg(Hsla::YELLOW))
            .active(|s| s.bg(Hsla::GREEN));

        let mut input = WindowInput::new();
        input.set_pointer_position(Point::xy(3, 3));
        prepare_element(&mut el, Point::ZERO, Size::wh(8, 8), &mut input);
        dispatch_mouse_down(&mut el, &mut input, Point::xy(3, 3));

        let mut atlas = SpriteAtlas::new();
        let mut env = PaintEnv {
            atlas: &mut atlas,
            text: None,
        };
        let mut scene = Scene::new();
        el.paint(el.computed(), &mut scene, &input, &mut env);
        let mut buf = RasterBuffer::new(8, 8);
        buf.draw(&scene);
        // active wins over hover and focus
        assert_eq!(buf.pixel(0, 0), Some(Hsla::GREEN.to_bgra()));

        dispatch_mouse_up(&mut el, &mut input, Point::xy(3, 3));
        scene.clear();
        el.paint(el.computed(), &mut scene, &input, &mut env);
        buf.clear(crate::Bgra::TRANSPARENT);
        buf.draw(&scene);
        // still hovered + focused; focus overlays hover
        assert_eq!(buf.pixel(0, 0), Some(Hsla::YELLOW.to_bgra()));
    }

    /// D3.5 done-when: host test click increments a counter.
    #[test]
    fn click_increments_counter() {
        let n = Arc::new(AtomicU32::new(0));
        let c = n.clone();
        let mut root = div().w(px(10)).h(px(10)).on_click(move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        });

        let mut input = WindowInput::new();
        prepare_element(&mut root, Point::ZERO, Size::wh(10, 10), &mut input);
        dispatch_mouse_down(&mut root, &mut input, Point::xy(5, 5));
        assert!(dispatch_mouse_up(&mut root, &mut input, Point::xy(5, 5)));
        assert_eq!(n.load(Ordering::SeqCst), 1);

        dispatch_mouse_down(&mut root, &mut input, Point::xy(5, 5));
        assert!(!dispatch_mouse_up(&mut root, &mut input, Point::xy(50, 50)));
        assert_eq!(n.load(Ordering::SeqCst), 1);

        dispatch_mouse_down(&mut root, &mut input, Point::xy(50, 50));
        assert!(!dispatch_mouse_up(&mut root, &mut input, Point::xy(50, 50)));
        assert_eq!(n.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn mouse_down_up_move_handlers() {
        let n = Arc::new(AtomicU32::new(0));
        let a = n.clone();
        let b = n.clone();
        let c = n.clone();
        let mut root = div()
            .w(px(10))
            .h(px(10))
            .on_mouse_down(move |_| {
                a.fetch_add(1, Ordering::SeqCst);
            })
            .on_mouse_up(move |_| {
                b.fetch_add(10, Ordering::SeqCst);
            })
            .on_mouse_move(move |_| {
                c.fetch_add(100, Ordering::SeqCst);
            });

        let mut input = WindowInput::new();
        prepare_element(&mut root, Point::ZERO, Size::wh(10, 10), &mut input);
        crate::element::dispatch_mouse_move(&mut root, &mut input, Point::xy(2, 2));
        dispatch_mouse_down(&mut root, &mut input, Point::xy(2, 2));
        dispatch_mouse_up(&mut root, &mut input, Point::xy(2, 2));
        assert_eq!(n.load(Ordering::SeqCst), 111);
    }

    /// D6.2 done-when: focus ring paints around the tabbed-to button.
    #[test]
    fn tab_paints_focus_ring() {
        let mut root = div()
            .flex_col()
            .child(div().w(px(12)).h(px(8)).bg(Hsla::BLUE).on_click(|_| {}))
            .child(div().w(px(12)).h(px(8)).bg(Hsla::BLUE).on_click(|_| {}));

        let mut input = WindowInput::new();
        prepare_element(&mut root, Point::xy(4, 4), Size::wh(20, 24), &mut input);
        crate::element::dispatch_key_down(
            &mut root,
            &mut input,
            &crate::input::KeyDownEvent::new(crate::input::Key::Tab, crate::input::Modifiers::NONE),
        );
        let id = input.focused().expect("tab stop");
        let bounds = input.bounds_of(id).expect("bounds");

        let mut atlas = SpriteAtlas::new();
        let mut env = PaintEnv {
            atlas: &mut atlas,
            text: None,
        };
        let mut scene = Scene::new();
        root.paint(root.computed(), &mut scene, &input, &mut env);
        let mut buf = RasterBuffer::new(24, 28);
        buf.draw(&scene);
        let ring = crate::style::ring_color().to_bgra();
        let rx = bounds.x() - 1;
        let ry = bounds.y() - 1;
        assert_eq!(buf.pixel(rx, ry), Some(ring), "ring pixel at ({rx},{ry})");
        assert_eq!(
            buf.pixel(bounds.x(), bounds.y()),
            Some(Hsla::BLUE.to_bgra())
        );
    }
}
