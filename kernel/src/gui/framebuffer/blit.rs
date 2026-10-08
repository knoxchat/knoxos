use super::{FrameBuffer, Pixel};

impl FrameBuffer {
    /// Blit a pre-rendered BGRA icon onto the framebuffer with alpha blending.
    /// `data` is a &[u8] of length `width * height * 4` in BGRA byte order.
    pub fn blit_bgra(&mut self, x: i32, y: i32, width: u32, height: u32, data: &[u8]) {
        let w = width as i32;
        let h = height as i32;
        let stride = width as usize * 4;
        for row in 0..h {
            let py = y + row;
            if py < 0 || py >= self.height as i32 {
                continue;
            }
            let src_row = row as usize * stride;
            for col in 0..w {
                let px = x + col;
                if px < 0 || px >= self.width as i32 {
                    continue;
                }
                let src_off = src_row + col as usize * 4;
                let b = data[src_off];
                let g = data[src_off + 1];
                let r = data[src_off + 2];
                let a = data[src_off + 3];
                if a == 0 {
                    continue;
                }
                if a == 255 {
                    self.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
                } else {
                    self.blend_pixel(px as usize, py as usize, Pixel::new(r, g, b, a));
                }
            }
        }
    }

    /// Blit a BGRA image scaled to a target size using bilinear interpolation.
    /// Produces smooth, high-quality icon scaling without jagged edges.
    /// `src_data`: source BGRA pixel data, `src_w`/`src_h`: source dimensions,
    /// `dst_x`/`dst_y`: destination position, `dst_w`/`dst_h`: destination size.
    pub fn blit_bgra_scaled(
        &mut self,
        dst_x: i32,
        dst_y: i32,
        dst_w: u32,
        dst_h: u32,
        src_data: &[u8],
        src_w: u32,
        src_h: u32,
    ) {
        if dst_w == 0 || dst_h == 0 || src_w == 0 || src_h == 0 {
            return;
        }
        // 1:1 fast path
        if dst_w == src_w && dst_h == src_h {
            self.blit_bgra(dst_x, dst_y, src_w, src_h, src_data);
            return;
        }

        let src_stride = src_w as usize * 4;
        // Fixed-point scale factors (16.16)
        let x_ratio = ((src_w as u64) << 16) / dst_w as u64;
        let y_ratio = ((src_h as u64) << 16) / dst_h as u64;

        for dy in 0..dst_h as i32 {
            let py = dst_y + dy;
            if py < 0 || py >= self.height as i32 {
                continue;
            }
            // Source Y in 16.16 fixed point
            let src_y_fp = (dy as u64 * y_ratio) as u32;
            let src_y = (src_y_fp >> 16) as usize;
            let y_frac = (src_y_fp & 0xFFFF) >> 8; // 0-255
            let y_inv = 255 - y_frac;
            let src_y1 = (src_y + 1).min(src_h as usize - 1);

            for dx in 0..dst_w as i32 {
                let px = dst_x + dx;
                if px < 0 || px >= self.width as i32 {
                    continue;
                }
                // Source X in 16.16 fixed point
                let src_x_fp = (dx as u64 * x_ratio) as u32;
                let src_x = (src_x_fp >> 16) as usize;
                let x_frac = (src_x_fp & 0xFFFF) >> 8; // 0-255
                let x_inv = 255 - x_frac;
                let src_x1 = (src_x + 1).min(src_w as usize - 1);

                // Sample 4 source pixels for bilinear interpolation
                let off00 = src_y * src_stride + src_x * 4;
                let off10 = src_y * src_stride + src_x1 * 4;
                let off01 = src_y1 * src_stride + src_x * 4;
                let off11 = src_y1 * src_stride + src_x1 * 4;

                // Bilinear blend each channel
                let blend = |c00: u8, c10: u8, c01: u8, c11: u8| -> u8 {
                    let top = (c00 as u32 * x_inv + c10 as u32 * x_frac + 128) >> 8;
                    let bot = (c01 as u32 * x_inv + c11 as u32 * x_frac + 128) >> 8;
                    ((top * y_inv + bot * y_frac + 128) >> 8) as u8
                };

                let b = blend(
                    src_data[off00],
                    src_data[off10],
                    src_data[off01],
                    src_data[off11],
                );
                let g = blend(
                    src_data[off00 + 1],
                    src_data[off10 + 1],
                    src_data[off01 + 1],
                    src_data[off11 + 1],
                );
                let r = blend(
                    src_data[off00 + 2],
                    src_data[off10 + 2],
                    src_data[off01 + 2],
                    src_data[off11 + 2],
                );
                let a = blend(
                    src_data[off00 + 3],
                    src_data[off10 + 3],
                    src_data[off01 + 3],
                    src_data[off11 + 3],
                );

                if a == 0 {
                    continue;
                }
                if a == 255 {
                    self.set_pixel(px as usize, py as usize, Pixel::rgb(r, g, b));
                } else {
                    self.blend_pixel(px as usize, py as usize, Pixel::new(r, g, b, a));
                }
            }
        }
    }

    /// Apply a lightweight unsharp mask to a rectangular region of the framebuffer.
    /// This restores edge crispness lost during bilinear downscaling.
    /// `strength` is 0-255 where 128 = subtle, 255 = maximum sharpening.
    pub fn sharpen_region(&mut self, x: i32, y: i32, w: u32, h: u32, strength: u8) {
        let x0 = (x.max(1) as usize).min(self.width - 1);
        let y0 = (y.max(1) as usize).min(self.height - 1);
        let x1 = ((x + w as i32) as usize).min(self.width - 1);
        let y1 = ((y + h as i32) as usize).min(self.height - 1);

        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let bpp = self.bytes_per_pixel;
        let pitch = self.pitch;
        let s = strength as i32;

        // Simple 3×3 unsharp mask: sharpen = original + strength * (original - blur)
        // We use a cross kernel for blur: (center*4 - N - S - E - W) / 4
        // To avoid allocating a temp buffer, process in-place with read-before-write.
        // This creates slight directional bias but is acceptable for icon sharpening.
        for y in y0..y1 {
            for x in x0..x1 {
                let center = y * pitch + x * bpp;
                let north = (y - 1) * pitch + x * bpp;
                let south = (y + 1) * pitch + x * bpp;
                let west = y * pitch + (x - 1) * bpp;
                let east = y * pitch + (x + 1) * bpp;

                // For each color channel (B, G, R)
                for ch in 0..3usize {
                    let c = self.buffer[center + ch] as i32;
                    let n = self.buffer[north + ch] as i32;
                    let so = self.buffer[south + ch] as i32;
                    let w = self.buffer[west + ch] as i32;
                    let e = self.buffer[east + ch] as i32;

                    // High-pass = center - average_of_neighbors
                    let highpass = c * 4 - n - so - w - e; // Range: -1020..1020
                    // Add scaled high-pass back: result = center + (highpass * strength) / (4 * 256)
                    let sharpened = c + (highpass * s) / 1024;
                    self.buffer[center + ch] = sharpened.clamp(0, 255) as u8;
                }
            }
        }
    }
}
