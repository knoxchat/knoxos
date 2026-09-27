//! Selection, clipboard-oriented text extract, and buffer search.
use alloc::string::String;
use alloc::vec::Vec;

impl super::VtEmulator {
    /// Set selection start point
    pub fn start_selection(&mut self, row: usize, col: usize) {
        self.selection_start = Some((row, col));
        self.selection_end = Some((row, col));
        self.dirty = true;
    }

    /// Update selection end point
    pub fn update_selection(&mut self, row: usize, col: usize) {
        self.selection_end = Some((row, col));
        self.dirty = true;
    }

    /// Clear selection
    pub fn clear_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
        self.dirty = true;
    }

    /// Get selected text
    pub fn get_selection_text(&self) -> Option<String> {
        let start = self.selection_start?;
        let end = self.selection_end?;

        let (start, end) = if start.0 < end.0 || (start.0 == end.0 && start.1 <= end.1) {
            (start, end)
        } else {
            (end, start)
        };

        let mut text = String::new();
        for row in start.0..=end.0 {
            if row >= self.rows {
                break;
            }
            let col_start = if row == start.0 { start.1 } else { 0 };
            let col_end = if row == end.0 { end.1 + 1 } else { self.cols };

            for col in col_start..col_end.min(self.cols) {
                text.push(self.cells[row][col].ch);
            }
            if row < end.0 {
                text.push('\n');
            }
        }

        // Trim trailing whitespace per line
        let trimmed: Vec<&str> = text.lines().map(|l| l.trim_end()).collect();
        Some(trimmed.join("\n"))
    }

    /// Check if a cell is within the current selection
    pub fn is_selected(&self, row: usize, col: usize) -> bool {
        let start = match self.selection_start {
            Some(s) => s,
            None => return false,
        };
        let end = match self.selection_end {
            Some(e) => e,
            None => return false,
        };

        let (start, end) = if start.0 < end.0 || (start.0 == end.0 && start.1 <= end.1) {
            (start, end)
        } else {
            (end, start)
        };

        if row < start.0 || row > end.0 {
            return false;
        }
        if row == start.0 && row == end.0 {
            return col >= start.1 && col <= end.1;
        }
        if row == start.0 {
            return col >= start.1;
        }
        if row == end.0 {
            return col <= end.1;
        }
        true
    }

    /// Search for text in the terminal buffer and scrollback
    pub fn search(&self, query: &str) -> Vec<(usize, usize)> {
        let mut matches = Vec::new();
        if query.is_empty() {
            return matches;
        }

        // Search scrollback
        for (row_idx, row) in self.scrollback.iter().enumerate() {
            let line: String = row.iter().map(|c| c.ch).collect();
            let mut search_from = 0;
            while let Some(pos) = line[search_from..].find(query) {
                matches.push((row_idx, search_from + pos));
                search_from += pos + 1;
            }
        }

        // Search visible buffer
        for (row_idx, row) in self.cells.iter().enumerate() {
            let line: String = row.iter().map(|c| c.ch).collect();
            let mut search_from = 0;
            while let Some(pos) = line[search_from..].find(query) {
                matches.push((self.scrollback.len() + row_idx, search_from + pos));
                search_from += pos + 1;
            }
        }

        matches
    }

    /// Get total line count (scrollback + visible)
    pub fn total_lines(&self) -> usize {
        self.scrollback.len() + self.rows
    }
}
