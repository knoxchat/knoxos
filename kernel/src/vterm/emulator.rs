//! Terminal emulator state and lifecycle (create, reset, resize, feed).
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;

use super::types::{Cell, CellAttr, KittyImage, ParserState, SixelImage};

/// Terminal emulator state
pub struct VtEmulator {
    /// Screen buffer (rows × cols of characters)
    pub cells: Vec<Vec<Cell>>,
    /// Number of columns
    pub cols: usize,
    /// Number of rows
    pub rows: usize,
    /// Current cursor position
    pub cursor_row: usize,
    pub cursor_col: usize,
    /// Saved cursor position
    pub saved_row: usize,
    pub saved_col: usize,
    /// Current text attributes
    pub current_attr: CellAttr,
    /// Parser state
    pub state: ParserState,
    /// ESC sequence buffer
    pub esc_buf: Vec<u8>,
    /// Tab stops
    pub tab_stops: Vec<bool>,
    /// Scroll region (top, bottom)
    pub scroll_top: usize,
    pub scroll_bottom: usize,
    /// Modes
    pub cursor_visible: bool,
    pub auto_wrap: bool,
    pub origin_mode: bool,
    pub insert_mode: bool,
    pub new_line_mode: bool,
    pub application_cursor_keys: bool,
    pub application_keypad: bool,
    /// Whether content has changed since last check
    pub dirty: bool,
    /// Terminal title
    pub title: String,
    /// Scrollback buffer - lines that scrolled off the top
    pub scrollback: Vec<Vec<Cell>>,
    /// Maximum scrollback lines
    pub max_scrollback: usize,
    /// Alternate screen buffer (for vim, less, etc.)
    pub alt_cells: Option<Vec<Vec<Cell>>>,
    /// Saved cursor for alternate screen
    pub alt_saved_row: usize,
    pub alt_saved_col: usize,
    /// Selection state
    pub selection_start: Option<(usize, usize)>,
    pub selection_end: Option<(usize, usize)>,
    /// Bracketed paste mode
    pub bracketed_paste: bool,
    /// Mouse tracking mode — 0=off, 1000=X10, 1002=button, 1003=any, 1006=SGR
    pub mouse_tracking: bool,
    /// Mouse encoding mode (0=default/X10, 1005=UTF-8, 1006=SGR, 1015=urxvt)
    pub mouse_encoding: u32,
    /// Focus tracking
    pub focus_tracking: bool,
    /// Synchronized output mode (CSI ? 2026 h/l)
    pub synchronized_output: bool,
    /// UTF-8 partial byte buffer
    pub utf8_buf: Vec<u8>,
    /// UTF-8 bytes remaining
    pub utf8_remaining: usize,
    /// Response queue — bytes to send back to the host/PTY master
    pub response_queue: Vec<u8>,
    /// OSC 52 clipboard content
    pub clipboard: String,
    /// Active hyperlink (OSC 8)
    pub active_hyperlink: Option<String>,
    /// DCS string buffer (P8.4)
    pub dcs_buf: Vec<u8>,
    /// Sixel graphics images stored as (row, col, width, height, pixel_data) (P8.2)
    pub sixel_images: Vec<SixelImage>,
    /// Kitty graphics image store (P8.3)
    pub kitty_images: Vec<KittyImage>,
    /// Kitty image ID counter
    pub kitty_next_id: u32,
}

