// SPDX-License-Identifier: MIT
//! List and Tree (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Key, Styled, div, px};

use crate::icon::{Icon, IconName};
use crate::interact::IndexHandler;
use crate::label::Label;
use crate::size::Sizable;
use crate::theme::Theme;

/// Flat selectable list (kit `List`).
pub struct List {
    theme: Theme,
    items: Vec<String>,
    selected: Option<usize>,
    on_select: Option<IndexHandler>,
}

impl List {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            items: Vec::new(),
            selected: None,
            on_select: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn items<I, S>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.items = items.into_iter().map(Into::into).collect();
        self
    }

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }
}

impl Default for List {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for List {
    type Element = Div;

    fn into_element(self) -> Div {
        let n = self.items.len();
        let selected = self.selected;
        let mut col = div().flex_col().w(px(200)).focusable();
        for (i, label) in self.items.into_iter().enumerate() {
            let bg = if selected == Some(i) {
                self.theme.colors.accent.opacity(0.25)
            } else {
                knox_gpui::Hsla::TRANSPARENT
            };
            let pick = self.on_select.clone();
            let keys = self.on_select.clone();
            col = col.child(
                div()
                    .h(px(22))
                    .p(self.theme.spacing.xs)
                    .bg(bg)
                    .child(
                        Label::new(label)
                            .themed(&self.theme)
                            .size(self.theme.typo.caption),
                    )
                    .on_click(move |_| {
                        if let Some(p) = &pick {
                            p(i);
                        }
                    })
                    .on_key_down(move |e| {
                        if n == 0 {
                            return false;
                        }
                        let cur = selected.unwrap_or(0);
                        let next = match e.key {
                            Key::Down => Some((cur + 1).min(n - 1)),
                            Key::Up => Some(cur.saturating_sub(1)),
                            Key::Enter => Some(cur),
                            _ => None,
                        };
                        if let Some(j) = next {
                            if let Some(p) = &keys {
                                p(j);
                            }
                            true
                        } else {
                            false
                        }
                    }),
            );
        }
        let pick = self.on_select;
        col.on_key_down(move |e| {
            if n == 0 {
                return false;
            }
            let cur = selected.unwrap_or(0);
            let next = match e.key {
                Key::Down => Some((cur + 1).min(n - 1)),
                Key::Up => Some(cur.saturating_sub(1)),
                Key::Enter => Some(cur),
                _ => None,
            };
            if let Some(i) = next {
                if let Some(p) = &pick {
                    p(i);
                }
                true
            } else {
                false
            }
        })
    }
}

/// Tree node. `expanded` is ignored when `children` is empty.
#[derive(Clone, Debug)]
pub struct TreeNode {
    pub label: String,
    pub expanded: bool,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn leaf(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            expanded: false,
            children: Vec::new(),
        }
    }

    pub fn branch(label: impl Into<String>, children: impl IntoIterator<Item = TreeNode>) -> Self {
        Self {
            label: label.into(),
            expanded: true,
            children: children.into_iter().collect(),
        }
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }
}

/// Flattened row for keyboard / click.
#[derive(Clone, Copy, Debug)]
pub struct TreeRow {
    pub path: &'static [usize],
}

/// Toggle a node at a depth-first index among visible rows.
pub fn toggle_visible(nodes: &mut [TreeNode], visible_index: usize) {
    let mut n = 0;
    fn walk(nodes: &mut [TreeNode], target: usize, n: &mut usize) -> bool {
        for node in nodes.iter_mut() {
            if *n == target {
                if !node.children.is_empty() {
                    node.expanded = !node.expanded;
                }
                return true;
            }
            *n += 1;
            if node.expanded && walk(&mut node.children, target, n) {
                return true;
            }
        }
        false
    }
    walk(nodes, visible_index, &mut n);
}

fn visible_count(nodes: &[TreeNode]) -> usize {
    let mut n = 0;
    for node in nodes {
        n += 1;
        if node.expanded {
            n += visible_count(&node.children);
        }
    }
    n
}

/// Hierarchical list (kit `Tree`).
pub struct Tree {
    theme: Theme,
    nodes: Vec<TreeNode>,
    selected: usize,
    on_select: Option<IndexHandler>,
    on_toggle: Option<IndexHandler>,
}

impl Tree {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            nodes: Vec::new(),
            selected: 0,
            on_select: None,
            on_toggle: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn nodes(mut self, nodes: impl IntoIterator<Item = TreeNode>) -> Self {
        self.nodes = nodes.into_iter().collect();
        self
    }

    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_toggle = Some(Arc::new(f));
        self
    }
}

