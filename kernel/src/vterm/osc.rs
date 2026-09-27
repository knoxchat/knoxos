//! OSC (Operating System Command) sequences: title, hyperlink, clipboard.
use alloc::string::String;

use super::base64::{base64_decode, base64_encode};
use super::types::ParserState;

impl super::VtEmulator {
    /// Process OSC (Operating System Command) start
    pub(crate) fn process_osc(&mut self, byte: u8) {
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

    pub(crate) fn process_osc_string(&mut self, byte: u8) {
        if byte == b'\\' {
            self.execute_osc();
        }
        self.state = ParserState::Normal;
    }

    pub(crate) fn execute_osc(&mut self) {
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
}
