// SPDX-License-Identifier: MIT
//! Session Alt-Tab switcher and Mission Control overview (D9.6).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS shell chrome, not a kit catalog control.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, FluentBuilder, IntoElement, Pixels, Styled, div, px, text};

use crate::icon::{Icon, IconName};
use crate::interact::Handler;
use crate::overlay::FocusTrap;
use crate::size::Sizable;
use crate::theme::Theme;

type IdHandler = Arc<dyn Fn(u32) + Send + Sync>;
type IndexHandler = Arc<dyn Fn(usize) + Send + Sync>;

/// One window in the Alt-Tab strip or overview grid.
#[derive(Clone, Debug)]
pub struct SwitcherEntry {
    pub id: u32,
    pub title: String,
    pub icon: IconName,
}

impl SwitcherEntry {
    pub fn new(id: u32, title: impl Into<String>, icon: IconName) -> Self {
        Self {
            id,
            title: title.into(),
            icon,
        }
    }
}

fn clip_title(title: &str, max_chars: usize) -> String {
    let n = title.chars().count();
    if n <= max_chars {
        return String::from(title);
    }
    let take = max_chars.saturating_sub(2);
    let mut out: String = title.chars().take(take).collect();
    out.push_str("..");
    out
}

fn thumb(
    theme: &Theme,
    entry: &SwitcherEntry,
    selected: bool,
    width: i32,
    height: i32,
    on_select: Option<IdHandler>,
) -> Div {
    let id = entry.id;
    let pick = on_select;
    let bg = if selected {
        theme.colors.accent
    } else {
        theme.colors.muted
    };
    let border = if selected {
        theme.colors.primary
    } else {
        theme.colors.border
    };
    let icon_color = if selected {
        theme.colors.primary
    } else {
        theme.colors.muted_foreground
    };
    div()
        .w(px(width))
        .h(px(height))
        .flex()
        .items_center()
        .justify_center()
        .rounded(theme.radius.lg)
        .bg(bg)
        .border_1()
        .border_color(border)
        .hover(|s| s.bg(theme.colors.accent).border_color(theme.colors.ring))
        .on_click(move |_| {
            if let Some(h) = &pick {
                h(id);
            }
        })
        .child(
            Icon::new(entry.icon)
                .themed(theme)
                .large()
                .color(icon_color),
        )
}

/// Centered Alt-Tab window strip.
pub struct AltTab {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    entries: Vec<SwitcherEntry>,
    selected: usize,
    on_select: Option<IdHandler>,
    on_dismiss: Option<Handler>,
}

