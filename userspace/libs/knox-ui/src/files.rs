// SPDX-License-Identifier: MIT
//! Files app chrome: path bar + bookmark tree + entry list (D11.3).
//! Domain actions stay in `kernel/src/gui/explorer/`.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS app view, not a kit catalog control.

use alloc::string::String;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::icon::IconName;
use crate::interact::IndexHandler;
use crate::list::{Tree, TreeNode};
use crate::size::{Selectable, Sizable};
use crate::theme::Theme;

/// One row in the files list.
#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
}

impl FileEntry {
    pub fn new(name: impl Into<String>, is_dir: bool) -> Self {
        Self {
            name: name.into(),
            is_dir,
        }
    }
}

/// Files window body (Tree + path + Lucide file rows).
pub struct FilesView {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    path: String,
    bookmarks: Vec<String>,
    bookmark_sel: usize,
    entries: Vec<FileEntry>,
    selected: Option<usize>,
    on_back: Option<crate::interact::Handler>,
    on_forward: Option<crate::interact::Handler>,
    on_up: Option<crate::interact::Handler>,
    on_bookmark: Option<IndexHandler>,
    on_entry: Option<IndexHandler>,
}

impl FilesView {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(720),
            height: px(480),
            path: String::from("/"),
            bookmarks: Vec::new(),
            bookmark_sel: 0,
            entries: Vec::new(),
            selected: None,
            on_back: None,
            on_forward: None,
            on_up: None,
            on_bookmark: None,
            on_entry: None,
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

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    pub fn bookmarks(mut self, items: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.bookmarks = items.into_iter().map(Into::into).collect();
        self
    }

    pub fn bookmark_sel(mut self, selected: usize) -> Self {
        self.bookmark_sel = selected;
        self
    }

    pub fn entries(mut self, entries: impl IntoIterator<Item = FileEntry>) -> Self {
        self.entries = entries.into_iter().collect();
        self
    }

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
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
}

impl Default for FilesView {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for FilesView {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut nav = div().flex().gap_1().p(self.theme.spacing.xs).items_center();
        if let Some(h) = self.on_back {
            nav = nav.child(
                Button::new("files-back")
                    .themed(&self.theme)
                    .ghost()
                    .compact()
                    .icon(IconName::ChevronLeft)
                    .on_press(move || h()),
            );
        }
        if let Some(h) = self.on_forward {
            nav = nav.child(
                Button::new("files-fwd")
                    .themed(&self.theme)
                    .ghost()
                    .compact()
                    .icon(IconName::ChevronRight)
                    .on_press(move || h()),
            );
        }
        if let Some(h) = self.on_up {
            nav = nav.child(
                Button::new("files-up")
                    .themed(&self.theme)
                    .ghost()
                    .compact()
                    .icon(IconName::ChevronUp)
                    .on_press(move || h()),
            );
        }
        nav = nav.child(
            div()
                .grow()
                .h(px(28))
                .p(self.theme.spacing.xs)
                .rounded(self.theme.radius.sm)
                .bg(self.theme.colors.muted.opacity(0.4))
                .child(
                    text(self.path.clone())
                        .color(self.theme.colors.foreground)
                        .size(13),
                ),
        );

        let nodes: Vec<TreeNode> = self
            .bookmarks
            .iter()
            .map(|b| TreeNode::leaf(b.clone()))
            .collect();
        let mut tree = Tree::new()
            .themed(&self.theme)
            .nodes(nodes)
            .selected(self.bookmark_sel);
        if let Some(on) = self.on_bookmark {
            tree = tree.on_select(move |i| on(i));
        }

        let mut list = div().flex_col();
        for (i, entry) in self.entries.into_iter().enumerate() {
            let pick = self.on_entry.clone();
            let glyph = if entry.is_dir {
                IconName::Folder
            } else {
                IconName::File
            };
            list = list.child(
                Button::new(alloc::format!("files-row-{i}"))
                    .themed(&self.theme)
                    .ghost()
                    .small()
                    .icon(glyph)
                    .label(entry.name)
                    .selected(self.selected == Some(i))
                    .on_press(move || {
                        if let Some(p) = &pick {
                            p(i);
                        }
                    }),
            );
        }

        let body_h = px((self.height.0 - 40).max(80));
        let sidebar_bg = self.theme.colors.muted;
        let list_bg = self.theme.colors.background;
        div()
            .flex_col()
            .w(self.width)
            .h(self.height)
            .bg(self.theme.colors.background)
            .child(nav)
            .child(
                div()
                    .flex()
                    .grow()
                    .h(body_h)
                    .child(div().w(px(160)).h(body_h).bg(sidebar_bg).child(tree))
                    .child(div().grow().h(body_h).bg(list_bg).child(list)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{FileEntry, FilesView};
    use crate::theme::Theme;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element, px,
    };

    #[test]
    fn entry_click_selects() {
        let hit = Arc::new(AtomicUsize::new(99));
        let flag = hit.clone();
        let mut el = FilesView::new()
            .themed(&Theme::aurora_dark())
            .size(px(400), px(240))
            .path("/home/user")
            .bookmarks(["Home", "Documents"])
            .entries([
                FileEntry::new("Documents", true),
                FileEntry::new("readme.txt", false),
            ])
            .on_entry(move |i| flag.store(i, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(400, 240), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| hb.bounds.x() >= 160 && hb.bounds.y() >= 32 && hb.bounds.height() >= 16)
            .min_by_key(|hb| (hb.bounds.height(), hb.bounds.y()))
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("list row");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert_eq!(hit.load(Ordering::SeqCst), 0);
    }
}
