// SPDX-License-Identifier: MIT
//! Login and lock-screen cards (D12.1 / D12.2).
//!
//! Branded GPUI panels matching the boot splash (mark, surface, fields).
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS session chrome, not a kit catalog control.

use alloc::string::String;

use knox_gpui::{Div, IntoElement, Styled, div, px, text};

use crate::alert::Alert;
use crate::avatar::Avatar;
use crate::button::{Button, ButtonVariants};
use crate::form::Field;
use crate::icon::{Icon, IconName};
use crate::interact::Handler;
use crate::size::{ControlSize, Sizable};
use crate::splash::brand_lockup;
use crate::theme::Theme;

fn field_box(
    theme: &Theme,
    value: &str,
    placeholder: &str,
    focused: bool,
    icon: IconName,
    on_click: Option<Handler>,
) -> Div {
    let border = if focused {
        theme.colors.ring
    } else {
        theme.colors.border
    };
    let empty = value.is_empty();
    let shown = if empty {
        String::from(placeholder)
    } else {
        String::from(value)
    };
    let color = if empty {
        theme.colors.muted_foreground
    } else {
        theme.colors.foreground
    };
    let mut box_ = div()
        .w(px(320))
        .h(px(36))
        .flex()
        .items_center()
        .gap_2()
        .pad_x(px(12))
        .bg(theme.colors.input)
        .border_1()
        .border_color(border)
        .rounded(theme.radius.md)
        .child(
            Icon::new(icon)
                .themed(theme)
                .color(theme.colors.muted_foreground)
                .with_size(ControlSize::Small),
        )
        .child(text(shown).color(color).size(13));
    if let Some(h) = on_click {
        box_ = box_.on_click(move |_| h());
    }
    box_
}

fn session_panel(theme: &Theme, body: Div) -> Div {
    div()
        .w(px(400))
        .flex_col()
        .items_center()
        .gap(px(16))
        .p(px(28))
        .bg(theme.colors.surface)
        .border_1()
        .border_color(theme.colors.border)
        .rounded(theme.radius.xl)
        .child(body)
}

fn session_stage(
    theme: &Theme,
    width: knox_gpui::Pixels,
    height: knox_gpui::Pixels,
    panel: Div,
) -> Div {
    div()
        .w(width)
        .h(height)
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.colors.background)
        .child(panel)
}

/// Full-screen login Dialog (username + password + Sign In).
pub struct LoginView {
    theme: Theme,
    viewport_w: knox_gpui::Pixels,
    viewport_h: knox_gpui::Pixels,
    username: String,
    password_mask: String,
    focused: u8,
    error: String,
    authenticating: bool,
    on_focus_user: Option<Handler>,
    on_focus_pass: Option<Handler>,
    on_submit: Option<Handler>,
}

impl LoginView {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            viewport_w: px(800),
            viewport_h: px(600),
            username: String::new(),
            password_mask: String::new(),
            focused: 1,
            error: String::new(),
            authenticating: false,
            on_focus_user: None,
            on_focus_pass: None,
            on_submit: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn viewport(mut self, width: knox_gpui::Pixels, height: knox_gpui::Pixels) -> Self {
        self.viewport_w = width;
        self.viewport_h = height;
        self
    }

    pub fn username(mut self, username: impl Into<String>) -> Self {
        self.username = username.into();
        self
    }

    pub fn password_mask(mut self, mask: impl Into<String>) -> Self {
        self.password_mask = mask.into();
        self
    }

    pub fn focused(mut self, field: u8) -> Self {
        self.focused = field;
        self
    }

    pub fn error(mut self, error: impl Into<String>) -> Self {
        self.error = error.into();
        self
    }

    pub fn authenticating(mut self, on: bool) -> Self {
        self.authenticating = on;
        self
    }

