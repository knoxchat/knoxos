/// Framebuffer - Direct pixel manipulation for the display
/// Provides a software framebuffer with double-buffering for tear-free rendering
use alloc::vec;
use alloc::vec::Vec;
use core::ptr;

/// RGBA color
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Pixel {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn from_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as u8,
            g: ((hex >> 8) & 0xFF) as u8,
            b: (hex & 0xFF) as u8,
            a: 255,
        }
    }

    pub const fn from_hex_alpha(hex: u32) -> Self {
        Self {
            r: ((hex >> 24) & 0xFF) as u8,
            g: ((hex >> 16) & 0xFF) as u8,
            b: ((hex >> 8) & 0xFF) as u8,
            a: (hex & 0xFF) as u8,
        }
    }

    /// Blend this pixel over another (alpha compositing)
    /// Uses fast (a*b + 128) >> 8 approximation for /255 (max error: 1 LSB)
    pub fn blend_over(self, below: Pixel) -> Pixel {
        if self.a == 255 {
            return self;
        }
        if self.a == 0 {
            return below;
        }
        let alpha = self.a as u16;
        let inv_alpha = 255 - alpha;
        Pixel {
            r: ((self.r as u16 * alpha + below.r as u16 * inv_alpha + 128) >> 8) as u8,
            g: ((self.g as u16 * alpha + below.g as u16 * inv_alpha + 128) >> 8) as u8,
            b: ((self.b as u16 * alpha + below.b as u16 * inv_alpha + 128) >> 8) as u8,
            a: 255,
        }
    }

    /// Linearly interpolate between two colors
    /// Uses fast (a*b + 128) >> 8 approximation for /255
    pub fn lerp(a: Pixel, b: Pixel, t: u8) -> Pixel {
        let t16 = t as u16;
        let inv_t = 255 - t16;
        Pixel {
            r: ((a.r as u16 * inv_t + b.r as u16 * t16 + 128) >> 8) as u8,
            g: ((a.g as u16 * inv_t + b.g as u16 * t16 + 128) >> 8) as u8,
            b: ((a.b as u16 * inv_t + b.b as u16 * t16 + 128) >> 8) as u8,
            a: 255,
        }
    }

    /// Multiply alpha of this pixel by a factor (0-255)
    pub fn with_alpha(self, alpha: u8) -> Pixel {
        Pixel::new(
            self.r,
            self.g,
            self.b,
            ((self.a as u16 * alpha as u16 + 128) >> 8) as u8,
        )
    }
}

/// Rect structure for drawing operations
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && px < self.x + self.width as i32
            && py >= self.y
            && py < self.y + self.height as i32
    }

    pub fn right(&self) -> i32 {
        self.x + self.width as i32
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }

    /// Compute the intersection of two rectangles. Returns None if they don't overlap.
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self.right().min(other.right());
        let y2 = self.bottom().min(other.bottom());
        if x2 > x1 && y2 > y1 {
            Some(Rect::new(x1, y1, (x2 - x1) as u32, (y2 - y1) as u32))
        } else {
            None
        }
    }

    /// Check if this rect overlaps with another
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    /// Inset (shrink) the rect by a given amount on all sides
    pub fn inset(&self, amount: i32) -> Rect {
        Rect::new(
            self.x + amount,
            self.y + amount,
            (self.width as i32 - amount * 2).max(0) as u32,
            (self.height as i32 - amount * 2).max(0) as u32,
        )
    }
}

/// Software framebuffer with double-buffering
pub struct FrameBuffer {
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub bytes_per_pixel: usize,
    /// Back buffer (we draw here, then present to HW framebuffer)
    pub buffer: Vec<u8>,
    /// Physical/virtual framebuffer address from bootloader
    pub framebuffer_addr: usize,
    /// Length of the HW framebuffer mapping
    pub framebuffer_len: usize,
    /// HW framebuffer bytes per pixel (may differ from back buffer)
    pub hw_bytes_per_pixel: usize,
    /// HW framebuffer stride in pixels
    pub hw_stride: usize,
    /// Whether we have a real HW framebuffer to present to
    pub use_hw_framebuffer: bool,
    /// Clip rectangle stack — drawing is restricted to the topmost clip rect.
    /// When empty, the entire framebuffer is the clip region.
    clip_stack: Vec<Rect>,
    /// Accumulated dirty region (union of all modified areas since last reset).
    /// Used for damage-tracking to present only changed pixels.
    dirty: Option<Rect>,
}

