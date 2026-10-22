// SPDX-License-Identifier: MIT
//! Session notification center + toast stack (D9.4).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS shell chrome, not a kit catalog control. Toast cards
//! reuse the D8.10 catalog skin (`Toast` / `NotificationList`).

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, FluentBuilder, FontId, IntoElement, Key, Pixels, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::empty::Empty;
use crate::icon::{Icon, IconName};
use crate::interact::Handler;
use crate::overlay::FocusTrap;
use crate::progress::Progress;
use crate::scroll::ScrollArea;
use crate::separator::Separator;
use crate::size::{ControlSize, Selectable, Sizable};
use crate::switch::Switch;
use crate::theme::Theme;
use crate::toast::{NotificationList, Toast, ToastData};

type IdHandler = Arc<dyn Fn(u32) + Send + Sync>;
type ActionHandler = Arc<dyn Fn(u32, &str) + Send + Sync>;
type NameHandler = Arc<dyn Fn(&str) + Send + Sync>;
type OffsetHandler = Arc<dyn Fn(i32) + Send + Sync>;
type BoolHandler = Arc<dyn Fn(bool) + Send + Sync>;

/// Visual role for a session notification (maps kernel `NotificationIcon`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotifyKind {
    #[default]
    Info,
    Success,
    Warning,
    Error,
    System,
    App,
    Network,
    Battery,
    Volume,
    Update,
}

impl NotifyKind {
    pub fn icon(self) -> IconName {
        match self {
            Self::Info => IconName::Info,
            Self::Success => IconName::Check,
            Self::Warning | Self::Error => IconName::Alert,
            Self::System | Self::App => IconName::Bell,
            Self::Network => IconName::Globe,
            Self::Battery => IconName::Star,
            Self::Volume => IconName::Settings,
            Self::Update => IconName::Plus,
        }
    }
}

/// Action chip on a toast / center row.
#[derive(Clone, Debug)]
pub struct NotifyAction {
    pub label: String,
    pub action_id: String,
    pub primary: bool,
}

impl NotifyAction {
    pub fn new(label: impl Into<String>, action_id: impl Into<String>, primary: bool) -> Self {
        Self {
            label: label.into(),
            action_id: action_id.into(),
            primary,
        }
    }
}

/// One notification in the session stack or center list.
#[derive(Clone, Debug)]
pub struct NotifyItem {
    pub id: u32,
    pub title: String,
    pub body: String,
    pub app_name: String,
    pub age: String,
    pub kind: NotifyKind,
    pub unread: bool,
    pub progress: Option<u8>,
    pub actions: Vec<NotifyAction>,
}

impl NotifyItem {
    pub fn new(id: u32, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            body: body.into(),
            app_name: String::from("System"),
            age: String::from("now"),
            kind: NotifyKind::Info,
            unread: true,
            progress: None,
            actions: Vec::new(),
        }
    }

    pub fn app(mut self, name: impl Into<String>) -> Self {
        self.app_name = name.into();
        self
    }

    pub fn age(mut self, age: impl Into<String>) -> Self {
        self.age = age.into();
        self
    }

    pub fn kind(mut self, kind: NotifyKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn unread(mut self, unread: bool) -> Self {
        self.unread = unread;
        self
    }

    pub fn progress(mut self, value: u8) -> Self {
        self.progress = Some(value.min(100));
        self
    }

    pub fn actions(mut self, actions: impl IntoIterator<Item = NotifyAction>) -> Self {
        self.actions = actions.into_iter().take(3).collect();
        self
    }

    /// Backing-store height for a toast card (title, body, optional extras).
    pub fn card_height(&self) -> i32 {
        let mut h = ToastStack::CARD_H;
        if self.progress.is_some() {
            h += 14;
        }
        if !self.actions.is_empty() {
            h += 28;
        }
        h
    }

    fn to_toast_data(&self) -> ToastData {
        let mut msg = self.body.clone();
        if !self.app_name.is_empty() {
            if !msg.is_empty() {
                msg.push(' ');
            }
            msg.push('·');
            msg.push(' ');
            msg.push_str(&self.app_name);
            if !self.age.is_empty() {
                msg.push(' ');
                msg.push_str(&self.age);
            }
        }
        ToastData::new(self.title.clone(), msg, 1)
    }
}

/// Stacked ephemeral toasts (top-right session overlay).
pub struct ToastStack {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    items: Vec<NotifyItem>,
    on_dismiss: Option<IdHandler>,
    on_action: Option<ActionHandler>,
}

