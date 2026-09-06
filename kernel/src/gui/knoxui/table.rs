/// Table — Data table with columns, rows, headers, sorting, and selection
///
/// Inspired by egui_extras::Table. Supports:
/// - Fixed and resizable column widths
/// - Sortable column headers
/// - Row selection (single / multi)
/// - Alternating row colors
/// - Scrollable body
///
/// ```ignore
/// Table::new("users", 3)
///     .column(Column::fixed("Name", 150))
///     .column(Column::fixed("Email", 200))
///     .column(Column::remainder("Role"))
///     .striped(true)
///     .header(20, |header| {
///         header.col(|ui| { ui.label("Name"); });
///         header.col(|ui| { ui.label("Email"); });
///         header.col(|ui| { ui.label("Role"); });
///     })
///     .body(|body| {
///         for user in &users {
///             body.row(20, |row| {
///                 row.col(|ui| { ui.label(&user.name); });
///                 row.col(|ui| { ui.label(&user.email); });
///                 row.col(|ui| { ui.label(&user.role); });
///             });
///         }
///     });
/// ```
use alloc::string::String;
use alloc::vec::Vec;

use super::text_helpers as fonts;
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::layout::{Align, Layout, Region};
use crate::gui::response::Response;
use crate::gui::ui::{UI_MEMORY, Ui};

/// Column sizing policy.
#[derive(Clone, Debug)]
pub enum ColumnSizing {
    /// Fixed width in pixels.
    Fixed(u32),
    /// Minimum width, expands to fill.
    Initial(u32),
    /// Fills all remaining space.
    Remainder,
}

/// A table column definition.
#[derive(Clone, Debug)]
pub struct Column {
    pub name: String,
    pub sizing: ColumnSizing,
    pub sortable: bool,
    pub resizable: bool,
}

impl Column {
    pub fn fixed(name: &str, width: u32) -> Self {
        Self {
            name: String::from(name),
            sizing: ColumnSizing::Fixed(width),
            sortable: false,
            resizable: false,
        }
    }

    pub fn initial(name: &str, width: u32) -> Self {
        Self {
            name: String::from(name),
            sizing: ColumnSizing::Initial(width),
            sortable: false,
            resizable: true,
        }
    }

    pub fn remainder(name: &str) -> Self {
        Self {
            name: String::from(name),
            sizing: ColumnSizing::Remainder,
            sortable: false,
            resizable: false,
        }
    }

    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }

    pub fn resizable(mut self, v: bool) -> Self {
        self.resizable = v;
        self
    }
}

/// Sort direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
    None,
}

/// Table sort state.
#[derive(Clone, Debug)]
pub struct SortState {
    pub column_index: Option<usize>,
    pub order: SortOrder,
}

impl SortState {
    pub fn none() -> Self {
        Self {
            column_index: None,
            order: SortOrder::None,
        }
    }
}

/// The Table builder.
pub struct Table {
    id: Id,
    columns: Vec<Column>,
    striped: bool,
    row_height: u32,
    header_height: u32,
    header_bg: Pixel,
    row_bg: Pixel,
    row_bg_alt: Pixel,
    row_bg_hover: Pixel,
    row_bg_selected: Pixel,
    border_color: Pixel,
    show_borders: bool,
    selected_row: Option<usize>,
}

