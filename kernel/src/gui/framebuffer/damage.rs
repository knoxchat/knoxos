use super::{FrameBuffer, Rect};

impl FrameBuffer {
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
}
