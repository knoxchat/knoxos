// SPDX-License-Identifier: MIT
//! File picker, screenshot, and wizard Dialog/Sheet flows (D11.10).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS app chrome, not a kit catalog control.

use alloc::string::String;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::interact::{Handler, IndexHandler};
use crate::list::List;
use crate::nav::Stepper;
use crate::overlay::{Dialog, Sheet};
use crate::scroll::VirtualList;
use crate::theme::Theme;

/// Open / Save / Folder picker inside a Dialog.
pub struct FilePickerDialog {
    theme: Theme,
    viewport_w: Pixels,
    viewport_h: Pixels,
    title: String,
    path: String,
    bookmarks: Vec<String>,
    bookmark_sel: Option<usize>,
    entries: Vec<String>,
    selected: Option<usize>,
    confirm: String,
    on_back: Option<Handler>,
    on_forward: Option<Handler>,
    on_up: Option<Handler>,
    on_bookmark: Option<IndexHandler>,
    on_entry: Option<IndexHandler>,
    on_confirm: Option<Handler>,
    on_dismiss: Option<Handler>,
}

impl FilePickerDialog {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            viewport_w: px(800),
            viewport_h: px(600),
            title: String::from("Open File"),
            path: String::from("/"),
            bookmarks: Vec::new(),
            bookmark_sel: None,
            entries: Vec::new(),
            selected: None,
            confirm: String::from("Open"),
            on_back: None,
            on_forward: None,
            on_up: None,
            on_bookmark: None,
            on_entry: None,
            on_confirm: None,
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn viewport(mut self, width: Pixels, height: Pixels) -> Self {
        self.viewport_w = width;
        self.viewport_h = height;
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    pub fn bookmarks(mut self, items: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.bookmarks = items.into_iter().map(Into::into).collect();
        self
    }

    pub fn bookmark_sel(mut self, i: Option<usize>) -> Self {
        self.bookmark_sel = i;
        self
    }

    pub fn entries(mut self, items: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.entries = items.into_iter().map(Into::into).collect();
        self
    }

    pub fn selected(mut self, i: Option<usize>) -> Self {
        self.selected = i;
        self
    }

    pub fn confirm_label(mut self, label: impl Into<String>) -> Self {
        self.confirm = label.into();
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

    pub fn on_up(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_up = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_bookmark(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_bookmark = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_entry(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_entry = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_confirm(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_confirm = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(alloc::sync::Arc::new(f));
        self
    }
}

impl Default for FilePickerDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for FilePickerDialog {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut nav = div().flex().gap_1().items_center();
        if let Some(h) = self.on_back {
            nav = nav.child(
                Button::new("pick-back")
                    .themed(&self.theme)
                    .ghost()
                    .label("<-")
                    .on_click(move || h()),
            );
        }
        if let Some(h) = self.on_forward {
            nav = nav.child(
                Button::new("pick-fwd")
                    .themed(&self.theme)
                    .ghost()
                    .label("->")
                    .on_click(move || h()),
            );
        }
        if let Some(h) = self.on_up {
            nav = nav.child(
                Button::new("pick-up")
                    .themed(&self.theme)
                    .ghost()
                    .label("^")
                    .on_click(move || h()),
            );
        }
        nav = nav.child(
            text(self.path.clone())
                .color(self.theme.colors.muted_foreground)
                .size(12),
        );

        let mut side = List::new()
            .themed(&self.theme)
            .items(self.bookmarks)
            .selected(self.bookmark_sel);
        if let Some(on) = self.on_bookmark {
            side = side.on_select(move |i| on(i));
        }

        let n = self.entries.len();
        let mut list = VirtualList::new()
            .themed(&self.theme)
            .count(n)
            .row_h(24)
            .size(px(480), px(320))
            .labels(self.entries)
            .selected(self.selected);
        if let Some(on) = self.on_entry {
            list = list.on_select(move |i| on(i));
        }

        let mut foot = div().flex().gap_2().justify_end();
        if let Some(h) = self.on_dismiss.clone() {
            foot = foot.child(
                Button::new("pick-cancel")
                    .themed(&self.theme)
                    .secondary()
                    .label("Cancel")
                    .on_click(move || h()),
            );
        }
        if let Some(h) = self.on_confirm {
            foot = foot.child(
                Button::new("pick-ok")
                    .themed(&self.theme)
                    .primary()
                    .label(self.confirm)
                    .on_click(move || h()),
            );
        }

        let body = div()
            .flex_col()
            .gap_2()
            .child(nav)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().w(px(140)).child(side))
                    .child(div().grow().child(list)),
            )
            .child(foot);

        let mut dlg = Dialog::new()
            .themed(&self.theme)
            .title(self.title)
            .open(true)
            .w(px(640))
            .viewport(self.viewport_w, self.viewport_h)
            .child(body);
        if let Some(h) = self.on_dismiss {
            dlg = dlg.on_dismiss(move || h());
        }
        dlg.into_element()
    }
}

/// Capture-mode chooser (Full / Window / Region).
pub struct ScreenshotDialog {
    theme: Theme,
    viewport_w: Pixels,
    viewport_h: Pixels,
    on_full: Option<Handler>,
    on_window: Option<Handler>,
    on_region: Option<Handler>,
    on_dismiss: Option<Handler>,
}

impl ScreenshotDialog {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            viewport_w: px(640),
            viewport_h: px(400),
            on_full: None,
            on_window: None,
            on_region: None,
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn viewport(mut self, width: Pixels, height: Pixels) -> Self {
        self.viewport_w = width;
        self.viewport_h = height;
        self
    }

    pub fn on_full(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_full = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_window(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_window = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_region(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_region = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(alloc::sync::Arc::new(f));
        self
    }
}

impl Default for ScreenshotDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for ScreenshotDialog {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut col = div().flex_col().gap_2();
        if let Some(h) = self.on_full {
            col = col.child(
                Button::new("shot-full")
                    .themed(&self.theme)
                    .primary()
                    .label("Full screen")
                    .on_click(move || h()),
            );
        }
        if let Some(h) = self.on_window {
            col = col.child(
                Button::new("shot-win")
                    .themed(&self.theme)
                    .secondary()
                    .label("Active window")
                    .on_click(move || h()),
            );
        }
        if let Some(h) = self.on_region {
            col = col.child(
                Button::new("shot-region")
                    .themed(&self.theme)
                    .secondary()
                    .label("Select region")
                    .on_click(move || h()),
            );
        }
        let mut dlg = Dialog::new()
            .themed(&self.theme)
            .title("Screenshot")
            .open(true)
            .w(px(280))
            .viewport(self.viewport_w, self.viewport_h)
            .child(col);
        if let Some(h) = self.on_dismiss {
            dlg = dlg.on_dismiss(move || h());
        }
        dlg.into_element()
    }
}

/// Shared OOBE / installer step card (Dialog or Sheet).
pub struct WizardFlow {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    title: String,
    steps: Vec<String>,
    step: usize,
    body: String,
    options: Vec<String>,
    selected: Option<usize>,
    back: String,
    next: String,
    sheet: bool,
    on_back: Option<Handler>,
    on_next: Option<Handler>,
    on_option: Option<IndexHandler>,
    on_dismiss: Option<Handler>,
}

impl WizardFlow {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(640),
            height: px(480),
            title: String::new(),
            steps: Vec::new(),
            step: 0,
            body: String::new(),
            options: Vec::new(),
            selected: None,
            back: String::from("Back"),
            next: String::from("Next"),
            sheet: false,
            on_back: None,
            on_next: None,
            on_option: None,
            on_dismiss: None,
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

    pub fn steps(mut self, labels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.steps = labels.into_iter().map(Into::into).collect();
        self
    }

    pub fn step(mut self, i: usize) -> Self {
        self.step = i;
        self
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    pub fn options(mut self, items: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.options = items.into_iter().map(Into::into).collect();
        self
    }

    pub fn selected(mut self, i: Option<usize>) -> Self {
        self.selected = i;
        self
    }

    pub fn back_label(mut self, label: impl Into<String>) -> Self {
        self.back = label.into();
        self
    }

    pub fn next_label(mut self, label: impl Into<String>) -> Self {
        self.next = label.into();
        self
    }

    pub fn as_sheet(mut self, sheet: bool) -> Self {
        self.sheet = sheet;
        self
    }

    pub fn on_back(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_back = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_next(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_next = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_option(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_option = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(alloc::sync::Arc::new(f));
        self
    }
}

impl Default for WizardFlow {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for WizardFlow {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut inner = div().flex_col().gap_2().w(px((self.width.0 - 48).max(200)));
        if !self.steps.is_empty() {
            inner = inner.child(
                Stepper::new()
                    .themed(&self.theme)
                    .labels(self.steps)
                    .step(self.step),
            );
        }
        if !self.body.is_empty() {
            inner = inner.child(
                text(self.body)
                    .color(self.theme.colors.muted_foreground)
                    .size(13),
            );
        }
        if !self.options.is_empty() {
            let mut list = List::new()
                .themed(&self.theme)
                .items(self.options)
                .selected(self.selected);
            if let Some(on) = self.on_option {
                list = list.on_select(move |i| on(i));
            }
            inner = inner.child(list);
        }
        let mut nav = div().flex().gap_2().justify_end();
        if let Some(h) = self.on_back {
            nav = nav.child(
                Button::new("wiz-back")
                    .themed(&self.theme)
                    .secondary()
                    .label(self.back)
                    .on_click(move || h()),
            );
        }
        if let Some(h) = self.on_next {
            nav = nav.child(
                Button::new("wiz-next")
                    .themed(&self.theme)
                    .primary()
                    .label(self.next)
                    .on_click(move || h()),
            );
        }
        inner = inner.child(nav);

        if self.sheet {
            let mut sheet = Sheet::new()
                .themed(&self.theme)
                .title(self.title)
                .open(true)
                .w(px((self.width.0 / 2).max(280)))
                .viewport(self.width, self.height)
                .child(inner);
            if let Some(h) = self.on_dismiss {
                sheet = sheet.on_dismiss(move || h());
            }
            sheet.into_element()
        } else {
            let mut dlg = Dialog::new()
                .themed(&self.theme)
                .title(self.title)
                .open(true)
                .w(px((self.width.0 - 32).max(280)))
                .viewport(self.width, self.height)
                .child(inner);
            if let Some(h) = self.on_dismiss {
                dlg = dlg.on_dismiss(move || h());
            }
            dlg.into_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FilePickerDialog, ScreenshotDialog, WizardFlow};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element, px,
    };

    #[test]
    fn picker_confirm_fires() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let mut el = FilePickerDialog::new()
            .themed(&Theme::aurora_dark())
            .viewport(px(800), px(600))
            .entries(["readme.txt"])
            .on_confirm(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(800, 600), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.height() >= 16 && hb.bounds.height() <= 40)
            .max_by_key(|hb| hb.bounds.x() + hb.bounds.y())
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("confirm");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert!(hit.load(Ordering::SeqCst));
    }

    #[test]
    fn screenshot_full_fires() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let mut el = ScreenshotDialog::new()
            .themed(&Theme::aurora_dark())
            .on_full(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(640, 400), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.height() >= 16 && hb.bounds.height() <= 40)
            .min_by_key(|hb| hb.bounds.y())
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("full");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert!(hit.load(Ordering::SeqCst));
    }

    #[test]
    fn wizard_next_fires() {
        let hit = Arc::new(AtomicUsize::new(0));
        let flag = hit.clone();
        let mut el = WizardFlow::new()
            .themed(&Theme::aurora_dark())
            .size(px(520), px(400))
            .title("Welcome")
            .steps(["Welcome", "Disk"])
            .next_label("Next")
            .on_next(move || {
                flag.fetch_add(1, Ordering::SeqCst);
            })
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(520, 400), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.height() >= 16 && hb.bounds.height() <= 40)
            .max_by_key(|hb| (hb.bounds.y(), hb.bounds.x()))
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("next");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert!(hit.load(Ordering::SeqCst) >= 1);
    }
}
