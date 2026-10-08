use super::{FrameBuffer, Pixel};

impl FrameBuffer {
    /// Set a single pixel
    #[inline(always)]
    pub fn set_pixel(&mut self, x: usize, y: usize, color: Pixel) {
        if x >= self.width || y >= self.height || self.is_clipped(x, y) {
            return;
        }
        let offset = y * self.pitch + x * self.bytes_per_pixel;
        if offset + 3 >= self.buffer.len() {
            return;
        }
        // BGRA format (common for VGA/VESA/GOP)
        self.buffer[offset] = color.b;
        self.buffer[offset + 1] = color.g;
        self.buffer[offset + 2] = color.r;
        if self.bytes_per_pixel >= 4 {
            self.buffer[offset + 3] = color.a;
        }
    }

    /// Get a pixel
    pub fn get_pixel(&self, x: usize, y: usize) -> Pixel {
        if x >= self.width || y >= self.height {
            return Pixel::rgb(0, 0, 0);
        }
        let offset = y * self.pitch + x * self.bytes_per_pixel;
        if offset + 3 >= self.buffer.len() {
            return Pixel::rgb(0, 0, 0);
        }
        Pixel {
            b: self.buffer[offset],
            g: self.buffer[offset + 1],
            r: self.buffer[offset + 2],
            a: if self.bytes_per_pixel >= 4 {
                self.buffer[offset + 3]
            } else {
                255
            },
        }
    }

    /// Set pixel with alpha blending
    /// Uses fast (a*b + 128) >> 8 approximation for /255 (max error: 1 LSB)
    #[inline(always)]
    pub fn blend_pixel(&mut self, x: usize, y: usize, color: Pixel) {
        if color.a == 255 {
            self.set_pixel(x, y, color);
        } else if color.a > 0 {
            if x >= self.width || y >= self.height || self.is_clipped(x, y) {
                return;
            }
            let offset = y * self.pitch + x * self.bytes_per_pixel;
            if offset + 3 >= self.buffer.len() {
                return;
            }
            // Inline alpha blending with fast >>8 approximation
            let alpha = color.a as u16;
            let inv_alpha = 255 - alpha;
            let bg_b = self.buffer[offset] as u16;
            let bg_g = self.buffer[offset + 1] as u16;
            let bg_r = self.buffer[offset + 2] as u16;
            self.buffer[offset] = ((color.b as u16 * alpha + bg_b * inv_alpha + 128) >> 8) as u8;
            self.buffer[offset + 1] =
                ((color.g as u16 * alpha + bg_g * inv_alpha + 128) >> 8) as u8;
            self.buffer[offset + 2] =
                ((color.r as u16 * alpha + bg_r * inv_alpha + 128) >> 8) as u8;
            if self.bytes_per_pixel >= 4 {
                self.buffer[offset + 3] = 255;
            }
        }
    }

    /// Fill the entire screen with a color
    pub fn clear(&mut self, color: Pixel) {
        // Build a single pixel in BGRA byte order
        let pixel_bytes = [color.b, color.g, color.r, color.a];
        // Fill entire buffer by stamping 4-byte pixel across all scanlines
        let bpp = self.bytes_per_pixel;
        for row_start in (0..self.buffer.len()).step_by(self.pitch) {
            let row_end = (row_start + self.width * bpp).min(self.buffer.len());
            let mut off = row_start;
            while off + bpp <= row_end {
                self.buffer[off] = pixel_bytes[0];
                self.buffer[off + 1] = pixel_bytes[1];
                self.buffer[off + 2] = pixel_bytes[2];
                if bpp >= 4 {
                    self.buffer[off + 3] = pixel_bytes[3];
                }
                off += bpp;
            }
        }
    }
}
