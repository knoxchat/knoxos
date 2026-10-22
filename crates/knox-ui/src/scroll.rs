// SPDX-License-Identifier: MIT
//! ScrollArea, Scrollbar, VirtualList (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code.

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px, text};

use crate::interact::IndexHandler;
use crate::theme::Theme;

type OffsetHandler = Arc<dyn Fn(i32) + Send + Sync>;

/// Visible row range for a virtual list (inclusive start, exclusive end).
pub fn visible_range(offset: i32, viewport: i32, row_h: i32, count: usize) -> Range<usize> {
    if row_h <= 0 || viewport <= 0 || count == 0 {
        return 0..0;
    }
    let start = (offset.max(0) / row_h) as usize;
    let start = start.min(count);
    let end = ((offset.max(0) + viewport + row_h - 1) / row_h) as usize;
    start..end.min(count)
}

/// How many rows a viewport would paint.
pub fn virtual_paint_count(offset: i32, viewport: i32, row_h: i32, count: usize) -> usize {
    visible_range(offset, viewport, row_h, count).len()
}

/// Clipped scrolling panel (kit `ScrollArea`).
pub struct ScrollArea {
    theme: Theme,
    offset: i32,
    width: Pixels,
    height: Pixels,
    child: Option<Div>,
    on_offset: Option<OffsetHandler>,
}

impl ScrollArea {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            offset: 0,
            width: px(240),
            height: px(120),
            child: None,
            on_offset: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn offset(mut self, offset: i32) -> Self {
        self.offset = offset.max(0);
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.child = Some(div().child(child));
        self
    }

    pub fn on_offset(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_offset = Some(Arc::new(f));
        self
    }
}

impl Default for ScrollArea {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for ScrollArea {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut inner = div().flex_col().items_start();
        if let Some(child) = self.child {
            inner = inner.child(div().flex_col().child(child));
        }
        let offset = self.offset;
        let mut viewport = div()
            .flex_col()
            .w(self.width)
            .h(self.height)
            .overflow_hidden()
            .scroll_y(px(offset))
            .child(inner);
        if let Some(on) = self.on_offset {
            viewport = viewport.on_scroll(move |e| {
                let next = (offset - e.delta.y.0).max(0);
                on(next);
            });
        }
        viewport
    }
}

/// Vertical scrollbar thumb (kit `Scrollbar`).
pub struct Scrollbar {
    theme: Theme,
    offset: i32,
    content_h: i32,
    viewport_h: i32,
    on_offset: Option<OffsetHandler>,
}

impl Scrollbar {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            offset: 0,
            content_h: 1,
            viewport_h: 1,
            on_offset: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn offset(mut self, offset: i32) -> Self {
        self.offset = offset.max(0);
        self
    }

    pub fn content_h(mut self, h: i32) -> Self {
        self.content_h = h.max(1);
        self
    }

    pub fn viewport_h(mut self, h: i32) -> Self {
        self.viewport_h = h.max(1);
        self
    }

    pub fn on_offset(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_offset = Some(Arc::new(f));
        self
    }
}

impl Default for Scrollbar {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for Scrollbar {
    type Element = Div;

