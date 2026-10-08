use super::{FrameBuffer, Rect};

impl FrameBuffer {
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
    pub(super) fn clamp_rect(&self, rect: &Rect) -> Option<(usize, usize, usize, usize)> {
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
    pub(super) fn is_clipped(&self, x: usize, y: usize) -> bool {
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
}
