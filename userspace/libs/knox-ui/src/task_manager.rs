// SPDX-License-Identifier: MIT
//! Task Manager chrome: tabs + virtual DataTable (D11.4).
//! Domain (process list, kill) stays in `kernel/src/gui/task_manager.rs`.
//!
//! the Longbridge kit is a design reference only (Apache-2.0). This file is new
//! `no_std` code. KnoxOS app view, not a kit catalog control.

use alloc::string::String;
use alloc::vec::Vec;

use knox_gpui::{Div, IntoElement, Pixels, Styled, div, px, text};

use crate::button::{Button, ButtonVariants};
use crate::chrome::DescriptionList;
use crate::interact::{Handler, IndexHandler};
use crate::nav::TabBar;
use crate::progress::Progress;
use crate::table::{DataTable, TableColumn};
use crate::theme::Theme;

/// Tab labels matching the kernel Task Manager.
pub const TAB_LABELS: [&str; 3] = ["Processes", "Windows", "Performance"];

/// Task Manager window body.
pub struct TaskManagerView {
    theme: Theme,
    width: Pixels,
    height: Pixels,
    tab: usize,
    columns: Vec<TableColumn>,
    rows: Vec<Vec<String>>,
    selected: Option<usize>,
    offset: i32,
    sort_col: Option<usize>,
    sort_asc: bool,
    status: String,
    mem_pct: i32,
    perf_rows: Vec<(String, String)>,
    on_tab: Option<IndexHandler>,
    on_row: Option<IndexHandler>,
    on_sort: Option<IndexHandler>,
    on_offset: Option<ArcFnI32>,
    on_end: Option<Handler>,
}

type ArcFnI32 = alloc::sync::Arc<dyn Fn(i32) + Send + Sync>;