impl AltTab {
    pub const THUMB_W: i32 = 120;
    pub const THUMB_H: i32 = 90;
    pub const TITLE_H: i32 = 24;
    pub const PAD: i32 = 16;
    pub const GAP: i32 = 12;
    pub const MAX: usize = 8;

    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(Self::THUMB_W + Self::PAD * 2),
            height: px(Self::THUMB_H + Self::TITLE_H + Self::PAD * 2 + 8),
            entries: Vec::new(),
            selected: 0,
            on_select: None,
            on_dismiss: None,
        }
    }

    /// Backing-store size for `n` visible entries.
    pub fn panel_size(n: usize) -> (i32, i32) {
        let n = n.clamp(1, Self::MAX) as i32;
        let w = n * (Self::THUMB_W + Self::GAP) - Self::GAP + Self::PAD * 2;
        let h = Self::THUMB_H + Self::TITLE_H + Self::PAD * 2 + 8;
        (w, h)
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

    pub fn entries(mut self, entries: impl IntoIterator<Item = SwitcherEntry>) -> Self {
        self.entries = entries.into_iter().take(Self::MAX).collect();
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    pub fn on_select(mut self, f: impl Fn(u32) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }
}

impl Default for AltTab {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for AltTab {
    type Element = Div;

    fn into_element(self) -> Div {
        let n = self.entries.len();
        let selected = if n == 0 { 0 } else { self.selected.min(n - 1) };
        let max_chars = (Self::THUMB_W / 7) as usize;
        let mut row = div().flex().gap(px(Self::GAP)).items_start();
        for (i, entry) in self.entries.iter().enumerate() {
            let title = clip_title(&entry.title, max_chars);
            let title_color = if i == selected {
                self.theme.colors.foreground
            } else {
                self.theme.colors.muted_foreground
            };
            row = row.child(
                div()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .w(px(Self::THUMB_W))
                    .child(thumb(
                        &self.theme,
                        entry,
                        i == selected,
                        Self::THUMB_W,
                        Self::THUMB_H,
                        self.on_select.clone(),
                    ))
                    .child(text(title).color(title_color).size(12)),
            );
        }
        let card = div()
            .w(self.width)
            .h(self.height)
            .flex()
            .items_center()
            .justify_center()
            .p(px(Self::PAD))
            .bg(self.theme.colors.surface.opacity(0.96))
            .border_1()
            .border_color(self.theme.colors.border)
            .rounded(self.theme.radius.xl)
            .child(row);
        let trap = if let Some(d) = self.on_dismiss {
            FocusTrap::new(card).on_escape(move || d())
        } else {
            FocusTrap::new(card)
        };
        div().w(self.width).h(self.height).child(trap)
    }
}

/// Full-desktop Mission Control grid.
pub struct Overview {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    entries: Vec<SwitcherEntry>,
    selected: Option<usize>,
    on_select: Option<IdHandler>,
    on_highlight: Option<IndexHandler>,
    on_dismiss: Option<Handler>,
}

impl Overview {
    pub const TITLE_H: i32 = 40;
    pub const HINT_H: i32 = 28;
    pub const PAD: i32 = 24;
    pub const GAP: i32 = 16;
    pub const TILE_W: i32 = 220;
    pub const LABEL_H: i32 = 24;
    pub const PREVIEW_H: i32 = 144;
    pub const TILE_H: i32 = Self::PREVIEW_H + Self::LABEL_H;

    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(Self::TILE_W + Self::PAD * 2),
            height: px(Self::TITLE_H + Self::TILE_H + Self::HINT_H + Self::PAD * 2),
            entries: Vec::new(),
            selected: None,
            on_select: None,
            on_highlight: None,
            on_dismiss: None,
        }
    }

    /// Columns for an `n`-window grid (same breakpoints as the old expose painter).
    pub fn columns(n: usize) -> usize {
        if n <= 1 {
            1
        } else if n <= 4 {
            2
        } else if n <= 9 {
            3
        } else {
            n.div_ceil(3).clamp(2, 6)
        }
    }

    /// Backing-store size for `n` tiles.
    pub fn panel_size(n: usize) -> (i32, i32) {
        let n = n.max(1);
        let cols = Self::columns(n) as i32;
        let rows = n.div_ceil(Self::columns(n)) as i32;
        let grid_w = cols * Self::TILE_W + (cols - 1) * Self::GAP;
        let grid_h = rows * Self::TILE_H + (rows - 1) * Self::GAP;
        (
            grid_w + Self::PAD * 2,
            Self::TITLE_H + grid_h + Self::HINT_H + Self::PAD * 2,
        )
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

    pub fn entries(mut self, entries: impl IntoIterator<Item = SwitcherEntry>) -> Self {
        self.entries = entries.into_iter().collect();
        self
    }

    pub fn selected(mut self, index: Option<usize>) -> Self {
        self.selected = index;
        self
    }

    pub fn on_select(mut self, f: impl Fn(u32) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }

    pub fn on_highlight(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_highlight = Some(Arc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }
}

impl Default for Overview {
    fn default() -> Self {
        Self::new()
    }
}

fn overview_tile(
    theme: &Theme,
    entry: &SwitcherEntry,
    index: usize,
    selected: bool,
    on_select: Option<IdHandler>,
    on_highlight: Option<IndexHandler>,
) -> Div {
    let pick = on_select;
    let hi = on_highlight;
    let max_chars = (Overview::TILE_W / 7) as usize;
    let title = clip_title(&entry.title, max_chars);
    let title_color = if selected {
        theme.colors.foreground
    } else {
        theme.colors.muted_foreground
    };
    let preview = thumb(
        theme,
        entry,
        selected,
        Overview::TILE_W,
        Overview::PREVIEW_H,
        pick,
    )
    .when(hi.is_some(), |el| {
        let hi = hi.clone();
        el.on_mouse_move(move |_| {
            if let Some(h) = &hi {
                h(index);
            }
        })
    });
    div()
        .flex_col()
        .items_center()
        .gap_1()
        .w(px(Overview::TILE_W))
        .h(px(Overview::TILE_H))
        .child(preview)
        .child(text(title).color(title_color).size(12))
}

impl IntoElement for Overview {
    type Element = Div;

    fn into_element(self) -> Div {
        let n = self.entries.len();
        let cols = Self::columns(n.max(1));
        let mut grid = div().flex_col().gap(px(Self::GAP));
        let mut row = div().flex().gap(px(Self::GAP));
        for (i, entry) in self.entries.iter().enumerate() {
            if i > 0 && i % cols == 0 {
                grid = grid.child(row);
                row = div().flex().gap(px(Self::GAP));
            }
            row = row.child(overview_tile(
                &self.theme,
                entry,
                i,
                self.selected == Some(i),
                self.on_select.clone(),
                self.on_highlight.clone(),
            ));
        }
        if n > 0 {
            grid = grid.child(row);
        }
        let body = div()
            .w(self.width)
            .h(self.height)
            .flex_col()
            .items_center()
            .p(px(Self::PAD))
            .bg(self.theme.colors.surface.opacity(0.94))
            .border_1()
            .border_color(self.theme.colors.border)
            .rounded(self.theme.radius.xl)
            .child(
                text(String::from("Mission Control"))
                    .color(self.theme.colors.foreground)
                    .size(16),
            )
            .child(grid)
            .child(
                text(String::from(
                    "Click a window to focus  |  Esc to close  |  Super+E to toggle",
                ))
                .color(self.theme.colors.muted_foreground)
                .size(12),
            );
        let trap = if let Some(d) = self.on_dismiss {
            FocusTrap::new(body).on_escape(move || d())
        } else {
            FocusTrap::new(body)
        };
        div().w(self.width).h(self.height).child(trap)
    }
}

#[cfg(test)]
mod tests {
    use super::{AltTab, Overview, SwitcherEntry};
    use crate::icon::IconName;
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use knox_gpui::{
        IntoElement, Key, KeyDownEvent, Modifiers, Point, Size, WindowInput, dispatch_key_down,
        dispatch_mouse_down, dispatch_mouse_up, prepare_element,
    };

    fn entries() -> [SwitcherEntry; 2] {
        [
            SwitcherEntry::new(11, "Terminal", IconName::SquareTerminal),
            SwitcherEntry::new(22, "Files", IconName::Folder),
        ]
    }

    fn click_at(el: &mut knox_gpui::Div, input: &mut WindowInput, p: Point) {
        dispatch_mouse_down(el, input, p);
        dispatch_mouse_up(el, input, p);
    }

    #[test]
    fn alt_tab_size_scales_with_entries() {
        let (w1, h1) = AltTab::panel_size(1);
        let (w2, h2) = AltTab::panel_size(2);
        assert!(w2 > w1);
        assert_eq!(h1, h2);
        assert_eq!(AltTab::panel_size(99), AltTab::panel_size(AltTab::MAX));
    }

    #[test]
    fn overview_columns_match_legacy_breakpoints() {
        assert_eq!(Overview::columns(1), 1);
        assert_eq!(Overview::columns(2), 2);
        assert_eq!(Overview::columns(4), 2);
        assert_eq!(Overview::columns(6), 3);
        assert_eq!(Overview::columns(9), 3);
        assert!(Overview::columns(12) >= 2);
        assert!(Overview::columns(12) <= 6);
        let (w, h) = Overview::panel_size(4);
        assert!(w > Overview::TILE_W);
        assert!(h > Overview::TILE_H);
    }

    #[test]
    fn alt_tab_click_selects_window() {
        let hit = Arc::new(AtomicU32::new(0));
        let id = hit.clone();
        let (w, h) = AltTab::panel_size(2);
        let mut el = AltTab::new()
            .themed(&Theme::aurora_dark())
            .size(knox_gpui::px(w), knox_gpui::px(h))
            .entries(entries())
            .selected(1)
            .on_select(move |wid| id.store(wid, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(w, h), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .find(|hb| hb.tab_stop && hb.bounds.width() >= AltTab::THUMB_W - 4)
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("alt-tab thumb");
        click_at(&mut el, &mut input, p);
        assert_eq!(hit.load(Ordering::SeqCst), 11);
    }

    #[test]
    fn overview_click_focuses_window() {
        let hit = Arc::new(AtomicU32::new(0));
        let id = hit.clone();
        let (w, h) = Overview::panel_size(2);
        let mut el = Overview::new()
            .themed(&Theme::aurora_dark())
            .size(knox_gpui::px(w), knox_gpui::px(h))
            .entries(entries())
            .on_select(move |wid| id.store(wid, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(w, h), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .find(|hb| hb.tab_stop && hb.bounds.width() >= Overview::TILE_W - 8)
            .map(|hb| Point::xy(hb.bounds.x() + 12, hb.bounds.y() + 12))
            .expect("overview tile");
        click_at(&mut el, &mut input, p);
        assert_eq!(hit.load(Ordering::SeqCst), 11);
    }

    #[test]
    fn escape_dismisses_alt_tab() {
        let hit = Arc::new(AtomicBool::new(false));
        let flag = hit.clone();
        let (w, h) = AltTab::panel_size(1);
        let mut el = AltTab::new()
            .themed(&Theme::aurora_dark())
            .size(knox_gpui::px(w), knox_gpui::px(h))
            .entries([SwitcherEntry::new(1, "Terminal", IconName::SquareTerminal)])
            .on_dismiss(move || flag.store(true, Ordering::SeqCst))
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
}
