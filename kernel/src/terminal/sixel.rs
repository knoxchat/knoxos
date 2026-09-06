use alloc::vec;
/// Sixel Graphics — Sixel image protocol for terminal
///
/// Provides in-terminal image rendering via the Sixel protocol:
///   - DCS (Device Control String) parser for Sixel data
///   - Sixel → pixel conversion (6 vertical pixels per character row)
///   - Color register management (HLS and RGB)
///   - Image scaling to terminal cell grid
use alloc::vec::Vec;

use crate::gui::framebuffer::Pixel;
use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// SIXEL PARSER
// ═══════════════════════════════════════════════════════════════════════

/// Maximum color registers
const MAX_COLORS: usize = 256;

/// Sixel image being decoded
pub struct SixelImage {
    /// Pixel data (RGBA)
    pub pixels: Vec<Pixel>,
    pub width: u32,
    pub height: u32,
    /// Color palette
    pub palette: [Pixel; MAX_COLORS],
    /// Current color register
    current_color: usize,
    /// Current X position
    cursor_x: u32,
    /// Current Y position (in sixel rows = 6 pixels)
    cursor_y: u32,
    /// Maximum X reached
    max_x: u32,
    /// Maximum Y reached (in pixel rows)
    max_y: u32,
}

impl SixelImage {
    pub fn new() -> Self {
        let mut palette = [Pixel::rgb(0, 0, 0); MAX_COLORS];
        // Initialize with default VGA colors
        palette[0] = Pixel::rgb(0, 0, 0);
        palette[1] = Pixel::rgb(51, 102, 204); // blue
        palette[2] = Pixel::rgb(204, 33, 33); // red
        palette[3] = Pixel::rgb(51, 204, 51); // green
        palette[4] = Pixel::rgb(204, 51, 204); // magenta
        palette[5] = Pixel::rgb(51, 204, 204); // cyan
        palette[6] = Pixel::rgb(204, 204, 51); // yellow
        palette[7] = Pixel::rgb(119, 119, 119); // gray
        palette[8] = Pixel::rgb(204, 204, 204); // white

        Self {
            pixels: Vec::new(),
            width: 0,
            height: 0,
            palette,
            current_color: 0,
            cursor_x: 0,
            cursor_y: 0,
            max_x: 0,
            max_y: 0,
        }
    }

    /// Parse a complete Sixel data stream
    pub fn parse(&mut self, data: &[u8]) {
        // Pre-allocate a reasonable buffer
        let estimated_width = 800;
        let estimated_height = 600;
        self.pixels = vec![Pixel::new(0, 0, 0, 0); estimated_width * estimated_height];
        self.width = estimated_width as u32;
        self.height = estimated_height as u32;

        let mut i = 0;
        while i < data.len() {
            match data[i] {
                // Sixel data character (? through ~, = 63..126)
                b'?'..=b'~' => {
                    let sixel_value = data[i] - b'?';
                    self.draw_sixel(sixel_value);
                    self.cursor_x += 1;
                    i += 1;
                }

                // $ = Carriage return (go to start of current sixel row)
                b'$' => {
                    self.cursor_x = 0;
                    i += 1;
                }

                // - = New line (go to start of next sixel row)
                b'-' => {
                    self.cursor_x = 0;
                    self.cursor_y += 1;
                    i += 1;
                }

                // # = Select color register or define color
                b'#' => {
                    i += 1;
                    let (reg, next) = parse_number(&data[i..]);
                    i += next;

                    // Check if this is a color definition
                    if i < data.len() && data[i] == b';' {
                        i += 1;
                        let (color_space, n1) = parse_number(&data[i..]);
                        i += n1;
                        if i < data.len() && data[i] == b';' {
                            i += 1;
                        }
                        let (p1, n2) = parse_number(&data[i..]);
                        i += n2;
                        if i < data.len() && data[i] == b';' {
                            i += 1;
                        }
                        let (p2, n3) = parse_number(&data[i..]);
                        i += n3;
                        if i < data.len() && data[i] == b';' {
                            i += 1;
                        }
                        let (p3, n4) = parse_number(&data[i..]);
                        i += n4;

                        if (reg as usize) < MAX_COLORS {
                            self.palette[reg as usize] = match color_space {
                                1 => hls_to_rgb(p1, p2, p3),
                                2 => Pixel::rgb(
                                    ((p1 * 255) / 100) as u8,
                                    ((p2 * 255) / 100) as u8,
                                    ((p3 * 255) / 100) as u8,
                                ),
                                _ => Pixel::rgb(0, 0, 0),
                            };
                        }
                    }

                    if (reg as usize) < MAX_COLORS {
                        self.current_color = reg as usize;
                    }
                }

                // ! = Repeat introducer
                b'!' => {
                    i += 1;
                    let (count, next) = parse_number(&data[i..]);
                    i += next;
                    if i < data.len() && data[i] >= b'?' && data[i] <= b'~' {
                        let sixel_value = data[i] - b'?';
                        for _ in 0..count {
                            self.draw_sixel(sixel_value);
                            self.cursor_x += 1;
                        }
                        i += 1;
                    }
                }

                // " = Raster attributes
                b'"' => {
                    i += 1;
                    let (_pan, n1) = parse_number(&data[i..]);
                    i += n1;
                    if i < data.len() && data[i] == b';' {
                        i += 1;
                    }
                    let (_pad, n2) = parse_number(&data[i..]);
                    i += n2;
                    if i < data.len() && data[i] == b';' {
                        i += 1;
                    }
                    let (ph, n3) = parse_number(&data[i..]);
                    i += n3;
                    if i < data.len() && data[i] == b';' {
                        i += 1;
                    }
                    let (pv, n4) = parse_number(&data[i..]);
                    i += n4;

                    // Set image dimensions
                    if ph > 0 && pv > 0 {
                        self.width = ph;
                        self.height = pv;
                        self.pixels = vec![Pixel::new(0, 0, 0, 0); (ph * pv) as usize];
                    }
                }

                _ => {
                    i += 1;
                }
            }
        }

        // Finalize dimensions
        if self.max_x > 0 {
            self.width = self.max_x + 1;
        }
        if self.max_y > 0 {
            self.height = self.max_y + 1;
        }
    }

