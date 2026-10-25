// SPDX-License-Identifier: MIT
//! Session launcher (D9.2). D8.17 `Command` + categorized apps + power.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. Not GPL Zed command_palette.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px};

use crate::button::{Button, ButtonVariants};
use crate::command::Command;
use crate::empty::Empty;
use crate::icon::IconName;
use crate::input::{FieldState, Input};
use crate::interact::{Handler, IndexHandler};
use crate::overlay::FocusTrap;
use crate::scroll::ScrollArea;
use crate::select::{ListNav, pick_with_keys};
use crate::separator::Separator;
use crate::size::{Selectable, Sizable};
use crate::theme::Theme;

/// App grouping in the empty-query launcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LauncherCategory {
    Pinned,
    Apps,
    System,
    Development,
    Media,
    Ai,
    Emulator,
    Game,
}

impl LauncherCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pinned => "Pinned",
            Self::Apps => "Apps",
            Self::System => "System",
            Self::Development => "Development",
            Self::Media => "Media",
            Self::Ai => "AI",
            Self::Emulator => "Emulator",
            Self::Game => "Games",
        }
    }

    fn all() -> [Self; 8] {
        [
            Self::Pinned,
            Self::Apps,
            Self::System,
            Self::Development,
            Self::Media,
            Self::Ai,
            Self::Emulator,
            Self::Game,
        ]
    }
}

/// One launchable app row.
#[derive(Clone, Debug)]
pub struct LauncherApp {
    pub name: String,
    pub category: LauncherCategory,
    pub pinned: bool,
    pub keywords: String,
    pub description: String,
    pub icon: IconName,
}

impl LauncherApp {
    pub fn new(name: impl Into<String>, category: LauncherCategory) -> Self {
        let name = name.into();
        Self {
            icon: IconName::for_label(&name),
            name,
            pinned: matches!(category, LauncherCategory::Pinned),
            category,
            keywords: String::new(),
            description: String::new(),
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = icon;
        self
    }

    pub fn pinned(mut self, pinned: bool) -> Self {
        self.pinned = pinned;
        self
    }

    pub fn keywords(mut self, keywords: impl Into<String>) -> Self {
        self.keywords = keywords.into();
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    fn in_pinned(&self) -> bool {
        self.pinned || self.category == LauncherCategory::Pinned
    }
}

/// Session power actions on the launcher footer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerAction {
    Lock,
    Restart,
    Shutdown,
}

type PowerHandler = Arc<dyn Fn(PowerAction) + Send + Sync>;
type QueryHandler = Arc<dyn Fn(FieldState) + Send + Sync>;
type ScrollHandler = Arc<dyn Fn(i32) + Send + Sync>;

const POWER_ITEMS: [(&str, PowerAction); 3] = [
    ("Lock", PowerAction::Lock),
    ("Restart", PowerAction::Restart),
    ("Shut down", PowerAction::Shutdown),
];

/// Session launcher (D9.2).
pub struct Launcher {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    apps: Vec<LauncherApp>,
    query: FieldState,
    highlight: usize,
    scroll: i32,
    on_query: Option<QueryHandler>,
    on_select: Option<IndexHandler>,
    on_highlight: Option<IndexHandler>,
    on_power: Option<PowerHandler>,
    on_dismiss: Option<Handler>,
    on_scroll: Option<ScrollHandler>,
    search_placeholder: String,
    power_lock: String,
    power_restart: String,
    power_shutdown: String,
}

impl Launcher {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(480),
            height: px(520),
            apps: Vec::new(),
            query: FieldState::empty(),
            highlight: 0,
            scroll: 0,
            on_query: None,
            on_select: None,
            on_highlight: None,
            on_power: None,
            on_dismiss: None,
            on_scroll: None,
            search_placeholder: String::from("Search apps…"),
            power_lock: String::from("Lock"),
            power_restart: String::from("Restart"),
            power_shutdown: String::from("Shut down"),
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

    pub fn apps(mut self, apps: impl IntoIterator<Item = LauncherApp>) -> Self {
        self.apps = apps.into_iter().collect();
        self
    }

    pub fn query(mut self, query: FieldState) -> Self {
        self.query = query;
        self
    }

    pub fn highlight(mut self, highlight: usize) -> Self {
        self.highlight = highlight;
        self
    }

    pub fn scroll(mut self, scroll: i32) -> Self {
        self.scroll = scroll.max(0);
        self
    }

    pub fn on_query(mut self, f: impl Fn(FieldState) + Send + Sync + 'static) -> Self {
        self.on_query = Some(Arc::new(f));
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }

    pub fn on_highlight(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_highlight = Some(Arc::new(f));
        self
    }

