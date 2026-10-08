use core::ptr;

use super::FrameBuffer;

impl FrameBuffer {
    /// Present only a rectangular region of the back buffer to the physical framebuffer.
    /// Much faster than full present() when only a small area changed (e.g. cursor).
    pub fn present_rect(&mut self, x: i32, y: i32, w: u32, h: u32) {
        if !self.use_hw_framebuffer || self.framebuffer_addr == 0 {
            return;
        }
        // Clamp rectangle to screen bounds (handle negative coords safely)
        let x0 = x.max(0) as usize;
        let y0 = y.max(0) as usize;
        let x1 = ((x + w as i32).max(0) as usize).min(self.width);
        let y1 = ((y + h as i32).max(0) as usize).min(self.height);
        if x0 >= x1 || y0 >= y1 || x0 >= self.width || y0 >= self.height {
            return;
        }

        let hw_bpp = self.hw_bytes_per_pixel;
        let hw_stride_bytes = self.hw_stride * hw_bpp;
        let internal_bpp = self.bytes_per_pixel;

        unsafe {
            let fb_ptr = self.framebuffer_addr as *mut u8;

            if hw_bpp == internal_bpp && self.hw_stride == self.width {
                // Same format, same stride — copy row spans
                for row in y0..y1 {
                    let src_off = row * self.pitch + x0 * internal_bpp;
                    let dst_off = row * hw_stride_bytes + x0 * hw_bpp;
                    let row_len = (x1 - x0) * internal_bpp;
                    if dst_off + row_len <= self.framebuffer_len {
                        ptr::copy_nonoverlapping(
                            self.buffer.as_ptr().add(src_off),
                            fb_ptr.add(dst_off),
                            row_len,
                        );
                    }
                }
            } else {
                // Conversion path (different bpp/stride)
                for y in y0..y1 {
                    let src_row = y * self.pitch;
                    let dst_row = y * hw_stride_bytes;
                    if dst_row + x1 * hw_bpp > self.framebuffer_len {
                        break;
                    }
                    let src_base = self.buffer.as_ptr().add(src_row);
                    let dst_base = fb_ptr.add(dst_row);

                    if hw_bpp == 3 {
                        // Fast 4bpp→3bpp conversion
                        let mut si = x0 * internal_bpp;
                        let mut di = x0 * hw_bpp;
                        for _ in x0..x1 {
                            ptr::write(dst_base.add(di), *src_base.add(si));
                            ptr::write(dst_base.add(di + 1), *src_base.add(si + 1));
                            ptr::write(dst_base.add(di + 2), *src_base.add(si + 2));
                            si += 4;
                            di += 3;
                        }
                    } else {
                        // Same bpp, different stride
                        let row_len = (x1 - x0) * internal_bpp;
                        ptr::copy_nonoverlapping(
                            src_base.add(x0 * internal_bpp),
                            dst_base.add(x0 * hw_bpp),
                            row_len,
                        );
                    }
                }
            }
        }
    }

    /// Flush back buffer to physical framebuffer (present)
    /// Handles conversion from internal 4-BPP BGRA to HW format (e.g. 3-BPP BGR)
    pub fn present(&mut self) {
        if !self.use_hw_framebuffer || self.framebuffer_addr == 0 {
            return;
        }

        let hw_bpp = self.hw_bytes_per_pixel;
        let hw_stride_bytes = self.hw_stride * hw_bpp;
        let internal_bpp = self.bytes_per_pixel; // always 4

        unsafe {
            let fb_ptr = self.framebuffer_addr as *mut u8;

            if hw_bpp == internal_bpp && self.hw_stride == self.width {
                // Fast path: same format, same stride - direct memcpy
                let copy_len = self.buffer.len().min(self.framebuffer_len);
                ptr::copy_nonoverlapping(self.buffer.as_ptr(), fb_ptr, copy_len);
            } else {
                // Conversion path: different BPP or stride
                // Process in row chunks for better cache locality
                let src_buf = self.buffer.as_ptr();
                for y in 0..self.height {
                    let src_row = y * self.pitch;
                    let dst_row = y * hw_stride_bytes;

                    if dst_row + self.width * hw_bpp > self.framebuffer_len {
                        break;
                    }

                    let src_base = src_buf.add(src_row);
                    let dst_base = fb_ptr.add(dst_row);

                    if hw_bpp == 3 {
                        // Fast path for 3bpp BGR: tight inner loop
                        let mut si = 0usize;
                        let mut di = 0usize;
                        for _ in 0..self.width {
                            ptr::write(dst_base.add(di), *src_base.add(si)); // B
                            ptr::write(dst_base.add(di + 1), *src_base.add(si + 1)); // G
                            ptr::write(dst_base.add(di + 2), *src_base.add(si + 2)); // R
                            si += 4;
                            di += 3;
                        }
                    } else {
                        // 4bpp: row copy (same pixel format)
                        ptr::copy_nonoverlapping(src_base, dst_base, self.width * 4);
                    }
                }
            }
        }
    }
}
