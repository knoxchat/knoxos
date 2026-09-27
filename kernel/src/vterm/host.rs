//! Host I/O: response queue, mouse encoding, focus events, bracketed paste.
use alloc::vec::Vec;

impl super::VtEmulator {
    // ── Response Queue ──────────────────────────────────────────

    /// Queue response bytes to send back to the host
    pub(crate) fn queue_response(&mut self, data: &[u8]) {
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
}
