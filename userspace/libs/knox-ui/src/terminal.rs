// SPDX-License-Identifier: MIT
//! Terminal chrome: tabs only. PTY cells stay kernel-rasterized (D11.9).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS app view, not a kit catalog control.

use alloc::string::String;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px};

use crate::button::{Button, ButtonVariants};
use crate::interact::{Handler, IndexHandler};
use crate::nav::TabBar;
use crate::theme::Theme;

/// Height of the tab strip below the CSD titlebar.
pub const TERM_TAB_H: i32 = 28;

/// Terminal tab chrome.
pub struct TerminalChrome {
    theme: Theme,
    width: Pixels,
    tabs: Vec<String>,
    active: usize,
    on_tab: Option<IndexHandler>,
    on_new: Option<Handler>,
}

impl TerminalChrome {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(640),
            tabs: Vec::new(),
            active: 0,
            on_tab: None,
            on_new: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn tabs(mut self, tabs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tabs = tabs.into_iter().map(Into::into).collect();
        self
    }

    pub fn active(mut self, i: usize) -> Self {
        self.active = i;
        self
    }

    pub fn on_tab(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_tab = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_new(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_new = Some(alloc::sync::Arc::new(f));
        self
    }
}

impl Default for TerminalChrome {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for TerminalChrome {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut bar = div()
            .flex()
            .items_center()
            .gap_1()
            .w(self.width)
            .h(px(TERM_TAB_H))
            .p(self.theme.spacing.xs)
            .bg(self.theme.colors.muted.opacity(0.35));
        let mut tabs = TabBar::new("term-tabs")
            .themed(&self.theme)
            .labels(self.tabs)
            .selected(self.active);
        if let Some(on) = self.on_tab {
            tabs = tabs.on_select(move |i| on(i));
        }
        bar = bar.child(tabs);
        if let Some(h) = self.on_new {
            bar = bar.child(
                Button::new("term-new")
                    .themed(&self.theme)
                    .ghost()
                    .label("+")
                    .on_press(move || h()),
            );
        }
        bar
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalChrome;
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element, px,
    };

    #[test]
    fn tab_click_selects() {
        let hit = Arc::new(AtomicUsize::new(99));
        let flag = hit.clone();
        let mut el = TerminalChrome::new()
            .themed(&Theme::aurora_dark())
            .width(px(400))
            .tabs(["Shell 1", "Shell 2"])
            .on_tab(move |i| flag.store(i, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(400, 28), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.height() >= 16 && hb.bounds.height() <= 32)
            .max_by_key(|hb| hb.bounds.x())
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("tab");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert!(hit.load(Ordering::SeqCst) < 99);
    }
}
