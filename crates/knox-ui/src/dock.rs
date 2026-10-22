// SPDX-License-Identifier: MIT
//! Resizable split + DockArea (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. Layout serializes to a compact byte dump for VFS round-trip.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, FluentBuilder, IntoElement, Pixels, Styled, div, px};

use crate::nav::TabBar;
use crate::separator::Separator;
use crate::theme::Theme;

/// Floating session-dock height (macOS-style pill, not the IDE [`DockArea`]).
pub const DOCK_BAR_H: i32 = 52;

/// Split axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

/// Serializable dock tree (tabs / splits).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DockLayout {
    Leaf {
        tabs: Vec<String>,
        active: u8,
    },
    Split {
        axis: SplitAxis,
        /// 0..=1000 (permille) size of the first pane.
        ratio: u16,
        first: alloc::boxed::Box<DockLayout>,
        second: alloc::boxed::Box<DockLayout>,
    },
}

impl DockLayout {
    pub fn leaf(tabs: impl IntoIterator<Item = impl Into<String>>, active: u8) -> Self {
        Self::Leaf {
            tabs: tabs.into_iter().map(Into::into).collect(),
            active,
        }
    }

    pub fn split(axis: SplitAxis, ratio: u16, first: Self, second: Self) -> Self {
        Self::Split {
            axis,
            ratio: ratio.min(1000),
            first: alloc::boxed::Box::new(first),
            second: alloc::boxed::Box::new(second),
        }
    }

    /// Compact encoding for VFS. Not a public file format.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        encode(self, &mut out);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let (node, rest) = decode(bytes)?;
        if rest.is_empty() { Some(node) } else { None }
    }
}

fn encode(node: &DockLayout, out: &mut Vec<u8>) {
    match node {
        DockLayout::Leaf { tabs, active } => {
            out.push(0);
            out.push(*active);
            out.push(tabs.len() as u8);
            for t in tabs {
                let b = t.as_bytes();
                out.push(b.len() as u8);
                out.extend_from_slice(b);
            }
        }
        DockLayout::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            out.push(1);
            out.push(match axis {
                SplitAxis::Horizontal => 0,
                SplitAxis::Vertical => 1,
            });
            out.extend_from_slice(&ratio.to_le_bytes());
            encode(first, out);
            encode(second, out);
        }
    }
}

fn decode(bytes: &[u8]) -> Option<(DockLayout, &[u8])> {
    let (kind, rest) = bytes.split_first()?;
    match kind {
        0 => {
            let (active, rest) = rest.split_first()?;
            let (n, mut rest) = rest.split_first()?;
            let mut tabs = Vec::new();
            for _ in 0..*n {
                let (len, r) = rest.split_first()?;
                let len = *len as usize;
                if r.len() < len {
                    return None;
                }
                let (s, r) = r.split_at(len);
                tabs.push(String::from_utf8(s.to_vec()).ok()?);
                rest = r;
            }
            Some((
                DockLayout::Leaf {
                    tabs,
                    active: *active,
                },
                rest,
            ))
        }
        1 => {
            let (axis_b, rest) = rest.split_first()?;
            let axis = match axis_b {
                0 => SplitAxis::Horizontal,
                1 => SplitAxis::Vertical,
                _ => return None,
            };
            if rest.len() < 2 {
                return None;
            }
            let ratio = u16::from_le_bytes([rest[0], rest[1]]);
            let rest = &rest[2..];
            let (first, rest) = decode(rest)?;
            let (second, rest) = decode(rest)?;
            Some((DockLayout::split(axis, ratio, first, second), rest))
        }
        _ => None,
    }
}

/// Two-pane split with a draggable gutter (kit `Resizable`).
pub struct Resizable {
    theme: Theme,
    axis: SplitAxis,
    ratio: u16,
    width: Pixels,
    height: Pixels,
    first: Option<Div>,
    second: Option<Div>,
    on_ratio: Option<Arc<dyn Fn(u16) + Send + Sync>>,
}

