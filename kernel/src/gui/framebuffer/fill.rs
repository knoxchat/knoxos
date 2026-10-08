use core::ptr;

use super::{FrameBuffer, Pixel, Rect};

impl FrameBuffer {
    /// Fill a rectangle with a solid color
    pub fn fill_rect(&mut self, rect: Rect, color: Pixel) {
        let (x_start, y_start, x_end, y_end) = match self.clamp_rect(&rect) {
            Some(r) => r,
            None => return,
        };

        let bpp = self.bytes_per_pixel;

        if color.a == 255 {
            // ── Fast path: opaque fill — write row of pixels then memcpy ──
            // Build the first row directly
            let first_row_offset = y_start * self.pitch + x_start * bpp;
            let row_pixel_count = x_end - x_start;
            let row_byte_len = row_pixel_count * bpp;

            // Stamp first row as packed BGRA u32s (unaligned-safe)
            if bpp == 4 {
                let packed = u32::from_le_bytes([color.b, color.g, color.r, color.a]);
                unsafe {
                    let mut p = self.buffer.as_mut_ptr().add(first_row_offset);
                    for _ in 0..row_pixel_count {
                        (p as *mut u32).write_unaligned(packed);
                        p = p.add(4);
                    }
                }
            } else {
                let mut off = first_row_offset;
                for _ in 0..row_pixel_count {
                    self.buffer[off] = color.b;
                    self.buffer[off + 1] = color.g;
                    self.buffer[off + 2] = color.r;
                    off += bpp;
                }
            }

            // Copy first row to remaining rows (much faster than per-pixel)
            for y in (y_start + 1)..y_end {
                let dst_offset = y * self.pitch + x_start * bpp;
                // Safety: both src and dst slices are within buffer bounds
                // and don't overlap (different rows)
                unsafe {
                    let src = self.buffer.as_ptr().add(first_row_offset);
                    let dst = self.buffer.as_mut_ptr().add(dst_offset);
                    ptr::copy_nonoverlapping(src, dst, row_byte_len);
                }
            }
        } else if color.a > 0 {
            // Semi-transparent: must blend per-pixel
            for y in y_start..y_end {
                for x in x_start..x_end {
                    self.blend_pixel(x, y, color);
                }
            }
        }
        // alpha == 0: fully transparent, nothing to draw
    }

    /// Draw a rectangle outline
    pub fn draw_rect(&mut self, rect: Rect, color: Pixel, thickness: u32) {
        let t = thickness as i32;
        // Top
        self.fill_rect(Rect::new(rect.x, rect.y, rect.width, thickness), color);
        // Bottom
        self.fill_rect(
            Rect::new(
                rect.x,
                rect.y + rect.height as i32 - t,
                rect.width,
                thickness,
            ),
            color,
        );
        // Left
        self.fill_rect(Rect::new(rect.x, rect.y, thickness, rect.height), color);
        // Right
        self.fill_rect(
            Rect::new(
                rect.x + rect.width as i32 - t,
                rect.y,
                thickness,
                rect.height,
            ),
            color,
        );
    }

    /// Draw a horizontal line
    pub fn draw_hline(&mut self, x: i32, y: i32, width: u32, color: Pixel) {
        self.fill_rect(Rect::new(x, y, width, 1), color);
    }

    /// Draw a vertical line
    pub fn draw_vline(&mut self, x: i32, y: i32, height: u32, color: Pixel) {
        self.fill_rect(Rect::new(x, y, 1, height), color);
    }
}
