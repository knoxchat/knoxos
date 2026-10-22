// SPDX-License-Identifier: MIT
//! Remaining D8.9–D8.29 gallery stories + full-catalog checklist (D8.30).

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, FluentBuilder, Hsla, IntoElement, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::carousel::Carousel;
use crate::chart::{Chart, ChartKind, Sample};
use crate::chrome::{DescriptionList, GroupBox, Sidebar, StatusBar, Toolbar};
use crate::command::Command;
use crate::diff::Diff;
use crate::dock::{Dock, DockArea, DockLayout, Resizable, SplitAxis};
use crate::editor::Editor;
use crate::gallery::{
    d8_buttons_catalog_types, d8_buttons_construct_all, d8_date_catalog_types,
    d8_date_construct_all, d8_display_catalog_types, d8_display_construct_all,
    d8_feedback_catalog_types, d8_feedback_construct_all, d8_forms_catalog_types,
    d8_forms_construct_all, d8_input_catalog_types, d8_input_construct_all,
    d8_overlay_catalog_types, d8_overlay_construct_all, d8_select_catalog_types,
    d8_select_construct_all, heading,
};
use crate::input::FieldState;
use crate::list::{List, Tree, TreeNode};
use crate::menu::{AppMenuBar, ContextMenu, DropdownMenu, MenuItem, NativeMenu, PopupMenu};
use crate::message::{
    Attachment, Bubble, BubbleSide, ChatMessage, Marker, Message, MessageScroller, Questionnaire,
    Rating,
};
use crate::nav::{
    Accordion, Breadcrumb, Collapsible, NavStack, NavStackState, Pagination, Stepper, TabBar, Tabs,
};
use crate::overlay::Dialog;
use crate::overlay_host::OverlayHost;
use crate::scroll::{ScrollArea, Scrollbar, VirtualList};
use crate::settings::{SettingGroup, SettingItem, SettingPage, SettingsWidget};
use crate::size::{Selectable, Sizable};
use crate::speech::{Speech, SpeechState};
use crate::table::{DataTable, TableColumn, TableHead};
use crate::textview::TextView;
use crate::theme::Theme;
use crate::titlebar::{TitleBar, WindowBorder};
use crate::toast::{NotificationList, Toast, ToastData};
use crate::webview::WebView;

/// Copy snapshot for D8.9–D8.29 live stories.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct D8RestSnapshot {
    pub page: u8,
    pub menu_open: bool,
    pub ctx_open: bool,
    pub drop_open: bool,
    pub bar_open: u8,
    pub picked: u8,
    pub toast_ms: u16,
    pub overlay_on: bool,
    pub scroll: i32,
    pub tab: u8,
    pub accordion: u8,
    pub crumb: u8,
    pub pager: u8,
    pub step: u8,
    pub stack: u8,
    pub side: u8,
    pub split: u16,
    pub cmd_open: bool,
    pub cmd_hi: u8,
    pub settings_page: u8,
    pub list_sel: u8,
    pub tree_sel: u8,
    pub tree_fold: bool,
    pub sort_col: u8,
    pub sort_asc: bool,
    pub carousel: u8,
    pub rating: u8,
    pub speech: u8,
    pub light: bool,
    pub large: bool,
    pub maxed: bool,
}

impl D8RestSnapshot {
    pub const INIT: Self = Self {
        page: 0,
        menu_open: false,
        ctx_open: false,
        drop_open: false,
        bar_open: 255,
        picked: 0,
        toast_ms: 2400,
        overlay_on: false,
        scroll: 0,
        tab: 0,
        accordion: 0,
        crumb: 1,
        pager: 0,
        step: 0,
        stack: 1,
        side: 0,
        split: 500,
        cmd_open: false,
        cmd_hi: 0,
        settings_page: 0,
        list_sel: 0,
        tree_sel: 0,
        tree_fold: true,
        sort_col: 0,
        sort_asc: true,
        carousel: 0,
        rating: 3,
        speech: 0,
        light: false,
        large: false,
        maxed: false,
    };
}

