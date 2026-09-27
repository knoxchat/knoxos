//! SGR attributes and DEC/ANSI mode setting (including alt screen).
use super::types::{CellAttr, Color};

impl super::VtEmulator {
    /// SGR - Set Graphics Rendition
    pub(crate) fn set_graphics(&mut self, params: &[u32]) {
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
    pub(crate) fn set_mode(&mut self, params: &[u32], set: bool) {
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
    pub(crate) fn enter_alt_screen(&mut self) {
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
    pub(crate) fn leave_alt_screen(&mut self) {
        if let Some(saved) = self.alt_cells.take() {
            self.cells = saved;
            self.cursor_row = self.alt_saved_row;
            self.cursor_col = self.alt_saved_col;
            self.dirty = true;
        }
    }
}