impl Resizable {
    pub fn new(axis: SplitAxis) -> Self {
        Self {
            theme: Theme::default(),
            axis,
            ratio: 500,
            width: px(320),
            height: px(160),
            first: None,
            second: None,
            on_ratio: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn ratio(mut self, ratio: u16) -> Self {
        self.ratio = ratio.min(1000);
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn first(mut self, child: impl IntoElement) -> Self {
        self.first = Some(div().child(child));
        self
    }

    pub fn second(mut self, child: impl IntoElement) -> Self {
        self.second = Some(div().child(child));
        self
    }

    pub fn on_ratio(mut self, f: impl Fn(u16) + Send + Sync + 'static) -> Self {
        self.on_ratio = Some(Arc::new(f));
        self
    }
}

impl IntoElement for Resizable {
    type Element = Div;

    fn into_element(self) -> Div {
        let horiz = self.axis == SplitAxis::Horizontal;
        let a = if horiz {
            (self.width.0 * self.ratio as i32) / 1000
        } else {
            (self.height.0 * self.ratio as i32) / 1000
        };
        let mut first = self.first.unwrap_or_else(div);
        let mut second = self.second.unwrap_or_else(div);
        if horiz {
            first = first.w(px(a.max(24))).h(self.height);
            second = second.grow().h(self.height);
        } else {
            first = first.h(px(a.max(24))).w(self.width);
            second = second.grow().w(self.width);
        }
        let mut gutter = div()
            .bg(self.theme.colors.border)
            .when(horiz, |d| d.w(px(4)).h(self.height))
            .when(!horiz, |d| d.h(px(4)).w(self.width));
        if let Some(on) = self.on_ratio {
            let width = self.width.0.max(1);
            let height = self.height.0.max(1);
            gutter = gutter.on_click(move |e| {
                let r = if horiz {
                    ((e.position.x.0 * 1000) / width) as u16
                } else {
                    ((e.position.y.0 * 1000) / height) as u16
                };
                on(r.min(900).max(100));
            });
        }
        let root = if horiz {
            div().flex()
        } else {
            div().flex_col()
        };
        root.w(self.width)
            .h(self.height)
            .child(first)
            .child(gutter)
            .child(second)
    }
}

/// Dock workspace: tabbed leaves inside splits (kit `DockArea`).
pub struct DockArea {
    theme: Theme,
    layout: DockLayout,
    on_tab: Option<Arc<dyn Fn(usize) + Send + Sync>>,
}

impl DockArea {
    pub fn new(layout: DockLayout) -> Self {
        Self {
            theme: Theme::default(),
            layout,
            on_tab: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn on_tab(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_tab = Some(Arc::new(f));
        self
    }
}

fn paint_layout(
    theme: &Theme,
    layout: DockLayout,
    on_tab: Option<Arc<dyn Fn(usize) + Send + Sync>>,
) -> Div {
    match layout {
        DockLayout::Leaf { tabs, active } => {
            let mut bar = TabBar::new("dock-tabs")
                .themed(theme)
                .labels(tabs)
                .selected(active as usize);
            if let Some(on) = on_tab {
                bar = bar.on_select(move |i| on(i));
            }
            div()
                .flex_col()
                .grow()
                .bg(theme.colors.surface)
                .border_1()
                .border_color(theme.colors.border)
                .child(bar)
        }
        DockLayout::Split {
            axis,
            ratio,
            first,
            second,
        } => Resizable::new(axis)
            .themed(theme)
            .ratio(ratio)
            .first(paint_layout(theme, *first, on_tab.clone()))
            .second(paint_layout(theme, *second, on_tab))
            .into_element(),
    }
}

impl IntoElement for DockArea {
    type Element = Div;

    fn into_element(self) -> Div {
        paint_layout(&self.theme, self.layout, self.on_tab)
    }
}

/// Click-to-nudge helper used by tests / gallery when a drag starts.
pub fn nudge_ratio(ratio: u16, delta: i32) -> u16 {
    let v = ratio as i32 + delta;
    v.clamp(100, 900) as u16
}

/// Session dock chrome: start | running apps | tray.
///
/// Distinct from [`DockArea`] (IDE splits). The live panel paints this.
pub struct Dock {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    leading: Vec<Div>,
    items: Vec<Div>,
    trailing: Vec<Div>,
}

impl Dock {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(560),
            height: px(DOCK_BAR_H),
            leading: Vec::new(),
            items: Vec::new(),
            trailing: Vec::new(),
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

    pub fn add_leading(mut self, child: impl IntoElement) -> Self {
        self.leading.push(div().child(child));
        self
    }

    pub fn add_item(mut self, child: impl IntoElement) -> Self {
        self.items.push(div().child(child));
        self
    }

    pub fn add_trailing(mut self, child: impl IntoElement) -> Self {
        self.trailing.push(div().child(child));
        self
    }
}

impl Default for Dock {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Dock {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut left = div().flex().gap_1().items_center();
        for child in self.leading {
            left = left.child(child);
        }
        if !self.items.is_empty() {
            left = left.child(
                div()
                    .ml(self.theme.spacing.xs)
                    .mr(self.theme.spacing.xs)
                    .child(Separator::vertical().themed(&self.theme)),
            );
            for child in self.items {
                left = left.child(child);
            }
        }
        let mut right = div().flex().gap_1().items_center();
        for (i, child) in self.trailing.into_iter().enumerate() {
            if i == 0 {
                right = right.child(
                    div()
                        .mr(self.theme.spacing.xs)
                        .child(Separator::vertical().themed(&self.theme)),
                );
            }
            right = right.child(child);
        }
        div()
            .flex()
            .items_center()
            .justify_between()
            .w(self.width)
            .h(self.height)
            .pad_x(self.theme.spacing.sm)
            .pad_y(self.theme.spacing.xs)
            .bg(self.theme.colors.surface.opacity(0.94))
            .border_1()
            .border_color(self.theme.colors.border)
            .rounded(self.theme.radius.xl)
            .child(left)
            .child(right)
    }
}

#[cfg(test)]
mod tests {
    use super::{Dock, DockArea, DockLayout, Resizable, SplitAxis, nudge_ratio};
    use crate::theme::Theme;
    use knox_gpui::{IntoElement, px, text};

    #[test]
    fn layout_round_trips() {
        let layout = DockLayout::split(
            SplitAxis::Horizontal,
            400,
            DockLayout::leaf(["Code"], 0),
            DockLayout::leaf(["Term", "Log"], 1),
        );
        let bytes = layout.to_bytes();
        assert_eq!(DockLayout::from_bytes(&bytes), Some(layout.clone()));
        assert_eq!(nudge_ratio(500, 40), 540);
        let theme = Theme::aurora_dark();
        let _ = Resizable::new(SplitAxis::Horizontal)
            .themed(&theme)
            .size(px(200), px(80))
            .first(text("a"))
            .second(text("b"))
            .into_element();
        let _ = DockArea::new(layout).themed(&theme).into_element();
        let _ = Dock::new()
            .themed(&theme)
            .add_leading(text("start"))
            .add_item(text("app"))
            .add_trailing(text("tray"))
            .into_element();
    }
}
