// SPDX-License-Identifier: MIT
//! Settings widget: SettingPage / Group / Item (GPUI kit catalog).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px, text};

use crate::chrome::Sidebar;
use crate::interact::IndexHandler;
use crate::scroll::ScrollArea;
use crate::switch::Switch;
use crate::theme::Theme;

type ItemHandler = Arc<dyn Fn(usize, usize, usize) + Send + Sync>;
type ToggleHandler = Arc<dyn Fn(usize, usize, usize, bool) + Send + Sync>;

/// One settings row.
#[derive(Clone, Debug)]
pub struct SettingItem {
    pub label: String,
    pub hint: String,
    pub toggle: Option<bool>,
    pub value: String,
    pub selected: bool,
}

impl SettingItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            hint: String::new(),
            toggle: None,
            value: String::new(),
            selected: false,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn toggle(mut self, on: bool) -> Self {
        self.toggle = Some(on);
        self
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

/// Named group of items.
#[derive(Clone, Debug)]
pub struct SettingGroup {
    pub title: String,
    pub items: Vec<SettingItem>,
}

impl SettingGroup {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            items: Vec::new(),
        }
    }

    pub fn item(mut self, item: SettingItem) -> Self {
        self.items.push(item);
        self
    }
}

/// One sidebar page.
#[derive(Clone, Debug)]
pub struct SettingPage {
    pub title: String,
    pub groups: Vec<SettingGroup>,
}

impl SettingPage {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            groups: Vec::new(),
        }
    }

    pub fn group(mut self, group: SettingGroup) -> Self {
        self.groups.push(group);
        self
    }
}

/// Settings shell: sidebar pages + body.
pub struct SettingsWidget {
    theme: Theme,
    pages: Vec<SettingPage>,
    selected: usize,
    width: Pixels,
    height: Pixels,
    scroll: i32,
    on_page: Option<IndexHandler>,
    on_toggle: Option<ToggleHandler>,
    on_item: Option<ItemHandler>,
    on_scroll: Option<Arc<dyn Fn(i32) + Send + Sync>>,
}

impl SettingsWidget {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            pages: Vec::new(),
            selected: 0,
            width: px(640),
            height: px(480),
            scroll: 0,
            on_page: None,
            on_toggle: None,
            on_item: None,
            on_scroll: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn page(mut self, page: SettingPage) -> Self {
        self.pages.push(page);
        self
    }

    pub fn pages(mut self, pages: impl IntoIterator<Item = SettingPage>) -> Self {
        self.pages.extend(pages);
        self
    }

    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn scroll(mut self, offset: i32) -> Self {
        self.scroll = offset.max(0);
        self
    }

    pub fn on_page(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_page = Some(Arc::new(f));
        self
    }

    pub fn on_toggle(
        mut self,
        f: impl Fn(usize, usize, usize, bool) + Send + Sync + 'static,
    ) -> Self {
        self.on_toggle = Some(Arc::new(f));
        self
    }

    pub fn on_item(mut self, f: impl Fn(usize, usize, usize) + Send + Sync + 'static) -> Self {
        self.on_item = Some(Arc::new(f));
        self
    }

    pub fn on_scroll(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_scroll = Some(Arc::new(f));
        self
    }
}

impl Default for SettingsWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for SettingsWidget {
    type Element = Div;

    fn into_element(self) -> Div {
        let titles: Vec<String> = self.pages.iter().map(|p| p.title.clone()).collect();
        let mut sidebar = Sidebar::new()
            .themed(&self.theme)
            .w(px(160))
            .h(self.height)
            .items(titles)
            .selected(self.selected);
        if let Some(on) = self.on_page.clone() {
            sidebar = sidebar.on_select(move |i| on(i));
        }
        let page = self.pages.get(self.selected);
        let mut body = div().flex_col().gap_2().p(self.theme.spacing.sm);
        if let Some(page) = page {
            body = body.child(
                text(page.title.clone())
                    .color(self.theme.colors.foreground)
                    .size(16),
            );
            for (gi, group) in page.groups.iter().enumerate() {
                body = body.child(
                    text(group.title.clone())
                        .color(self.theme.colors.muted_foreground)
                        .size(12),
                );
                for (ii, item) in group.items.iter().enumerate() {
                    let mut row = div().flex().justify_between().items_center().w(px((self
                        .width
                        .0
                        - 180)
                        .max(240)));
                    if item.selected {
                        row = row
                            .bg(self.theme.colors.accent.opacity(0.35))
                            .rounded(self.theme.radius.sm);
                    }
                    row = row.child(
                        div()
                            .flex_col()
                            .grow()
                            .child(
                                text(item.label.clone())
                                    .color(self.theme.colors.foreground)
                                    .size(13),
                            )
                            .child(
                                text(item.hint.clone())
                                    .color(self.theme.colors.muted_foreground)
                                    .size(11),
                            ),
                    );
                    if !item.value.is_empty() {
                        row = row.child(
                            text(item.value.clone())
                                .color(self.theme.colors.muted_foreground)
                                .size(12),
                        );
                    }
                    if let Some(on) = item.toggle {
                        let page_i = self.selected;
                        let fire = self.on_toggle.clone();
                        row = row.child(
                            Switch::new(alloc::format!("set-{page_i}-{gi}-{ii}"))
                                .themed(&self.theme)
                                .checked(on)
                                .on_change(move |v| {
                                    if let Some(cb) = &fire {
                                        cb(page_i, gi, ii, v);
                                    }
                                }),
                        );
                    } else if let Some(fire) = self.on_item.clone() {
                        let page_i = self.selected;
                        row = row.on_click(move |_| fire(page_i, gi, ii));
                    }
                    body = body.child(row);
                }
            }
        }
        let body_w = px((self.width.0 - 168).max(200));
        let mut scroller = ScrollArea::new()
            .themed(&self.theme)
            .size(body_w, self.height)
            .offset(self.scroll)
            .child(body);
        if let Some(on) = self.on_scroll {
            scroller = scroller.on_offset(move |v| on(v));
        }
        div()
            .flex()
            .w(self.width)
            .h(self.height)
            .bg(self.theme.colors.background)
            .child(sidebar)
            .child(scroller)
    }
}

#[cfg(test)]
mod tests {
    use super::{SettingGroup, SettingItem, SettingPage, SettingsWidget};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element,
    };

    #[test]
    fn sidebar_pages_switch() {
        let hit = Arc::new(AtomicUsize::new(0));
        let flag = hit.clone();
        let display = SettingPage::new("Display")
            .group(SettingGroup::new("Screen").item(SettingItem::new("Night light").toggle(false)));
        let sound = SettingPage::new("Sound")
            .group(SettingGroup::new("Output").item(SettingItem::new("Mute").toggle(true)));
        let mut el = SettingsWidget::new()
            .themed(&Theme::aurora_dark())
            .page(display)
            .page(sound)
            .selected(0)
            .on_page(move |i| flag.store(i, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(480, 240), &mut input);
        // Second sidebar item is below the first (~24px control).
        dispatch_mouse_down(&mut el, &mut input, Point::xy(40, 40));
        dispatch_mouse_up(&mut el, &mut input, Point::xy(40, 40));
        assert_eq!(hit.load(Ordering::SeqCst), 1);
    }
}