impl ToastStack {
    pub const WIDTH: i32 = 336;
    pub const CARD_H: i32 = 84;
    pub const GAP: i32 = 8;
    pub const MAX: usize = 5;

    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(Self::WIDTH),
            height: px(Self::CARD_H),
            items: Vec::new(),
            on_dismiss: None,
            on_action: None,
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

    pub fn items(mut self, items: impl IntoIterator<Item = NotifyItem>) -> Self {
        self.items = items.into_iter().take(Self::MAX).collect();
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn(u32) + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }

    pub fn on_action(mut self, f: impl Fn(u32, &str) + Send + Sync + 'static) -> Self {
        self.on_action = Some(Arc::new(f));
        self
    }

    /// Pixel size of a stack with `items` (1×1 when empty).
    pub fn stack_size(items: &[NotifyItem]) -> (i32, i32) {
        if items.is_empty() {
            return (1, 1);
        }
        let n = items.len().min(Self::MAX);
        let slice = &items[..n];
        let h = slice.iter().map(NotifyItem::card_height).sum::<i32>() + (n as i32 - 1) * Self::GAP;
        (Self::WIDTH, h.max(1))
    }
}

impl Default for ToastStack {
    fn default() -> Self {
        Self::new()
    }
}

fn extras_row(
    theme: &Theme,
    item: &NotifyItem,
    on_action: Option<ActionHandler>,
    width: Pixels,
) -> Option<Div> {
    let mut row = div().flex_col().gap_1();
    let mut any = false;
    if let Some(p) = item.progress {
        any = true;
        row = row.child(
            Progress::new()
                .themed(theme)
                .value(p as f32)
                .w(width)
                .with_size(ControlSize::Small),
        );
    }
    if !item.actions.is_empty() {
        any = true;
        let mut actions = div().flex().gap_1();
        for (j, act) in item.actions.iter().enumerate() {
            let id = format!("notify-act-{}-{}", item.id, j);
            let mut btn = Button::new(id)
                .themed(theme)
                .label(act.label.clone())
                .small()
                .compact();
            btn = if act.primary {
                btn.primary()
            } else {
                btn.ghost().outline()
            };
            if let Some(h) = on_action.clone() {
                let nid = item.id;
                let aid = act.action_id.clone();
                btn = btn.on_click(move || h(nid, &aid));
            }
            actions = actions.child(btn);
        }
        row = row.child(actions);
    }
    any.then_some(row)
}

fn toast_card(
    theme: &Theme,
    item: NotifyItem,
    on_dismiss: Option<IdHandler>,
    on_action: Option<ActionHandler>,
) -> Div {
    let extras = extras_row(theme, &item, on_action, px(ToastStack::WIDTH - 56));
    let mut card = Toast::new(item.to_toast_data())
        .themed(theme)
        .index(item.id as usize);
    if let Some(h) = on_dismiss.clone() {
        let id = item.id;
        card = card.on_dismiss(move |_| h(id));
    }
    let mut el = card.into_element().w(px(ToastStack::WIDTH));
    if let Some(extra) = extras {
        el = div()
            .flex_col()
            .w(px(ToastStack::WIDTH))
            .child(el)
            .child(extra.p(theme.spacing.sm));
    }
    el
}

impl IntoElement for ToastStack {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut stack = div()
            .w(self.width)
            .h(self.height)
            .flex_col()
            .gap(px(Self::GAP));
        if self.items.is_empty() {
            return stack;
        }
        let data: Vec<ToastData> = self.items.iter().map(NotifyItem::to_toast_data).collect();
        if self
            .items
            .iter()
            .all(|i| i.progress.is_none() && i.actions.is_empty())
        {
            let mut list = NotificationList::new().themed(&self.theme).items(data);
            if let Some(h) = self.on_dismiss.clone() {
                let ids: Vec<u32> = self.items.iter().map(|i| i.id).collect();
                list = list.on_dismiss(move |idx| {
                    if let Some(id) = ids.get(idx).copied() {
                        h(id);
                    }
                });
            }
            return stack.child(list);
        }
        for item in self.items {
            stack = stack.child(toast_card(
                &self.theme,
                item,
                self.on_dismiss.clone(),
                self.on_action.clone(),
            ));
        }
        stack
    }
}

/// Session notification center (tray flyout).
pub struct NotificationCenter {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    items: Vec<NotifyItem>,
    unread: u32,
    dnd: bool,
    group_by_app: bool,
    collapsed: Vec<String>,
    scroll: i32,
    on_dismiss: Option<IdHandler>,
    on_read: Option<IdHandler>,
    on_action: Option<ActionHandler>,
    on_clear: Option<Handler>,
    on_dnd: Option<BoolHandler>,
    on_group: Option<Handler>,
    on_toggle_app: Option<NameHandler>,
    on_scroll: Option<OffsetHandler>,
    on_dismiss_panel: Option<Handler>,
}