impl Default for D8RestSnapshot {
    fn default() -> Self {
        Self::INIT
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D8RestAction {
    Page(u8),
    MenuOpen(bool),
    CtxOpen(bool),
    DropOpen(bool),
    BarOpen(u8),
    Pick(u8),
    ToastMs(u16),
    Overlay(bool),
    Scroll(i32),
    Tab(u8),
    Accordion(u8),
    Crumb(u8),
    Pager(u8),
    Step(u8),
    Stack(u8),
    Side(u8),
    Split(u16),
    CmdOpen(bool),
    CmdHi(u8),
    Settings(u8),
    ListSel(u8),
    TreeSel(u8),
    TreeFold,
    Sort(u8),
    Carousel(u8),
    Rating(u8),
    Speech(u8),
    Light(bool),
    Large(bool),
    Maxed(bool),
}

pub fn apply_d8_rest(snap: &mut D8RestSnapshot, action: D8RestAction) {
    match action {
        D8RestAction::Page(v) => snap.page = v,
        D8RestAction::MenuOpen(v) => snap.menu_open = v,
        D8RestAction::CtxOpen(v) => snap.ctx_open = v,
        D8RestAction::DropOpen(v) => snap.drop_open = v,
        D8RestAction::BarOpen(v) => snap.bar_open = v,
        D8RestAction::Pick(v) => {
            snap.picked = v;
            snap.menu_open = false;
            snap.ctx_open = false;
            snap.drop_open = false;
            snap.bar_open = 255;
        }
        D8RestAction::ToastMs(v) => snap.toast_ms = v,
        D8RestAction::Overlay(v) => snap.overlay_on = v,
        D8RestAction::Scroll(v) => snap.scroll = v.max(0),
        D8RestAction::Tab(v) => snap.tab = v,
        D8RestAction::Accordion(v) => snap.accordion = v,
        D8RestAction::Crumb(v) => snap.crumb = v,
        D8RestAction::Pager(v) => snap.pager = v,
        D8RestAction::Step(v) => snap.step = v,
        D8RestAction::Stack(v) => snap.stack = v,
        D8RestAction::Side(v) => snap.side = v,
        D8RestAction::Split(v) => snap.split = v,
        D8RestAction::CmdOpen(v) => snap.cmd_open = v,
        D8RestAction::CmdHi(v) => snap.cmd_hi = v,
        D8RestAction::Settings(v) => snap.settings_page = v,
        D8RestAction::ListSel(v) => snap.list_sel = v,
        D8RestAction::TreeSel(v) => snap.tree_sel = v,
        D8RestAction::TreeFold => snap.tree_fold = !snap.tree_fold,
        D8RestAction::Sort(v) => {
            if snap.sort_col == v {
                snap.sort_asc = !snap.sort_asc;
            } else {
                snap.sort_col = v;
                snap.sort_asc = true;
            }
        }
        D8RestAction::Carousel(v) => snap.carousel = v,
        D8RestAction::Rating(v) => snap.rating = v,
        D8RestAction::Speech(v) => snap.speech = v,
        D8RestAction::Light(v) => snap.light = v,
        D8RestAction::Large(v) => snap.large = v,
        D8RestAction::Maxed(v) => snap.maxed = v,
    }
}

fn menu_items() -> [MenuItem; 3] {
    [
        MenuItem::new("Save").accel("cmd-s"),
        MenuItem::separator(),
        MenuItem::new("Quit").accel("cmd-q"),
    ]
}

pub fn d8_menu_catalog_types() -> &'static [&'static str] {
    &[
        "PopupMenu",
        "ContextMenu",
        "DropdownMenu",
        "AppMenuBar",
        "NativeMenu",
    ]
}

pub fn d8_menu_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        PopupMenu::new("c")
            .themed(theme)
            .items(menu_items())
            .into_element(),
        ContextMenu::new("c")
            .themed(theme)
            .items(menu_items())
            .into_element(),
        DropdownMenu::new("c")
            .themed(theme)
            .items(menu_items())
            .into_element(),
        AppMenuBar::new("c")
            .themed(theme)
            .menu("File", menu_items())
            .into_element(),
        NativeMenu::new("c")
            .themed(theme)
            .items(menu_items())
            .open(true)
            .into_element(),
    ]
}

pub fn d8_toast_catalog_types() -> &'static [&'static str] {
    &["Toast", "NotificationList"]
}

pub fn d8_toast_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        Toast::new(ToastData::new("Hi", "body", 1000))
            .themed(theme)
            .into_element(),
        NotificationList::new()
            .themed(theme)
            .items([ToastData::new("A", "", 10)])
            .into_element(),
    ]
}

pub fn d8_host_catalog_types() -> &'static [&'static str] {
    &["OverlayHost"]
}

pub fn d8_host_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![OverlayHost::new().themed(theme).into_element()]
}

pub fn d8_scroll_catalog_types() -> &'static [&'static str] {
    &["ScrollArea", "Scrollbar", "VirtualList"]
}

pub fn d8_scroll_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        ScrollArea::new().themed(theme).into_element(),
        Scrollbar::new().themed(theme).into_element(),
        VirtualList::new().themed(theme).count(8).into_element(),
    ]
}

