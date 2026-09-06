/// VT100/ANSI Terminal Emulator - Full escape sequence processing
/// Implements VT100, VT220, and xterm-compatible escape sequences
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;

/// Sixel graphics image (P8.2)
#[derive(Debug, Clone)]
pub struct SixelImage {
    /// Row where the image starts
    pub row: usize,
    /// Column where the image starts
    pub col: usize,
    /// Image width in pixels
    pub width: usize,
    /// Image height in pixels
    pub height: usize,
    /// RGBA pixel data (width * height * 4)
    pub pixels: Vec<u8>,
}

/// Kitty graphics protocol image (P8.3)
#[derive(Debug, Clone)]
pub struct KittyImage {
    /// Image ID
    pub id: u32,
    /// Placement row
    pub row: usize,
    /// Placement col
    pub col: usize,
    /// Image width
    pub width: usize,
    /// Image height
    pub height: usize,
    /// RGBA pixel data
    pub pixels: Vec<u8>,
    /// Z-index for layering
    pub z_index: i32,
}

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

/// A single cell on screen
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub attr: CellAttr,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            attr: CellAttr::default(),
        }
    }
}

/// Cell text attributes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellAttr {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub inverse: bool,
    pub hidden: bool,
    pub strikethrough: bool,
}

impl Default for CellAttr {
    fn default() -> Self {
        Self {
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            blink: false,
            inverse: false,
            hidden: false,
            strikethrough: false,
        }
    }
}

/// Terminal colors (16 standard + 256 extended + 24-bit)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    /// Convert to RGBA for rendering
    pub fn to_rgba(&self, is_bold: bool) -> (u8, u8, u8, u8) {
        match self {
            Color::Default => {
                if is_bold {
                    (255, 255, 255, 255)
                } else {
                    (204, 204, 204, 255)
                }
            }
            Color::Black => {
                if is_bold {
                    (85, 85, 85, 255)
                } else {
                    (0, 0, 0, 255)
                }
            }
            Color::Red => {
                if is_bold {
                    (255, 85, 85, 255)
                } else {
                    (170, 0, 0, 255)
                }
            }
            Color::Green => {
                if is_bold {
                    (85, 255, 85, 255)
                } else {
                    (0, 170, 0, 255)
                }
            }
            Color::Yellow => {
                if is_bold {
                    (255, 255, 85, 255)
                } else {
                    (170, 170, 0, 255)
                }
            }
            Color::Blue => {
                if is_bold {
                    (85, 85, 255, 255)
                } else {
                    (0, 0, 170, 255)
                }
            }
            Color::Magenta => {
                if is_bold {
                    (255, 85, 255, 255)
                } else {
                    (170, 0, 170, 255)
                }
            }
            Color::Cyan => {
                if is_bold {
                    (85, 255, 255, 255)
                } else {
                    (0, 170, 170, 255)
                }
            }
            Color::White => {
                if is_bold {
                    (255, 255, 255, 255)
                } else {
                    (170, 170, 170, 255)
                }
            }
            Color::BrightBlack => (85, 85, 85, 255),
            Color::BrightRed => (255, 85, 85, 255),
            Color::BrightGreen => (85, 255, 85, 255),
            Color::BrightYellow => (255, 255, 85, 255),
            Color::BrightBlue => (85, 85, 255, 255),
            Color::BrightMagenta => (255, 85, 255, 255),
            Color::BrightCyan => (85, 255, 255, 255),
            Color::BrightWhite => (255, 255, 255, 255),
            Color::Indexed(idx) => index_to_rgb(*idx),
            Color::Rgb(r, g, b) => (*r, *g, *b, 255),
        }
    }

    pub fn to_bg_rgba(&self) -> (u8, u8, u8, u8) {
        match self {
            Color::Default => (0, 0, 0, 255),
            _ => self.to_rgba(false),
        }
    }
}

/// Convert 256-color index to RGB
pub fn index_to_rgb(idx: u8) -> (u8, u8, u8, u8) {
    match idx {
        0 => (0, 0, 0, 255),
        1 => (170, 0, 0, 255),
        2 => (0, 170, 0, 255),
        3 => (170, 170, 0, 255),
        4 => (0, 0, 170, 255),
        5 => (170, 0, 170, 255),
        6 => (0, 170, 170, 255),
        7 => (170, 170, 170, 255),
        8 => (85, 85, 85, 255),
        9 => (255, 85, 85, 255),
        10 => (85, 255, 85, 255),
        11 => (255, 255, 85, 255),
        12 => (85, 85, 255, 255),
        13 => (255, 85, 255, 255),
        14 => (85, 255, 255, 255),
        15 => (255, 255, 255, 255),
        16..=231 => {
            let idx = idx - 16;
            let r = (idx / 36) * 51;
            let g = ((idx / 6) % 6) * 51;
            let b = (idx % 6) * 51;
            (r, g, b, 255)
        }
        232..=255 => {
            let gray = 8 + (idx - 232) * 10;
            (gray, gray, gray, 255)
        }
    }
}

