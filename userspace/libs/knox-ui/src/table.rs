// SPDX-License-Identifier: MIT
//! Table parts + DataTable (GPUI kit catalog, reimplemented).
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code: sort, resize column, virtual rows.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px};

use crate::icon::{Icon, IconName};
use crate::label::Label;
use crate::scroll::{VirtualList, visible_range};
use crate::size::Sizable;
use crate::theme::Theme;

/// One column descriptor.
#[derive(Clone, Debug)]
pub struct TableColumn {
    pub title: String,
    pub width: i32,
}

impl TableColumn {
    pub fn new(title: impl Into<String>, width: i32) -> Self {
        Self {
            title: title.into(),
            width: width.max(24),
        }
    }
}

/// Header-only table parts (kit table pieces).
pub struct TableHead {
    theme: Theme,
    columns: Vec<TableColumn>,
    sort_col: Option<usize>,
    sort_asc: bool,
    on_sort: Option<Arc<dyn Fn(usize) + Send + Sync>>,
    on_resize: Option<Arc<dyn Fn(usize, i32) + Send + Sync>>,
}

impl TableHead {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            columns: Vec::new(),
            sort_col: None,
            sort_asc: true,
            on_sort: None,
            on_resize: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn columns(mut self, columns: impl IntoIterator<Item = TableColumn>) -> Self {
        self.columns = columns.into_iter().collect();
        self
    }

    pub fn sort(mut self, col: Option<usize>, asc: bool) -> Self {
        self.sort_col = col;
        self.sort_asc = asc;
        self
    }

    pub fn on_sort(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_sort = Some(Arc::new(f));
        self
    }

    pub fn on_resize(mut self, f: impl Fn(usize, i32) + Send + Sync + 'static) -> Self {
        self.on_resize = Some(Arc::new(f));
        self
    }
}

impl Default for TableHead {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for TableHead {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut row = div()
            .flex()
            .items_center()
            .bg(self.theme.colors.muted.opacity(0.4))
            .h(px(24));
        for (i, col) in self.columns.into_iter().enumerate() {
            let sort = self.on_sort.clone();
            let resize = self.on_resize.clone();
            let width = col.width;
            let mut title = div().flex().items_center().gap_1().child(
                Label::new(col.title)
                    .themed(&self.theme)
                    .size(self.theme.typo.caption),
            );
            if self.sort_col == Some(i) {
                let glyph = if self.sort_asc {
                    IconName::ChevronUp
                } else {
                    IconName::ChevronDown
                };
                title = title.child(
                    Icon::new(glyph)
                        .themed(&self.theme)
                        .color(self.theme.colors.muted_foreground)
                        .small(),
                );
            }
            row = row.child(
                div()
                    .flex()
                    .w(px(width))
                    .h(px(24))
                    .child(title)
                    .on_click(move |_| {
                        if let Some(s) = &sort {
                            s(i);
                        }
                    })
                    .child(
                        div()
                            .w(px(4))
                            .h(px(24))
                            .bg(self.theme.colors.border)
                            .on_click(move |_| {
                                if let Some(r) = &resize {
                                    r(i, width + 16);
                                }
                            }),
                    ),
            );
        }
        row
    }
}

/// Sort `rows` in place by column `col`.
pub fn sort_rows(rows: &mut [Vec<String>], col: usize, asc: bool) {
    rows.sort_by(|a, b| {
        let av = a.get(col).map(String::as_str).unwrap_or("");
        let bv = b.get(col).map(String::as_str).unwrap_or("");
        if asc { av.cmp(bv) } else { bv.cmp(av) }
    });
}

/// Virtual data table (kit `DataTable`).
pub struct DataTable {
    theme: Theme,
    columns: Vec<TableColumn>,
    rows: Vec<Vec<String>>,
    sort_col: Option<usize>,
    sort_asc: bool,
    offset: i32,
    row_h: i32,
    width: Pixels,
    height: Pixels,
    selected: Option<usize>,
    on_sort: Option<Arc<dyn Fn(usize) + Send + Sync>>,
    on_resize: Option<Arc<dyn Fn(usize, i32) + Send + Sync>>,
    on_offset: Option<Arc<dyn Fn(i32) + Send + Sync>>,
    on_select: Option<Arc<dyn Fn(usize) + Send + Sync>>,
}