    /// Draw one sixel column (6 pixels vertically)
    fn draw_sixel(&mut self, value: u8) {
        let color = self.palette[self.current_color];
        let px = self.cursor_x;
        let base_py = self.cursor_y * 6;

        for bit in 0..6u32 {
            if value & (1 << bit) != 0 {
                let py = base_py + bit;
                self.set_pixel(px, py, color);

                if px > self.max_x {
                    self.max_x = px;
                }
                if py > self.max_y {
                    self.max_y = py;
                }
            }
        }
    }

    /// Set a pixel in the image buffer
    fn set_pixel(&mut self, x: u32, y: u32, color: Pixel) {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) as usize;
            if idx < self.pixels.len() {
                self.pixels[idx] = color;
            }
        }
    }

    /// Get a pixel from the image buffer
    pub fn get_pixel(&self, x: u32, y: u32) -> Pixel {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) as usize;
            if idx < self.pixels.len() {
                return self.pixels[idx];
            }
        }
        Pixel::new(0, 0, 0, 0)
    }

    /// Scale the image to fit within given dimensions
    pub fn scale_to_fit(&self, max_w: u32, max_h: u32) -> SixelImage {
        if self.width == 0 || self.height == 0 {
            return SixelImage::new();
        }

        let scale_x = max_w as f32 / self.width as f32;
        let scale_y = max_h as f32 / self.height as f32;
        let scale = if scale_x < scale_y { scale_x } else { scale_y };

        if scale >= 1.0 {
            // No need to scale down, just clone
            let mut img = SixelImage::new();
            img.width = self.width;
            img.height = self.height;
            img.pixels = self.pixels.clone();
            return img;
        }

        let new_w = (self.width as f32 * scale) as u32;
        let new_h = (self.height as f32 * scale) as u32;

        let mut img = SixelImage::new();
        img.width = new_w;
        img.height = new_h;
        img.pixels = vec![Pixel::new(0, 0, 0, 0); (new_w * new_h) as usize];

        for y in 0..new_h {
            for x in 0..new_w {
                let src_x = (x as f32 / scale) as u32;
                let src_y = (y as f32 / scale) as u32;
                let pixel = self.get_pixel(src_x.min(self.width - 1), src_y.min(self.height - 1));
                let idx = (y * new_w + x) as usize;
                if idx < img.pixels.len() {
                    img.pixels[idx] = pixel;
                }
            }
        }

        img
    }
}

// ═══════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════

/// Parse a decimal number from bytes, returns (value, bytes_consumed)
fn parse_number(data: &[u8]) -> (u32, usize) {
    let mut value = 0u32;
    let mut consumed = 0;
    for &b in data {
        if b.is_ascii_digit() {
            value = value * 10 + (b - b'0') as u32;
            consumed += 1;
        } else {
            break;
        }
    }
    (value, consumed)
}

/// Convert HLS to RGB (H=0-360, L=0-100, S=0-100)
fn hls_to_rgb(h: u32, l: u32, s: u32) -> Pixel {
    let l_f = l as f32 / 100.0;
    let s_f = s as f32 / 100.0;

    if s == 0 {
        let v = (l_f * 255.0) as u8;
        return Pixel::rgb(v, v, v);
    }

    let h_f = h as f32 / 360.0;
    let q = if l_f < 0.5 {
        l_f * (1.0 + s_f)
    } else {
        l_f + s_f - l_f * s_f
    };
    let p = 2.0 * l_f - q;

    let r = hue_to_rgb(p, q, h_f + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h_f);
    let b = hue_to_rgb(p, q, h_f - 1.0 / 3.0);

    Pixel::rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

/// Initialize Sixel support
pub fn init() {
    serial_println!("[KnoxOS] Terminal Sixel graphics initialized");
}