pub fn d8_nav_catalog_types() -> &'static [&'static str] {
    &[
        "Tabs",
        "TabBar",
        "Accordion",
        "Collapsible",
        "Breadcrumb",
        "Pagination",
        "Stepper",
        "NavStack",
    ]
}

pub fn d8_nav_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        Tabs::new("c")
            .themed(theme)
            .labels(["A"])
            .page(text("a"))
            .into_element(),
        TabBar::new("c").themed(theme).labels(["A"]).into_element(),
        Accordion::new()
            .themed(theme)
            .item("A", text("b"))
            .into_element(),
        Collapsible::new("A").themed(theme).into_element(),
        Breadcrumb::new()
            .themed(theme)
            .parts(["Home"])
            .into_element(),
        Pagination::new(2).themed(theme).into_element(),
        Stepper::new().themed(theme).labels(["A"]).into_element(),
        NavStack::new({
            let mut s = NavStackState::default();
            s.push("Home");
            s
        })
        .themed(theme)
        .into_element(),
    ]
}

pub fn d8_chrome_catalog_types() -> &'static [&'static str] {
    &[
        "Sidebar",
        "Toolbar",
        "StatusBar",
        "GroupBox",
        "DescriptionList",
    ]
}

pub fn d8_chrome_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        Sidebar::new().themed(theme).items(["A"]).into_element(),
        Toolbar::new().themed(theme).into_element(),
        StatusBar::new().themed(theme).left("ok").into_element(),
        GroupBox::new("G").themed(theme).into_element(),
        DescriptionList::new()
            .themed(theme)
            .row("K", "V")
            .into_element(),
    ]
}

pub fn d8_dock_catalog_types() -> &'static [&'static str] {
    &["Resizable", "DockArea", "Dock"]
}

pub fn d8_dock_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        Resizable::new(SplitAxis::Horizontal)
            .themed(theme)
            .first(text("a"))
            .second(text("b"))
            .into_element(),
        DockArea::new(DockLayout::leaf(["A"], 0))
            .themed(theme)
            .into_element(),
        Dock::new()
            .themed(theme)
            .add_leading(text("start"))
            .into_element(),
    ]
}

pub fn d8_title_catalog_types() -> &'static [&'static str] {
    &["TitleBar", "WindowBorder"]
}

pub fn d8_title_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        TitleBar::new("G").themed(theme).into_element(),
        WindowBorder::new("G").themed(theme).into_element(),
    ]
}

pub fn d8_command_catalog_types() -> &'static [&'static str] {
    &["Command"]
}

pub fn d8_command_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![Command::new().themed(theme).into_element()]
}

pub fn d8_settings_catalog_types() -> &'static [&'static str] {
    &[
        "SettingPage",
        "SettingGroup",
        "SettingItem",
        "SettingsWidget",
    ]
}

pub fn d8_settings_construct_all(theme: &Theme) -> Vec<Div> {
    let item = SettingItem::new("Night").toggle(false);
    let group = SettingGroup::new("Screen").item(item.clone());
    let page = SettingPage::new("Display").group(group.clone());
    alloc::vec![
        div().child(text(page.title.clone())),
        div().child(text(group.title.clone())),
        div().child(text(item.label.clone())),
        SettingsWidget::new()
            .themed(theme)
            .page(page)
            .into_element(),
    ]
}

pub fn d8_list_catalog_types() -> &'static [&'static str] {
    &["List", "Tree"]
}

pub fn d8_list_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        List::new().themed(theme).items(["A"]).into_element(),
        Tree::new()
            .themed(theme)
            .nodes([TreeNode::leaf("A")])
            .into_element(),
    ]
}

pub fn d8_table_catalog_types() -> &'static [&'static str] {
    &["TableHead", "DataTable"]
}

pub fn d8_table_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        TableHead::new()
            .themed(theme)
            .columns([TableColumn::new("A", 40)])
            .into_element(),
        DataTable::new()
            .themed(theme)
            .columns([TableColumn::new("A", 40)])
            .into_element(),
    ]
}

pub fn d8_textview_catalog_types() -> &'static [&'static str] {
    &["TextView"]
}

pub fn d8_textview_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        TextView::new()
            .themed(theme)
            .markdown("# Hi")
            .into_element()
    ]
}

pub fn d8_editor_catalog_types() -> &'static [&'static str] {
    &["Editor"]
}

pub fn d8_editor_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![Editor::new().themed(theme).into_element()]
}

pub fn d8_diff_catalog_types() -> &'static [&'static str] {
    &["Diff"]
}