impl TaskManagerView {
    pub fn new() -> Self {
        Self {
            theme: Theme::default(),
            width: px(720),
            height: px(480),
            tab: 0,
            columns: Vec::new(),
            rows: Vec::new(),
            selected: None,
            offset: 0,
            sort_col: None,
            sort_asc: true,
            status: String::new(),
            mem_pct: 0,
            perf_rows: Vec::new(),
            on_tab: None,
            on_row: None,
            on_sort: None,
            on_offset: None,
            on_end: None,
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

    pub fn tab(mut self, tab: usize) -> Self {
        self.tab = tab.min(2);
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

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn offset(mut self, offset: i32) -> Self {
        self.offset = offset.max(0);
        self
    }

    pub fn sort(mut self, col: Option<usize>, asc: bool) -> Self {
        self.sort_col = col;
        self.sort_asc = asc;
        self
    }

    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = status.into();
        self
    }

    pub fn mem_pct(mut self, pct: i32) -> Self {
        self.mem_pct = pct.clamp(0, 100);
        self
    }

    pub fn perf_rows(
        mut self,
        rows: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        self.perf_rows = rows
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        self
    }

    pub fn on_tab(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_tab = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_row(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_row = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_sort(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_sort = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_offset(mut self, f: impl Fn(i32) + Send + Sync + 'static) -> Self {
        self.on_offset = Some(alloc::sync::Arc::new(f));
        self
    }

    pub fn on_end(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_end = Some(alloc::sync::Arc::new(f));
        self
    }
}

impl Default for TaskManagerView {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoElement for TaskManagerView {
    type Element = Div;

    fn into_element(self) -> Div {
        let mut tabs = TabBar::new("tm-tabs")
            .themed(&self.theme)
            .labels(TAB_LABELS)
            .selected(self.tab);
        if let Some(on) = self.on_tab.clone() {
            tabs = tabs.on_select(move |i| on(i));
        }

        let table_h = px((self.height.0 - 64).max(80));
        let body = if self.tab == 2 {
            let mut list = DescriptionList::new().themed(&self.theme);
            for (k, v) in self.perf_rows {
                list = list.row(k, v);
            }
            div()
                .flex_col()
                .gap_2()
                .p(self.theme.spacing.sm)
                .h(table_h)
                .child(text("Memory").color(self.theme.colors.accent).size(13))
                .child(
                    Progress::new()
                        .themed(&self.theme)
                        .value(self.mem_pct as f32)
                        .w(px((self.width.0 - 32).max(80))),
                )
                .child(list)
        } else {
            let mut table = DataTable::new()
                .themed(&self.theme)
                .columns(self.columns)
                .rows(self.rows)
                .sort(self.sort_col, self.sort_asc)
                .offset(self.offset)
                .width(self.width)
                .height(table_h)
                .selected(self.selected);
            if let Some(on) = self.on_row {
                table = table.on_select(move |i| on(i));
            }
            if let Some(on) = self.on_sort {
                table = table.on_sort(move |i| on(i));
            }
            if let Some(on) = self.on_offset {
                table = table.on_offset(move |o| on(o));
            }
            table.into_element()
        };

        let mut bar = div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(28))
            .p(self.theme.spacing.xs)
            .bg(self.theme.colors.muted.opacity(0.4))
            .child(
                text(self.status)
                    .color(self.theme.colors.muted_foreground)
                    .size(12),
            );
        if self.tab == 0 {
            if let Some(h) = self.on_end {
                bar = bar.child(
                    Button::new("tm-end")
                        .themed(&self.theme)
                        .danger()
                        .label("End Process")
                        .on_click(move || h()),
                );
            }
        }

        div()
            .flex_col()
            .w(self.width)
            .h(self.height)
            .bg(self.theme.colors.background)
            .child(tabs)
            .child(div().grow().child(body))
            .child(bar)
    }
}

#[cfg(test)]
mod tests {
    use super::TaskManagerView;
    use crate::scroll::virtual_paint_count;
    use crate::table::TableColumn;
    use crate::theme::Theme;
    use alloc::string::String;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use knox_gpui::{
        IntoElement, Point, Size, WindowInput, dispatch_mouse_down, dispatch_mouse_up,
        prepare_element, px,
    };

    fn proc_rows(n: usize) -> Vec<Vec<String>> {
        (0..n)
            .map(|i| {
                vec![
                    alloc::format!("{i}"),
                    String::from("init"),
                    String::from("Ready"),
                ]
            })
            .collect()
    }

    #[test]
    fn process_list_virtualizes() {
        assert!(
            virtual_paint_count(0, 160, 20, 1000) < 20,
            "viewport must not materialize 1000 rows"
        );
        let theme = Theme::aurora_dark();
        let _ = TaskManagerView::new()
            .themed(&theme)
            .size(px(640), px(280))
            .columns([
                TableColumn::new("PID", 60),
                TableColumn::new("Name", 180),
                TableColumn::new("State", 80),
            ])
            .rows(proc_rows(1000))
            .status("1000 processes")
            .into_element();
    }

    #[test]
    fn row_click_selects() {
        let hit = Arc::new(AtomicUsize::new(99));
        let flag = hit.clone();
        let mut el = TaskManagerView::new()
            .themed(&Theme::aurora_dark())
            .size(px(480), px(240))
            .columns([TableColumn::new("PID", 60), TableColumn::new("Name", 180)])
            .rows(proc_rows(40))
            .on_row(move |i| flag.store(i, Ordering::SeqCst))
            .into_element();
        let mut input = WindowInput::new();
        prepare_element(&mut el, Point::ZERO, Size::wh(480, 240), &mut input);
        let p = input
            .hitboxes()
            .iter()
            .filter(|hb| {
                hb.bounds.y() >= 48 && hb.bounds.height() >= 16 && hb.bounds.height() <= 24
            })
            .min_by_key(|hb| hb.bounds.y())
            .map(|hb| Point::xy(hb.bounds.x() + 8, hb.bounds.y() + 8))
            .expect("process row");
        dispatch_mouse_down(&mut el, &mut input, p);
        dispatch_mouse_up(&mut el, &mut input, p);
        assert_eq!(hit.load(Ordering::SeqCst), 0);
    }
}
