// SPDX-License-Identifier: MIT
//! Editor app chrome: toolbar + tabs + D8.22 Editor (D11.5).
//! Buffer/save/open stay in `kernel/src/gui/editor.rs` + `TextEditorState`.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS app view, not a kit catalog control.

use alloc::string::String;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::chrome::Toolbar;
use crate::editor::Editor;
use crate::input::FieldState;
use crate::interact::{Handler, IndexHandler};
use crate::nav::TabBar;
use crate::theme::Theme;

/// Editor window body (Open/Save + tabs + buffer).
pub struct EditorApp {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    tabs: Vec<String>,
    active_tab: usize,
    state: FieldState,
    caret_line: usize,
    caret_col: usize,
    offset: i32,
    status: String,
    on_open: Option<Handler>,
    on_save: Option<Handler>,
    on_tab: Option<IndexHandler>,
    on_offset: Option<alloc::sync::Arc<dyn Fn(i32) + Send + Sync>>,
}

impl EditorApp {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(720),
            height: px(480),
            tabs: Vec::new(),
            active_tab: 0,
            state: FieldState::empty(),
            caret_line: 0,
            caret_col: 0,
            offset: 0,
            status: String::new(),
            on_open: None,
            on_save: None,
            on_tab: None,
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

    pub fn tabs(mut self, tabs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tabs = tabs.into_iter().map(Into::into).collect();
        self
    }

    pub fn active_tab(mut self, i: usize) -> Self {
        self.active_tab = i;
        self
    }

    pub fn state(mut self, state: FieldState) -> Self {
        self.state = state;
        self
    }

    pub fn caret(mut self, line: usize, col: usize) -> Self {
        self.caret_line = line;
        self.caret_col = col;
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

    pub fn on_open(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_open = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_save(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_save = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_tab(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_tab = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_offset(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_offset = Some(alloc::sync::Arc::new(f));
        self
    }
}

impl Default for EditorApp {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for EditorApp {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut tools = div().flex().gap_1().items_center();
        if let Some(h) = self.on_open {
            tools = tools.child(
                Button::new("ed-open")
                    .themed(&self.theme)
                    .ghost()
                    .label("Open")
                    .on_click(move || h()),
            );
        }
        if let Some(h) = self.on_save {
            tools = tools.child(
                Button::new("ed-save")
                    .themed(&self.theme)
                    .ghost()
                    .label("Save")
                    .on_click(move || h()),
            );
        }
        let toolbar = Toolbar::new().themed(&self.theme).child(tools);

        let mut tabs = TabBar::new("ed-tabs")
            .themed(&self.theme)
            .labels(self.tabs)
            .selected(self.active_tab);
        if let Some(on) = self.on_tab {
            tabs = tabs.on_select(move |i| on(i));
        }

        let body_h = px((self.height.0 - 88).max(80));
        let mut editor = Editor::new()
            .themed(&self.theme)
            .state(self.state)
            .caret(self.caret_line, self.caret_col)
            .width(self.width)
            .height(body_h)
            .offset(self.offset);
        if let Some(on) = self.on_offset {
            editor = editor.on_offset(move |o| on(o));
        }

        div()
            .flex_col()
            .w(self.width)
            .h(self.height)
            .bg(self.theme.colors.background)
            .child(toolbar)
            .child(tabs)
            .child(editor)
            .child(
                div()
                    .h(px(22))
                    .p(self.theme.spacing.xs)
                    .bg(self.theme.colors.muted.opacity(0.4))
                    .child(
                        text(self.status)
                            .color(self.theme.colors.muted_foreground)
                            .size(12),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::EditorApp;
    use crate::input::FieldState;
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element, px,
    };

    #[test]
    fn open_button_fires() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let mut el = EditorApp::new()
            .themed(&Theme::aurora_dark())
            .size(px(480), px(240))
            .tabs(["untitled"])
            .state(FieldState::from("fn main() {}"))
            .status("New file")
            .on_open(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(480, 240), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.y() < 36 && hb.bounds.height() >= 16 && hb.bounds.height() <= 32)
            .min_by_key(|hb| (hb.bounds.width(), hb.bounds.x()))
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("open button");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert!(hit.load(Ordering::SeqCst));
    }
}