pub fn d8_diff_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![Diff::new().themed(theme).patch("+a\n-b\n").into_element()]
}

pub fn d8_chart_catalog_types() -> &'static [&'static str] {
    &[
        "Line",
        "Bar",
        "Area",
        "Pie",
        "Candlestick",
        "Radar",
        "Sankey",
    ]
}

pub fn d8_chart_construct_all(theme: &Theme) -> Vec<Div> {
    let vals = [Sample::v(1.0), Sample::v(2.0), Sample::v(3.0)];
    d8_chart_catalog_types()
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let kind = match i {
                0 => ChartKind::Line,
                1 => ChartKind::Bar,
                2 => ChartKind::Area,
                3 => ChartKind::Pie,
                4 => ChartKind::Candlestick,
                5 => ChartKind::Radar,
                _ => ChartKind::Sankey,
            };
            div().child(Chart::new(kind).themed(theme).data(vals))
        })
        .collect()
}

pub fn d8_carousel_catalog_types() -> &'static [&'static str] {
    &["Carousel"]
}

pub fn d8_carousel_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![Carousel::new().themed(theme).slides(["A"]).into_element()]
}

pub fn d8_message_catalog_types() -> &'static [&'static str] {
    &[
        "Message",
        "MessageScroller",
        "Bubble",
        "Attachment",
        "Marker",
        "Rating",
        "Questionnaire",
    ]
}

pub fn d8_message_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        Message::new(ChatMessage::new(BubbleSide::Us, "hi"))
            .themed(theme)
            .into_element(),
        MessageScroller::new().themed(theme).into_element(),
        Bubble::new("hi").themed(theme).into_element(),
        Attachment::new("a").themed(theme).into_element(),
        Marker::new("m").themed(theme).into_element(),
        Rating::new(2).themed(theme).into_element(),
        Questionnaire::new("Q")
            .themed(theme)
            .choices(["A"])
            .into_element(),
    ]
}

pub fn d8_speech_catalog_types() -> &'static [&'static str] {
    &["Speech"]
}

pub fn d8_speech_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![Speech::new().themed(theme).into_element()]
}

pub fn d8_webview_catalog_types() -> &'static [&'static str] {
    &["WebView"]
}

pub fn d8_webview_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        WebView::demo_color(32, 16, Hsla::BLUE)
            .themed(theme)
            .into_element()
    ]
}

/// Every D8.1–D8.28 catalog name (D8.30 checklist).
pub fn d8_all_catalog_types() -> Vec<&'static str> {
    let mut out = Vec::new();
    for block in [
        d8_buttons_catalog_types(),
        d8_display_catalog_types(),
        d8_feedback_catalog_types(),
        d8_input_catalog_types(),
        d8_forms_catalog_types(),
        d8_select_catalog_types(),
        d8_date_catalog_types(),
        d8_overlay_catalog_types(),
        d8_menu_catalog_types(),
        d8_toast_catalog_types(),
        d8_host_catalog_types(),
        d8_scroll_catalog_types(),
        d8_nav_catalog_types(),
        d8_chrome_catalog_types(),
        d8_dock_catalog_types(),
        d8_title_catalog_types(),
        d8_command_catalog_types(),
        d8_settings_catalog_types(),
        d8_list_catalog_types(),
        d8_table_catalog_types(),
        d8_textview_catalog_types(),
        d8_editor_catalog_types(),
        d8_diff_catalog_types(),
        d8_chart_catalog_types(),
        d8_carousel_catalog_types(),
        d8_message_catalog_types(),
        d8_speech_catalog_types(),
        d8_webview_catalog_types(),
    ] {
        out.extend_from_slice(block);
    }
    out
}

/// Construct one of every D8.1–D8.28 catalog control.
pub fn d8_all_construct_all(theme: &Theme) -> Vec<Div> {
    let mut out = Vec::new();
    out.extend(d8_buttons_construct_all(theme));
    out.extend(d8_display_construct_all(theme));
    out.extend(d8_feedback_construct_all(theme));
    out.extend(d8_input_construct_all(theme));
    out.extend(d8_forms_construct_all(theme));
    out.extend(d8_select_construct_all(theme));
    out.extend(d8_date_construct_all(theme));
    out.extend(d8_overlay_construct_all(theme));
    out.extend(d8_menu_construct_all(theme));
    out.extend(d8_toast_construct_all(theme));
    out.extend(d8_host_construct_all(theme));
    out.extend(d8_scroll_construct_all(theme));
    out.extend(d8_nav_construct_all(theme));
    out.extend(d8_chrome_construct_all(theme));
    out.extend(d8_dock_construct_all(theme));
    out.extend(d8_title_construct_all(theme));
    out.extend(d8_command_construct_all(theme));
    out.extend(d8_settings_construct_all(theme));
    out.extend(d8_list_construct_all(theme));
    out.extend(d8_table_construct_all(theme));
    out.extend(d8_textview_construct_all(theme));
    out.extend(d8_editor_construct_all(theme));
    out.extend(d8_diff_construct_all(theme));
    out.extend(d8_chart_construct_all(theme));
    out.extend(d8_carousel_construct_all(theme));
    out.extend(d8_message_construct_all(theme));
    out.extend(d8_speech_construct_all(theme));
    out.extend(d8_webview_construct_all(theme));
    out
}

