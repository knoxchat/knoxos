// SPDX-License-Identifier: MIT
//! Dialog, AlertDialog, Sheet, Popover, HoverCard, FocusTrap, Positioner
//! (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. Menus stay drawn (no AppKit/Win32).

use alloc::string::String;
use alloc::sync::Arc;

use knox_gpui::{
    Bounds, Div, Element, IntoElement, Key, LayoutHints, MouseButton, PaintEnv, Pixels, Point,
    Scene, Size, Styled, WindowInput, div, px, text,
};

use crate::button::{Button, ButtonVariants};
use crate::interact::Handler;
use crate::theme::Theme;

/// Places `child` at an absolute offset (kit `Positioner`).
pub struct Positioner {
    offset: Point,
    child: Option<Div>,
}

impl Positioner {
    pub fn new() -> Self {
        Self {
            offset: Point::xy(0, 0),
            child: None,
        }
    }

    pub fn offset(mut self, offset: Point) -> Self {
        self.offset = offset;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(div().child(child));
        self
    }
}

impl Default for Positioner {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Positioner {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut el = div()
            .absolute()
            .left(px(self.offset.x.0))
            .top(px(self.offset.y.0));
        if let Some(child) = self.child {
            el = el.child(child);
        }
        el
    }
}

/// Restricts Tab to descendants (kit `FocusTrap`).
pub struct FocusTrap {
    inner: Div,
    on_escape: Option<Handler>,
}

impl FocusTrap {
    pub fn new(inner: Div) -> Self {
        Self {
            inner,
            on_escape: None,
        }
    }

    pub fn child(child: impl IntoElement) -> Self {
        Self {
            inner: div().child(child),
            on_escape: None,
        }
    }

    pub fn on_escape(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_escape = Some(Arc::new(f));
        self
    }
}

impl Element for FocusTrap {
    fn request_layout(&mut self, origin: Point, available: Size) -> Bounds {
        self.inner.request_layout(origin, available)
    }

    fn layout_hints(&self) -> LayoutHints {
        self.inner.layout_hints()
    }

    fn prepaint(&mut self, bounds: Bounds, cx: &mut WindowInput) {
        let start = cx.peek_next_hitbox();
        self.inner.prepaint(bounds, cx);
        let end = cx.peek_next_hitbox().saturating_sub(1);
        if end >= start {
            cx.set_tab_trap(start, end);
            let focused_in = cx.focused().is_some_and(|id| {
                let n = id.as_u32();
                n >= start && n <= end
            });
            if !focused_in && !cx.is_mouse_down() {
                let _ = cx.tab(false);
            }
        }
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

    fn mouse_down(&mut self, event: &knox_gpui::MouseEvent, cx: &mut WindowInput) {
        self.inner.mouse_down(event, cx);
    }
    fn mouse_up(&mut self, event: &knox_gpui::MouseEvent, cx: &mut WindowInput) {
        self.inner.mouse_up(event, cx);
    }
    fn mouse_move(&mut self, event: &knox_gpui::MouseEvent, cx: &mut WindowInput) {
        self.inner.mouse_move(event, cx);
    }
    fn click(&mut self, event: &knox_gpui::ClickEvent, cx: &mut WindowInput) {
        self.inner.click(event, cx);
    }
    fn scroll(&mut self, event: &knox_gpui::ScrollEvent, cx: &mut WindowInput) {
        self.inner.scroll(event, cx);
    }
    fn key_down(&mut self, event: &knox_gpui::KeyDownEvent, cx: &mut WindowInput) -> bool {
        if self.inner.key_down(event, cx) {
            return true;
        }
        if event.key == Key::Escape {
            if let Some(h) = &self.on_escape {
                h();
                return true;
            }
        }
        false
    }
    fn ime_commit(&mut self, text: &str, cx: &mut WindowInput) {
        self.inner.ime_commit(text, cx);
    }
}

fn panel(theme: &Theme, width: Pixels, title: &str, body: Div, on_dismiss: Option<Handler>) -> Div {
    let mut head = div().flex().justify_between().items_center();
    if !title.is_empty() {
        head = head.child(
            text(String::from(title))
                .color(theme.colors.foreground)
                .size(14),
        );
    }
    if let Some(dismiss) = on_dismiss.clone() {
        head = head.child(
            Button::new("overlay-x")
                .themed(theme)
                .ghost()
                .label("x")
                .on_press(move || dismiss()),
        );
    }
    let mut card = div()
        .w(width)
        .flex_col()
        .gap_2()
        .p(theme.spacing.md)
        .bg(theme.colors.surface)
        .border_1()
        .border_color(theme.colors.border)
        .rounded(theme.radius.lg)
        .child(head)
        .child(body)
        .focusable();
    if let Some(dismiss) = on_dismiss {
        card = card.on_key_down(move |e| {
            if e.key == Key::Escape {
                dismiss();
                true
            } else {
                false
            }
        });
    }
    card
}

/// Modal dialog with dimmed backdrop (kit `Dialog`).
pub struct Dialog {
    theme: Theme,
    title: String,
    open: bool,
    width: Pixels,
    viewport_w: Pixels,
    viewport_h: Pixels,
    child: Option<Div>,
    on_dismiss: Option<Handler>,
}

impl Dialog {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            title: String::new(),
            open: false,
            width: px(280),
            viewport_w: px(640),
            viewport_h: px(400),
            child: None,
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn viewport(mut self, width: Pixels, height: Pixels) -> Self {
        self.viewport_w = width;
        self.viewport_h = height;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(div().child(child));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }
}

impl Default for Dialog {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Dialog {
    type Element = Div;