impl Table {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: Id::from_str(id_salt),
            columns: Vec::new(),
            striped: true,
            row_height: 24,
            header_height: 28,
            header_bg: Pixel::rgb(30, 33, 40),
            row_bg: Pixel::rgb(20, 22, 28),
            row_bg_alt: Pixel::rgb(24, 26, 33),
            row_bg_hover: Pixel::rgb(35, 50, 80),
            row_bg_selected: Pixel::rgb(40, 80, 160),
            border_color: Pixel::rgb(45, 48, 56),
            show_borders: true,
            selected_row: None,
        }
    }

    pub fn column(mut self, col: Column) -> Self {
        self.columns.push(col);
        self
    }

    pub fn columns(mut self, cols: Vec<Column>) -> Self {
        self.columns = cols;
        self
    }

    pub fn striped(mut self, v: bool) -> Self {
        self.striped = v;
        self
    }

    pub fn row_height(mut self, h: u32) -> Self {
        self.row_height = h;
        self
    }

    pub fn selected_row(mut self, idx: Option<usize>) -> Self {
        self.selected_row = idx;
        self
    }

    /// Resolve column widths given available width.
    fn resolve_widths(&self, available: u32) -> Vec<u32> {
        let mut widths = Vec::with_capacity(self.columns.len());
        let mut used = 0u32;
        let mut remainder_count = 0u32;

        for col in &self.columns {
            match col.sizing {
                ColumnSizing::Fixed(w) => {
                    widths.push(w);
                    used += w;
                }
                ColumnSizing::Initial(w) => {
                    widths.push(w);
                    used += w;
                }
                ColumnSizing::Remainder => {
                    widths.push(0); // placeholder
                    remainder_count += 1;
                }
            }
        }

        // Distribute remaining space
        let leftover = available.saturating_sub(used);
        let per_rem = leftover.checked_div(remainder_count).unwrap_or(0);
        for (i, col) in self.columns.iter().enumerate() {
            if matches!(col.sizing, ColumnSizing::Remainder) {
                widths[i] = per_rem;
            }
        }

        widths
    }

    /// Show the table header.
    pub fn header<'a>(&self, ui: &mut Ui<'a>, sort_state: &mut SortState) {
        let avail_w = ui.available_width().max(0) as u32;
        let widths = self.resolve_widths(avail_w);
        let header_rect = ui.allocate_space(avail_w, self.header_height);

        // Header background
        ui.fb.fill_rect(header_rect, self.header_bg);

        // Draw each column header
        let mut x = header_rect.x;
        for (i, col) in self.columns.iter().enumerate() {
            let w = widths.get(i).copied().unwrap_or(80);
            let cell_rect = Rect::new(x, header_rect.y, w, self.header_height);

            let cell_id = self.id.with("hdr").with_index(i);
            let resp = ui.interact(cell_rect, cell_id, col.sortable, false);

            // Hover highlight
            if resp.hovered && col.sortable {
                ui.fb.fill_rect(cell_rect, Pixel::rgb(40, 44, 55));
            }

            // Sort on click
            if resp.clicked() && col.sortable {
                if sort_state.column_index == Some(i) {
                    sort_state.order = match sort_state.order {
                        SortOrder::Ascending => SortOrder::Descending,
                        SortOrder::Descending => SortOrder::None,
                        SortOrder::None => SortOrder::Ascending,
                    };
                    if sort_state.order == SortOrder::None {
                        sort_state.column_index = None;
                    }
                } else {
                    sort_state.column_index = Some(i);
                    sort_state.order = SortOrder::Ascending;
                }
            }

            // Header text
            fonts::draw_string_bold_compact(
                ui.fb,
                &col.name,
                x + 6,
                header_rect.y + (self.header_height as i32 - 12) / 2,
                Pixel::rgb(180, 190, 210),
            );

            // Sort indicator
            if sort_state.column_index == Some(i) {
                let arrow_x = x + 6 + (col.name.len() as i32 + 1) * 8;
                let arrow_y = header_rect.y + self.header_height as i32 / 2;
                match sort_state.order {
                    SortOrder::Ascending => {
                        // Up arrow ▲
                        for dy in 0..5i32 {
                            let half = 4 - dy;
                            ui.fb.draw_hline(
                                arrow_x + 4 - half,
                                arrow_y - 2 + dy,
                                (half * 2 + 1) as u32,
                                colors::HIGHLIGHT,
                            );
                        }
                    }
                    SortOrder::Descending => {
                        // Down arrow ▼
                        for dy in 0..5i32 {
                            let half = dy;
                            ui.fb.draw_hline(
                                arrow_x + 4 - half,
                                arrow_y - 2 + dy,
                                (half * 2 + 1) as u32,
                                colors::HIGHLIGHT,
                            );
                        }
                    }
                    SortOrder::None => {}
                }
            }

            // Column border
            if self.show_borders && i < self.columns.len() - 1 {
                ui.fb.draw_vline(
                    x + w as i32,
                    header_rect.y,
                    self.header_height,
                    self.border_color,
                );
            }

            x += w as i32;
        }

        // Bottom border
        ui.fb.draw_hline(
            header_rect.x,
            header_rect.y + self.header_height as i32,
            avail_w,
            self.border_color,
        );
    }

    /// Show the table body. The closure receives a `TableBody` for adding rows.
    pub fn body<'a>(
        &self,
        ui: &mut Ui<'a>,
        selected: &mut Option<usize>,
        add_rows: impl FnOnce(&mut TableBody),
    ) {
        let avail_w = ui.available_width().max(0) as u32;
        let widths = self.resolve_widths(avail_w);

        let mut body = TableBody {
            ui,
            id: self.id,
            widths,
            row_index: 0,
            row_height: self.row_height,
            striped: self.striped,
            row_bg: self.row_bg,
            row_bg_alt: self.row_bg_alt,
            row_bg_hover: self.row_bg_hover,
            row_bg_selected: self.row_bg_selected,
            border_color: self.border_color,
            show_borders: self.show_borders,
            selected_row: *selected,
            clicked_row: None,
            total_width: avail_w,
        };

        add_rows(&mut body);

        if let Some(clicked) = body.clicked_row {
            *selected = Some(clicked);
        }
    }
}

