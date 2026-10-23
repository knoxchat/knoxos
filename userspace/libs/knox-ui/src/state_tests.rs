// SPDX-License-Identifier: MIT
//! D8.1 done-when: hover / active / disabled / focus-visible paint distinctly.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering};

use knox_gpui::{
    Element, HostTextSystem, IntoElement, Key, KeyDownEvent, Modifiers, PaintEnv, Point,
    RasterBuffer, Scene, Size, SpriteAtlas, WindowInput, dispatch_key_down, dispatch_mouse_down,
    dispatch_mouse_up, paint_element_with_input, px,
};

use crate::button::Button;
use crate::checkbox::Checkbox;
use crate::link::Link;
use crate::radio::{Radio, RadioGroup};
use crate::size::Disableable;
use crate::switch::Switch;
use crate::theme::Theme;
use crate::toggle::Toggle;
use crate::{ButtonGroup, ButtonVariants};

fn paint(
    mut el: knox_gpui::Div,
    origin: Point,
    size: Size,
    input: &mut WindowInput,
) -> RasterBuffer {
    let mut atlas = SpriteAtlas::new();
    let sys = HostTextSystem;
    let mut env = PaintEnv {
        atlas: &mut atlas,
        text: Some(&sys),
    };
    let mut scene = Scene::new();
    paint_element_with_input(&mut el, origin, size, &mut scene, input, &mut env);
    let mut buf = RasterBuffer::new(size.width.0.max(1) as usize, size.height.0.max(1) as usize);
    buf.draw_with_atlas(&scene, Some(&atlas));
    buf
}

fn sample(buf: &RasterBuffer, x: i32, y: i32) -> knox_gpui::Bgra {
    buf.pixel(x, y).unwrap_or(knox_gpui::Bgra::TRANSPARENT)
}

/// D8.1 done-when: hover/active/disabled/focus-visible all distinct.
#[test]
fn button_states_are_distinct() {
    let theme = Theme::aurora_dark();
    let origin = Point::xy(8, 8);
    let canvas = Size::wh(96, 48);
    let rest_px = (12, 24);

    let rest = paint(
        Button::new("s")
            .themed(&theme)
            .primary()
            .min_w(px(72))
            .label("Go")
            .into_element(),
        origin,
        canvas,
        &mut WindowInput::new(),
    );
    let rest_c = sample(&rest, rest_px.0, rest_px.1);

    let mut hover_in = WindowInput::new();
    hover_in.set_pointer_position(Point::xy(20, 24));
    let hover = paint(
        Button::new("s")
            .themed(&theme)
            .primary()
            .min_w(px(72))
            .label("Go")
            .into_element(),
        origin,
        canvas,
        &mut hover_in,
    );
    let hover_c = sample(&hover, rest_px.0, rest_px.1);

    let mut active_in = WindowInput::new();
    let mut active_el = Button::new("s")
        .themed(&theme)
        .primary()
        .min_w(px(72))
        .label("Go")
        .into_element();
    knox_gpui::prepare_element(&mut active_el, origin, canvas, &mut active_in);
    dispatch_mouse_down(&mut active_el, &mut active_in, Point::xy(20, 24));
    let mut atlas = SpriteAtlas::new();
    let sys = HostTextSystem;
    let mut env = PaintEnv {
        atlas: &mut atlas,
        text: Some(&sys),
    };
    let mut scene = Scene::new();
    active_el.paint(active_el.computed(), &mut scene, &active_in, &mut env);
    let mut active = RasterBuffer::new(96, 48);
    active.draw_with_atlas(&scene, Some(&atlas));
    let active_c = sample(&active, rest_px.0, rest_px.1);

    let disabled = paint(
        Button::new("s")
            .themed(&theme)
            .primary()
            .min_w(px(72))
            .label("Go")
            .disabled(true)
            .into_element(),
        origin,
        canvas,
        &mut {
            let mut i = WindowInput::new();
            i.set_pointer_position(Point::xy(20, 24));
            i
        },
    );
    let disabled_c = sample(&disabled, rest_px.0, rest_px.1);

    let mut focus_in = WindowInput::new();
    let mut focus_el = Button::new("s")
        .themed(&theme)
        .primary()
        .min_w(px(72))
        .label("Go")
        .on_click(|| {})
        .into_element();
    knox_gpui::prepare_element(&mut focus_el, origin, canvas, &mut focus_in);
    dispatch_key_down(
        &mut focus_el,
        &mut focus_in,
        &KeyDownEvent::new(Key::Tab, Modifiers::NONE),
    );
    let bounds = focus_el.computed();
    let mut atlas = SpriteAtlas::new();
    let sys = HostTextSystem;
    let mut env = PaintEnv {
        atlas: &mut atlas,
        text: Some(&sys),
    };
    let mut scene = Scene::new();
    focus_el.paint(bounds, &mut scene, &focus_in, &mut env);
    let mut focused = RasterBuffer::new(96, 48);
    focused.draw_with_atlas(&scene, Some(&atlas));
    // Mid-left of the 2px halo, not a rounded corner (coverage 0).
    let hx = bounds.x() - 1;
    let hy = bounds.y() + bounds.height() / 2;
    let ring = sample(&focused, hx, hy);
    let rest_halo = sample(&rest, hx, hy);

    assert_ne!(rest_c, hover_c, "hover fill must differ from rest");
    assert_ne!(hover_c, active_c, "active fill must differ from hover");
    assert_ne!(rest_c, active_c, "active fill must differ from rest");
    assert_ne!(rest_c, disabled_c, "disabled fill must differ from rest");
    assert_ne!(hover_c, disabled_c, "disabled must ignore hover");
    assert_ne!(
        ring, rest_halo,
        "focus-visible ring must paint outside the fill"
    );
    assert!(ring.a > 0, "focus ring pixel should be inked");
}