    fn into_element(self) -> Div {
        if !self.open {
            return div();
        }
        let body = self.child.unwrap_or_else(div);
        let dismiss = self.on_dismiss.clone();
        let card = panel(&self.theme, self.width, &self.title, body, dismiss.clone());
        let mut dim = div()
            .w(self.viewport_w)
            .h(self.viewport_h)
            .flex()
            .items_center()
            .justify_center()
            .bg(knox_gpui::Hsla::BLACK.alpha(0.45));
        if let Some(d) = dismiss.clone() {
            dim = dim.on_mouse_down(move |e| {
                if e.button == MouseButton::Left {
                    d();
                }
            });
        }
        let trap = if let Some(d) = dismiss {
            FocusTrap::new(card).on_escape(move || d())
        } else {
            FocusTrap::new(card)
        };
        dim.child(trap)
    }
}

/// Confirm / cancel dialog (kit `AlertDialog`).
pub struct AlertDialog {
    theme: Theme,
    title: String,
    message: String,
    open: bool,
    on_dismiss: Option<Handler>,
    on_confirm: Option<Handler>,
}

impl AlertDialog {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            title: String::new(),
            message: String::new(),
            open: false,
            on_dismiss: None,
            on_confirm: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }

    pub fn on_confirm(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_confirm = Some(Arc::new(f));
        self
    }
}

impl Default for AlertDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for AlertDialog {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut actions = div().flex().gap_2().justify_end();
        if let Some(dismiss) = self.on_dismiss.clone() {
            actions = actions.child(
                Button::new("alert-cancel")
                    .themed(&self.theme)
                    .secondary()
                    .label("Cancel")
                    .on_press(move || dismiss()),
            );
        }
        if let Some(confirm) = self.on_confirm.clone() {
            actions = actions.child(
                Button::new("alert-ok")
                    .themed(&self.theme)
                    .primary()
                    .label("OK")
                    .on_press(move || confirm()),
            );
        }
        let theme = self.theme;
        let title = self.title;
        let open = self.open;
        let message = self.message;
        let dismiss = self.on_dismiss;
        Dialog::new()
            .themed(&theme)
            .title(title)
            .open(open)
            .on_dismiss(move || {
                if let Some(d) = &dismiss {
                    d();
                }
            })
            .child(
                div()
                    .flex_col()
                    .gap_2()
                    .child(text(message).color(theme.colors.muted_foreground).size(13))
                    .child(actions),
            )
            .into_element()
    }
}

