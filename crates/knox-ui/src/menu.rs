// SPDX-License-Identifier: MIT
//! PopupMenu, ContextMenu, DropdownMenu, AppMenuBar, NativeMenu
//! (GPUI kit catalog, reimplemented as drawn menus).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. No AppKit / Win32 menu hosts.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{
    Div, IntoElement, Key, KeyDownEvent, Keystroke, MouseButton, Styled, div, px, text,
};

use crate::button::{Button, ButtonVariants};
use crate::icon::{Icon, IconName};
use crate::interact::{Handler, IndexHandler};
use crate::kbd::Kbd;
use crate::size::{Selectable, Sizable};
use crate::theme::Theme;

/// One row in a drawn menu.
#[derive(Clone, Debug)]
pub struct MenuItem {
    pub label: String,
    pub accel: Option<Keystroke>,
    pub disabled: bool,
    pub separator: bool,
    pub icon: Option<IconName>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            accel: None,
            disabled: false,
            separator: false,
            icon: None,
        }
    }

    pub fn icon(mut self, name: IconName) -> Self {
        self.icon = Some(name);
        self
    }

    pub fn accel(mut self, spec: &str) -> Self {
        self.accel = Keystroke::parse(spec);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn separator() -> Self {
        Self {
            label: String::new(),
            accel: None,
            disabled: true,
            separator: true,
            icon: None,
        }
    }
}

fn fire_accel(items: &[MenuItem], event: &KeyDownEvent, on_pick: &Option<IndexHandler>) -> bool {
    for (i, item) in items.iter().enumerate() {
        if item.disabled || item.separator {
            continue;
        }
        if let Some(accel) = item.accel {
            if event.keystroke == accel {
                if let Some(pick) = on_pick {
                    pick(i);
                }
                return true;
            }
        }
    }
    false
}

fn menu_panel(
    theme: &Theme,
    id: &str,
    items: Vec<MenuItem>,
    on_pick: Option<IndexHandler>,
    on_dismiss: Option<Handler>,
) -> Div {
    let mut panel = div()
        .flex_col()
        .w(px(180))
        .bg(theme.colors.surface)
        .border_1()
        .border_color(theme.colors.border)
        .rounded(theme.radius.md)
        .p(theme.spacing.xxs)
        .focusable();
    let accels = items.clone();
    let pick_keys = on_pick.clone();
    let dismiss_keys = on_dismiss.clone();
    panel = panel.on_key_down(move |e| {
        if fire_accel(&accels, e, &pick_keys) {
            return true;
        }
        if e.key == Key::Escape {
            if let Some(d) = &dismiss_keys {
                d();
            }
            return true;
        }
        false
    });
    let row_accels = items.clone();
    for (i, item) in items.into_iter().enumerate() {
        if item.separator {
            panel = panel.child(
                div()
                    .h(px(1))
                    .w(px(164))
                    .bg(theme.colors.border)
                    .m(theme.spacing.xxs),
            );
            continue;
        }
        let mut row = div()
            .flex()
            .justify_between()
            .items_center()
            .w(px(164))
            .p(theme.spacing.xs);
        let accels = row_accels.clone();
        let pick_keys = on_pick.clone();
        let dismiss_keys = on_dismiss.clone();
        row = row.on_key_down(move |e| {
            if fire_accel(&accels, e, &pick_keys) {
                return true;
            }
            if e.key == Key::Escape {
                if let Some(d) = &dismiss_keys {
                    d();
                }
                return true;
            }
            false
        });
        let fg = if item.disabled {
            theme.colors.muted_foreground
        } else {
            theme.colors.foreground
        };
        let mut left = div().flex().items_center().gap_1();
        if let Some(name) = item.icon {
            left = left.child(Icon::new(name).themed(theme).color(fg).small());
        }
        left = left.child(text(item.label.clone()).color(fg).size(12));
        if item.disabled {
            row = row.child(left);
        } else {
            let pick = on_pick.clone();
            let dismiss = on_dismiss.clone();
            row = row.child(left).on_click(move |_| {
                if let Some(p) = &pick {
                    p(i);
                }
                if let Some(d) = &dismiss {
                    d();
                }
            });
        }
        if let Some(accel) = item.accel {
            row = row.child(Kbd::new(accel).themed(theme));
        }
        let _ = id;
        panel = panel.child(row);
    }
    panel
}

/// Anchored list of actions (kit `PopupMenu`).
pub struct PopupMenu {
    id: String,
    theme: Theme,
    items: Vec<MenuItem>,
    open: bool,
    on_pick: Option<IndexHandler>,
    on_dismiss: Option<Handler>,
}