    pub fn on_power(mut self, f: impl Fn(PowerAction) + Send + Sync + 'static) -> Self {
        self.on_power = Some(Arc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }

    pub fn on_scroll(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_scroll = Some(Arc::new(f));
        self
    }

    pub fn search_placeholder(mut self, s: impl Into<String>) -> Self {
        self.search_placeholder = s.into();
        self
    }

    pub fn power_labels(
        mut self,
        lock: impl Into<String>,
        restart: impl Into<String>,
        shutdown: impl Into<String>,
    ) -> Self {
        self.power_lock = lock.into();
        self.power_restart = restart.into();
        self.power_shutdown = shutdown.into();
        self
    }
}

impl Default for Launcher {
    fn default() -> Self {
        Self::new()
    }
}

fn power_row(
    theme: &Theme,
    width: Pixels,
    on_power: Option<PowerHandler>,
    labels: &[String; 3],
) -> Div {
    let mut row = div().flex().gap_1().w(width).justify_between();
    for (i, (fallback, action)) in POWER_ITEMS.iter().enumerate() {
        let cb = on_power.clone();
        let action = *action;
        let label = labels.get(i).map(|s| s.as_str()).unwrap_or(*fallback);
        let glyph = match action {
            PowerAction::Lock => IconName::Lock,
            PowerAction::Restart => IconName::RotateCw,
            PowerAction::Shutdown => IconName::Power,
        };
        let mut b = Button::new(format!("launch-power-{i}"))
            .themed(theme)
            .small()
            .icon(glyph)
            .label(label);
        b = if matches!(action, PowerAction::Shutdown) {
            b.danger()
        } else {
            b.ghost()
        };
        if let Some(cb) = cb {
            b = b.on_press(move || cb(action));
        }
        row = row.child(b);
    }
    row
}

fn categorized_list(
    theme: &Theme,
    width: Pixels,
    apps: &[LauncherApp],
    highlight: usize,
    on_select: Option<IndexHandler>,
) -> Div {
    let mut list = div().flex_col().gap_1().w(width);
    let mut any = false;
    for cat in LauncherCategory::all() {
        let rows: Vec<(usize, &LauncherApp)> = apps
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                if cat == LauncherCategory::Pinned {
                    a.in_pinned()
                } else {
                    !a.in_pinned() && a.category == cat
                }
            })
            .collect();
        if rows.is_empty() {
            continue;
        }
        any = true;
        list = list.child(Separator::horizontal().themed(theme).label(cat.label()));
        for (i, app) in rows {
            let pick = on_select.clone();
            list = list.child(
                Button::new(format!("launch-app-{i}"))
                    .themed(theme)
                    .ghost()
                    .small()
                    .align_start()
                    .w(width)
                    .icon(app.icon)
                    .label(app.name.clone())
                    .selected(highlight == i)
                    .on_press(move || {
                        if let Some(p) = &pick {
                            p(i);
                        }
                    }),
            );
        }
    }
    if !any {
        list = list.child(
            Empty::new()
                .themed(theme)
                .title("No apps")
                .description("Nothing is installed yet"),
        );
    }
    list
}

impl IntoElement for Launcher {
    type Element = Div;