impl NotificationCenter {
    pub const WIDTH: i32 = 360;
    pub const HEIGHT: i32 = 440;

    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(Self::WIDTH),
            height: px(Self::HEIGHT),
            items: Vec::new(),
            unread: 0,
            dnd: false,
            group_by_app: false,
            collapsed: Vec::new(),
            scroll: 0,
            on_dismiss: None,
            on_read: None,
            on_action: None,
            on_clear: None,
            on_dnd: None,
            on_group: None,
            on_toggle_app: None,
            on_scroll: None,
            on_dismiss_panel: None,
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

    pub fn items(mut self, items: impl IntoIterator<Item = NotifyItem>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    pub fn unread(mut self, n: u32) -> Self {
        self.unread = n;
        self
    }

    pub fn dnd(mut self, on: bool) -> Self {
        self.dnd = on;
        self
    }

    pub fn group_by_app(mut self, on: bool) -> Self {
        self.group_by_app = on;
        self
    }

    pub fn collapsed(mut self, names: impl IntoIterator<Item = String>) -> Self {
        self.collapsed = names.into_iter().collect();
        self
    }

    pub fn scroll(mut self, offset: i32) -> Self {
        self.scroll = offset.max(0);
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn(u32) + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }

    pub fn on_read(mut self, f: impl Fn(u32) + Send + Sync + 'static) -> Self {
        self.on_read = Some(Arc::new(f));
        self
    }

    pub fn on_action(mut self, f: impl Fn(u32, &str) + Send + Sync + 'static) -> Self {
        self.on_action = Some(Arc::new(f));
        self
    }

    pub fn on_clear(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_clear = Some(Arc::new(f));
        self
    }

    pub fn on_dnd(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_dnd = Some(Arc::new(f));
        self
    }

    pub fn on_group(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_group = Some(Arc::new(f));
        self
    }

    pub fn on_toggle_app(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_toggle_app = Some(Arc::new(f));
        self
    }

    pub fn on_scroll(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_scroll = Some(Arc::new(f));
        self
    }

    pub fn on_dismiss_panel(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss_panel = Some(Arc::new(f));
        self
    }
}

impl Default for NotificationCenter {
    fn default() -> Self {
        Self::new()
    }
}

fn ellipsize(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return String::from(s);
    }
    let mut t: String = s.chars().take(max.saturating_sub(3)).collect();
    t.push_str("...");
    t
}

fn center_row(
    theme: &Theme,
    item: NotifyItem,
    on_dismiss: Option<IdHandler>,
    on_read: Option<IdHandler>,
    on_action: Option<ActionHandler>,
    width: Pixels,
) -> Div {
    let accent = match item.kind {
        NotifyKind::Error => theme.colors.destructive,
        NotifyKind::Warning => theme.colors.accent,
        NotifyKind::Success => theme.colors.primary,
        _ => theme.colors.primary,
    };
    let bg = if item.unread {
        theme.colors.muted.opacity(0.45)
    } else {
        theme.colors.surface.opacity(0.7)
    };
    let extras = extras_row(theme, &item, on_action, px((width.0 - 48).max(80)));
    let mut body = div()
        .flex()
        .gap_2()
        .items_center()
        .child(
            div()
                .w(px(3))
                .h(px(36))
                .rounded(theme.radius.full)
                .bg(accent),
        )
        .child(
            Icon::new(item.kind.icon())
                .themed(theme)
                .color(accent)
                .with_size(ControlSize::Small),
        )
        .child(
            div()
                .flex_col()
                .gap_1()
                .grow()
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(
                            text(ellipsize(&item.title, 28))
                                .color(theme.colors.foreground)
                                .font(FontId::UiBold)
                                .size(12),
                        )
                        .child(
                            text(item.age.clone())
                                .color(theme.colors.muted_foreground)
                                .size(10),
                        ),
                )
                .child(
                    text(ellipsize(&item.body, 34))
                        .color(theme.colors.muted_foreground)
                        .size(11),
                )
                .child(
                    text(item.app_name.clone())
                        .color(theme.colors.muted_foreground)
                        .size(10),
                ),
        );
    if let Some(h) = on_dismiss {
        let id = item.id;
        body = body.child(
            Button::new(format!("ncenter-x-{}", item.id))
                .themed(theme)
                .ghost()
                .label("x")
                .small()
                .compact()
                .on_click(move || h(id)),
        );
    }
    let mut card = div()
        .w(width)
        .flex_col()
        .gap_1()
        .p(theme.spacing.sm)
        .bg(bg)
        .rounded(theme.radius.md)
        .child(body);
    if let Some(extra) = extras {
        card = card.child(extra);
    }
    if let Some(h) = on_read {
        let id = item.id;
        card = card.on_click(move |_| h(id));
    }
    card
}