/// Parser state for escape sequences
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserState {
    Normal,
    Escape,    // Got ESC
    Csi,       // Got ESC [
    Osc,       // Got ESC ]
    OscString, // Inside OSC string
    SixelData, // Sixel graphics data
    Dcs,       // Device Control String
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

    /// Process a byte of input
    pub fn process_byte(&mut self, byte: u8) {
        match self.state {
            ParserState::Normal => self.process_normal(byte),
            ParserState::Escape => self.process_escape(byte),
            ParserState::Csi => self.process_csi(byte),
            ParserState::Osc => self.process_osc(byte),
            ParserState::OscString => self.process_osc_string(byte),
            ParserState::Dcs => self.process_dcs(byte),
            ParserState::SixelData => self.process_sixel(byte),
        }
    }

    /// Process bytes in normal (non-escape) mode
    fn process_normal(&mut self, byte: u8) {
        // Handle UTF-8 continuation bytes
        if self.utf8_remaining > 0 {
            self.utf8_buf.push(byte);
            self.utf8_remaining -= 1;
            if self.utf8_remaining == 0 {
                if let Ok(s) = core::str::from_utf8(&self.utf8_buf) {
                    if let Some(ch) = s.chars().next() {
                        self.put_char(ch);
                    }
                }
                self.utf8_buf.clear();
            }
            return;
        }

        match byte {
            // ESC - start escape sequence
            0x1B => {
                self.state = ParserState::Escape;
                self.esc_buf.clear();
            }
            // BEL - bell
            0x07 => {}
            // BS - backspace
            0x08 if self.cursor_col > 0 => {
                self.cursor_col -= 1;
            }
            // HT - horizontal tab
            0x09 => {
                let next_tab = (self.cursor_col + 8) & !7;
                self.cursor_col = next_tab.min(self.cols - 1);
            }
            // LF, VT, FF - line feed (with implicit CR in new-line mode)
            0x0A..=0x0C => {
                if self.new_line_mode {
                    self.cursor_col = 0;
                }
                self.line_feed();
            }
            // CR - carriage return
            0x0D => {
                self.cursor_col = 0;
            }
            // Regular ASCII character
            0x20..=0x7E => {
                self.put_char(byte as char);
            }
            // UTF-8 2-byte start
            0xC0..=0xDF => {
                self.utf8_buf.clear();
                self.utf8_buf.push(byte);
                self.utf8_remaining = 1;
            }
            // UTF-8 3-byte start
            0xE0..=0xEF => {
                self.utf8_buf.clear();
                self.utf8_buf.push(byte);
                self.utf8_remaining = 2;
            }
            // UTF-8 4-byte start
            0xF0..=0xF7 => {
                self.utf8_buf.clear();
                self.utf8_buf.push(byte);
                self.utf8_remaining = 3;
            }
            // Fallback for high bytes (Latin-1)
            0x80..=0xBF | 0xF8..=0xFF => {
                self.put_char(byte as char);
            }
            _ => {}
        }
    }

    /// Process byte after ESC
    fn process_escape(&mut self, byte: u8) {
        match byte {
            b'[' => {
                self.state = ParserState::Csi;
                self.esc_buf.clear();
            }
            b']' => {
                self.state = ParserState::Osc;
                self.esc_buf.clear();
            }
            b'D' => {
                self.line_feed();
                self.state = ParserState::Normal;
            }
            b'M' => {
                self.reverse_line_feed();
                self.state = ParserState::Normal;
            }
            b'E' => {
                self.cursor_col = 0;
                self.line_feed();
                self.state = ParserState::Normal;
            }
            b'7' => {
                self.save_cursor();
                self.state = ParserState::Normal;
            }
            b'8' => {
                self.restore_cursor();
                self.state = ParserState::Normal;
            }
            b'c' => {
                self.reset();
                self.state = ParserState::Normal;
            }
            b'H' => {
                // Set tab stop
                if self.cursor_col < self.cols {
                    self.tab_stops[self.cursor_col] = true;
                }
                self.state = ParserState::Normal;
            }
            b'P' => {
                // DCS - Device Control String (P8.4)
                self.dcs_buf.clear();
                self.state = ParserState::Dcs;
            }
            b'_' => {
                // APC - Application Program Command (used by Kitty graphics, P8.3)
                self.dcs_buf.clear();
                self.state = ParserState::Dcs; // Reuse DCS processing
            }
            _ => {
                self.state = ParserState::Normal;
            }
        }
    }

    /// Process CSI (Control Sequence Introducer) sequences: ESC [ ...
    fn process_csi(&mut self, byte: u8) {
        match byte {
            // Parameter bytes and intermediates
            0x30..=0x3F => {
                self.esc_buf.push(byte);
            }
            // Final byte - execute the sequence
            0x40..=0x7E => {
                self.execute_csi(byte);
                self.state = ParserState::Normal;
            }
            _ => {
                self.state = ParserState::Normal;
            }
        }
    }

    /// Execute a CSI sequence
    fn execute_csi(&mut self, final_byte: u8) {
        let params = self.parse_params();

        match final_byte {
            b'A' => {
                // CUU - Cursor Up
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.cursor_row = self.cursor_row.saturating_sub(n);
            }
            b'B' => {
                // CUD - Cursor Down
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
            }
            b'C' => {
                // CUF - Cursor Forward
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.cursor_col = (self.cursor_col + n).min(self.cols - 1);
            }
            b'D' => {
                // CUB - Cursor Back
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.cursor_col = self.cursor_col.saturating_sub(n);
            }
            b'E' => {
                // CNL - Cursor Next Line
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
                self.cursor_col = 0;
            }
            b'F' => {
                // CPL - Cursor Previous Line
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.cursor_row = self.cursor_row.saturating_sub(n);
                self.cursor_col = 0;
            }
            b'G' => {
                // CHA - Cursor Horizontal Absolute
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.cursor_col = (n - 1).min(self.cols - 1);
            }
            b'H' | b'f' => {
                // CUP - Cursor Position
                let row = params.first().copied().unwrap_or(1).max(1) as usize;
                let col = params.get(1).copied().unwrap_or(1).max(1) as usize;
                self.cursor_row = (row - 1).min(self.rows - 1);
                self.cursor_col = (col - 1).min(self.cols - 1);
            }
            b'J' => {
                // ED - Erase in Display
                let mode = params.first().copied().unwrap_or(0);
                self.erase_display(mode as u8);
            }
            b'K' => {
                // EL - Erase in Line
                let mode = params.first().copied().unwrap_or(0);
                self.erase_line(mode as u8);
            }
            b'L' => {
                // IL - Insert Lines
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.insert_lines(n);
            }
            b'M' => {
                // DL - Delete Lines
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.delete_lines(n);
            }
            b'P' => {
                // DCH - Delete Characters
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.delete_chars(n);
            }
            b'S' => {
                // SU - Scroll Up
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                for _ in 0..n {
                    self.scroll_up();
                }
            }
            b'T' => {
                // SD - Scroll Down
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                for _ in 0..n {
                    self.scroll_down();
                }
            }
            b'X' => {
                // ECH - Erase Characters
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                let end = (self.cursor_col + n).min(self.cols);
                for col in self.cursor_col..end {
                    self.cells[self.cursor_row][col] = Cell::default();
                }
                self.dirty = true;
            }
            b'd' => {
                // VPA - Vertical Position Absolute
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.cursor_row = (n - 1).min(self.rows - 1);
            }
            b'h' => {
                // SM - Set Mode
                self.set_mode(&params, true);
            }
            b'l' => {
                // RM - Reset Mode
                self.set_mode(&params, false);
            }
            b'm' => {
                // SGR - Select Graphic Rendition
                self.set_graphics(&params);
            }
            b'n' => {
                // DSR - Device Status Report
                let code = params.first().copied().unwrap_or(0);
                match code {
                    5 => {
                        // Status report: reply "terminal OK"
                        self.queue_response(b"\x1b[0n");
                    }
                    6 => {
                        // Cursor position report: reply ESC [ row ; col R
                        let resp =
                            alloc::format!("\x1b[{};{}R", self.cursor_row + 1, self.cursor_col + 1);
                        self.queue_response(resp.as_bytes());
                    }
                    _ => {}
                }
            }
            b'r' => {
                // DECSTBM - Set Scrolling Region
                let top = params.first().copied().unwrap_or(1).max(1) as usize;
                let bottom = params.get(1).copied().unwrap_or(self.rows as u32) as usize;
                self.scroll_top = (top - 1).min(self.rows - 1);
                self.scroll_bottom = (bottom - 1).min(self.rows - 1);
                self.cursor_row = 0;
                self.cursor_col = 0;
            }
            b's' => {
                self.save_cursor();
            }
            b'u' => {
                self.restore_cursor();
            }
            b'@' => {
                // ICH - Insert Characters
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                self.insert_chars(n);
            }
            b'p'
                // DECRPM - Report Mode (CSI ? Ps $ p → CSI ? Ps ; Pm $ y)
                if self.is_dec_private() => {
                    let mode = params.first().copied().unwrap_or(0);
                    let setting = match mode {
                        1 => {
                            if self.application_cursor_keys {
                                1
                            } else {
                                2
                            }
                        }
                        7 => {
                            if self.auto_wrap {
                                1
                            } else {
                                2
                            }
                        }
                        25 => {
                            if self.cursor_visible {
                                1
                            } else {
                                2
                            }
                        }
                        1000 | 1002 | 1003 => {
                            if self.mouse_tracking {
                                1
                            } else {
                                2
                            }
                        }
                        1004 => {
                            if self.focus_tracking {
                                1
                            } else {
                                2
                            }
                        }
                        2004 => {
                            if self.bracketed_paste {
                                1
                            } else {
                                2
                            }
                        }
                        2026 => {
                            if self.synchronized_output {
                                1
                            } else {
                                2
                            }
                        }
                        _ => 0, // not recognized
                    };
                    let resp = alloc::format!("\x1b[?{};{}$y", mode, setting);
                    self.queue_response(resp.as_bytes());
                }
            _ => {}
        }
    }

    /// Parse CSI parameter bytes into integers
    fn parse_params(&self) -> Vec<u32> {
        if self.esc_buf.is_empty() {
            return Vec::new();
        }

        let s: String = self
            .esc_buf
            .iter()
            .filter(|&&b| b != b'?') // Skip DEC private mode prefix
            .map(|&b| b as char)
            .collect();

        s.split(';')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect()
    }

    /// Check if CSI has DEC private mode prefix '?'
    fn is_dec_private(&self) -> bool {
        self.esc_buf.first() == Some(&b'?')
    }

    /// Put a character at current cursor position
    fn put_char(&mut self, ch: char) {
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
    fn line_feed(&mut self) {
        if self.cursor_row >= self.scroll_bottom {
            self.scroll_up();
        } else {
            self.cursor_row += 1;
        }
        self.dirty = true;
    }

    /// Reverse line feed - move cursor up, scroll if needed
    fn reverse_line_feed(&mut self) {
        if self.cursor_row <= self.scroll_top {
            self.scroll_down();
        } else {
            self.cursor_row -= 1;
        }
        self.dirty = true;
    }

    /// Scroll up - move all lines up, blank bottom line
    fn scroll_up(&mut self) {
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
    fn scroll_down(&mut self) {
        if self.scroll_top < self.scroll_bottom {
            self.cells.remove(self.scroll_bottom);
            let mut blank_row = Vec::with_capacity(self.cols);
            blank_row.resize(self.cols, Cell::default());
            self.cells.insert(self.scroll_top, blank_row);
        }
        self.dirty = true;
    }

    /// Erase display
    fn erase_display(&mut self, mode: u8) {
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
    fn erase_line(&mut self, mode: u8) {
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
    fn insert_lines(&mut self, n: usize) {
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
    fn delete_lines(&mut self, n: usize) {
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
    fn insert_chars(&mut self, n: usize) {
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
    fn delete_chars(&mut self, n: usize) {
        let row = &mut self.cells[self.cursor_row];
        for _ in 0..n {
            if self.cursor_col < row.len() {
                row.remove(self.cursor_col);
                row.push(Cell::default());
            }
        }
        self.dirty = true;
    }

    /// SGR - Set Graphics Rendition
    fn set_graphics(&mut self, params: &[u32]) {
        if params.is_empty() {
            self.current_attr = CellAttr::default();
            return;
        }

        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => self.current_attr = CellAttr::default(),
                1 => self.current_attr.bold = true,
                2 => self.current_attr.dim = true,
                3 => self.current_attr.italic = true,
                4 => self.current_attr.underline = true,
                5 => self.current_attr.blink = true,
                7 => self.current_attr.inverse = true,
                8 => self.current_attr.hidden = true,
                9 => self.current_attr.strikethrough = true,
                21 => self.current_attr.bold = false,
                22 => {
                    self.current_attr.bold = false;
                    self.current_attr.dim = false;
                }
                23 => self.current_attr.italic = false,
                24 => self.current_attr.underline = false,
                25 => self.current_attr.blink = false,
                27 => self.current_attr.inverse = false,
                28 => self.current_attr.hidden = false,
                29 => self.current_attr.strikethrough = false,
                // Foreground colors
                30 => self.current_attr.fg = Color::Black,
                31 => self.current_attr.fg = Color::Red,
                32 => self.current_attr.fg = Color::Green,
                33 => self.current_attr.fg = Color::Yellow,
                34 => self.current_attr.fg = Color::Blue,
                35 => self.current_attr.fg = Color::Magenta,
                36 => self.current_attr.fg = Color::Cyan,
                37 => self.current_attr.fg = Color::White,
                38
                    // Extended foreground: 38;5;n or 38;2;r;g;b
                    if i + 1 < params.len() => {
                        match params[i + 1] {
                            5 if i + 2 < params.len() => {
                                self.current_attr.fg = Color::Indexed(params[i + 2] as u8);
                                i += 2;
                            }
                            2 if i + 4 < params.len() => {
                                self.current_attr.fg = Color::Rgb(
                                    params[i + 2] as u8,
                                    params[i + 3] as u8,
                                    params[i + 4] as u8,
                                );
                                i += 4;
                            }
                            _ => {}
                        }
                    }
                39 => self.current_attr.fg = Color::Default,
                // Background colors
                40 => self.current_attr.bg = Color::Black,
                41 => self.current_attr.bg = Color::Red,
                42 => self.current_attr.bg = Color::Green,
                43 => self.current_attr.bg = Color::Yellow,
                44 => self.current_attr.bg = Color::Blue,
                45 => self.current_attr.bg = Color::Magenta,
                46 => self.current_attr.bg = Color::Cyan,
                47 => self.current_attr.bg = Color::White,
                48
                    // Extended background
                    if i + 1 < params.len() => {
                        match params[i + 1] {
                            5 if i + 2 < params.len() => {
                                self.current_attr.bg = Color::Indexed(params[i + 2] as u8);
                                i += 2;
                            }
                            2 if i + 4 < params.len() => {
                                self.current_attr.bg = Color::Rgb(
                                    params[i + 2] as u8,
                                    params[i + 3] as u8,
                                    params[i + 4] as u8,
                                );
                                i += 4;
                            }
                            _ => {}
                        }
                    }
                49 => self.current_attr.bg = Color::Default,
                // Bright foreground colors
                90 => self.current_attr.fg = Color::BrightBlack,
                91 => self.current_attr.fg = Color::BrightRed,
                92 => self.current_attr.fg = Color::BrightGreen,
                93 => self.current_attr.fg = Color::BrightYellow,
                94 => self.current_attr.fg = Color::BrightBlue,
                95 => self.current_attr.fg = Color::BrightMagenta,
                96 => self.current_attr.fg = Color::BrightCyan,
                97 => self.current_attr.fg = Color::BrightWhite,
                // Bright background colors
                100 => self.current_attr.bg = Color::BrightBlack,
                101 => self.current_attr.bg = Color::BrightRed,
                102 => self.current_attr.bg = Color::BrightGreen,
                103 => self.current_attr.bg = Color::BrightYellow,
                104 => self.current_attr.bg = Color::BrightBlue,
                105 => self.current_attr.bg = Color::BrightMagenta,
                106 => self.current_attr.bg = Color::BrightCyan,
                107 => self.current_attr.bg = Color::BrightWhite,
                _ => {}
            }
            i += 1;
        }
    }

    /// Set/reset modes
    fn set_mode(&mut self, params: &[u32], set: bool) {
        let is_dec = self.is_dec_private();

        for &param in params {
            if is_dec {
                match param {
                    1 => self.application_cursor_keys = set,
                    7 => self.auto_wrap = set,
                    12 => {} // Blinking cursor
                    25 => self.cursor_visible = set,
                    47 | 1047 => {
                        // Alternate screen buffer
                        if set {
                            self.enter_alt_screen();
                        } else {
                            self.leave_alt_screen();
                        }
                    }
                    1049 => {
                        // Alternate screen + save/restore cursor
                        if set {
                            self.save_cursor();
                            self.enter_alt_screen();
                        } else {
                            self.leave_alt_screen();
                            self.restore_cursor();
                        }
                    }
                    2004 => self.bracketed_paste = set,
                    1000 | 1002 | 1003 => self.mouse_tracking = set,
                    1004 => self.focus_tracking = set,
                    1005 => self.mouse_encoding = if set { 1005 } else { 0 },
                    1006 => self.mouse_encoding = if set { 1006 } else { 0 },
                    1015 => self.mouse_encoding = if set { 1015 } else { 0 },
                    2026 => self.synchronized_output = set,
                    _ => {}
                }
            } else {
                match param {
                    4 => self.insert_mode = set,
                    20 => self.new_line_mode = set, // LNM: LF implies CR
                    _ => {}
                }
            }
        }
    }

    /// Enter alternate screen buffer (used by vim, less, htop, etc.)
    fn enter_alt_screen(&mut self) {
        if self.alt_cells.is_some() {
            return; // Already in alt screen
        }
        // Save main screen
        self.alt_cells = Some(self.cells.clone());
        self.alt_saved_row = self.cursor_row;
        self.alt_saved_col = self.cursor_col;
        // Clear screen for alt buffer
        self.erase_display(2);
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.dirty = true;
    }

    /// Leave alternate screen buffer
    fn leave_alt_screen(&mut self) {
        if let Some(saved) = self.alt_cells.take() {
            self.cells = saved;
            self.cursor_row = self.alt_saved_row;
            self.cursor_col = self.alt_saved_col;
            self.dirty = true;
        }
    }

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

    /// Process OSC (Operating System Command) start
    fn process_osc(&mut self, byte: u8) {
        match byte {
            0x07 => {
                // BEL - end of OSC
                self.execute_osc();
                self.state = ParserState::Normal;
            }
            0x1B => {
                // ESC - might be ST (ESC \)
                self.state = ParserState::OscString;
            }
            _ => {
                self.esc_buf.push(byte);
            }
        }
    }

    fn process_osc_string(&mut self, byte: u8) {
        if byte == b'\\' {
            self.execute_osc();
        }
        self.state = ParserState::Normal;
    }

    fn execute_osc(&mut self) {
        let s: String = self.esc_buf.iter().map(|&b| b as char).collect();
        if let Some(semicolon) = s.find(';') {
            let cmd = &s[..semicolon];
            let arg = &s[semicolon + 1..];
            match cmd {
                "0" | "2" => {
                    // Set window title
                    self.title = String::from(arg);
                }
                "1" => {
                    // Set icon name (ignore, use for title)
                    self.title = String::from(arg);
                }
                "8" => {
                    // OSC 8 — Hyperlinks: ESC ] 8 ; params ; uri ST
                    // Format: 8;params;uri or 8;;uri (to set) or 8;; (to close)
                    if let Some(semi2) = arg.find(';') {
                        let uri = &arg[semi2 + 1..];
                        if uri.is_empty() {
                            self.active_hyperlink = None;
                        } else {
                            self.active_hyperlink = Some(String::from(uri));
                        }
                    }
                }
                "52" => {
                    // OSC 52 — Clipboard: ESC ] 52 ; selection ; base64data ST
                    // selection = c (clipboard), p (primary), etc.
                    if let Some(semi2) = arg.find(';') {
                        let data = &arg[semi2 + 1..];
                        if data == "?" {
                            // Query clipboard — respond with current content (base64)
                            let encoded = base64_encode(self.clipboard.as_bytes());
                            let resp = alloc::format!("\x1b]52;c;{}\x07", encoded);
                            self.queue_response(resp.as_bytes());
                        } else {
                            // Set clipboard
                            if let Some(decoded) = base64_decode(data) {
                                self.clipboard = String::from_utf8(decoded).unwrap_or_default();
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Save cursor position
    fn save_cursor(&mut self) {
        self.saved_row = self.cursor_row;
        self.saved_col = self.cursor_col;
    }

    /// Restore cursor position
    fn restore_cursor(&mut self) {
        self.cursor_row = self.saved_row;
        self.cursor_col = self.saved_col;
    }

    // ── P8.4: DCS String Processing ─────────────────────────────

    /// Process DCS (Device Control String) bytes
    fn process_dcs(&mut self, byte: u8) {
        match byte {
            // ST (String Terminator) = ESC backslash
            0x1B => {
                // Next byte should be '\' to complete ST
                // For simplicity, treat ESC in DCS as end-of-DCS
                self.execute_dcs();
                self.state = ParserState::Normal;
            }
            // BEL also terminates
            0x07 => {
                self.execute_dcs();
                self.state = ParserState::Normal;
            }
            // C1 ST (0x9C)
            0x9C => {
                self.execute_dcs();
                self.state = ParserState::Normal;
            }
            _ => {
                if self.dcs_buf.len() < 65536 {
                    self.dcs_buf.push(byte);
                } else {
                    // Buffer overflow — abort
                    self.state = ParserState::Normal;
                    self.dcs_buf.clear();
                }
            }
        }
    }

    /// Execute a completed DCS string
    fn execute_dcs(&mut self) {
        if self.dcs_buf.is_empty() {
            return;
        }

        let first = self.dcs_buf[0];
        match first {
            // Kitty graphics: _G<params>;<payload> (APC variant)
            b'G' => {
                let data = self.dcs_buf[1..].to_vec();
                self.process_kitty_graphics(&data);
            }
            // Sixel: DCS [params] q [sixel-data] ST
            b'q' | b'0'..=b'9' => {
                // Check if this is a Sixel sequence
                // Format: DCS P1;P2;P3 q <sixel-data> ST
                let data = self.dcs_buf.clone();
                if let Some(q_pos) = data.iter().position(|&b| b == b'q') {
                    let sixel_data = data[q_pos + 1..].to_vec();
                    self.parse_sixel(&sixel_data);
                } else if first == b'q' {
                    // DCS q <data> — immediate sixel
                    let sixel_data = data[1..].to_vec();
                    self.parse_sixel(&sixel_data);
                }
            }
            // DECRQSS — Request Status String: DCS $ q <string> ST
            b'$' if self.dcs_buf.len() >= 2 && self.dcs_buf[1] == b'q' => {
                // Respond with DCS 1 $ r <status> ST for valid requests
                let query = &self.dcs_buf[2..];
                let resp = if query == b"m" {
                    // SGR query — respond with current attributes
                    alloc::format!("\x1bP1$r0m\x1b\\")
                } else if query == b"r" {
                    // DECSTBM query
                    alloc::format!(
                        "\x1bP1$r{};{}r\x1b\\",
                        self.scroll_top + 1,
                        self.scroll_bottom + 1
                    )
                } else {
                    // Unknown — respond with DCS 0 $ r ST
                    alloc::format!("\x1bP0$r\x1b\\")
                };
                self.queue_response(resp.as_bytes());
            }
            // XTGETTCAP — DCS + q <hex-encoded-name> ST
            b'+' if self.dcs_buf.len() >= 2 && self.dcs_buf[1] == b'q' => {
                // Respond that the capability is not found
                let resp = alloc::format!("\x1bP0+r\x1b\\");
                self.queue_response(resp.as_bytes());
            }
            _ => {
                // Unknown DCS — ignore
            }
        }
        self.dcs_buf.clear();
    }

    // ── P8.2: Sixel Graphics ────────────────────────────────────

    /// Process Sixel data in streaming mode
    fn process_sixel(&mut self, byte: u8) {
        match byte {
            0x1B | 0x07 | 0x9C => {
                // Terminator — finalize sixel
                let data = self.dcs_buf.clone();
                self.parse_sixel(&data);
                self.dcs_buf.clear();
                self.state = ParserState::Normal;
            }
            _ => {
                if self.dcs_buf.len() < 1048576 {
                    self.dcs_buf.push(byte);
                } else {
                    self.state = ParserState::Normal;
                    self.dcs_buf.clear();
                }
            }
        }
    }

    /// Parse Sixel data and create an image
    fn parse_sixel(&mut self, data: &[u8]) {
        // Sixel format: rows of 6-pixel vertical strips
        // Characters 0x3F-0x7E encode 6 vertical pixels each
        // '#' followed by color params sets the current color
        // '-' = Graphics CR/LF, '$' = Graphics CR
        // '!' followed by count and char = repeat

        let mut x: usize = 0;
        let mut y: usize = 0;
        let mut max_x: usize = 0;
        let mut max_y: usize = 0;
        let mut current_color: [u8; 4] = [255, 255, 255, 255]; // RGBA white default
        let mut colors: Vec<[u8; 4]> = Vec::new();
        // Pre-populate with 256 default colors
        for _ in 0..256 {
            colors.push([0, 0, 0, 255]);
        }
        // Pixel buffer: we'll accumulate and determine size at the end
        let mut pixel_rows: Vec<Vec<[u8; 4]>> = Vec::new();
        // Ensure we have at least 6 rows
        for _ in 0..6 {
            pixel_rows.push(Vec::new());
        }

        let mut i = 0;
        while i < data.len() {
            let b = data[i];
            match b {
                // Sixel data character: 0x3F ('?') through 0x7E ('~')
                0x3F..=0x7E => {
                    let bits = b - 0x3F;
                    // Set 6 vertical pixels
                    for bit in 0..6 {
                        let row = y + bit;
                        while pixel_rows.len() <= row {
                            pixel_rows.push(Vec::new());
                        }
                        while pixel_rows[row].len() <= x {
                            pixel_rows[row].push([0, 0, 0, 0]); // transparent
                        }
                        if (bits >> bit) & 1 != 0 {
                            pixel_rows[row][x] = current_color;
                        }
                    }
                    x += 1;
                    if x > max_x {
                        max_x = x;
                    }
                    if y + 6 > max_y {
                        max_y = y + 6;
                    }
                    i += 1;
                }
                // Graphics newline
                b'-' => {
                    y += 6;
                    x = 0;
                    i += 1;
                }
                // Graphics carriage return
                b'$' => {
                    x = 0;
                    i += 1;
                }
                // Repeat: !<count><char>
                b'!' => {
                    i += 1;
                    let mut count: usize = 0;
                    while i < data.len() && data[i].is_ascii_digit() {
                        count = count * 10 + (data[i] - b'0') as usize;
                        i += 1;
                    }
                    if i < data.len() && data[i] >= 0x3F && data[i] <= 0x7E {
                        let bits = data[i] - 0x3F;
                        for _ in 0..count {
                            for bit in 0..6 {
                                let row = y + bit;
                                while pixel_rows.len() <= row {
                                    pixel_rows.push(Vec::new());
                                }
                                while pixel_rows[row].len() <= x {
                                    pixel_rows[row].push([0, 0, 0, 0]);
                                }
                                if (bits >> bit) & 1 != 0 {
                                    pixel_rows[row][x] = current_color;
                                }
                            }
                            x += 1;
                        }
                        if x > max_x {
                            max_x = x;
                        }
                        if y + 6 > max_y {
                            max_y = y + 6;
                        }
                        i += 1;
                    }
                }
                // Color definition: #<index>;<type>;<p1>;<p2>;<p3>
                // Color selection: #<index>
                b'#' => {
                    i += 1;
                    let mut index: usize = 0;
                    while i < data.len() && data[i].is_ascii_digit() {
                        index = index * 10 + (data[i] - b'0') as usize;
                        i += 1;
                    }
                    if i < data.len() && data[i] == b';' {
                        // Color definition
                        i += 1;
                        let mut params = [0u16; 4];
                        let mut pi = 0;
                        while i < data.len() && pi < 4 {
                            if data[i] == b';' {
                                pi += 1;
                                i += 1;
                            } else if data[i].is_ascii_digit() {
                                params[pi] = params[pi] * 10 + (data[i] - b'0') as u16;
                                i += 1;
                            } else {
                                break;
                            }
                        }
                        // params[0] = type (2=RGB), params[1..3] = values
                        if params[0] == 2 {
                            // RGB percentages (0-100)
                            let r = (params[1] as u32 * 255 / 100).min(255) as u8;
                            let g = (params[2] as u32 * 255 / 100).min(255) as u8;
                            let b_val = (params[3] as u32 * 255 / 100).min(255) as u8;
                            while colors.len() <= index {
                                colors.push([0, 0, 0, 255]);
                            }
                            colors[index] = [r, g, b_val, 255];
                        }
                        if index < colors.len() {
                            current_color = colors[index];
                        }
                    } else {
                        // Color selection only
                        if index < colors.len() {
                            current_color = colors[index];
                        }
                    }
                }
                _ => {
                    i += 1; // skip unknown
                }
            }
        }

        // Build the image if we got any pixels
        if max_x > 0 && max_y > 0 {
            let mut pixels = Vec::with_capacity(max_x * max_y * 4);
            for row in 0..max_y {
                for col in 0..max_x {
                    if row < pixel_rows.len() && col < pixel_rows[row].len() {
                        pixels.extend_from_slice(&pixel_rows[row][col]);
                    } else {
                        pixels.extend_from_slice(&[0, 0, 0, 0]);
                    }
                }
            }

            let image = SixelImage {
                row: self.cursor_row,
                col: self.cursor_col,
                width: max_x,
                height: max_y,
                pixels,
            };
            self.sixel_images.push(image);
            // Advance cursor past the image
            let rows_used = max_y.div_ceil(6);
            self.cursor_row = (self.cursor_row + rows_used).min(self.rows - 1);
            self.dirty = true;
        }
    }

    // ── P8.3: Kitty Graphics Protocol ───────────────────────────

    /// Process a Kitty graphics command (received via APC or DCS)
    /// Format: ESC_G<key>=<value>,<key>=<value>;<payload>
    pub fn process_kitty_graphics(&mut self, data: &[u8]) {
        // Parse key=value pairs before semicolon, and payload after
        let semi_pos = data.iter().position(|&b| b == b';');
        let (params_data, payload_data) = match semi_pos {
            Some(pos) => (&data[..pos], &data[pos + 1..]),
            None => (data, &[] as &[u8]),
        };

        // Parse parameters
        let params_str = core::str::from_utf8(params_data).unwrap_or("");
        let mut action = b't'; // transmit (default)
        let mut format = 32u8; // RGBA (default)
        let mut width = 0usize;
        let mut height = 0usize;
        let mut image_id = 0u32;
        let mut z_index = 0i32;
        let mut _more_chunks = false;

        for pair in params_str.split(',') {
            if let Some((key, val)) = pair.split_once('=') {
                match key {
                    "a" => action = val.as_bytes().first().copied().unwrap_or(b't'),
                    "f" => format = val.parse().unwrap_or(32),
                    "s" => width = val.parse().unwrap_or(0),
                    "v" => height = val.parse().unwrap_or(0),
                    "i" => image_id = val.parse().unwrap_or(0),
                    "z" => z_index = val.parse().unwrap_or(0),
                    "m" => _more_chunks = val == "1",
                    _ => {}
                }
            }
        }

        match action {
            b't' | b'T' => {
                // Transmit (and display) image data
                // Payload is base64-encoded pixel data
                if width == 0 || height == 0 {
                    // Can't create image without dimensions
                    let resp = alloc::format!("\x1b_Gi={};EINVAL\x1b\\", image_id);
                    self.queue_response(resp.as_bytes());
                    return;
                }

                let decoded = match core::str::from_utf8(payload_data) {
                    Ok(s) => base64_decode(s).unwrap_or_default(),
                    Err(_) => Vec::new(),
                };
                let expected = match format {
                    24 => width * height * 3, // RGB
                    32 => width * height * 4, // RGBA
                    _ => 0,
                };

                // Convert to RGBA if needed
                let pixels = if format == 24 && decoded.len() >= expected {
                    let mut rgba = Vec::with_capacity(width * height * 4);
                    for chunk in decoded.chunks(3) {
                        if chunk.len() == 3 {
                            rgba.push(chunk[0]);
                            rgba.push(chunk[1]);
                            rgba.push(chunk[2]);
                            rgba.push(255);
                        }
                    }
                    rgba
                } else {
                    decoded
                };

                let id = if image_id > 0 {
                    image_id
                } else {
                    let id = self.kitty_next_id;
                    self.kitty_next_id += 1;
                    id
                };

                let image = KittyImage {
                    id,
                    row: self.cursor_row,
                    col: self.cursor_col,
                    width,
                    height,
                    pixels,
                    z_index,
                };
                self.kitty_images.push(image);
                self.dirty = true;

                // Send OK response
                let resp = alloc::format!("\x1b_Gi={};OK\x1b\\", id);
                self.queue_response(resp.as_bytes());
            }
            b'd' => {
                // Delete image(s)
                if image_id > 0 {
                    self.kitty_images.retain(|img| img.id != image_id);
                } else {
                    // Delete all
                    self.kitty_images.clear();
                }
                self.dirty = true;
            }
            b'q' => {
                // Query — respond with OK
                let resp = alloc::format!("\x1b_Gi={};OK\x1b\\", image_id);
                self.queue_response(resp.as_bytes());
            }
            _ => {
                // Unknown action
            }
        }
    }

    // ── Response Queue ──────────────────────────────────────────

    /// Queue response bytes to send back to the host
    fn queue_response(&mut self, data: &[u8]) {
        self.response_queue.extend_from_slice(data);
    }

    /// Drain and return all pending response bytes
    pub fn drain_response(&mut self) -> Vec<u8> {
        let data = self.response_queue.clone();
        self.response_queue.clear();
        data
    }

    /// Check if there are pending responses
    pub fn has_response(&self) -> bool {
        !self.response_queue.is_empty()
    }

    // ── Mouse Event Encoding ────────────────────────────────────

    /// Encode a mouse event in the current encoding mode
    /// button: 0=left, 1=middle, 2=right, 3=release, 64=scroll up, 65=scroll down
    /// modifiers: bits for shift(4), alt(8), ctrl(16)
    pub fn encode_mouse_event(&mut self, button: u8, col: usize, row: usize, pressed: bool) {
        if !self.mouse_tracking {
            return;
        }
        let cb = button | if !pressed { 3 } else { 0 };

        match self.mouse_encoding {
            1006 => {
                // SGR encoding: ESC [ < Cb ; Cx ; Cy M/m
                let suffix = if pressed { 'M' } else { 'm' };
                let resp = alloc::format!("\x1b[<{};{};{}{}", cb, col + 1, row + 1, suffix);
                self.queue_response(resp.as_bytes());
            }
            1005 => {
                // UTF-8 encoding
                let mut resp = Vec::new();
                resp.push(0x1b);
                resp.push(b'[');
                resp.push(b'M');
                resp.push(cb + 32);
                // UTF-8 encode col+33 and row+33
                let cx = (col as u8).wrapping_add(33);
                let cy = (row as u8).wrapping_add(33);
                resp.push(cx);
                resp.push(cy);
                self.queue_response(&resp);
            }
            _ => {
                // X10 / normal encoding: ESC [ M Cb Cx Cy
                if pressed {
                    let mut resp = Vec::new();
                    resp.push(0x1b);
                    resp.push(b'[');
                    resp.push(b'M');
                    resp.push(cb + 32);
                    resp.push((col as u8).wrapping_add(33));
                    resp.push((row as u8).wrapping_add(33));
                    self.queue_response(&resp);
                }
            }
        }
    }

    // ── Focus Events ────────────────────────────────────────────

    /// Send focus in event
    pub fn focus_in(&mut self) {
        if self.focus_tracking {
            self.queue_response(b"\x1b[I");
        }
    }

    /// Send focus out event
    pub fn focus_out(&mut self) {
        if self.focus_tracking {
            self.queue_response(b"\x1b[O");
        }
    }

    // ── Bracketed Paste ─────────────────────────────────────────

    /// Wrap pasted text in bracketed paste sequences
    pub fn paste_text(&mut self, text: &str) -> Vec<u8> {
        if self.bracketed_paste {
            let mut data = Vec::new();
            data.extend_from_slice(b"\x1b[200~");
            data.extend_from_slice(text.as_bytes());
            data.extend_from_slice(b"\x1b[201~");
            data
        } else {
            text.as_bytes().to_vec()
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

/// Initialize the VT100 emulator module
pub fn init() {
    crate::serial_println!("[KnoxOS] VT100/ANSI terminal emulator initialized");
}

// ── Base64 helpers for OSC 52 ────────────────────────────────

const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() {
            data[i + 1] as u32
        } else {
            0
        };
        let b2 = if i + 2 < data.len() {
            data[i + 2] as u32
        } else {
            0
        };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(B64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(B64_CHARS[((triple >> 12) & 0x3F) as usize] as char);

        if i + 1 < data.len() {
            result.push(B64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if i + 2 < data.len() {
            result.push(B64_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        i += 3;
    }
    result
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let bytes: Vec<u8> = input
        .bytes()
        .filter(|b| *b != b'\n' && *b != b'\r' && *b != b' ')
        .collect();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let a = b64_val(bytes[i])?;
        let b = b64_val(bytes[i + 1])?;
        result.push((a << 2) | (b >> 4));
        if bytes[i + 2] != b'=' {
            let c = b64_val(bytes[i + 2])?;
            result.push(((b & 0xF) << 4) | (c >> 2));
            if bytes[i + 3] != b'=' {
                let d = b64_val(bytes[i + 3])?;
                result.push(((c & 0x3) << 6) | d);
            }
        }
        i += 4;
    }
    Some(result)
}

fn b64_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => Some(0),
    }
}