const PAGES: [&str; 7] = [
    "Basics", "Forms", "Menus", "Layout", "Data", "Media", "Theme",
];

pub fn d8_page_bar(theme: &Theme, page: u8, on: Arc<dyn Fn(D8RestAction) + Send + Sync>) -> Div {
    TabBar::new("d8-pages")
        .themed(theme)
        .labels(PAGES)
        .selected(page as usize)
        .on_select(move |i| on(D8RestAction::Page(i as u8)))
        .into_element()
}

pub fn d8_rest_gallery(
    theme: &Theme,
    snap: D8RestSnapshot,
    on: Arc<dyn Fn(D8RestAction) + Send + Sync>,
    cmd_query: FieldState,
    on_cmd_query: Arc<dyn Fn(FieldState) + Send + Sync>,
) -> Div {
    match snap.page {
        0 => d8_menus_gallery(theme, snap, on),
        1 => d8_toast_host_gallery(theme, snap, on),
        2 => d8_scroll_nav_gallery(theme, snap, on),
        3 => d8_chrome_dock_gallery(theme, snap, on),
        4 => d8_data_gallery(theme, snap, on, cmd_query, on_cmd_query),
        5 => d8_media_gallery(theme, snap, on),
        _ => d8_theme_gallery(theme, snap, on),
    }
}

fn d8_menus_gallery(
    theme: &Theme,
    snap: D8RestSnapshot,
    on: Arc<dyn Fn(D8RestAction) + Send + Sync>,
) -> Div {
    let fire = on;
    div()
        .flex_col()
        .gap_2()
        .child(heading(theme, "Menus"))
        .child(
            DropdownMenu::new("g-dropm")
                .themed(theme)
                .label("File")
                .items(menu_items())
                .open(snap.drop_open)
                .on_toggle({
                    let c = fire.clone();
                    move || c(D8RestAction::DropOpen(!snap.drop_open))
                })
                .on_pick({
                    let c = fire.clone();
                    move |i| c(D8RestAction::Pick(i as u8))
                }),
        )
        .child(
            ContextMenu::new("g-ctx")
                .themed(theme)
                .trigger(
                    text("Right-click me")
                        .color(theme.colors.foreground)
                        .size(12),
                )
                .items(menu_items())
                .open(snap.ctx_open)
                .on_open({
                    let c = fire.clone();
                    move || c(D8RestAction::CtxOpen(true))
                })
                .on_dismiss({
                    let c = fire.clone();
                    move || c(D8RestAction::CtxOpen(false))
                })
                .on_pick({
                    let c = fire.clone();
                    move |i| c(D8RestAction::Pick(i as u8))
                }),
        )
        .child(
            AppMenuBar::new("g-bar")
                .themed(theme)
                .menu("File", menu_items())
                .menu("Edit", [MenuItem::new("Copy")])
                .open(if snap.bar_open == 255 {
                    None
                } else {
                    Some(snap.bar_open as usize)
                })
                .on_open({
                    let c = fire.clone();
                    move |i| c(D8RestAction::BarOpen(i as u8))
                })
                .on_dismiss({
                    let c = fire.clone();
                    move || c(D8RestAction::BarOpen(255))
                })
                .on_pick({
                    let c = fire;
                    move |_, i| c(D8RestAction::Pick(i as u8))
                }),
        )
}