impl IntoElement for NotificationCenter {
    type Element = Div;

    fn into_element(self) -> Div {
        let theme = self.theme;
        let list_w = px((self.width.0 - 24).max(120));
        let list_h = px((self.height.0 - 72).max(80));

        let unread_label = if self.unread > 0 {
            format!("{}", self.unread.min(99))
        } else {
            String::new()
        };

        let mut header = div().flex().justify_between().items_center().child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    text("Notifications")
                        .color(theme.colors.foreground)
                        .font(FontId::UiBold)
                        .size(14),
                )
                .when(!unread_label.is_empty(), |d| {
                    d.child(
                        div()
                            .h(px(18))
                            .min_w(px(18))
                            .pad_x(px(5))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(theme.radius.full)
                            .bg(theme.colors.primary)
                            .child(
                                text(unread_label)
                                    .color(theme.colors.primary_foreground)
                                    .size(10),
                            ),
                    )
                }),
        );

        let mut tools = div().flex().items_center().gap_1();
        let mut dnd = Switch::new("ncenter-dnd")
            .themed(&theme)
            .label("DND")
            .checked(self.dnd)
            .small();
        if let Some(h) = self.on_dnd.clone() {
            let next = !self.dnd;
            dnd = dnd.on_change(move |_| h(next));
        }
        tools = tools.child(dnd);

        let mut group = Button::new("ncenter-group")
            .themed(&theme)
            .ghost()
            .label(if self.group_by_app {
                "Ungroup"
            } else {
                "Group"
            })
            .small()
            .compact()
            .selected(self.group_by_app);
        if let Some(h) = self.on_group.clone() {
            group = group.on_click(move || h());
        }
        tools = tools.child(group);

        let mut clear = Button::new("ncenter-clear")
            .themed(&theme)
            .ghost()
            .label("Clear")
            .small()
            .compact();
        if let Some(h) = self.on_clear.clone() {
            clear = clear.on_click(move || h());
        }
        tools = tools.child(clear);
        header = header.child(tools);

        let body = if self.items.is_empty() {
            div()
                .w(list_w)
                .h(list_h)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Empty::new()
                        .themed(&theme)
                        .icon(IconName::Bell)
                        .title("No notifications")
                        .description("You're all caught up"),
                )
        } else if self.group_by_app {
            let mut order: Vec<String> = Vec::new();
            for item in &self.items {
                if !order.iter().any(|n| n == &item.app_name) {
                    order.push(item.app_name.clone());
                }
            }
            let mut col = div().flex_col().gap_2();
            for app in order {
                let collapsed = self.collapsed.iter().any(|n| n == &app);
                let count = self.items.iter().filter(|i| i.app_name == app).count();
                let unread = self
                    .items
                    .iter()
                    .filter(|i| i.app_name == app && i.unread)
                    .count();
                let arrow = if collapsed { "▸" } else { "▾" };
                let label = format!("{} {} ({})", arrow, app, count);
                let mut hdr = Button::new(format!("ncenter-app-{}", app))
                    .themed(&theme)
                    .ghost()
                    .label(label)
                    .small()
                    .selected(unread > 0);
                if let Some(h) = self.on_toggle_app.clone() {
                    let name = app.clone();
                    hdr = hdr.on_click(move || h(&name));
                }
                col = col.child(hdr);
                if !collapsed {
                    for item in self
                        .items
                        .iter()
                        .filter(|i| i.app_name == app)
                        .take(5)
                        .cloned()
                    {
                        col = col.child(center_row(
                            &theme,
                            item,
                            self.on_dismiss.clone(),
                            self.on_read.clone(),
                            self.on_action.clone(),
                            list_w,
                        ));
                    }
                }
            }
            let mut area = ScrollArea::new()
                .themed(&theme)
                .size(list_w, list_h)
                .offset(self.scroll)
                .child(col);
            if let Some(h) = self.on_scroll.clone() {
                area = area.on_offset(move |y| h(y));
            }
            area.into_element()
        } else {
            let mut col = div().flex_col().gap_2();
            for item in self.items {
                col = col.child(center_row(
                    &theme,
                    item,
                    self.on_dismiss.clone(),
                    self.on_read.clone(),
                    self.on_action.clone(),
                    list_w,
                ));
            }
            let mut area = ScrollArea::new()
                .themed(&theme)
                .size(list_w, list_h)
                .offset(self.scroll)
                .child(col);
            if let Some(h) = self.on_scroll.clone() {
                area = area.on_offset(move |y| h(y));
            }
            area.into_element()
        };

        let mut card = div()
            .w(self.width)
            .h(self.height)
            .flex_col()
            .p(theme.spacing.md)
            .gap_2()
            .bg(theme.colors.surface.opacity(0.96))
            .border_1()
            .border_color(theme.colors.border)
            .rounded(theme.radius.lg)
            .child(header)
            .child(Separator::horizontal().themed(&theme))
            .child(body)
            .focusable();
        if let Some(dismiss) = self.on_dismiss_panel.clone() {
            card = card.on_key_down(move |e| {
                if e.key == Key::Escape {
                    dismiss();
                    true
                } else {
                    false
                }
            });
        }
        let trap = if let Some(d) = self.on_dismiss_panel {
            FocusTrap::new(card).on_escape(move || d())
        } else {
            FocusTrap::new(card)
        };
        div().w(self.width).h(self.height).child(trap)
    }
}

