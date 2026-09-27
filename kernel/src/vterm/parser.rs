//! Escape-sequence state machine: normal, ESC, and CSI.
use alloc::string::String;
use alloc::vec::Vec;

use super::types::{Cell, ParserState};

impl super::VtEmulator {
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
    pub(crate) fn process_normal(&mut self, byte: u8) {
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
    pub(crate) fn process_escape(&mut self, byte: u8) {
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
    pub(crate) fn process_csi(&mut self, byte: u8) {
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
    pub(crate) fn execute_csi(&mut self, final_byte: u8) {
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
    pub(crate) fn parse_params(&self) -> Vec<u32> {
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
    pub(crate) fn is_dec_private(&self) -> bool {
        self.esc_buf.first() == Some(&b'?')
    }
}
