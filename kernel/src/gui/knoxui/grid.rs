// ─── Grid — Auto-layout grid for aligned columns ────────────────────
//
// Inspired by egui::Grid. Lays out widgets in a grid pattern where
// columns auto-size based on their content. Call `ui.end_row()` between
// rows.

use alloc::vec::Vec;

use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Direction, Layout, Region};
use crate::gui::ui::{UI_MEMORY, Ui};

/// An auto-layout grid that aligns widgets into columns.
///
/// ```ignore
/// Grid::new("settings_grid").show(ui, |ui| {
///     ui.label("Name:");   ui.text_edit(&mut name);   ui.end_row();
///     ui.label("Age:");    ui.drag_value(&mut age);   ui.end_row();
/// });
/// ```
pub struct Grid<'a> {
    id_str: &'a str,
    num_columns: usize,
    spacing_x: i32,
    spacing_y: i32,
    striped: bool,
    min_col_width: u32,
    max_col_width: Option<u32>,
}

impl<'a> Grid<'a> {
    pub fn new(id_str: &'a str) -> Self {
        Self {
            id_str,
            num_columns: 2,
            spacing_x: 8,
            spacing_y: 4,
            striped: false,
            min_col_width: 40,
            max_col_width: None,
        }
    }

    pub fn num_columns(mut self, n: usize) -> Self {
        self.num_columns = n;
        self
    }

    pub fn spacing(mut self, x: i32, y: i32) -> Self {
        self.spacing_x = x;
        self.spacing_y = y;
        self
    }

    pub fn striped(mut self, s: bool) -> Self {
        self.striped = s;
        self
    }

    pub fn min_col_width(mut self, w: u32) -> Self {
        self.min_col_width = w;
        self
    }

    pub fn max_col_width(mut self, w: u32) -> Self {
        self.max_col_width = Some(w);
        self
    }

    /// Show the grid. The closure receives a `GridUi` that tracks columns.
    pub fn show<R>(self, ui: &mut Ui, add_contents: impl FnOnce(&mut GridUi)) -> R
    where
        R: Default,
    {
        let id = ui.id.with(self.id_str);
        let avail_w = ui.available_width().max(0) as u32;

        // Calculate column widths — evenly divide with spacing
        let total_spacing = (self.num_columns.saturating_sub(1) as u32) * self.spacing_x as u32;
        let col_w = if avail_w > total_spacing {
            let raw = (avail_w - total_spacing) / self.num_columns.max(1) as u32;
            let w = raw.max(self.min_col_width);
            if let Some(max_w) = self.max_col_width {
                w.min(max_w)
            } else {
                w
            }
        } else {
            self.min_col_width
        };

        let start_x = ui.region.cursor_x;
        let start_y = ui.region.cursor_y;

        let mut grid_ui = GridUi {
            ui,
            id,
            col_w,
            num_columns: self.num_columns,
            spacing_x: self.spacing_x,
            spacing_y: self.spacing_y,
            striped: self.striped,
            start_x,
            current_col: 0,
            current_row: 0,
            row_y: start_y,
            row_h: 0,
        };

        add_contents(&mut grid_ui);

        // Advance cursor past all grid rows
        let final_y = grid_ui.row_y + grid_ui.row_h as i32 + grid_ui.spacing_y;
        grid_ui.ui.region.cursor_y = final_y;
        grid_ui.ui.region.cursor_x = start_x;

        R::default()
    }
}

/// The UI context passed inside a Grid closure.
pub struct GridUi<'a, 'b> {
    pub ui: &'a mut Ui<'b>,
    id: Id,
    col_w: u32,
    num_columns: usize,
    spacing_x: i32,
    spacing_y: i32,
    striped: bool,
    start_x: i32,
    current_col: usize,
    current_row: usize,
    row_y: i32,
    row_h: u32,
}

impl<'a, 'b> GridUi<'a, 'b> {
    /// Move to the next column. Call this after adding a widget for each cell.
    pub fn next_cell(&mut self) -> Rect {
        let x = self.start_x + (self.current_col as i32) * (self.col_w as i32 + self.spacing_x);
        let cell_rect = Rect::new(x, self.row_y, self.col_w, 24);

        // Striped background on even rows
        if self.striped && self.current_row % 2 == 0 && self.current_col == 0 {
            let full_w = (self.num_columns as u32) * self.col_w
                + (self.num_columns.saturating_sub(1) as u32) * self.spacing_x as u32;
            let row_rect = Rect::new(self.start_x, self.row_y, full_w, 24);
            self.ui.fb.fill_rect(row_rect, Pixel::new(255, 255, 255, 4));
        }

        self.current_col += 1;

        // Position ui cursor within this cell
        self.ui.region.cursor_x = x;
        self.ui.region.cursor_y = self.row_y;

        cell_rect
    }

    /// End the current row and move to the next.
    pub fn end_row(&mut self) {
        self.current_col = 0;
        self.current_row += 1;
        self.row_y += self.row_h.max(24) as i32 + self.spacing_y;
        self.row_h = 0;
        self.ui.region.cursor_x = self.start_x;
        self.ui.region.cursor_y = self.row_y;
    }

    /// Report a cell's height so the row can size to the tallest cell.
    pub fn set_row_height(&mut self, h: u32) {
        if h > self.row_h {
            self.row_h = h;
        }
    }
}