impl Default for Tree {
    fn default() -> Self {
        Self::new()
    }
}

fn paint_nodes(
    theme: &Theme,
    nodes: &[TreeNode],
    depth: i32,
    idx: &mut usize,
    selected: usize,
    on_select: &Option<IndexHandler>,
    on_toggle: &Option<IndexHandler>,
    out: &mut Div,
) {
    for node in nodes {
        let i = *idx;
        *idx += 1;
        let bg = if i == selected {
            theme.colors.accent.opacity(0.25)
        } else {
            knox_gpui::Hsla::TRANSPARENT
        };
        let pick = on_select.clone();
        let tog = on_toggle.clone();
        let expandable = !node.children.is_empty();
        let mut row = div()
            .flex()
            .items_center()
            .gap_1()
            .h(px(20))
            .ml(px(depth * 12))
            .bg(bg);
        if expandable {
            let caret = if node.expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            };
            row = row.child(
                Icon::new(caret)
                    .themed(theme)
                    .color(theme.colors.muted_foreground)
                    .small(),
            );
        }
        row = row.child(
            Label::new(node.label.clone())
                .themed(theme)
                .size(theme.typo.caption),
        );
        out.add_child(row.on_click(move |_| {
            if expandable {
                if let Some(t) = &tog {
                    t(i);
                }
            }
            if let Some(p) = &pick {
                p(i);
            }
        }));
        if node.expanded {
            paint_nodes(
                theme,
                &node.children,
                depth + 1,
                idx,
                selected,
                on_select,
                on_toggle,
                out,
            );
        }
    }
}

impl IntoElement for Tree {
    type Element = Div;

    fn into_element(self) -> Div {
        let n = visible_count(&self.nodes);
        let mut col = div().flex_col().w(px(220)).focusable();
        let mut idx = 0;
        paint_nodes(
            &self.theme,
            &self.nodes,
            0,
            &mut idx,
            self.selected,
            &self.on_select,
            &self.on_toggle,
            &mut col,
        );
        let pick = self.on_select;
        let tog = self.on_toggle;
        let selected = self.selected;
        col.on_key_down(move |e| {
            if n == 0 {
                return false;
            }
            match e.key {
                Key::Down => {
                    if let Some(p) = &pick {
                        p((selected + 1).min(n - 1));
                    }
                    true
                }
                Key::Up => {
                    if let Some(p) = &pick {
                        p(selected.saturating_sub(1));
                    }
                    true
                }
                Key::Left | Key::Right | Key::Enter => {
                    if let Some(t) = &tog {
                        t(selected);
                    }
                    true
                }
                _ => false,
            }
        })
    }
}

trait AddChild {
    fn add_child(&mut self, child: impl IntoElement);
}

impl AddChild for Div {
    fn add_child(&mut self, child: impl IntoElement) {
        // Div::child consumes self; rebuild via take-and-replace is not
        // available, so we use the fluent builder by swapping an empty div.
        let taken = core::mem::replace(self, div());
        *self = taken.child(child);
    }
}

#[cfg(test)]
mod tests {
    use super::{List, Tree, TreeNode, toggle_visible};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use knox_gpui::{
        IntoElement, Key, KeyDownEvent, Modifiers, Point, Size, WindowInput, dispatch_key_down,
        prepare_element,
    };

    #[test]
    fn expand_collapse_and_keyboard() {
        let mut nodes =
            [
                TreeNode::branch("src", [TreeNode::leaf("main.rs"), TreeNode::leaf("lib.rs")])
                    .expanded(true),
            ];
        toggle_visible(&mut nodes, 0);
        assert!(!nodes[0].expanded);
        toggle_visible(&mut nodes, 0);
        assert!(nodes[0].expanded);

        let hit = Arc::new(AtomicUsize::new(0));
        let flag = hit.clone();
        let mut el = List::new()
            .themed(&Theme::aurora_dark())
            .items(["A", "B", "C"])
            .selected(Some(0))
            .on_select(move |i| flag.store(i, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(200, 80), &mut input);
        knox_gpui::dispatch_mouse_down(&mut el, &mut input, Point::xy(8, 8));
        dispatch_key_down(
            &mut el,
            &mut input,
            &KeyDownEvent::new(Key::Down, Modifiers::NONE),
        );
        assert_eq!(hit.load(Ordering::SeqCst), 1);
        let _ = Tree::new()
            .themed(&Theme::aurora_dark())
            .nodes(nodes)
            .into_element();
    }
}