impl DataTable {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            columns: Vec::new(),
            rows: Vec::new(),
            sort_col: None,
            sort_asc: true,
            offset: 0,
            row_h: 20,
            width: px(360),
            height: px(120),
            selected: None,
            on_sort: None,
            on_resize: None,
            on_offset: None,
            on_select: None,
        }
    }

    pub fn themed(mut self, theme: &Theme) -> Self {
        self.theme = *theme;
        self
    }

    pub fn columns(mut self, columns: impl IntoIterator<Item = TableColumn>) -> Self {
        self.columns = columns.into_iter().collect();
        self
    }

    pub fn rows(mut self, rows: impl IntoIterator<Item = Vec<String>>) -> Self {
        self.rows = rows.into_iter().collect();
        self
    }

    pub fn sort(mut self, col: Option<usize>, asc: bool) -> Self {
        self.sort_col = col;
        self.sort_asc = asc;
        self
    }

    pub fn offset(mut self, offset: i32) -> Self {
        self.offset = offset.max(0);
        self
    }

    pub fn height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
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

    pub fn on_sort(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_sort = Some(Arc::new(f));
        self
    }

    pub fn on_resize(mut self, f: impl Fn(usize, i32) + Send + Sync + 'static) -> Self {
        self.on_resize = Some(Arc::new(f));
        self
    }

    pub fn on_offset(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_offset = Some(Arc::new(f));
        self
    }
}

impl Default for DataTable {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for DataTable {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut head = TableHead::new()
            .themed(&self.theme)
            .columns(self.columns.clone())
            .sort(self.sort_col, self.sort_asc);
        if let Some(s) = self.on_sort.clone() {
            head = head.on_sort(move |i| s(i));
        }
        if let Some(r) = self.on_resize.clone() {
            head = head.on_resize(move |i, w| r(i, w));
        }
        let range = visible_range(self.offset, self.height.0, self.row_h, self.rows.len());
        let labels: Vec<String> = range
            .clone()
            .filter_map(|i| {
                self.rows
                    .get(i)
                    .map(|row| row.iter().cloned().collect::<Vec<_>>().join("  "))
            })
            .collect();
        let mut list = VirtualList::new()
            .themed(&self.theme)
            .count(self.rows.len())
            .row_h(self.row_h)
            .offset(self.offset)
            .size(self.width, self.height)
            .labels(labels)
            .selected(self.selected);
        if let Some(on) = self.on_offset {
            list = list.on_offset(move |o| on(o));
        }
        if let Some(on) = self.on_select {
            list = list.on_select(move |i| on(i));
        }
        div().flex_col().child(head).child(list)
    }
}

#[cfg(test)]
mod tests {
    use super::{DataTable, TableColumn, TableHead, sort_rows};
    use crate::theme::Theme;
    use alloc::string::String;
    use alloc::vec;
    use knox_gpui::IntoElement;

    #[test]
    fn click_header_sorts() {
        let mut rows = vec![
            vec![String::from("b"), String::from("2")],
            vec![String::from("a"), String::from("1")],
            vec![String::from("c"), String::from("3")],
        ];
        sort_rows(&mut rows, 0, true);
        assert_eq!(rows[0][0], "a");
        sort_rows(&mut rows, 0, false);
        assert_eq!(rows[0][0], "c");
        let theme = Theme::aurora_dark();
        let _ = TableHead::new()
            .themed(&theme)
            .columns([TableColumn::new("Name", 80), TableColumn::new("Val", 40)])
            .into_element();
        let _ = DataTable::new()
            .themed(&theme)
            .columns([TableColumn::new("Name", 80)])
            .rows(rows)
            .into_element();
    }
}