/// Side panel (kit `Sheet`).
pub struct Sheet {
    theme: Theme,
    title: String,
    open: bool,
    width: Pixels,
    viewport_w: Pixels,
    viewport_h: Pixels,
    child: Option<Div>,
    on_dismiss: Option<Handler>,
}

impl Sheet {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            title: String::new(),
            open: false,
            width: px(240),
            viewport_w: px(640),
            viewport_h: px(400),
            child: None,
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn w(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn viewport(mut self, width: Pixels, height: Pixels) -> Self {
        self.viewport_w = width;
        self.viewport_h = height;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(div().child(child));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }
}

impl Default for Sheet {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Sheet {
    type Element = Div;

    fn into_element(self) -> Div {
        if !self.open {
            return div();
        }
        let body = self.child.unwrap_or_else(div);
        let card = panel(
            &self.theme,
            self.width,
            &self.title,
            body,
            self.on_dismiss.clone(),
        );
        let trap = if let Some(d) = self.on_dismiss {
            FocusTrap::new(card).on_escape(move || d())
        } else {
            FocusTrap::new(card)
        };
        div()
            .flex()
            .justify_end()
            .w(self.viewport_w)
            .h(self.viewport_h)
            .child(trap)
    }
}

/// Anchored overlay (kit `Popover`).
pub struct Popover {
    theme: Theme,
    open: bool,
    child: Option<Div>,
    on_dismiss: Option<Handler>,
}

impl Popover {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            open: false,
            child: None,
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(div().child(child));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }
}

impl Default for Popover {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Popover {
    type Element = Div;

    fn into_element(self) -> Div {
        if !self.open {
            return div();
        }
        let body = self.child.unwrap_or_else(div);
        let mut card = panel(&self.theme, px(200), "", body, self.on_dismiss.clone());
        if let Some(dismiss) = self.on_dismiss {
            card = card.on_key_down(move |e| {
                if e.key == Key::Escape {
                    dismiss();
                    true
                } else {
                    false
                }
            });
        }
        Positioner::new()
            .offset(Point::xy(0, 4))
            .child(card)
            .into_element()
    }
}

/// Hover popover (kit `HoverCard`). Controlled `open` from the host.
pub struct HoverCard {
    theme: Theme,
    open: bool,
    child: Option<Div>,
}

impl HoverCard {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            open: false,
            child: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(div().child(child));
        self
    }
}

impl Default for HoverCard {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for HoverCard {
    type Element = Div;

    fn into_element(self) -> Div {
        if !self.open {
            return div();
        }
        let body = self.child.unwrap_or_else(|| {
            div().child(text("Hover").color(self.theme.colors.foreground).size(12))
        });
        panel(&self.theme, px(180), "", body, None)
    }
}

#[cfg(test)]
mod tests {
    use super::{AlertDialog, Dialog, FocusTrap, HoverCard, Popover, Positioner, Sheet};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};
    use knox_gpui::{
        IntoElement, Key, KeyDownEvent, Modifiers, Point, Size, WindowInput, dispatch_key_down,
        prepare_element, text,
    };

    #[test]
    fn dialog_esc_dismisses() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let mut el = Dialog::new()
            .themed(&Theme::aurora_dark())
            .title("Hi")
            .open(true)
            .child(text("Body"))
            .on_dismiss(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(640, 400), &mut input);
        assert!(input.focused().is_some());
        dispatch_key_down(
            &mut el,
            &mut input,
            &KeyDownEvent::new(Key::Escape, Modifiers::NONE),
        );
        assert!(hit.load(Ordering::SeqCst));
    }

    #[test]
    fn constructs_overlay_catalog() {
        let theme = Theme::aurora_dark();
        let _ = AlertDialog::new().themed(&theme).open(true).into_element();
        let _ = Sheet::new().themed(&theme).open(true).into_element();
        let _ = Popover::new().themed(&theme).open(true).into_element();
        let _ = HoverCard::new().themed(&theme).open(true).into_element();
        let _ = Positioner::new().child(text("x")).into_element();
        let _ = FocusTrap::child(text("y"));
    }
}