/// Table body context — used inside the `body` closure to add rows.
pub struct TableBody<'a, 'b> {
    ui: &'b mut Ui<'a>,
    id: Id,
    widths: Vec<u32>,
    row_index: usize,
    row_height: u32,
    striped: bool,
    row_bg: Pixel,
    row_bg_alt: Pixel,
    row_bg_hover: Pixel,
    row_bg_selected: Pixel,
    border_color: Pixel,
    show_borders: bool,
    selected_row: Option<usize>,
    clicked_row: Option<usize>,
    total_width: u32,
}

impl<'a, 'b> TableBody<'a, 'b> {
    /// Add a row. The closure receives a `TableRow` for adding cell contents.
    pub fn row(&mut self, add_cells: impl FnOnce(&mut TableRow)) {
        let row_rect = self.ui.allocate_space(self.total_width, self.row_height);
        let is_selected = self.selected_row == Some(self.row_index);

        // Row interaction
        let row_id = self.id.with("row").with_index(self.row_index);
        let resp = self.ui.interact(row_rect, row_id, true, false);

        if resp.clicked() {
            self.clicked_row = Some(self.row_index);
        }

        // Row background
        let bg = if is_selected {
            self.row_bg_selected
        } else if resp.hovered {
            self.row_bg_hover
        } else if self.striped && self.row_index % 2 == 1 {
            self.row_bg_alt
        } else {
            self.row_bg
        };
        self.ui.fb.fill_rect(row_rect, bg);

        // Cells
        let mut row = TableRow {
            ui: self.ui,
            widths: &self.widths,
            col_index: 0,
            y: row_rect.y,
            x: row_rect.x,
            row_height: self.row_height,
            border_color: self.border_color,
            show_borders: self.show_borders,
            num_columns: self.widths.len(),
        };

        add_cells(&mut row);

        self.row_index += 1;
    }
}

/// Table row context — used inside a row closure to add cell contents.
pub struct TableRow<'a, 'b> {
    ui: &'b mut Ui<'a>,
    widths: &'b [u32],
    col_index: usize,
    y: i32,
    x: i32,
    row_height: u32,
    border_color: Pixel,
    show_borders: bool,
    num_columns: usize,
}

impl<'a, 'b> TableRow<'a, 'b> {
    /// Add a cell. The closure draws content inside the cell.
    pub fn col(&mut self, add_content: impl FnOnce(&mut Ui)) {
        if self.col_index >= self.widths.len() {
            return;
        }

        let w = self.widths[self.col_index];
        let cell_rect = Rect::new(self.x, self.y, w, self.row_height);

        // Create a mini-Ui for the cell
        let content_rect = Rect::new(
            cell_rect.x + 6,
            cell_rect.y + (self.row_height as i32 - 12) / 2,
            (w as i32 - 12).max(0) as u32,
            12,
        );

        let saved_region = self.ui.region;
        let saved_layout = self.ui.layout;

        self.ui.region = Region::from_max_rect(&Layout::left_to_right(Align::Center), content_rect);
        self.ui.layout = Layout::left_to_right(Align::Center);

        add_content(self.ui);

        self.ui.region = saved_region;
        self.ui.layout = saved_layout;

        // Column border
        if self.show_borders && self.col_index < self.num_columns - 1 {
            self.ui.fb.draw_vline(
                self.x + w as i32,
                self.y,
                self.row_height,
                self.border_color,
            );
        }

        self.x += w as i32;
        self.col_index += 1;
    }
}