#[cfg(test)]
mod tests {
    use super::{NotificationCenter, NotifyItem, NotifyKind, ToastStack};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use alloc::vec;
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use knox_gpui::{
        IntoElement, Key, KeyDownEvent, Modifiers, Point, Size, WindowInput, dispatch_key_down,
        dispatch_mouse_down, dispatch_mouse_up, prepare_element,
    };

    fn click_at(el: &mut knox_gpui::Div, input: &mut WindowInput, p: Point) {
        dispatch_mouse_down(el, input, p);
        dispatch_mouse_up(el, input, p);
    }

    fn sample(id: u32) -> NotifyItem {
        NotifyItem::new(id, "Welcome to KnoxOS", "Desktop environment loaded")
            .app("System")
            .kind(NotifyKind::System)
    }

    #[test]
    fn constructs_stack_and_center() {
        let theme = Theme::aurora_dark();
        let items = vec![sample(1), sample(2)];
        let (w, h) = ToastStack::stack_size(&items);
        assert!(w >= 280);
        assert!(h >= ToastStack::CARD_H);
        let _ = ToastStack::new()
            .themed(&theme)
            .items(items.clone())
            .into_element();
        let _ = NotificationCenter::new()
            .themed(&theme)
            .items(items)
            .into_element();
        let _ = NotificationCenter::new().themed(&theme).into_element();
    }

    #[test]
    fn toast_dismiss_fires() {
        let n = Arc::new(AtomicU32::new(0));
        let c = n.clone();
        let items = vec![sample(7)];
        let (w, h) = ToastStack::stack_size(&items);
        let mut el = ToastStack::new()
            .themed(&Theme::aurora_dark())
            .size(px_size(w), px_size(h))
            .items(items)
            .on_dismiss(move |id| c.store(id, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(w, h), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .rev()
            .find(|hb| hb.tab_stop && hb.bounds.width() <= 48)
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("toast dismiss");
        click_at(&mut el, &mut input, p);
        assert_eq!(n.load(Ordering::SeqCst), 7);
    }

    #[test]
    fn center_clear_fires() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let (w, h) = (NotificationCenter::WIDTH, NotificationCenter::HEIGHT);
        let mut el = NotificationCenter::new()
            .themed(&Theme::aurora_dark())
            .items([sample(1)])
            .on_clear(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(w, h), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .rev()
            .find(|hb| hb.tab_stop && hb.bounds.y() < 48 && hb.bounds.x() > 200)
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("clear");
        click_at(&mut el, &mut input, p);
        assert!(hit.load(Ordering::SeqCst));
    }

    #[test]
    fn escape_dismisses_center() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let (w, h) = (NotificationCenter::WIDTH, NotificationCenter::HEIGHT);
        let mut el = NotificationCenter::new()
            .themed(&Theme::aurora_dark())
            .on_dismiss_panel(move || flag.store(true, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(w, h), &mut input);
        dispatch_key_down(
            &mut el,
            &mut input,
            &KeyDownEvent::new(Key::Escape, Modifiers::NONE),
        );
        assert!(hit.load(Ordering::SeqCst));
    }

    fn px_size(v: i32) -> knox_gpui::Pixels {
        knox_gpui::px(v)
    }
}