fn d8_toast_host_gallery(
    theme: &Theme,
    snap: D8RestSnapshot,
    on: Arc<dyn Fn(D8RestAction) + Send + Sync>,
) -> Div {
    let fire = on;
    let toast = ToastData::new("Saved", "Document written", snap.toast_ms as u32);
    let overlay = if snap.overlay_on {
        Some(
            Dialog::new()
                .themed(theme)
                .title("Hosted")
                .open(true)
                .child(
                    text("Dim still clicks")
                        .color(theme.colors.foreground)
                        .size(12),
                )
                .on_dismiss({
                    let c = fire.clone();
                    move || c(D8RestAction::Overlay(false))
                }),
        )
    } else {
        None
    };
    let mut host = OverlayHost::new()
        .themed(theme)
        .size(px(400), px(160))
        .dim(snap.overlay_on)
        .content(
            div()
                .flex_col()
                .gap_2()
                .child(heading(theme, "Toasts / Overlay host"))
                .child(
                    Button::new("ov-open")
                        .themed(theme)
                        .label("Open dialog")
                        .on_click({
                            let c = fire.clone();
                            move || c(D8RestAction::Overlay(true))
                        }),
                )
                .child(NotificationList::new().themed(theme).items([toast])),
        );
    if let Some(ov) = overlay {
        host = host.overlay(ov);
    }
    host.into_element()
}

fn d8_scroll_nav_gallery(
    theme: &Theme,
    snap: D8RestSnapshot,
    on: Arc<dyn Fn(D8RestAction) + Send + Sync>,
) -> Div {
    let fire = on;
    let mut stack = NavStackState::default();
    stack.push("Home");
    if snap.stack > 0 {
        stack.push("Child");
    }
    div()
        .flex_col()
        .gap_2()
        .child(heading(theme, "Scroll / Nav"))
        .child(
            div()
                .flex()
                .gap_2()
                .child(
                    ScrollArea::new()
                        .themed(theme)
                        .size(px(160), px(64))
                        .offset(snap.scroll)
                        .child(div().h(px(200)).child(text("Tall").size(12)))
                        .on_offset({
                            let c = fire.clone();
                            move |o| c(D8RestAction::Scroll(o))
                        }),
                )
                .child(
                    Scrollbar::new()
                        .themed(theme)
                        .offset(snap.scroll)
                        .content_h(200)
                        .viewport_h(64)
                        .on_offset({
                            let c = fire.clone();
                            move |o| c(D8RestAction::Scroll(o))
                        }),
                )
                .child(
                    VirtualList::new()
                        .themed(theme)
                        .count(1000)
                        .row_h(16)
                        .size(px(140), px(64))
                        .offset(snap.scroll)
                        .on_offset({
                            let c = fire.clone();
                            move |o| c(D8RestAction::Scroll(o))
                        }),
                ),
        )
        .child(
            Tabs::new("g-tabs")
                .themed(theme)
                .labels(["One", "Two"])
                .selected(snap.tab as usize)
                .on_select({
                    let c = fire.clone();
                    move |i| c(D8RestAction::Tab(i as u8))
                })
                .page(text("Page one").size(12))
                .page(text("Page two").size(12)),
        )
        .child(
            Accordion::new()
                .themed(theme)
                .open(Some(snap.accordion as usize))
                .item("Alpha", text("A"))
                .item("Beta", text("B"))
                .on_open({
                    let c = fire.clone();
                    move |i| c(D8RestAction::Accordion(i as u8))
                }),
        )
        .child(
            Breadcrumb::new()
                .themed(theme)
                .parts(["Home", "Docs", "API"])
                .on_select({
                    let c = fire.clone();
                    move |i| c(D8RestAction::Crumb(i as u8))
                }),
        )
        .child(
            Pagination::new(4)
                .themed(theme)
                .page(snap.pager as usize)
                .on_page({
                    let c = fire.clone();
                    move |i| c(D8RestAction::Pager(i as u8))
                }),
        )
        .child(
            Stepper::new()
                .themed(theme)
                .labels(["One", "Two", "Three"])
                .step(snap.step as usize)
                .on_step({
                    let c = fire.clone();
                    move |i| c(D8RestAction::Step(i as u8))
                }),
        )
        .child(NavStack::new(stack).themed(theme).on_back({
            let c = fire;
            move || c(D8RestAction::Stack(0))
        }))
}