    pub fn on_focus_user(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_focus_user = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_focus_pass(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_focus_pass = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_submit(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_submit = Some(alloc::sync::Arc::new(f));
        self
    }
}

impl Default for LoginView {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for LoginView {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut body = div()
            .flex_col()
            .items_center()
            .gap(px(14))
            .child(brand_lockup(&self.theme, true))
            .child(
                text(String::from("Sign in to continue"))
                    .color(self.theme.colors.muted_foreground)
                    .size(13),
            );
        if !self.username.is_empty() {
            body = body.child(
                Avatar::new()
                    .themed(&self.theme)
                    .name(self.username.clone())
                    .with_size(ControlSize::Large),
            );
        }
        body = body.child(
            div()
                .flex_col()
                .gap_2()
                .child(Field::new("Username").themed(&self.theme).child(field_box(
                    &self.theme,
                    &self.username,
                    "Enter username",
                    self.focused == 0,
                    IconName::User,
                    self.on_focus_user,
                )))
                .child(Field::new("Password").themed(&self.theme).child(field_box(
                    &self.theme,
                    &self.password_mask,
                    "Enter password",
                    self.focused == 1,
                    IconName::Lock,
                    self.on_focus_pass,
                ))),
        );
        if !self.error.is_empty() {
            body = body.child(
                Alert::error(self.error)
                    .themed(&self.theme)
                    .title("Sign in failed"),
            );
        }
        let label = if self.authenticating {
            "Signing in..."
        } else {
            "Sign In"
        };
        let mut btn = Button::new("login-submit")
            .themed(&self.theme)
            .primary()
            .icon(IconName::ArrowRight)
            .label(label)
            .loading(self.authenticating)
            .min_w(px(320))
            .large();
        if let Some(h) = self.on_submit {
            btn = btn.on_click(move || h());
        }
        body = body.child(btn);
        session_stage(
            &self.theme,
            self.viewport_w,
            self.viewport_h,
            session_panel(&self.theme, body),
        )
    }
}

/// Full-screen lock Dialog (password + Unlock).
pub struct LockView {
    theme: Theme,
    viewport_w: knox_gpui::Pixels,
    viewport_h: knox_gpui::Pixels,
    username: String,
    password_mask: String,
    error: String,
    on_submit: Option<Handler>,
}

impl LockView {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            viewport_w: px(800),
            viewport_h: px(600),
            username: String::new(),
            password_mask: String::new(),
            error: String::new(),
            on_submit: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn viewport(mut self, width: knox_gpui::Pixels, height: knox_gpui::Pixels) -> Self {
        self.viewport_w = width;
        self.viewport_h = height;
        self
    }

    pub fn username(mut self, username: impl Into<String>) -> Self {
        self.username = username.into();
        self
    }

    pub fn password_mask(mut self, mask: impl Into<String>) -> Self {
        self.password_mask = mask.into();
        self
    }

    pub fn error(mut self, error: impl Into<String>) -> Self {
        self.error = error.into();
        self
    }

    pub fn on_submit(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_submit = Some(alloc::sync::Arc::new(f));
        self
    }
}

impl Default for LockView {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for LockView {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut body = div()
            .flex_col()
            .items_center()
            .gap(px(14))
            .child(brand_lockup(&self.theme, true))
            .child(
                text(String::from("Session locked"))
                    .color(self.theme.colors.muted_foreground)
                    .size(13),
            );
        if !self.username.is_empty() {
            body = body.child(
                Avatar::new()
                    .themed(&self.theme)
                    .name(self.username.clone())
                    .with_size(ControlSize::Large),
            );
        }
        body = body.child(Field::new("Password").themed(&self.theme).child(field_box(
            &self.theme,
            &self.password_mask,
            "Enter password",
            true,
            IconName::Lock,
            None,
        )));
        if !self.error.is_empty() {
            body = body.child(
                Alert::error(self.error)
                    .themed(&self.theme)
                    .title("Unlock failed"),
            );
        }
        let mut btn = Button::new("lock-unlock")
            .themed(&self.theme)
            .primary()
            .icon(IconName::Lock)
            .label("Unlock")
            .min_w(px(320))
            .large();
        if let Some(h) = self.on_submit {
            btn = btn.on_click(move || h());
        }
        body = body.child(btn);
        session_stage(
            &self.theme,
            self.viewport_w,
            self.viewport_h,
            session_panel(&self.theme, body),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{LockView, LoginView};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element, px,
    };

    #[test]
    fn login_sign_in_fires() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let mut el = LoginView::new()
            .themed(&Theme::aurora_dark())
            .viewport(px(800), px(600))
            .username("user")
            .on_submit(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(800, 600), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.height() >= 16 && hb.bounds.height() <= 48)
            .max_by_key(|hb| (hb.bounds.y(), hb.bounds.x()))
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("sign in");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert!(hit.load(Ordering::SeqCst));
    }

    #[test]
    fn lock_unlock_fires() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let mut el = LockView::new()
            .themed(&Theme::aurora_dark())
            .viewport(px(800), px(600))
            .username("user")
            .on_submit(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(800, 600), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.height() >= 16 && hb.bounds.height() <= 48)
            .max_by_key(|hb| (hb.bounds.y(), hb.bounds.x()))
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("unlock");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert!(hit.load(Ordering::SeqCst));
    }
}
