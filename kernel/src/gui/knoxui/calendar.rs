use super::text_helpers as fonts;
/// Calendar — A month-view calendar widget for date selection.
///
/// ```ignore
/// let mut selected_day: Option<u32> = None;
/// Calendar::new("cal", 2025, 6)
///     .show(ui, &mut selected_day);
/// ```
use crate::gui::colors;
use crate::gui::framebuffer::{Pixel, Rect};
use crate::gui::id::Id;
use crate::gui::response::Response;
use crate::gui::ui::Ui;

pub struct Calendar {
    id: Id,
    year: u32,
    month: u32, // 1-12
    cell_size: u32,
}

impl Calendar {
    pub fn new(id_salt: &str, year: u32, month: u32) -> Self {
        Self {
            id: Id::from_str(id_salt),
            year,
            month: month.clamp(1, 12),
            cell_size: 28,
        }
    }

    pub fn cell_size(mut self, s: u32) -> Self {
        self.cell_size = s;
        self
    }

    /// Returns the day of week (0=Mon..6=Sun) for the 1st of the month.
    /// Uses Tomohiko Sakamoto's algorithm.
    fn day_of_week(y: u32, m: u32, d: u32) -> u32 {
        let t = [0u32, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = if m < 3 { y - 1 } else { y };
        (y + y / 4 - y / 100 + y / 400 + t[(m - 1) as usize] + d) % 7
    }

    fn days_in_month(y: u32, m: u32) -> u32 {
        match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                    29
                } else {
                    28
                }
            }
            _ => 30,
        }
    }

    fn month_name(m: u32) -> &'static str {
        match m {
            1 => "January",
            2 => "February",
            3 => "March",
            4 => "April",
            5 => "May",
            6 => "June",
            7 => "July",
            8 => "August",
            9 => "September",
            10 => "October",
            11 => "November",
            12 => "December",
            _ => "???",
        }
    }

    /// Show the calendar. `selected_day` is updated if a day is clicked.
    pub fn show(self, ui: &mut Ui, selected_day: &mut Option<u32>) {
        let cols = 7u32;
        let cs = self.cell_size;
        let grid_w = cols * cs;
        let header_h = 24u32;
        let day_labels_h = 18u32;
        let num_days = Self::days_in_month(self.year, self.month);
        let first_dow = Self::day_of_week(self.year, self.month, 1);
        // Adjust: Sakamoto returns 0=Sunday, we want 0=Monday
        let first_dow = if first_dow == 0 { 6 } else { first_dow - 1 };
        let total_cells = first_dow + num_days;
        let rows = total_cells.div_ceil(7);
        let total_h = header_h + day_labels_h + rows * cs;

        let outer = ui.allocate_space(grid_w, total_h);

        // Month/year header
        let header_text = {
            use alloc::format;
            format!("{} {}", Self::month_name(self.month), self.year)
        };
        fonts::draw_string_centered_bold_compact(
            ui.fb,
            &header_text,
            outer.x + grid_w as i32 / 2,
            outer.y + 6,
            colors::TEXT_PRIMARY,
        );

        // Day of week labels
        let day_labels = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
        let label_y = outer.y + header_h as i32 + 3;
        for (i, label) in day_labels.iter().enumerate() {
            let lx = outer.x + i as i32 * cs as i32 + cs as i32 / 2;
            fonts::draw_string_centered_compact(ui.fb, label, lx, label_y, colors::TEXT_MUTED);
        }

        // Day grid
        let grid_y = outer.y + header_h as i32 + day_labels_h as i32;
        let id = self.id;

        for day in 1..=num_days {
            let cell_idx = first_dow + day - 1;
            let col = cell_idx % 7;
            let row = cell_idx / 7;
            let cx = outer.x + col as i32 * cs as i32;
            let cy = grid_y + row as i32 * cs as i32;
            let cell_rect = Rect::new(cx, cy, cs, cs);

            let cell_id = id.with_index(day as usize);
            let resp = ui.interact(cell_rect, cell_id, true, false);

            let is_selected = *selected_day == Some(day);
            let is_weekend = col >= 5;

            // Background
            if is_selected {
                ui.fb
                    .fill_rounded_rect_aa(cell_rect, colors::ACCENT_PRIMARY, cs / 2);
            } else if resp.hovered {
                ui.fb
                    .fill_rounded_rect_aa(cell_rect, Pixel::new(255, 255, 255, 15), cs / 2);
            }

            // Day number
            let text_color = if is_selected {
                Pixel::new(255, 255, 255, 240)
            } else if is_weekend {
                colors::TEXT_MUTED
            } else {
                colors::TEXT_SECONDARY
            };

            let day_str = {
                use alloc::format;
                format!("{}", day)
            };
            fonts::draw_string_centered_compact(
                ui.fb,
                &day_str,
                cx + cs as i32 / 2,
                cy + (cs as i32 - 10) / 2,
                text_color,
            );

            if resp.clicked {
                *selected_day = Some(day);
            }
        }
    }
}