#[test]
fn disabled_button_does_not_click() {
    let n = Arc::new(AtomicU32::new(0));
    let c = n.clone();
    let mut el = Button::new("x")
        .label("Nope")
        .disabled(true)
        .on_click(move || {
            c.fetch_add(1, Ordering::SeqCst);
        })
        .into_element();
    let mut input = WindowInput::new();
    knox_gpui::prepare_element(&mut el, Point::ZERO, Size::wh(80, 32), &mut input);
    dispatch_mouse_down(&mut el, &mut input, Point::xy(10, 10));
    dispatch_mouse_up(&mut el, &mut input, Point::xy(10, 10));
    assert_eq!(n.load(Ordering::SeqCst), 0);
}

#[test]
fn button_click_and_group_index() {
    let n = Arc::new(AtomicU32::new(0));
    let c = n.clone();
    let mut el = Button::new("ok")
        .primary()
        .label("OK")
        .on_click(move || {
            c.fetch_add(1, Ordering::SeqCst);
        })
        .into_element();
    let mut input = WindowInput::new();
    knox_gpui::prepare_element(&mut el, Point::ZERO, Size::wh(80, 32), &mut input);
    dispatch_mouse_down(&mut el, &mut input, Point::xy(8, 8));
    assert!(dispatch_mouse_up(&mut el, &mut input, Point::xy(8, 8)));
    assert_eq!(n.load(Ordering::SeqCst), 1);

    let idx = Arc::new(AtomicU32::new(99));
    let i = idx.clone();
    let mut group = ButtonGroup::new("g")
        .child(Button::new("a").label("A"))
        .child(Button::new("b").label("B"))
        .on_click(move |n| {
            i.store(n as u32, Ordering::SeqCst);
        })
        .into_element();
    let mut input = WindowInput::new();
    knox_gpui::prepare_element(&mut group, Point::ZERO, Size::wh(160, 32), &mut input);
    dispatch_mouse_down(&mut group, &mut input, Point::xy(50, 8));
    dispatch_mouse_up(&mut group, &mut input, Point::xy(50, 8));
    assert_eq!(idx.load(Ordering::SeqCst), 1);
}