fn d8_chrome_dock_gallery(
    theme: &Theme,
    snap: D8RestSnapshot,
    on: Arc<dyn Fn(D8RestAction) + Send + Sync>,
) -> Div {
    let fire = on;
    div()
        .flex_col()
        .gap_2()
        .child(heading(theme, "Chrome / Dock / CSD"))
        .child(Toolbar::new().themed(theme).child(
            Button::new("tb-new").themed(theme).label("New").on_click({
                let c = fire.clone();
                move || c(D8RestAction::Pick(1))
            }),
        ))
        .child(
            div()
                .flex()
                .gap_2()
                .child(
                    Sidebar::new()
                        .themed(theme)
                        .items(["Home", "Search"])
                        .selected(snap.side as usize)
                        .on_select({
                            let c = fire.clone();
                            move |i| c(D8RestAction::Side(i as u8))
                        }),
                )
                .child(
                    GroupBox::new("Info")
                        .themed(theme)
                        .child(DescriptionList::new().themed(theme).row("App", "Gallery")),
                ),
        )
        .child(StatusBar::new().themed(theme).left("Ready").right("Ln 1"))
        .child(
            Resizable::new(SplitAxis::Horizontal)
                .themed(theme)
                .size(px(320), px(80))
                .ratio(snap.split)
                .first(text("Left").size(12))
                .second(text("Right").size(12))
                .on_ratio({
                    let c = fire.clone();
                    move |r| c(D8RestAction::Split(r))
                }),
        )
        .child(
            TitleBar::new("Kit Gallery")
                .themed(theme)
                .maximized(snap.maxed)
                .on_max({
                    let c = fire.clone();
                    move || c(D8RestAction::Maxed(!snap.maxed))
                })
                .on_min({
                    let c = fire.clone();
                    move || c(D8RestAction::Pick(2))
                })
                .on_close({
                    let c = fire;
                    move || c(D8RestAction::Pick(3))
                }),
        )
}

fn d8_data_gallery(
    theme: &Theme,
    snap: D8RestSnapshot,
    on: Arc<dyn Fn(D8RestAction) + Send + Sync>,
    cmd_query: FieldState,
    on_cmd_query: Arc<dyn Fn(FieldState) + Send + Sync>,
) -> Div {
    let fire = on;
    let tree_src = TreeNode::branch("src", [TreeNode::leaf("main.rs"), TreeNode::leaf("lib.rs")])
        .expanded(snap.tree_fold);
    let rows = alloc::vec![
        alloc::vec![String::from("zeta"), String::from("2")],
        alloc::vec![String::from("alpha"), String::from("1")],
    ];
    div()
        .flex_col()
        .gap_2()
        .child(heading(theme, "Command / Settings / Data"))
        .child(
            Button::new("cmd-open")
                .themed(theme)
                .label("Command palette")
                .on_click({
                    let c = fire.clone();
                    move || c(D8RestAction::CmdOpen(!snap.cmd_open))
                }),
        )
        .child(
            Command::new()
                .themed(theme)
                .open(snap.cmd_open)
                .query(cmd_query)
                .items(["Open Terminal", "Open Files", "Settings"])
                .highlight(snap.cmd_hi as usize)
                .on_query(move |s| on_cmd_query(s))
                .on_highlight({
                    let c = fire.clone();
                    move |i| c(D8RestAction::CmdHi(i as u8))
                })
                .on_select({
                    let c = fire.clone();
                    move |_| c(D8RestAction::CmdOpen(false))
                })
                .on_dismiss({
                    let c = fire.clone();
                    move || c(D8RestAction::CmdOpen(false))
                }),
        )
        .child(
            SettingsWidget::new()
                .themed(theme)
                .selected(snap.settings_page as usize)
                .page(SettingPage::new("Display").group(
                    SettingGroup::new("Screen").item(SettingItem::new("Night light").toggle(false)),
                ))
                .page(
                    SettingPage::new("Sound").group(
                        SettingGroup::new("Output").item(SettingItem::new("Mute").toggle(true)),
                    ),
                )
                .on_page({
                    let c = fire.clone();
                    move |i| c(D8RestAction::Settings(i as u8))
                }),
        )
        .child(
            List::new()
                .themed(theme)
                .items(["Alpha", "Beta", "Gamma"])
                .selected(Some(snap.list_sel as usize))
                .on_select({
                    let c = fire.clone();
                    move |i| c(D8RestAction::ListSel(i as u8))
                }),
        )
        .child(
            Tree::new()
                .themed(theme)
                .nodes([tree_src])
                .selected(snap.tree_sel as usize)
                .on_select({
                    let c = fire.clone();
                    move |i| c(D8RestAction::TreeSel(i as u8))
                })
                .on_toggle({
                    let c = fire.clone();
                    move |_| c(D8RestAction::TreeFold)
                }),
        )
        .child(
            DataTable::new()
                .themed(theme)
                .columns([TableColumn::new("Name", 80), TableColumn::new("N", 40)])
                .rows(rows)
                .sort(Some(snap.sort_col as usize), snap.sort_asc)
                .on_sort({
                    let c = fire;
                    move |i| c(D8RestAction::Sort(i as u8))
                }),
        )
}

