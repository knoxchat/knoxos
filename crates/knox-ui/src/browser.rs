// SPDX-License-Identifier: MIT
//! Browser chrome: URL bar + tabs + page surface (D11.7).
//! Navigation stays in `kernel/src/gui/browser.rs`.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS app view, not a kit catalog control.

use alloc::string::String;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::input::{FieldState, Input};
use crate::interact::{Handler, IndexHandler};
use crate::scroll::VirtualList;
use crate::theme::Theme;

/// Browser window body (chrome + page lines until a real engine exists).
pub struct BrowserChrome {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    title: String,
    url: FieldState,
    lines: Vec<String>,
    offset: i32,
    status: String,
    on_back: Option<Handler>,
    on_forward: Option<Handler>,
    on_reload: Option<Handler>,
    on_go: Option<Handler>,
    on_new_tab: Option<Handler>,
    on_url: Option<alloc::sync::Arc<dyn Fn(FieldState) + Send + Sync>>,
    on_line: Option<IndexHandler>,
    on_offset: Option<alloc::sync::Arc<dyn Fn(i32) + Send + Sync>>,
}

impl BrowserChrome {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(800),
            height: px(560),
            title: String::from("New Tab"),
            url: FieldState::empty(),
            lines: Vec::new(),
            offset: 0,
            status: String::new(),
            on_back: None,
            on_forward: None,
            on_reload: None,
            on_go: None,
            on_new_tab: None,
            on_url: None,
            on_line: None,
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

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn url(mut self, url: impl AsRef<str>) -> Self {
        self.url = FieldState::from(url);
        self
    }

    pub fn lines(mut self, lines: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.lines = lines.into_iter().map(Into::into).collect();
        self
    }

    pub fn offset(mut self, offset: i32) -> Self {
        self.offset = offset.max(0);
        self
    }

    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = status.into();
        self
    }

    pub fn on_back(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_back = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_forward(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_forward = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_reload(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_reload = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_go(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_go = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_new_tab(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_new_tab = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_url(mut self, f: impl Fn(FieldState) + Send + Sync + 'static) -> Self {
        self.on_url = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_line(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_line = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_offset(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_offset = Some(alloc::sync::Arc::new(f));
        self
    }
}

impl Default for BrowserChrome {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for BrowserChrome {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut tabs = div().flex().gap_1().p(self.theme.spacing.xs).items_center();
        tabs = tabs.child(
            div()
                .h(px(24))
                .p(self.theme.spacing.xs)
                .rounded(self.theme.radius.sm)
                .bg(self.theme.colors.muted.opacity(0.4))
                .child(
                    text(self.title)
                        .color(self.theme.colors.foreground)
                        .size(12),
                ),
        );
        if let Some(h) = self.on_new_tab {
            tabs = tabs.child(
                Button::new("br-new")
                    .themed(&self.theme)
                    .ghost()
                    .label("+")
                    .on_click(move || h()),
            );
        }

        let mut nav = div().flex().gap_1().p(self.theme.spacing.xs).items_center();
        if let Some(h) = self.on_back {
            nav = nav.child(
                Button::new("br-back")
                    .themed(&self.theme)
                    .ghost()
                    .label("<")
                    .on_click(move || h()),
            );
        }
        if let Some(h) = self.on_forward {
            nav = nav.child(
                Button::new("br-fwd")
                    .themed(&self.theme)
                    .ghost()
                    .label(">")
                    .on_click(move || h()),
            );
        }
        if let Some(h) = self.on_reload {
            nav = nav.child(
                Button::new("br-reload")
                    .themed(&self.theme)
                    .ghost()
                    .label("O")
                    .on_click(move || h()),
            );
        }
        let mut url = Input::new("br-url")
            .themed(&self.theme)
            .state(self.url)
            .placeholder("Search or enter URL")
            .w(px((self.width.0 - 160).max(80)))
            .grow();
        if let Some(on) = self.on_url {
            url = url.on_change(move |s| on(s));
        }
        nav = nav.child(url);
        if let Some(h) = self.on_go {
            nav = nav.child(
                Button::new("br-go")
                    .themed(&self.theme)
                    .primary()
                    .label("Go")
                    .on_click(move || h()),
            );
        }

        let body_h = px((self.height.0 - 88).max(80));
        let mut page = VirtualList::new()
            .themed(&self.theme)
            .count(self.lines.len())
            .row_h(18)
            .offset(self.offset)
            .size(self.width, body_h)
            .labels(self.lines);
        if let Some(on) = self.on_line {
            page = page.on_select(move |i| on(i));
        }
        if let Some(on) = self.on_offset {
            page = page.on_offset(move |o| on(o));
        }

        div()
            .flex_col()
            .w(self.width)
            .h(self.height)
            .bg(self.theme.colors.background)
            .child(tabs)
            .child(nav)
            .child(page)
            .child(
                div().h(px(20)).p(self.theme.spacing.xs).child(
                    text(self.status)
                        .color(self.theme.colors.muted_foreground)
                        .size(11),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::BrowserChrome;
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element, px,
    };

    #[test]
    fn go_button_fires() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let mut el = BrowserChrome::new()
            .themed(&Theme::aurora_dark())
            .size(px(480), px(240))
            .title("Home")
            .url("knoxos://home")
            .lines(["KnoxOS", "Welcome"])
            .on_go(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(480, 240), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.y() >= 28 && hb.bounds.y() < 72 && hb.bounds.height() <= 32)
            .max_by_key(|hb| hb.bounds.x())
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("go button");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert!(hit.load(Ordering::SeqCst));
    }
}
