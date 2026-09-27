//! Screen buffer operations: put, scroll, erase, insert/delete.
use alloc::vec::Vec;

use super::types::Cell;

impl super::VtEmulator {
    /// Put a character at current cursor position
    pub(crate) fn put_char(&mut self, ch: char) {
        if self.cursor_col >= self.cols {
            if self.auto_wrap {
                self.cursor_col = 0;
                self.line_feed();
            } else {
                self.cursor_col = self.cols - 1;
            }
        }

        if self.insert_mode {
            self.insert_chars(1);
        }

        self.cells[self.cursor_row][self.cursor_col] = Cell {
            ch,
            attr: self.current_attr,
        };
        self.cursor_col += 1;
        self.dirty = true;
    }

    /// Line feed - move cursor down, scroll if needed
    pub(crate) fn line_feed(&mut self) {
        if self.cursor_row >= self.scroll_bottom {
            self.scroll_up();
        } else {
            self.cursor_row += 1;
        }
        self.dirty = true;
    }

    /// Reverse line feed - move cursor up, scroll if needed
    pub(crate) fn reverse_line_feed(&mut self) {
        if self.cursor_row <= self.scroll_top {
            self.scroll_down();
        } else {
            self.cursor_row -= 1;
        }
        self.dirty = true;
    }

    /// Scroll up - move all lines up, blank bottom line
    pub(crate) fn scroll_up(&mut self) {
        if self.scroll_top < self.scroll_bottom {
            let removed = self.cells.remove(self.scroll_top);
            // Save to scrollback buffer (only when scrolling the whole screen)
            if self.scroll_top == 0 && self.alt_cells.is_none() {
                self.scrollback.push(removed);
                if self.scrollback.len() > self.max_scrollback {
                    self.scrollback.remove(0);
                }
            }
            let mut blank_row = Vec::with_capacity(self.cols);
            blank_row.resize(self.cols, Cell::default());
            self.cells.insert(self.scroll_bottom, blank_row);
        }
        self.dirty = true;
    }

    /// Scroll down - move all lines down, blank top line
    pub(crate) fn scroll_down(&mut self) {
        if self.scroll_top < self.scroll_bottom {
            self.cells.remove(self.scroll_bottom);
            let mut blank_row = Vec::with_capacity(self.cols);
            blank_row.resize(self.cols, Cell::default());
            self.cells.insert(self.scroll_top, blank_row);
        }
        self.dirty = true;
    }

    /// Erase display
    pub(crate) fn erase_display(&mut self, mode: u8) {
        match mode {
            0 => {
                // Erase from cursor to end
                self.erase_line(0);
                for row in (self.cursor_row + 1)..self.rows {
                    for col in 0..self.cols {
                        self.cells[row][col] = Cell::default();
                    }
                }
            }
            1 => {
                // Erase from start to cursor
                for row in 0..self.cursor_row {
                    for col in 0..self.cols {
                        self.cells[row][col] = Cell::default();
                    }
                }
                self.erase_line(1);
            }
            2 | 3 => {
                // Erase entire display
                for row in 0..self.rows {
                    for col in 0..self.cols {
                        self.cells[row][col] = Cell::default();
                    }
                }
            }
            _ => {}
        }
        self.dirty = true;
    }

    /// Erase line
    pub(crate) fn erase_line(&mut self, mode: u8) {
        match mode {
            0 => {
                // Erase from cursor to end of line
                for col in self.cursor_col..self.cols {
                    self.cells[self.cursor_row][col] = Cell::default();
                }
            }
            1 => {
                // Erase from start to cursor
                for col in 0..=self.cursor_col.min(self.cols - 1) {
                    self.cells[self.cursor_row][col] = Cell::default();
                }
            }
            2 => {
                // Erase entire line
                for col in 0..self.cols {
                    self.cells[self.cursor_row][col] = Cell::default();
                }
            }
            _ => {}
        }
        self.dirty = true;
    }

    /// Insert lines at current position
    pub(crate) fn insert_lines(&mut self, n: usize) {
        for _ in 0..n {
            if self.cursor_row < self.scroll_bottom {
                self.cells.remove(self.scroll_bottom);
                let mut blank = Vec::with_capacity(self.cols);
                blank.resize(self.cols, Cell::default());
                self.cells.insert(self.cursor_row, blank);
            }
        }
        self.dirty = true;
    }

    /// Delete lines at current position
    pub(crate) fn delete_lines(&mut self, n: usize) {
        for _ in 0..n {
            if self.cursor_row <= self.scroll_bottom {
                self.cells.remove(self.cursor_row);
                let mut blank = Vec::with_capacity(self.cols);
                blank.resize(self.cols, Cell::default());
                self.cells.insert(self.scroll_bottom, blank);
            }
        }
        self.dirty = true;
    }

    /// Insert characters at current position
    pub(crate) fn insert_chars(&mut self, n: usize) {
        let row = &mut self.cells[self.cursor_row];
        for _ in 0..n {
            if self.cursor_col < self.cols {
                row.pop();
                row.insert(self.cursor_col, Cell::default());
            }
        }
        self.dirty = true;
    }

    /// Delete characters at current position
    pub(crate) fn delete_chars(&mut self, n: usize) {
        let row = &mut self.cells[self.cursor_row];
        for _ in 0..n {
            if self.cursor_col < row.len() {
                row.remove(self.cursor_col);
                row.push(Cell::default());
            }
        }
        self.dirty = true;
    }

    /// Save cursor position
    pub(crate) fn save_cursor(&mut self) {
        self.saved_row = self.cursor_row;
        self.saved_col = self.cursor_col;
    }

    /// Restore cursor position
    pub(crate) fn restore_cursor(&mut self) {
        self.cursor_row = self.saved_row;
        self.cursor_col = self.saved_col;
    }
}