fn d8_media_gallery(
    theme: &Theme,
    snap: D8RestSnapshot,
    on: Arc<dyn Fn(D8RestAction) + Send + Sync>,
) -> Div {
    let fire = on;
    let speech = match snap.speech {
        1 => SpeechState::Recording,
        2 => SpeechState::NoRecognizer,
        _ => SpeechState::Idle,
    };
    div()
        .flex_col()
        .gap_2()
        .child(heading(theme, "Content / Media"))
        .child(
            TextView::new()
                .themed(theme)
                .markdown("# Notes\n\n- one\n\n[Docs](knoxos://help)\n"),
        )
        .child(
            Editor::new()
                .themed(theme)
                .state(FieldState::from("fn main() {\n    // hi\n}\n"))
                .height(px(64)),
        )
        .child(Diff::new().themed(theme).patch("@@ -1 +1 @@\n-old\n+new\n"))
        .child(
            div()
                .flex()
                .gap_2()
                .child(
                    Chart::new(ChartKind::Line)
                        .themed(theme)
                        .values([1.0, 3.0, 2.0, 4.0]),
                )
                .child(
                    Chart::new(ChartKind::Bar)
                        .themed(theme)
                        .values([2.0, 1.0, 4.0]),
                )
                .child(
                    Chart::new(ChartKind::Pie)
                        .themed(theme)
                        .values([1.0, 2.0, 1.0]),
                ),
        )
        .child(
            Carousel::new()
                .themed(theme)
                .slides(["One", "Two", "Three"])
                .index(snap.carousel as usize)
                .on_index({
                    let c = fire.clone();
                    move |i| c(D8RestAction::Carousel(i as u8))
                }),
        )
        .child(
            MessageScroller::new()
                .themed(theme)
                .items([
                    ChatMessage::new(BubbleSide::Them, "Hello"),
                    ChatMessage::new(BubbleSide::Us, "Hi there"),
                ])
                .offset(MessageScroller::offset_to_latest(2, 36, 80)),
        )
        .child(Rating::new(snap.rating).themed(theme).on_change({
            let c = fire.clone();
            move |v| c(D8RestAction::Rating(v as u8))
        }))
        .child(
            Speech::new()
                .themed(theme)
                .state(speech)
                .samples([0, 8000, -2000, 12000])
                .on_start({
                    let c = fire.clone();
                    move || c(D8RestAction::Speech(1))
                })
                .on_stop({
                    let c = fire;
                    move || c(D8RestAction::Speech(0))
                }),
        )
        .child(WebView::demo_color(120, 48, Hsla::from_rgb(40, 120, 200)).themed(theme))
}

/// D8.29 theme gallery page.
pub fn d8_theme_gallery(
    theme: &Theme,
    snap: D8RestSnapshot,
    on: Arc<dyn Fn(D8RestAction) + Send + Sync>,
) -> Div {
    let fire = on;
    let shown = if snap.light {
        Theme::arctic_light()
    } else {
        *theme
    };
    div()
        .flex_col()
        .gap_2()
        .child(heading(theme, "Theme"))
        .child(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("th-dark")
                        .themed(&shown)
                        .label("Dark")
                        .selected(!snap.light)
                        .on_click({
                            let c = fire.clone();
                            move || c(D8RestAction::Light(false))
                        }),
                )
                .child(
                    Button::new("th-light")
                        .themed(&shown)
                        .label("Light")
                        .selected(snap.light)
                        .on_click({
                            let c = fire.clone();
                            move || c(D8RestAction::Light(true))
                        }),
                )
                .child(
                    Button::new("th-den")
                        .themed(&shown)
                        .label(if snap.large { "Large" } else { "Medium" })
                        .on_click({
                            let c = fire;
                            move || c(D8RestAction::Large(!snap.large))
                        }),
                ),
        )
        .child(
            Button::new("th-demo")
                .themed(&shown)
                .primary()
                .label("Primary")
                .when(snap.large, |b| b.large()),
        )
        .child(
            text("Radius / density follow tokens")
                .color(shown.colors.muted_foreground)
                .size(12),
        )
}

#[cfg(test)]
mod tests {
    use super::{D8RestSnapshot, d8_all_catalog_types, d8_all_construct_all, d8_rest_gallery};
    use crate::input::FieldState;
    use crate::theme::Theme;
    use alloc::sync::Arc;

    #[test]
    fn catalog_constructs_every_d81_to_d828_type() {
        let names = d8_all_catalog_types();
        assert!(names.len() >= 90, "catalog too small: {}", names.len());
        let theme = Theme::aurora_dark();
        let built = d8_all_construct_all(&theme);
        assert_eq!(built.len(), names.len());
        let _ = d8_rest_gallery(
            &theme,
            D8RestSnapshot::default(),
            Arc::new(|_| {}),
            FieldState::empty(),
            Arc::new(|_| {}),
        );
    }
}