impl FrameBuffer {
    pub fn new(width: usize, height: usize) -> Self {
        let bpp = 4; // 32bpp BGRA internal format for easy alpha blending
        let pitch = width * bpp;
        let buffer_size = pitch * height;
        Self {
            width,
            height,
            pitch,
            bytes_per_pixel: bpp,
            buffer: vec![0u8; buffer_size],
            framebuffer_addr: 0,
            framebuffer_len: 0,
            hw_bytes_per_pixel: 0,
            hw_stride: 0,
            use_hw_framebuffer: false,
            clip_stack: Vec::new(),
            dirty: None,
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // CLIP RECTANGLE STACK
    // ═══════════════════════════════════════════════════════════════════

    /// Push a clip rectangle. All subsequent drawing is restricted to the
    /// intersection of this rect and the current clip region.
    pub fn push_clip(&mut self, rect: Rect) {
        let effective = if let Some(current) = self.clip_rect() {
            // Intersect with current clip
            if let Some(intersection) = current.intersection(&rect) {
                intersection
            } else {
                // No overlap — push a zero-area rect (nothing will draw)
                Rect::new(0, 0, 0, 0)
            }
        } else {
            // No current clip — intersect with framebuffer bounds
            let fb_rect = Rect::new(0, 0, self.width as u32, self.height as u32);
            if let Some(intersection) = fb_rect.intersection(&rect) {
                intersection
            } else {
                Rect::new(0, 0, 0, 0)
            }
        };
        self.clip_stack.push(effective);
    }

    /// Pop the top clip rectangle, restoring the previous clip region.
    pub fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    /// Get the current effective clip rectangle, or None if no clip is active.
    #[inline(always)]
    pub fn clip_rect(&self) -> Option<Rect> {
        self.clip_stack.last().copied()
    }

    /// Clamp a rect to the current clip region (or screen bounds if no clip).
    /// Returns (x_start, y_start, x_end, y_end) as usize, or None if fully clipped.
    #[inline(always)]
    fn clamp_rect(&self, rect: &Rect) -> Option<(usize, usize, usize, usize)> {
        let (cx0, cy0, cx1, cy1) = if let Some(clip) = self.clip_rect() {
            (
                clip.x.max(0) as usize,
                clip.y.max(0) as usize,
                (clip.x + clip.width as i32).min(self.width as i32).max(0) as usize,
                (clip.y + clip.height as i32).min(self.height as i32).max(0) as usize,
            )
        } else {
            (0, 0, self.width, self.height)
        };

        let x_start = (rect.x.max(0) as usize).max(cx0);
        let y_start = (rect.y.max(0) as usize).max(cy0);
        let x_end = ((rect.x + rect.width as i32) as usize).min(cx1);
        let y_end = ((rect.y + rect.height as i32) as usize).min(cy1);

        if x_start >= x_end || y_start >= y_end {
            None
        } else {
            Some((x_start, y_start, x_end, y_end))
        }
    }

    /// Check if a pixel coordinate is within the current clip region.
    #[inline(always)]
    fn is_clipped(&self, x: usize, y: usize) -> bool {
        if let Some(clip) = self.clip_rect() {
            let cx0 = clip.x.max(0) as usize;
            let cy0 = clip.y.max(0) as usize;
            let cx1 = (clip.x + clip.width as i32).max(0) as usize;
            let cy1 = (clip.y + clip.height as i32).max(0) as usize;
            x < cx0 || x >= cx1 || y < cy0 || y >= cy1
        } else {
            false
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // DAMAGE TRACKING (DIRTY REGION)
    // ═══════════════════════════════════════════════════════════════════

    /// Mark a rectangular area as dirty (modified). Expands the current dirty
    /// region to include this rect.
    pub fn mark_dirty(&mut self, rect: Rect) {
        // Clamp to screen bounds
        let x0 = rect.x.max(0).min(self.width as i32);
        let y0 = rect.y.max(0).min(self.height as i32);
        let x1 = (rect.x + rect.width as i32).max(0).min(self.width as i32);
        let y1 = (rect.y + rect.height as i32).max(0).min(self.height as i32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let clamped = Rect::new(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32);
        self.dirty = Some(if let Some(existing) = self.dirty {
            // Union of existing and new
            let ux0 = existing.x.min(clamped.x);
            let uy0 = existing.y.min(clamped.y);
            let ux1 = (existing.x + existing.width as i32).max(clamped.x + clamped.width as i32);
            let uy1 = (existing.y + existing.height as i32).max(clamped.y + clamped.height as i32);
            Rect::new(ux0, uy0, (ux1 - ux0) as u32, (uy1 - uy0) as u32)
        } else {
            clamped
        });
    }

    /// Mark the entire screen as dirty.
    pub fn mark_all_dirty(&mut self) {
        self.dirty = Some(Rect::new(0, 0, self.width as u32, self.height as u32));
    }

    /// Get the current dirty region and reset it.
    pub fn take_dirty(&mut self) -> Option<Rect> {
        self.dirty.take()
    }

    /// Get the current dirty region without consuming it.
    pub fn dirty_region(&self) -> Option<Rect> {
        self.dirty
    }

    /// Present only the dirty region to the HW framebuffer, then clear it.
    /// Falls back to full present if no dirty region is tracked.
    pub fn present_dirty(&mut self) {
        if let Some(d) = self.dirty.take() {
            self.present_rect(d.x, d.y, d.width, d.height);
        } else {
            self.present();
        }
    }

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

            // Stamp first row pixel-by-pixel (no bounds check needed, already clamped)
            {
                let mut off = first_row_offset;
                for _ in 0..row_pixel_count {
                    self.buffer[off] = color.b;
                    self.buffer[off + 1] = color.g;
                    self.buffer[off + 2] = color.r;
                    if bpp >= 4 {
                        self.buffer[off + 3] = color.a;
                    }
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

    /// Draw a line between two points (Bresenham's algorithm)
    pub fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Pixel) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            if x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32 {
                self.blend_pixel(x as usize, y as usize, color);
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw an anti-aliased line using Wu's algorithm
    /// Produces smooth lines with sub-pixel alpha blending
    pub fn draw_line_aa(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Pixel) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();

        // Trivial cases: horizontal or vertical lines don't need AA
        if dy == 0 {
            let xs = x0.min(x1);
            let xe = x0.max(x1);
            for x in xs..=xe {
                if x >= 0 && x < self.width as i32 && y0 >= 0 && y0 < self.height as i32 {
                    self.blend_pixel(x as usize, y0 as usize, color);
                }
            }
            return;
        }
        if dx == 0 {
            let ys = y0.min(y1);
            let ye = y0.max(y1);
            for y in ys..=ye {
                if x0 >= 0 && x0 < self.width as i32 && y >= 0 && y < self.height as i32 {
                    self.blend_pixel(x0 as usize, y as usize, color);
                }
            }
            return;
        }

        let base_alpha = color.a as u32;
        let steep = dy > dx;

        // Work in fixed-point (8 fractional bits = 256 scale)
        let (mut ax, mut ay, mut bx, mut by) = if steep {
            (y0, x0, y1, x1) // swap axes so we always iterate along the longer axis
        } else {
            (x0, y0, x1, y1)
        };

        if ax > bx {
            core::mem::swap(&mut ax, &mut bx);
            core::mem::swap(&mut ay, &mut by);
        }

        let run = bx - ax;
        if run == 0 {
            return;
        }
        let rise = by - ay;
        // gradient in 8.8 fixed-point
        let gradient = (rise * 256) / run;

        // First endpoint
        let mut y_fp = ay * 256 + 128; // start at pixel center

        for x in ax..=bx {
            let y_int = y_fp >> 8;
            let frac = (y_fp & 0xFF) as u32; // fractional part 0..255
            let inv_frac = 255 - frac;

            // Plot two pixels straddling the ideal line
            let (px1, py1, px2, py2) = if steep {
                (y_int, x, y_int + 1, x)
            } else {
                (x, y_int, x, y_int + 1)
            };

            if px1 >= 0 && px1 < self.width as i32 && py1 >= 0 && py1 < self.height as i32 {
                let a1 = ((base_alpha * inv_frac) / 255).min(255) as u8;
                self.blend_pixel(
                    px1 as usize,
                    py1 as usize,
                    Pixel::new(color.r, color.g, color.b, a1),
                );
            }
            if px2 >= 0 && px2 < self.width as i32 && py2 >= 0 && py2 < self.height as i32 {
                let a2 = ((base_alpha * frac) / 255).min(255) as u8;
                self.blend_pixel(
                    px2 as usize,
                    py2 as usize,
                    Pixel::new(color.r, color.g, color.b, a2),
                );
            }

            y_fp += gradient;
        }
    }

    /// Draw a thick anti-aliased line (for UI elements like close buttons)
    /// Uses distance-to-line for smooth edges on any thickness
    pub fn draw_line_thick_aa(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: Pixel,
        thickness: f32,
    ) {
        let dx = (x1 - x0) as f32;
        let dy = (y1 - y0) as f32;
        let len = libm::sqrtf(dx * dx + dy * dy);
        if len < 0.001 {
            return;
        }
        // Normal vector components (perpendicular to line)
        let nx = -dy / len;
        let ny = dx / len;

        let half_t = thickness * 0.5;
        let base_alpha = color.a as u32;

        // Bounding box with 1px padding for AA fringe
        let min_x = (x0.min(x1) as f32 - half_t - 1.0) as i32;
        let max_x = (x0.max(x1) as f32 + half_t + 1.0) as i32;
        let min_y = (y0.min(y1) as f32 - half_t - 1.0) as i32;
        let max_y = (y0.max(y1) as f32 + half_t + 1.0) as i32;

        for py in min_y.max(0)..=max_y.min(self.height as i32 - 1) {
            for px in min_x.max(0)..=max_x.min(self.width as i32 - 1) {
                let fx = px as f32 + 0.5;
                let fy = py as f32 + 0.5;

                // Project point onto line to get distance along and perpendicular
                let to_px = fx - x0 as f32;
                let to_py = fy - y0 as f32;
                let along = (to_px * dx + to_py * dy) / len;
                let perp = (to_px * nx + to_py * ny).abs();

                // Clamp: only draw between endpoints (with small extension for caps)
                if along < -0.5 || along > len + 0.5 {
                    continue;
                }

                // Distance from line edge
                let dist_from_edge = perp - half_t;
                if dist_from_edge >= 1.0 {
                    continue;
                }

                let coverage = if dist_from_edge <= 0.0 {
                    255u32
                } else {
                    (255.0 * (1.0 - dist_from_edge)) as u32
                };

                // Also fade at endpoints
                let end_fade = if along < 0.0 {
                    (255.0 * (1.0 + along)) as u32
                } else if along > len {
                    (255.0 * (1.0 - (along - len))).max(0.0) as u32
                } else {
                    255u32
                };

                let final_alpha = ((base_alpha * coverage * end_fade) / (255 * 255)).min(255) as u8;
                if final_alpha > 0 {
                    self.blend_pixel(
                        px as usize,
                        py as usize,
                        Pixel::new(color.r, color.g, color.b, final_alpha),
                    );
                }
            }
        }
    }

    /// Draw an anti-aliased arc (portion of a circle outline)
    /// Useful for speaker wave icons, wifi arcs, etc.
    /// `start_angle` and `end_angle` in radians, `thickness` in pixels
    pub fn draw_arc_aa(
        &mut self,
        cx: i32,
        cy: i32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        thickness: f32,
        color: Pixel,
    ) {
        let half_t = thickness * 0.5;
        let r_outer = radius + half_t + 1.0;
        let r_inner = (radius - half_t - 1.0).max(0.0);
        let base_alpha = color.a as u32;

        let min_x = (cx as f32 - r_outer) as i32;
        let max_x = (cx as f32 + r_outer) as i32;
        let min_y = (cy as f32 - r_outer) as i32;
        let max_y = (cy as f32 + r_outer) as i32;

        for py in min_y.max(0)..=max_y.min(self.height as i32 - 1) {
            for px in min_x.max(0)..=max_x.min(self.width as i32 - 1) {
                let fx = px as f32 + 0.5 - cx as f32;
                let fy = py as f32 + 0.5 - cy as f32;

                let dist = libm::sqrtf(fx * fx + fy * fy);
                if dist < r_inner || dist > r_outer {
                    continue;
                }

                // Check angle (atan2)
                let mut angle = libm::atan2f(fy, fx);
                if angle < 0.0 {
                    angle += 2.0 * core::f32::consts::PI;
                }

                // Normalize angles
                let mut sa = start_angle;
                let mut ea = end_angle;
                if sa < 0.0 {
                    sa += 2.0 * core::f32::consts::PI;
                }
                if ea < 0.0 {
                    ea += 2.0 * core::f32::consts::PI;
                }

                let in_arc = if sa <= ea {
                    angle >= sa && angle <= ea
                } else {
                    angle >= sa || angle <= ea
                };
                if !in_arc {
                    continue;
                }

                // Distance from ideal circle
                let dist_from_circle = (dist - radius).abs();
                let dist_from_edge = dist_from_circle - half_t;

                if dist_from_edge >= 1.0 {
                    continue;
                }

                let coverage = if dist_from_edge <= 0.0 {
                    255u32
                } else {
                    (255.0 * (1.0 - dist_from_edge)) as u32
                };

                let final_alpha = ((base_alpha * coverage) / 255).min(255) as u8;
                if final_alpha > 0 {
                    self.blend_pixel(
                        px as usize,
                        py as usize,
                        Pixel::new(color.r, color.g, color.b, final_alpha),
                    );
                }
            }
        }
    }

    /// Draw a filled rounded rectangle
    pub fn fill_rounded_rect(&mut self, rect: Rect, color: Pixel, radius: u32) {
        // Main body
        self.fill_rect(
            Rect::new(
                rect.x + radius as i32,
                rect.y,
                rect.width - radius * 2,
                rect.height,
            ),
            color,
        );
        self.fill_rect(
            Rect::new(
                rect.x,
                rect.y + radius as i32,
                rect.width,
                rect.height - radius * 2,
            ),
            color,
        );

        // Corners (filled circles at each corner)
        let r = radius as i32;
        self.fill_circle(rect.x + r, rect.y + r, radius, color);
        self.fill_circle(
            rect.x + rect.width as i32 - r - 1,
            rect.y + r,
            radius,
            color,
        );
        self.fill_circle(
            rect.x + r,
            rect.y + rect.height as i32 - r - 1,
            radius,
            color,
        );
        self.fill_circle(
            rect.x + rect.width as i32 - r - 1,
            rect.y + rect.height as i32 - r - 1,
            radius,
            color,
        );
    }

    /// Draw a rounded rectangle outline (border only)
    pub fn draw_rounded_rect(&mut self, rect: Rect, color: Pixel, radius: u32, thickness: u32) {
        let r = radius as i32;
        let t = thickness.max(1) as i32;
        // Top edge (between corners)
        for i in 0..t {
            self.draw_hline(rect.x + r, rect.y + i, rect.width - radius * 2, color);
        }
        // Bottom edge
        for i in 0..t {
            self.draw_hline(
                rect.x + r,
                rect.y + rect.height as i32 - 1 - i,
                rect.width - radius * 2,
                color,
            );
        }
        // Left edge (between corners)
        for i in 0..t {
            self.draw_vline(rect.x + i, rect.y + r, rect.height - radius * 2, color);
        }
        // Right edge
        for i in 0..t {
            self.draw_vline(
                rect.x + rect.width as i32 - 1 - i,
                rect.y + r,
                rect.height - radius * 2,
                color,
            );
        }
        // Corner arcs using AA circle outline
        self.draw_circle_aa(rect.x + r, rect.y + r, radius, color);
        self.draw_circle_aa(
            rect.x + rect.width as i32 - r - 1,
            rect.y + r,
            radius,
            color,
        );
        self.draw_circle_aa(
            rect.x + r,
            rect.y + rect.height as i32 - r - 1,
            radius,
            color,
        );
        self.draw_circle_aa(
            rect.x + rect.width as i32 - r - 1,
            rect.y + rect.height as i32 - r - 1,
            radius,
            color,
        );
    }

    /// Fill a circle using horizontal line spans (much faster than per-pixel)
    pub fn fill_circle(&mut self, cx: i32, cy: i32, radius: u32, color: Pixel) {
        let r = radius as i32;
        let r_sq = r * r;
        for dy in -r..=r {
            let dy_sq = dy * dy;
            if dy_sq > r_sq {
                continue;
            }
            let mut dx_max = 0i32;
            while (dx_max + 1) * (dx_max + 1) + dy_sq <= r_sq {
                dx_max += 1;
            }
            let py = cy + dy;
            let x_start = cx - dx_max;
            let x_end = cx + dx_max;
            if py >= 0 && py < self.height as i32 {
                let xs = x_start.max(0) as usize;
                let xe = (x_end + 1).min(self.width as i32).max(0) as usize;
                let span_w = xe.saturating_sub(xs);
                if span_w > 0 {
                    self.fill_rect(Rect::new(xs as i32, py, span_w as u32, 1), color);
                }
            }
        }
    }

    /// Fill a circle with anti-aliasing (smooth edges via coverage-based alpha)
    pub fn fill_circle_aa(&mut self, cx: i32, cy: i32, radius: u32, color: Pixel) {
        if radius == 0 {
            return;
        }
        // Use 16x fixed-point for sub-pixel precision
        // radius in 8.8 fixed point
        let r = radius as i32;
        let r_outer = r + 1; // 1px anti-alias fringe
        let base_alpha = color.a as u32;

        for dy in -r_outer..=r_outer {
            let py = cy + dy;
            if py < 0 || py >= self.height as i32 {
                continue;
            }
            for dx in -r_outer..=r_outer {
                let px = cx + dx;
                if px < 0 || px >= self.width as i32 {
                    continue;
                }

                // Distance from center (using 256x precision for smoother result)
                let dist_sq_256 = (dx * dx + dy * dy) * 256;
                let r_sq_256 = r * r * 256;

                if dist_sq_256 <= r_sq_256 - 256 * r {
                    // Fully inside: draw at full alpha
                    self.blend_pixel(px as usize, py as usize, color);
                } else if dist_sq_256 <= r_sq_256 + 256 * r {
                    // On the edge: compute coverage for anti-aliasing
                    // Approximate: linear falloff over 1px
                    // dist ≈ sqrt(dist_sq), but we can linearize around r
                    // coverage ≈ 1 - (dist - r + 0.5)
                    // Using integer approximation:
                    let dist_x256 = isqrt_256(dist_sq_256 as u32);
                    let r_x256 = r as u32 * 256;
                    let coverage = if dist_x256 <= r_x256 {
                        255u32
                    } else {
                        let overshoot = dist_x256 - r_x256; // in 1/256 units
                        255u32.saturating_sub(overshoot)
                    };
                    if coverage > 0 {
                        let aa_alpha = ((base_alpha * coverage) / 255).min(255) as u8;
                        let aa_color = Pixel::new(color.r, color.g, color.b, aa_alpha);
                        self.blend_pixel(px as usize, py as usize, aa_color);
                    }
                }
                // else: fully outside, skip
            }
        }
    }

    /// Fill a rounded rectangle with anti-aliased edges and vertical gradient
    /// Optimized: middle rows use fast span fill, only corner rows do per-pixel AA.
    pub fn fill_rounded_rect_gradient_aa(
        &mut self,
        rect: Rect,
        top_color: Pixel,
        bottom_color: Pixel,
        radius: u32,
    ) {
        let x0 = rect.x;
        let y0 = rect.y;
        let w = rect.width as i32;
        let h = rect.height as i32;
        let r = (radius as i32).min(w / 2).min(h / 2);

        if w <= 0 || h <= 0 {
            return;
        }

        for py in y0.max(0)..(y0 + h).min(self.height as i32) {
            // Compute gradient color for this row
            let t = ((py - y0) as u32 * 255) / (h.max(1) as u32);
            let row_color = Pixel::lerp(top_color, bottom_color, t as u8);

            let ly = py - y0; // local y within rect
            let in_corner = ly < r || ly >= h - r;

            if !in_corner {
                // Middle rows: full width span, no AA needed — fast path
                let xs = x0.max(0);
                let xe = (x0 + w).min(self.width as i32);
                if xe > xs {
                    if row_color.a == 255 {
                        // Opaque: direct fill (no blending)
                        self.fill_rect(Rect::new(xs, py, (xe - xs) as u32, 1), row_color);
                    } else {
                        // Semi-transparent: blend entire span
                        for px in xs..xe {
                            self.blend_pixel(px as usize, py as usize, row_color);
                        }
                    }
                }
            } else {
                // Corner rows: per-pixel AA coverage
                for px in x0.max(0)..(x0 + w).min(self.width as i32) {
                    let coverage = rounded_rect_coverage(px, py, x0, y0, w, h, r);
                    if coverage == 0 {
                        continue;
                    }

                    if coverage == 255 {
                        self.blend_pixel(px as usize, py as usize, row_color);
                    } else {
                        let aa_alpha =
                            ((row_color.a as u32 * coverage as u32) / 255).min(255) as u8;
                        let aa_color = Pixel::new(row_color.r, row_color.g, row_color.b, aa_alpha);
                        self.blend_pixel(px as usize, py as usize, aa_color);
                    }
                }
            }
        }
    }

    /// Fill a rounded rectangle with anti-aliased edges (single color)
    pub fn fill_rounded_rect_aa(&mut self, rect: Rect, color: Pixel, radius: u32) {
        let x0 = rect.x;
        let y0 = rect.y;
        let w = rect.width as i32;
        let h = rect.height as i32;
        let r = (radius as i32).min(w / 2).min(h / 2);

        if w <= 0 || h <= 0 {
            return;
        }

        for py in y0.max(0)..(y0 + h).min(self.height as i32) {
            // Find the span of fully-inside pixels for this row for fast fill
            let ly = py - y0; // local y within rect

            // Check if we're in a corner row
            let in_top_corner = ly < r;
            let in_bottom_corner = ly >= h - r;

            if !in_top_corner && !in_bottom_corner {
                // Middle rows: full width, no AA needed
                let xs = x0.max(0);
                let xe = (x0 + w).min(self.width as i32);
                if xe > xs {
                    self.fill_rect(Rect::new(xs, py, (xe - xs) as u32, 1), color);
                }
            } else {
                // Corner rows: need per-pixel AA
                for px in x0.max(0)..(x0 + w).min(self.width as i32) {
                    let coverage = rounded_rect_coverage(px, py, x0, y0, w, h, r);
                    if coverage == 0 {
                        continue;
                    }

                    if coverage == 255 {
                        self.blend_pixel(px as usize, py as usize, color);
                    } else {
                        let aa_alpha = ((color.a as u32 * coverage as u32) / 255).min(255) as u8;
                        let aa_color = Pixel::new(color.r, color.g, color.b, aa_alpha);
                        self.blend_pixel(px as usize, py as usize, aa_color);
                    }
                }
            }
        }
    }

    /// Draw a circle outline using horizontal spans
    pub fn draw_circle(&mut self, cx: i32, cy: i32, radius: u32, color: Pixel) {
        let r = radius as i32;
        let r_sq = r * r;
        let inner_sq = (r - 1) * (r - 1);
        for dy in -r..=r {
            let dy_sq = dy * dy;
            if dy_sq > r_sq {
                continue;
            }
            let py = cy + dy;
            if py < 0 || py >= self.height as i32 {
                continue;
            }
            // Find the range of dx where inner² < dx²+dy² <= r²
            for dx in -r..=r {
                let dist_sq = dx * dx + dy_sq;
                if dist_sq >= inner_sq && dist_sq <= r_sq {
                    let px = cx + dx;
                    if px >= 0 && (px as usize) < self.width {
                        self.blend_pixel(px as usize, py as usize, color);
                    }
                }
            }
        }
    }

    /// Draw an anti-aliased circle outline using distance-based alpha
    pub fn draw_circle_aa(&mut self, cx: i32, cy: i32, radius: u32, color: Pixel) {
        let r = radius as f32;
        let thickness = 1.0f32;
        let ri = r - thickness * 0.5;
        let ro = r + thickness * 0.5;
        let ro2 = (ro + 1.0) as i32;

        for dy in -ro2..=ro2 {
            let py = cy + dy;
            if py < 0 || py >= self.height as i32 {
                continue;
            }
            for dx in -ro2..=ro2 {
                let px = cx + dx;
                if px < 0 || px >= self.width as i32 {
                    continue;
                }
                let dist = libm::sqrtf((dx * dx + dy * dy) as f32);
                // Distance to the ring center-line (radius)
                let ring_dist = (dist - r).abs();
                if ring_dist > thickness * 0.5 + 1.0 {
                    continue;
                }
                // Alpha based on coverage
                let alpha = if ring_dist <= thickness * 0.5 {
                    1.0
                } else {
                    1.0 - (ring_dist - thickness * 0.5)
                };
                if alpha <= 0.0 {
                    continue;
                }
                let a = (alpha * color.a as f32) as u8;
                let aa_color = Pixel::new(color.r, color.g, color.b, a);
                self.blend_pixel(px as usize, py as usize, aa_color);
            }
        }
    }

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

// ═══════════════════════════════════════════════════════════════════════
// Anti-aliasing helper functions (outside impl block)
// ═══════════════════════════════════════════════════════════════════════

/// Integer square root scaled by 256 (8.8 fixed point result)
/// Input: value already scaled by 256 (i.e., compute sqrt(val/256)*256)
fn isqrt_256(val_x256: u32) -> u32 {
    // We want sqrt(dist_sq) * 256 = sqrt(val_x256 / 256) * 256 = sqrt(val_x256 * 256)
    let n = val_x256 as u64 * 256;
    if n == 0 {
        return 0;
    }
    // Integer square root via binary digit-by-digit method
    let mut guess = n;
    let mut result = 0u64;
    let mut bit = 1u64 << 62;
    while bit > n {
        bit >>= 2;
    }
    while bit != 0 {
        if guess >= result + bit {
            guess -= result + bit;
            result = (result >> 1) + bit;
        } else {
            result >>= 1;
        }
        bit >>= 2;
    }
    // result is floor(sqrt(n)) where n = val_x256 * 256
    // We want this / 16 to get back to 8.8 scale... no wait.
    // val_x256 = dist_sq * 256. We want sqrt(dist_sq) * 256.
    // sqrt(val_x256) * sqrt(256) = sqrt(dist_sq) * 16 * 16 = sqrt(dist_sq)*256
    // So result = floor(sqrt(val_x256 * 256))
    // = floor(sqrt(dist_sq * 256 * 256)) = floor(sqrt(dist_sq) * 256) ✓
    result as u32
}

/// Compute anti-aliased coverage for a pixel at (px,py) inside a rounded rect.
/// Returns 0-255 where 255 = fully covered, 0 = fully outside.
fn rounded_rect_coverage(px: i32, py: i32, rx: i32, ry: i32, rw: i32, rh: i32, r: i32) -> u8 {
    // Local coords within the rect
    let lx = px - rx;
    let ly = py - ry;

    // Quick reject: outside bounding box
    if lx < 0 || ly < 0 || lx >= rw || ly >= rh {
        return 0;
    }

    // Check if we're in a corner region
    let in_left = lx < r;
    let in_right = lx >= rw - r;
    let in_top = ly < r;
    let in_bottom = ly >= rh - r;

    if (!in_left && !in_right) || (!in_top && !in_bottom) {
        // Not in a corner: fully inside
        return 255;
    }

    // In a corner: compute distance from corner circle center
    let (ccx, ccy) = match (in_left, in_top) {
        (true, true) => (r, r),                     // top-left
        (false, true) => (rw - r - 1, r),           // top-right
        (true, false) => (r, rh - r - 1),           // bottom-left
        (false, false) => (rw - r - 1, rh - r - 1), // bottom-right
    };

    let dx = lx - ccx;
    let dy = ly - ccy;
    let dist_sq = dx * dx + dy * dy;
    let r_sq = r * r;

    if dist_sq <= (r - 1) * (r - 1) {
        // Fully inside the rounded corner
        255
    } else if dist_sq > (r + 1) * (r + 1) {
        // Fully outside the rounded corner
        0
    } else {
        // Anti-alias zone: compute smooth coverage
        // dist = sqrt(dist_sq), coverage = clamp(r + 0.5 - dist, 0, 1) * 255
        let dist_x16 = isqrt_x16(dist_sq as u32);
        let r_x16 = r as u32 * 16 + 8; // r + 0.5 in 4.4 fixed point
        if dist_x16 <= r_x16 {
            255
        } else {
            let overshoot = dist_x16 - r_x16; // in 1/16 units
            // 16 units = 1 pixel of falloff
            let coverage = 255u32.saturating_sub(overshoot * 16);
            coverage.min(255) as u8
        }
    }
}

/// Integer sqrt in 4.4 fixed point (result * 16) — public for use in icon drawing
pub fn isqrt_x16_pub(n: u32) -> u32 {
    isqrt_x16(n)
}

/// Integer sqrt in 4.4 fixed point (result * 16)
fn isqrt_x16(n: u32) -> u32 {
    if n == 0 {
        return 0;
    }
    // We want sqrt(n) * 16 = sqrt(n * 256)
    let val = n as u64 * 256;
    // Integer square root via Newton's method
    let mut guess = val;
    let mut result = 0u64;
    let mut bit = 1u64 << 62;
    while bit > val {
        bit >>= 2;
    }
    while bit != 0 {
        if guess >= result + bit {
            guess -= result + bit;
            result = (result >> 1) + bit;
        } else {
            result >>= 1;
        }
        bit >>= 2;
    }
    result as u32
}