impl VtEmulator {
    /// Create a new terminal emulator
    pub fn new(cols: usize, rows: usize) -> Self {
        let mut cells = Vec::with_capacity(rows);
        for _ in 0..rows {
            let mut row = Vec::with_capacity(cols);
            row.resize(cols, Cell::default());
            cells.push(row);
        }

        let mut tab_stops = vec![false; cols];
        for i in (0..cols).step_by(8) {
            tab_stops[i] = true;
        }

        Self {
            cells,
            cols,
            rows,
            cursor_row: 0,
            cursor_col: 0,
            saved_row: 0,
            saved_col: 0,
            current_attr: CellAttr::default(),
            state: ParserState::Normal,
            esc_buf: Vec::new(),
            tab_stops,
            scroll_top: 0,
            scroll_bottom: rows - 1,
            cursor_visible: true,
            auto_wrap: true,
            origin_mode: false,
            insert_mode: false,
            new_line_mode: true,
            application_cursor_keys: false,
            application_keypad: false,
            dirty: false,
            title: String::from("Terminal"),
            scrollback: Vec::new(),
            max_scrollback: 10000,
            alt_cells: None,
            alt_saved_row: 0,
            alt_saved_col: 0,
            selection_start: None,
            selection_end: None,
            bracketed_paste: false,
            mouse_tracking: false,
            mouse_encoding: 0,
            focus_tracking: false,
            synchronized_output: false,
            utf8_buf: Vec::new(),
            utf8_remaining: 0,
            response_queue: Vec::new(),
            clipboard: String::new(),
            active_hyperlink: None,
            dcs_buf: Vec::new(),
            sixel_images: Vec::new(),
            kitty_images: Vec::new(),
            kitty_next_id: 1,
        }
    }

    // ── SIGWINCH ────────────────────────────────────────────────

    /// Resize terminal and deliver SIGWINCH to foreground process group
    pub fn resize_and_notify(&mut self, new_rows: usize, new_cols: usize, foreground_pgid: u32) {
        self.resize(new_rows, new_cols);
        // Send SIGWINCH to foreground process group
        if foreground_pgid > 0 {
            let _ = crate::pgrp::killpg(foreground_pgid, crate::signals::Signal::SIGWINCH);
        }
    }

    /// Reset terminal to initial state
    pub fn reset(&mut self) {
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.current_attr = CellAttr::default();
        self.scroll_top = 0;
        self.scroll_bottom = self.rows - 1;
        self.cursor_visible = true;
        self.auto_wrap = true;
        self.origin_mode = false;
        self.insert_mode = false;
        self.new_line_mode = true;
        self.bracketed_paste = false;
        self.mouse_tracking = false;
        self.mouse_encoding = 0;
        self.focus_tracking = false;
        self.synchronized_output = false;
        self.selection_start = None;
        self.selection_end = None;
        self.response_queue.clear();
        self.clipboard.clear();
        self.active_hyperlink = None;
        self.utf8_buf.clear();
        self.utf8_remaining = 0;
        if self.alt_cells.is_some() {
            self.leave_alt_screen();
        }
        self.erase_display(2);
    }

    /// Process a string of bytes
    pub fn process_bytes(&mut self, data: &[u8]) {
        for &byte in data {
            self.process_byte(byte);
        }
    }

    /// Process a string
    pub fn process_str(&mut self, s: &str) {
        self.process_bytes(s.as_bytes());
    }

    /// Get screen content as a string (for debugging)
    pub fn screen_to_string(&self) -> String {
        let mut output = String::new();
        for row in &self.cells {
            let line: String = row.iter().map(|c| c.ch).collect();
            writeln!(output, "{}", line.trim_end()).unwrap();
        }
        output
    }

    /// Take dirty flag (read and clear)
    pub fn take_dirty(&mut self) -> bool {
        let was_dirty = self.dirty;
        self.dirty = false;
        was_dirty
    }

    /// Resize the terminal
    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        // Adjust rows
        while self.cells.len() < new_rows {
            let mut row = Vec::with_capacity(new_cols);
            row.resize(new_cols, Cell::default());
            self.cells.push(row);
        }
        while self.cells.len() > new_rows {
            self.cells.pop();
        }

        // Adjust columns
        for row in &mut self.cells {
            row.resize(new_cols, Cell::default());
        }

        self.cols = new_cols;
        self.rows = new_rows;
        self.scroll_bottom = new_rows - 1;
        self.cursor_row = self.cursor_row.min(new_rows - 1);
        self.cursor_col = self.cursor_col.min(new_cols - 1);

        // Reset tab stops
        self.tab_stops = vec![false; new_cols];
        for i in (0..new_cols).step_by(8) {
            self.tab_stops[i] = true;
        }

        self.dirty = true;
    }
}