    fn into_element(self) -> Div {
        let track_h = self.viewport_h.max(24);
        let thumb_h = ((self.viewport_h * track_h) / self.content_h.max(self.viewport_h)).max(8);
        let max_off = (self.content_h - self.viewport_h).max(1);
        let thumb_y = (self.offset * (track_h - thumb_h)) / max_off;
        let mut track = div()
            .w(px(8))
            .h(px(track_h))
            .bg(self.theme.colors.muted)
            .rounded(self.theme.radius.full)
            .child(
                div()
                    .w(px(8))
                    .h(px(thumb_h))
                    .mt(px(thumb_y))
                    .bg(self.theme.colors.border)
                    .rounded(self.theme.radius.full),
            );
        if let Some(on) = self.on_offset {
            let content = self.content_h;
            let view = self.viewport_h;
            track = track.on_click(move |e| {
                let y = e.position.y.0.max(0);
                let next = (y * content) / view.max(1);
                on(next.max(0));
            });
        }
        track
    }
}

/// Windowed list: only viewport rows exist in the tree (kit `VirtualList`).
pub struct VirtualList {
    theme: Theme,
    count: usize,
    row_h: i32,
    offset: i32,
    width: Pixels,
    height: Pixels,
    labels: Vec<String>,
    selected: Option<usize>,
    on_select: Option<IndexHandler>,
    on_offset: Option<OffsetHandler>,
}

impl VirtualList {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            count: 0,
            row_h: 20,
            offset: 0,
            width: px(200),
            height: px(80),
            labels: Vec::new(),
            selected: None,
            on_select: None,
            on_offset: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn count(mut self, count: usize) -> Self {
        self.count = count;
        self
    }

    pub fn row_h(mut self, h: i32) -> Self {
        self.row_h = h.max(1);
        self
    }

    pub fn offset(mut self, offset: i32) -> Self {
        self.offset = offset.max(0);
        self
    }

    pub fn size(mut self, width: Pixels, height: Pixels) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn labels(mut self, labels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.labels = labels.into_iter().map(Into::into).collect();
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

    pub fn on_offset(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_offset = Some(Arc::new(f));
        self
    }
}

impl Default for VirtualList {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for VirtualList {
    type Element = Div;

    fn into_element(self) -> Div {
        let viewport = self.height.0;
        let range = visible_range(self.offset, viewport, self.row_h, self.count);
        let start = range.start;
        let top_pad = (start as i32) * self.row_h;
        let mut col = div().flex_col().mt(px(top_pad));
        for i in range {
            let selected = self.selected == Some(i);
            let bg = if selected {
                self.theme.colors.accent.opacity(0.25)
            } else {
                knox_gpui::Hsla::TRANSPARENT
            };
            let pick = self.on_select.clone();
            let vis = i.saturating_sub(start);
            let label = self
                .labels
                .get(i)
                .cloned()
                .or_else(|| self.labels.get(vis).cloned())
                .unwrap_or_else(|| format!("Row {i}"));
            col = col.child(
                div()
                    .h(px(self.row_h))
                    .w(self.width)
                    .bg(bg)
                    .child(text(label).color(self.theme.colors.foreground).size(12))
                    .on_click(move |_| {
                        if let Some(p) = &pick {
                            p(i);
                        }
                    }),
            );
        }
        let offset = self.offset;
        let mut view = div()
            .w(self.width)
            .h(self.height)
            .overflow_hidden()
            .child(col);
        if let Some(on) = self.on_offset {
            view = view.on_scroll(move |e| on((offset - e.delta.y.0).max(0)));
        }
        view
    }
}

#[cfg(test)]
mod tests {
    use super::{ScrollArea, Scrollbar, VirtualList, virtual_paint_count, visible_range};
    use crate::theme::Theme;
    use knox_gpui::{IntoElement, Styled, div, px};

    #[test]
    fn one_k_rows_only_paint_viewport() {
        let count = virtual_paint_count(0, 80, 16, 1000);
        assert!(count <= 6, "painted {count} rows for an 80px viewport");
        assert_eq!(visible_range(0, 80, 16, 1000), 0..5);
        assert_eq!(visible_range(160, 80, 16, 1000), 10..15);
        let theme = Theme::aurora_dark();
        let _ = VirtualList::new()
            .themed(&theme)
            .count(1000)
            .row_h(16)
            .size(px(200), px(80))
            .into_element();
        let _ = ScrollArea::new()
            .themed(&theme)
            .child(div().h(px(400)))
            .into_element();
        let _ = Scrollbar::new()
            .themed(&theme)
            .content_h(400)
            .viewport_h(80)
            .into_element();
    }
}
