// SPDX-License-Identifier: MIT
//! Shared utility-app chrome for D11.6 (logs, disks, BT, archive, …).
//! Domain stays in the matching `kernel/src/gui/*` modules.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS app view, not a kit catalog control.

use alloc::string::String;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::calendar::{Calendar, Date};
use crate::image::{Image, ObjectFit};
use crate::interact::IndexHandler;
use crate::nav::TabBar;
use crate::progress::Progress;
use crate::scroll::VirtualList;
use crate::theme::Theme;

/// One utility window body: actions + optional tabs/list/calendar/image.
pub struct UtilityApp {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    actions: Vec<String>,
    tabs: Vec<String>,
    tab: usize,
    rows: Vec<String>,
    selected: Option<usize>,
    offset: i32,
    status: String,
    date: Option<(Date, Date)>,
    progress: Option<i32>,
    image: Option<(u32, u32, Vec<u8>)>,
    on_action: Option<IndexHandler>,
    on_tab: Option<IndexHandler>,
    on_row: Option<IndexHandler>,
    on_offset: Option<alloc::sync::Arc<dyn Fn(i32) + Send + Sync>>,
    on_date: Option<alloc::sync::Arc<dyn Fn(Date) + Send + Sync>>,
    on_view: Option<alloc::sync::Arc<dyn Fn(Date) + Send + Sync>>,
}

impl UtilityApp {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(640),
            height: px(420),
            actions: Vec::new(),
            tabs: Vec::new(),
            tab: 0,
            rows: Vec::new(),
            selected: None,
            offset: 0,
            status: String::new(),
            date: None,
            progress: None,
            image: None,
            on_action: None,
            on_tab: None,
            on_row: None,
            on_offset: None,
            on_date: None,
            on_view: None,
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

    pub fn actions(mut self, actions: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.actions = actions.into_iter().map(Into::into).collect();
        self
    }

    pub fn tabs(mut self, tabs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tabs = tabs.into_iter().map(Into::into).collect();
        self
    }

    pub fn tab(mut self, tab: usize) -> Self {
        self.tab = tab;
        self
    }

    pub fn rows(mut self, rows: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.rows = rows.into_iter().map(Into::into).collect();
        self
    }

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
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

    pub fn calendar(mut self, view: Date, selected: Date) -> Self {
        self.date = Some((view, selected));
        self
    }

    pub fn progress(mut self, pct: i32) -> Self {
        self.progress = Some(pct.clamp(0, 100));
        self
    }

    pub fn image(mut self, w: u32, h: u32, pixels: Vec<u8>) -> Self {
        self.image = Some((w, h, pixels));
        self
    }

    pub fn on_action(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_action = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_tab(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_tab = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_row(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_row = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_offset(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_offset = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_date(mut self, f: impl Fn(Date) + Send + Sync + 'static) -> Self {
        self.on_date = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_view(mut self, f: impl Fn(Date) + Send + Sync + 'static) -> Self {
        self.on_view = Some(alloc::sync::Arc::new(f));
        self
    }
}

impl Default for UtilityApp {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for UtilityApp {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut col = div()
            .flex_col()
            .w(self.width)
            .h(self.height)
            .bg(self.theme.colors.background);

        if !self.actions.is_empty() {
            let mut bar = div().flex().gap_1().p(self.theme.spacing.xs).items_center();
            for (i, label) in self.actions.into_iter().enumerate() {
                let fire = self.on_action.clone();
                bar = bar.child(
                    Button::new(alloc::format!("util-act-{i}"))
                        .themed(&self.theme)
                        .ghost()
                        .label(label)
                        .on_click(move || {
                            if let Some(f) = &fire {
                                f(i);
                            }
                        }),
                );
            }
            col = col.child(bar);
        }

        if !self.tabs.is_empty() {
            let mut tabs = TabBar::new("util-tabs")
                .themed(&self.theme)
                .labels(self.tabs)
                .selected(self.tab);
            if let Some(on) = self.on_tab {
                tabs = tabs.on_select(move |i| on(i));
            }
            col = col.child(tabs);
        }

        if let Some(pct) = self.progress {
            col = col.child(
                Progress::new()
                    .themed(&self.theme)
                    .value(pct as f32)
                    .w(px((self.width.0 - 24).max(80))),
            );
        }

        if let Some((view, selected)) = self.date {
            let mut cal = Calendar::new()
                .themed(&self.theme)
                .view(view)
                .selected(selected);
            if let Some(on) = self.on_date {
                cal = cal.on_select(move |d| on(d));
            }
            if let Some(on) = self.on_view {
                cal = cal.on_view(move |d| on(d));
            }
            col = col.child(cal);
        }

        if let Some((iw, ih, pixels)) = self.image {
            if iw > 0 && ih > 0 && !pixels.is_empty() {
                col = col.child(
                    Image::new()
                        .themed(&self.theme)
                        .size(px((self.width.0 - 16).max(32)), px(220))
                        .object_fit(ObjectFit::Contain)
                        .pixels(iw, ih, pixels),
                );
            }
        }

        let body_h = px((self.height.0 - 96).max(80));
        if !self.rows.is_empty() {
            let n = self.rows.len();
            let mut list = VirtualList::new()
                .themed(&self.theme)
                .count(n)
                .row_h(22)
                .offset(self.offset)
                .size(self.width, body_h)
                .labels(self.rows)
                .selected(self.selected);
            if let Some(on) = self.on_row {
                list = list.on_select(move |i| on(i));
            }
            if let Some(on) = self.on_offset {
                list = list.on_offset(move |o| on(o));
            }
            col = col.child(list);
        }

        if !self.status.is_empty() {
            col = col.child(
                div().h(px(22)).p(self.theme.spacing.xs).child(
                    text(self.status)
                        .color(self.theme.colors.muted_foreground)
                        .size(12),
                ),
            );
        }
        col
    }
}

#[cfg(test)]
mod tests {
    use super::UtilityApp;
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element, px,
    };

    #[test]
    fn action_click_fires() {
        let hit = Arc::new(AtomicUsize::new(99));
        let flag = hit.clone();
        let mut el = UtilityApp::new()
            .themed(&Theme::aurora_dark())
            .size(px(400), px(200))
            .actions(["All", "Err"])
            .rows(["line 0", "line 1"])
            .on_action(move |i| flag.store(i, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(400, 200), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.y() < 36 && hb.bounds.height() >= 16 && hb.bounds.height() <= 32)
            .min_by_key(|hb| (hb.bounds.width(), hb.bounds.x()))
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("action");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert_eq!(hit.load(Ordering::SeqCst), 0);
    }
}
