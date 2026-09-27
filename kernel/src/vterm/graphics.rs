//! DCS, Sixel (P8.2), and Kitty graphics (P8.3) protocol handling.
use alloc::vec::Vec;

use super::base64::base64_decode;
use super::types::{KittyImage, ParserState, SixelImage};

impl super::VtEmulator {
    // ── P8.4: DCS String Processing ─────────────────────────────

    /// Process DCS (Device Control String) bytes
    pub(crate) fn process_dcs(&mut self, byte: u8) {
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
    pub(crate) fn execute_dcs(&mut self) {
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
    pub(crate) fn process_sixel(&mut self, byte: u8) {
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
    pub(crate) fn parse_sixel(&mut self, data: &[u8]) {
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
}