impl PopupMenu {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            items: Vec::new(),
            open: false,
            on_pick: None,
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = MenuItem>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_pick(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_pick = Some(Arc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }
}

impl IntoElement for PopupMenu {
    type Element = Div;

    fn into_element(self) -> Div {
        if !self.open {
            return div();
        }
        menu_panel(
            &self.theme,
            &self.id,
            self.items,
            self.on_pick,
            self.on_dismiss,
        )
    }
}

/// Right-click menu over a trigger (kit `ContextMenu`).
pub struct ContextMenu {
    id: String,
    theme: Theme,
    trigger: Option<Div>,
    items: Vec<MenuItem>,
    open: bool,
    on_open: Option<Handler>,
    on_pick: Option<IndexHandler>,
    on_dismiss: Option<Handler>,
}

impl ContextMenu {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            trigger: None,
            items: Vec::new(),
            open: false,
            on_open: None,
            on_pick: None,
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn trigger(mut self, child: impl IntoElement) -> Self {
        self.trigger = Some(div().child(child));
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = MenuItem>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_open(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_open = Some(Arc::new(f));
        self
    }

    pub fn on_pick(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_pick = Some(Arc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }
}

impl IntoElement for ContextMenu {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut trigger = self.trigger.unwrap_or_else(div);
        if let Some(open) = self.on_open.clone() {
            trigger = trigger.on_mouse_down(move |e| {
                if e.button == MouseButton::Right {
                    open();
                }
            });
        }
        let mut root = div().flex_col().child(trigger);
        if self.open {
            if let Some(dismiss) = self.on_dismiss.clone() {
                root = root.child(
                    div()
                        .absolute()
                        .left(px(0))
                        .top(px(0))
                        .w(px(640))
                        .h(px(400))
                        .on_mouse_down(move |e| {
                            if e.button == MouseButton::Left || e.button == MouseButton::Right {
                                dismiss();
                            }
                        }),
                );
            }
            root = root.child(
                PopupMenu::new(self.id)
                    .themed(&self.theme)
                    .items(self.items)
                    .open(true)
                    .on_pick({
                        let pick = self.on_pick.clone();
                        move |i| {
                            if let Some(p) = &pick {
                                p(i);
                            }
                        }
                    })
                    .on_dismiss({
                        let d = self.on_dismiss.clone();
                        move || {
                            if let Some(h) = &d {
                                h();
                            }
                        }
                    }),
            );
        }
        root
    }
}

/// Button that opens a popup menu (kit `DropdownMenu`).
pub struct DropdownMenu {
    id: String,
    theme: Theme,
    label: String,
    items: Vec<MenuItem>,
    open: bool,
    on_toggle: Option<Handler>,
    on_pick: Option<IndexHandler>,
}

impl DropdownMenu {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            label: String::from("Menu"),
            items: Vec::new(),
            open: false,
            on_toggle: None,
            on_pick: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = MenuItem>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_toggle(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_toggle = Some(Arc::new(f));
        self
    }

    pub fn on_pick(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_pick = Some(Arc::new(f));
        self
    }
}

impl IntoElement for DropdownMenu {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut btn = Button::new(format!("{}-btn", self.id))
            .themed(&self.theme)
            .ghost()
            .label(format!("{} v", self.label))
            .selected(self.open);
        if let Some(toggle) = self.on_toggle.clone() {
            btn = btn.on_click(move || toggle());
        }
        let mut root = div().flex_col().child(btn);
        if self.open {
            root = root.child(
                PopupMenu::new(self.id)
                    .themed(&self.theme)
                    .items(self.items)
                    .open(true)
                    .on_pick({
                        let p = self.on_pick;
                        move |i| {
                            if let Some(h) = &p {
                                h(i);
                            }
                        }
                    })
                    .on_dismiss({
                        let t = self.on_toggle;
                        move || {
                            if let Some(h) = &t {
                                h();
                            }
                        }
                    }),
            );
        }
        root
    }
}

/// Horizontal application menus (kit `AppMenuBar`).
pub struct AppMenuBar {
    id: String,
    theme: Theme,
    titles: Vec<String>,
    menus: Vec<Vec<MenuItem>>,
    open: Option<usize>,
    on_open: Option<IndexHandler>,
    on_pick: Option<Arc<dyn Fn(usize, usize) + Send + Sync>>,
    on_dismiss: Option<Handler>,
}

impl AppMenuBar {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            theme: Theme::default(),
            titles: Vec::new(),
            menus: Vec::new(),
            open: None,
            on_open: None,
            on_pick: None,
            on_dismiss: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn menu(
        mut self,
        title: impl Into<String>,
        items: impl IntoIterator<Item = MenuItem>,
    ) -> Self {
        self.titles.push(title.into());
        self.menus.push(items.into_iter().collect());
        self
    }