#[test]
fn switch_checkbox_toggle_and_radio_group() {
    let theme = Theme::aurora_dark();
    let sw = Arc::new(AtomicU32::new(0));
    let s = sw.clone();
    let mut el = Switch::new("sw")
        .themed(&theme)
        .label("On")
        .checked(false)
        .on_change(move |v| {
            s.store(u32::from(v), Ordering::SeqCst);
        })
        .into_element();
    let mut input = WindowInput::new();
    knox_gpui::prepare_element(&mut el, Point::ZERO, Size::wh(80, 24), &mut input);
    dispatch_mouse_down(&mut el, &mut input, Point::xy(8, 8));
    dispatch_mouse_up(&mut el, &mut input, Point::xy(8, 8));
    assert_eq!(sw.load(Ordering::SeqCst), 1);

    let ck = Arc::new(AtomicU32::new(1));
    let c = ck.clone();
    let mut el = Checkbox::new("cb")
        .themed(&theme)
        .label("Ok")
        .checked(true)
        .on_change(move |v| {
            c.store(u32::from(v), Ordering::SeqCst);
        })
        .into_element();
    let mut input = WindowInput::new();
    knox_gpui::prepare_element(&mut el, Point::ZERO, Size::wh(80, 24), &mut input);
    dispatch_mouse_down(&mut el, &mut input, Point::xy(4, 8));
    dispatch_mouse_up(&mut el, &mut input, Point::xy(4, 8));
    assert_eq!(ck.load(Ordering::SeqCst), 0);

    let tg = Arc::new(AtomicU32::new(0));
    let t = tg.clone();
    let mut el = Toggle::new("tg")
        .themed(&theme)
        .label("Bold")
        .checked(false)
        .on_change(move |v| {
            t.store(u32::from(v), Ordering::SeqCst);
        })
        .into_element();
    let mut input = WindowInput::new();
    knox_gpui::prepare_element(&mut el, Point::ZERO, Size::wh(80, 32), &mut input);
    dispatch_mouse_down(&mut el, &mut input, Point::xy(8, 8));
    dispatch_mouse_up(&mut el, &mut input, Point::xy(8, 8));
    assert_eq!(tg.load(Ordering::SeqCst), 1);

    let rd = Arc::new(AtomicU32::new(0));
    let r = rd.clone();
    let mut el = RadioGroup::new("rg")
        .themed(&theme)
        .selected(Some(0))
        .child(Radio::new("a").themed(&theme).label("A"))
        .child(Radio::new("b").themed(&theme).label("B"))
        .on_change(move |i| {
            r.store(i as u32, Ordering::SeqCst);
        })
        .into_element();
    let mut input = WindowInput::new();
    knox_gpui::prepare_element(&mut el, Point::ZERO, Size::wh(200, 32), &mut input);
    let p = input
        .hitboxes()
        .iter()
        .filter(|hb| hb.tab_stop)
        .max_by_key(|hb| hb.bounds.x())
        .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + hb.bounds.height() / 2))
        .expect("radio B");
    dispatch_mouse_down(&mut el, &mut input, p);
    dispatch_mouse_up(&mut el, &mut input, p);
    assert_eq!(rd.load(Ordering::SeqCst), 1);
}

#[test]
fn link_clicks() {
    let n = Arc::new(AtomicU32::new(0));
    let c = n.clone();
    let mut el = Link::new("docs")
        .label("Docs")
        .href("https://knoxos.local")
        .on_click(move || {
            c.fetch_add(1, Ordering::SeqCst);
        })
        .into_element();
    let mut input = WindowInput::new();
    knox_gpui::prepare_element(&mut el, Point::ZERO, Size::wh(80, 20), &mut input);
    dispatch_mouse_down(&mut el, &mut input, Point::xy(4, 4));
    dispatch_mouse_up(&mut el, &mut input, Point::xy(4, 4));
    assert_eq!(n.load(Ordering::SeqCst), 1);
}