    fn into_element(self) -> Div {
        let q = self.query.value();
        let searching = !q.is_empty();
        let n_apps = self.apps.len();
        let pad = self.theme.spacing.md.0;
        let content_w = px((self.width.0 - pad * 2).max(200));
        let list_h = px((self.height.0 - 48 - 40 - pad * 2).max(80));
        let power_labels = [
            self.power_lock.clone(),
            self.power_restart.clone(),
            self.power_shutdown.clone(),
        ];
        let power = power_row(&self.theme, content_w, self.on_power.clone(), &power_labels);
        let dismiss = self.on_dismiss.clone();

        let main = if searching {
            let mut items: Vec<String> = self.apps.iter().map(|a| a.name.clone()).collect();
            let mut keywords: Vec<String> = self
                .apps
                .iter()
                .map(|a| {
                    let mut s = a.keywords.clone();
                    if !a.description.is_empty() {
                        if !s.is_empty() {
                            s.push(' ');
                        }
                        s.push_str(&a.description);
                    }
                    s
                })
                .collect();
            items.extend(power_labels.iter().cloned());
            keywords.push(String::from("lock log out session"));
            keywords.push(String::from("reboot restart"));
            keywords.push(String::from("shutdown power off"));
            let pick = self.on_select.clone();
            let power_cb = self.on_power.clone();
            Command::new()
                .themed(&self.theme)
                .inline()
                .open(true)
                .placeholder(self.search_placeholder.clone())
                .w(content_w)
                .query(self.query)
                .items(items)
                .keywords(keywords)
                .highlight(self.highlight)
                .on_query({
                    let on_q = self.on_query.clone();
                    move |s| {
                        if let Some(h) = &on_q {
                            h(s);
                        }
                    }
                })
                .on_select(move |i| {
                    if i < n_apps {
                        if let Some(p) = &pick {
                            p(i);
                        }
                    } else if let Some(cb) = &power_cb
                        && let Some((_, action)) = POWER_ITEMS.get(i - n_apps)
                    {
                        cb(*action);
                    }
                })
                .on_highlight({
                    let hi = self.on_highlight.clone();
                    move |i| {
                        if let Some(h) = &hi {
                            h(i);
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
                })
                .into_element()
        } else {
            let shown: Vec<usize> = (0..n_apps).collect();
            let highlight = if n_apps == 0 {
                0
            } else {
                self.highlight.min(n_apps - 1)
            };
            let mut field = Input::new("cmd-q")
                .themed(&self.theme)
                .placeholder(self.search_placeholder)
                .w(content_w)
                .state(self.query);
            if let Some(on_q) = self.on_query.clone() {
                field = field.on_change(move |s| on_q(s));
            }
            let pick = self.on_select.clone();
            let hi = self.on_highlight.clone();
            let close = self.on_dismiss.clone();
            field = field.on_key(move |e| {
                if let Some(nav) = pick_with_keys(e, &shown, highlight) {
                    match nav {
                        ListNav::Highlight(h) => {
                            if let Some(cb) = &hi {
                                cb(h);
                            }
                        }
                        ListNav::Select(orig) => {
                            if let Some(p) = &pick {
                                p(orig);
                            }
                        }
                        ListNav::Close => {
                            if let Some(d) = &close {
                                d();
                            }
                        }
                    }
                    return true;
                }
                false
            });
            let mut list = ScrollArea::new()
                .themed(&self.theme)
                .offset(self.scroll)
                .size(content_w, list_h)
                .child(categorized_list(
                    &self.theme,
                    content_w,
                    &self.apps,
                    self.highlight,
                    self.on_select.clone(),
                ));
            if let Some(on) = self.on_scroll {
                list = list.on_offset(move |y| on(y));
            }
            div()
                .flex_col()
                .gap_2()
                .w(content_w)
                .child(field)
                .child(list)
        };

        let card = div()
            .flex_col()
            .w(self.width)
            .h(self.height)
            .p(self.theme.spacing.md)
            .gap_2()
            .bg(self.theme.colors.surface.opacity(0.96))
            .border_1()
            .border_color(self.theme.colors.border)
            .rounded(self.theme.radius.xl)
            .overflow_hidden()
            .child(main)
            .child(power);
        let trap = if let Some(d) = dismiss {
            FocusTrap::new(card).on_escape(move || d())
        } else {
            FocusTrap::new(card)
        };
        div().child(trap)
    }
}

#[cfg(test)]
mod tests {
    use super::{Launcher, LauncherApp, LauncherCategory, PowerAction};
    use crate::command::fuzzy_filter_with;
    use crate::input::FieldState;
    use crate::theme::Theme;
    use alloc::string::String;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element,
    };

    fn sample_apps() -> [LauncherApp; 3] {
        [
            LauncherApp::new("Terminal", LauncherCategory::Pinned).keywords("shell bash cli"),
            LauncherApp::new("Files", LauncherCategory::Pinned).keywords("folder explorer"),
            LauncherApp::new("Settings", LauncherCategory::System)
                .pinned(true)
                .keywords("preferences config"),
        ]
    }

    #[test]
    fn search_filters_terminal() {
        let names = [
            String::from("Terminal"),
            String::from("Files"),
            String::from("Settings"),
        ];
        let extra = [
            String::from("shell bash"),
            String::from("folder"),
            String::from("config"),
        ];
        let ranked = fuzzy_filter_with("term", &names, &extra);
        assert_eq!(ranked.first().copied(), Some(0));
        let shell = fuzzy_filter_with("shell", &names, &extra);
        assert_eq!(shell.first().copied(), Some(0));
        let _ = Launcher::new()
            .themed(&Theme::aurora_dark())
            .apps(sample_apps())
            .query(FieldState::from("te"))
            .into_element();
    }

    #[test]
    fn click_launches_terminal() {
        let hit = Arc::new(AtomicUsize::new(99));
        let idx = hit.clone();
        let mut el = Launcher::new()
            .themed(&Theme::aurora_dark())
            .apps(sample_apps())
            .on_select(move |i| idx.store(i, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(480, 400), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .find(|h| h.bounds.y() > 40 && h.tab_stop)
            .map(|h| Point::xy(h.bounds.x() + 8, h.bounds.y() + 8))
            .expect("app row");
        dispatch_mouse_down(&mut el, &mut input, p);
        assert_eq!(hit.load(Ordering::SeqCst), 0);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert_eq!(hit.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn power_shutdown_fires() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let mut el = Launcher::new()
            .themed(&Theme::aurora_dark())
            .apps(sample_apps())
            .on_power(move |a| {
                if a == PowerAction::Shutdown {
                    flag.store(true, Ordering::SeqCst);
                }
            })
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(480, 400), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .rev()
            .find(|h| h.tab_stop)
            .map(|h| Point::xy(h.bounds.x() + 8, h.bounds.y() + 8))
            .expect("power row");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert!(hit.load(Ordering::SeqCst));
    }
}