    pub fn open(mut self, open: Option<usize>) -> Self {
        self.open = open;
        self
    }

    pub fn on_open(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_open = Some(Arc::new(f));
        self
    }

    pub fn on_pick(mut self, f: impl Fn(usize, usize) + Send + Sync + 'static) -> Self {
        self.on_pick = Some(Arc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }
}

impl IntoElement for AppMenuBar {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut bar = div()
            .flex()
            .gap_1()
            .items_center()
            .w(px(400))
            .h(px(28))
            .bg(self.theme.colors.surface)
            .border_1()
            .border_color(self.theme.colors.border);
        for (i, title) in self.titles.iter().enumerate() {
            let open = self.on_open.clone();
            bar = bar.child(
                Button::new(format!("{}-m{i}", self.id))
                    .themed(&self.theme)
                    .ghost()
                    .small()
                    .label(title.clone())
                    .selected(self.open == Some(i))
                    .on_click(move || {
                        if let Some(h) = &open {
                            h(i);
                        }
                    }),
            );
        }
        let mut root = div().flex_col().child(bar);
        if let Some(idx) = self.open {
            if let Some(items) = self.menus.get(idx).cloned() {
                let pick = self.on_pick.clone();
                root = root.child(
                    PopupMenu::new(format!("{}-p{idx}", self.id))
                        .themed(&self.theme)
                        .items(items)
                        .open(true)
                        .on_pick(move |item| {
                            if let Some(h) = &pick {
                                h(idx, item);
                            }
                        })
                        .on_dismiss({
                            let d = self.on_dismiss;
                            move || {
                                if let Some(h) = &d {
                                    h();
                                }
                            }
                        }),
                );
            }
        }
        root
    }
}

/// Drawn stand-in for a platform menu (kit `NativeMenu` on Linux).
pub struct NativeMenu {
    inner: PopupMenu,
}

impl NativeMenu {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            inner: PopupMenu::new(id),
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.inner = self.inner.themed(theme);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = MenuItem>) -> Self {
        self.inner = self.inner.items(items);
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.inner = self.inner.open(open);
        self
    }

    pub fn on_pick(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.on_pick(f);
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.inner = self.inner.on_dismiss(f);
        self
    }
}

impl IntoElement for NativeMenu {
    type Element = Div;

    fn into_element(self) -> Div {
        self.inner.into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{AppMenuBar, ContextMenu, DropdownMenu, MenuItem, NativeMenu, PopupMenu};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use knox_gpui::{
        IntoElement, Key, KeyDownEvent, Modifiers, MouseButton, Point, Size, WindowInput,
        dispatch_key_down, dispatch_mouse_down_button, prepare_element, text,
    };

    fn items() -> [MenuItem; 3] {
        [
            MenuItem::new("Save").accel("cmd-s"),
            MenuItem::separator(),
            MenuItem::new("Quit").accel("cmd-q"),
        ]
    }

    #[test]
    fn accelerator_fires_on_open_popup() {
        let hit = Arc::new(AtomicUsize::new(99));
        let flag = hit.clone();
        let mut el = PopupMenu::new("file")
            .themed(&Theme::aurora_dark())
            .items(items())
            .open(true)
            .on_pick(move |i| flag.store(i, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(200, 120), &mut input);
        knox_gpui::dispatch_mouse_down(&mut el, &mut input, Point::xy(8, 8));
        dispatch_key_down(
            &mut el,
            &mut input,
            &KeyDownEvent::new(Key::Char('s'), Modifiers::new(false, false, false, true)),
        );
        assert_eq!(hit.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn right_click_opens_without_left_leak() {
        let opened = Arc::new(AtomicBool::new(false));
        let flag = opened.clone();
        let mut el = ContextMenu::new("ctx")
            .themed(&Theme::aurora_dark())
            .trigger(text("Region"))
            .items(items())
            .open(false)
            .on_open(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(200, 80), &mut input);
        dispatch_mouse_down_button(&mut el, &mut input, Point::xy(4, 4), MouseButton::Right);
        assert!(opened.load(Ordering::SeqCst));
    }

    #[test]
    fn constructs_menu_catalog() {
        let theme = Theme::aurora_dark();
        let _ = DropdownMenu::new("d")
            .themed(&theme)
            .items(items())
            .into_element();
        let _ = AppMenuBar::new("bar")
            .themed(&theme)
            .menu("File", items())
            .into_element();
        let _ = NativeMenu::new("os")
            .themed(&theme)
            .items(items())
            .open(true)
            .into_element();
    }
}
