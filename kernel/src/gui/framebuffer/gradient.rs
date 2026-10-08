use core::ptr;

use super::{FrameBuffer, Pixel, Rect};

impl FrameBuffer {
    /// Draw a gradient rectangle (vertical gradient) using integer math
    pub fn fill_gradient_v(&mut self, rect: Rect, top_color: Pixel, bottom_color: Pixel) {
        let (x_start, y_start, x_end, y_end) = match self.clamp_rect(&rect) {
            Some(r) => r,
            None => return,
        };
        let h = rect.height;

        if h == 0 || x_start >= x_end || y_start >= y_end {
            return;
        }

        let bpp = self.bytes_per_pixel;
        let row_pixel_count = x_end - x_start;
        let row_byte_len = row_pixel_count * bpp;

        // Pre-compute the color for each row of the gradient, then stamp rows.
        // For consecutive rows with the same color (common in small gradients),
        // we can memcpy from the previous row instead of re-stamping per-pixel.
        let mut prev_b: u8 = 255;
        let mut prev_g: u8 = 255;
        let mut prev_r: u8 = 255;
        let mut prev_row_offset: usize = 0;
        let mut has_prev = false;

        for y in y_start..y_end {
            let t = (y - y_start) as u32;
            let b = ((top_color.b as u32 * (h - t) + bottom_color.b as u32 * t) / h) as u8;
            let g = ((top_color.g as u32 * (h - t) + bottom_color.g as u32 * t) / h) as u8;
            let r = ((top_color.r as u32 * (h - t) + bottom_color.r as u32 * t) / h) as u8;
            let row_offset = y * self.pitch + x_start * bpp;

            if has_prev && b == prev_b && g == prev_g && r == prev_r {
                // Same color as previous row — fast memcpy
                unsafe {
                    let src = self.buffer.as_ptr().add(prev_row_offset);
                    let dst = self.buffer.as_mut_ptr().add(row_offset);
                    ptr::copy_nonoverlapping(src, dst, row_byte_len);
                }
            } else {
                // Stamp this row pixel-by-pixel
                let mut off = row_offset;
                for _ in 0..row_pixel_count {
                    self.buffer[off] = b;
                    self.buffer[off + 1] = g;
                    self.buffer[off + 2] = r;
                    if bpp >= 4 {
                        self.buffer[off + 3] = 255;
                    }
                    off += bpp;
                }
                prev_b = b;
                prev_g = g;
                prev_r = r;
            }
            prev_row_offset = row_offset;
            has_prev = true;
        }
    }

    /// Draw a horizontal gradient rectangle
    pub fn fill_gradient_h(&mut self, rect: Rect, left_color: Pixel, right_color: Pixel) {
        let (x_start, y_start, x_end, y_end) = match self.clamp_rect(&rect) {
            Some(r) => r,
            None => return,
        };
        let w = rect.width;

        if w == 0 || x_start >= x_end || y_start >= y_end {
            return;
        }

        let bpp = self.bytes_per_pixel;
        let row_byte_len = (x_end - x_start) * bpp;

        // Horizontal gradient: every row is identical, so stamp first row then memcpy.
        let first_row_offset = y_start * self.pitch + x_start * bpp;
        {
            let mut off = first_row_offset;
            for x in x_start..x_end {
                let t = (x - x_start) as u32;
                let b = ((left_color.b as u32 * (w - t) + right_color.b as u32 * t) / w) as u8;
                let g = ((left_color.g as u32 * (w - t) + right_color.g as u32 * t) / w) as u8;
                let r = ((left_color.r as u32 * (w - t) + right_color.r as u32 * t) / w) as u8;
                self.buffer[off] = b;
                self.buffer[off + 1] = g;
                self.buffer[off + 2] = r;
                if bpp >= 4 {
                    self.buffer[off + 3] = 255;
                }
                off += bpp;
            }
        }

        // Copy first row to all remaining rows
        for y in (y_start + 1)..y_end {
            let dst_offset = y * self.pitch + x_start * bpp;
            unsafe {
                let src = self.buffer.as_ptr().add(first_row_offset);
                let dst = self.buffer.as_mut_ptr().add(dst_offset);
                ptr::copy_nonoverlapping(src, dst, row_byte_len);
            }
        }
    }
}
