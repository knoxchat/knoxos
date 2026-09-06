/// Window animation system — open, close, minimize, restore, transition animations
/// Extracted from window.rs for better code organization.
use super::framebuffer::Rect;
use super::scale;
use super::window::{ANIM_DURATION_MS, Window, WindowAnimation, WindowAnimationType};

/// Ease-out cubic: fast start → smooth deceleration
#[inline]
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = 1.0 - t;
    1.0 - t * t * t
}

impl Window {
    /// Returns true if this window is currently being animated
    pub fn is_animating(&self) -> bool {
        self.anim.is_some()
    }

    /// Start an open animation (scale up from 85% + fade in)
    pub fn start_open_animation(&mut self) {
        let tsc_freq = super::min_frame_ticks() * 60; // approximate TSC freq (ticks-per-sec)
        let duration = tsc_freq * ANIM_DURATION_MS / 1000;
        // Scale from 85% centered
        let scale = 0.85f32;
        let cx = self.rect.x + self.rect.width as i32 / 2;
        let cy = self.rect.y + self.rect.height as i32 / 2;
        let sw = (self.rect.width as f32 * scale) as u32;
        let sh = (self.rect.height as f32 * scale) as u32;
        let from = Rect::new(cx - sw as i32 / 2, cy - sh as i32 / 2, sw, sh);
        self.anim = Some(WindowAnimation {
            kind: WindowAnimationType::Open,
            progress: 0.0,
            start_tsc: super::read_tsc_public(),
            duration_ticks: duration.max(1),
            from_rect: from,
            to_rect: self.rect,
        });
    }

    /// Start a close animation (scale down to 85% + fade out)
    pub fn start_close_animation(&mut self) {
        let tsc_freq = super::min_frame_ticks() * 60;
        let duration = tsc_freq * ANIM_DURATION_MS / 1000;
        let scale = 0.85f32;
        let cx = self.rect.x + self.rect.width as i32 / 2;
        let cy = self.rect.y + self.rect.height as i32 / 2;
        let sw = (self.rect.width as f32 * scale) as u32;
        let sh = (self.rect.height as f32 * scale) as u32;
        let to = Rect::new(cx - sw as i32 / 2, cy - sh as i32 / 2, sw, sh);
        self.anim = Some(WindowAnimation {
            kind: WindowAnimationType::Close,
            progress: 0.0,
            start_tsc: super::read_tsc_public(),
            duration_ticks: duration.max(1),
            from_rect: self.rect,
            to_rect: to,
        });
    }

    /// Start a minimize animation (scale down toward taskbar)
    pub fn start_minimize_animation(&mut self, screen_w: i32, screen_h: i32) {
        let tsc_freq = super::min_frame_ticks() * 60;
        let duration = tsc_freq * ANIM_DURATION_MS / 1000;
        // Target: small rect at bottom-center (taskbar area)
        let taskbar_h = scale::taskbar_height() as i32;
        let target_w = 80u32;
        let target_h = 40u32;
        let target_x = screen_w / 2 - target_w as i32 / 2;
        let target_y = screen_h - taskbar_h;
        self.anim = Some(WindowAnimation {
            kind: WindowAnimationType::Minimize,
            progress: 0.0,
            start_tsc: super::read_tsc_public(),
            duration_ticks: duration.max(1),
            from_rect: self.rect,
            to_rect: Rect::new(target_x, target_y, target_w, target_h),
        });
    }

    /// Start a rect transition animation (for maximize, restore, snap)
    pub fn start_transition_animation(&mut self, from: Rect, to: Rect) {
        let tsc_freq = super::min_frame_ticks() * 60;
        let duration = tsc_freq * ANIM_DURATION_MS / 1000;
        self.anim = Some(WindowAnimation {
            kind: WindowAnimationType::Transition,
            progress: 0.0,
            start_tsc: super::read_tsc_public(),
            duration_ticks: duration.max(1),
            from_rect: from,
            to_rect: to,
        });
    }

    /// Get the current animated rect and opacity (0.0-1.0).
    /// Returns (rect, opacity). If no animation, returns (self.rect, 1.0).
    pub fn animated_rect_opacity(&self) -> (Rect, f32) {
        match &self.anim {
            None => (self.rect, 1.0),
            Some(a) => {
                let t = ease_out_cubic(a.progress);
                let lerp_i32 = |a: i32, b: i32| -> i32 { a + ((b - a) as f32 * t) as i32 };
                let lerp_u32 = |a: u32, b: u32| -> u32 {
                    (a as f32 + (b as f32 - a as f32) * t).max(1.0) as u32
                };
                let rect = Rect::new(
                    lerp_i32(a.from_rect.x, a.to_rect.x),
                    lerp_i32(a.from_rect.y, a.to_rect.y),
                    lerp_u32(a.from_rect.width, a.to_rect.width),
                    lerp_u32(a.from_rect.height, a.to_rect.height),
                );
                let opacity = match a.kind {
                    WindowAnimationType::Open | WindowAnimationType::Restore => t,
                    WindowAnimationType::Close | WindowAnimationType::Minimize => 1.0 - t,
                    WindowAnimationType::Transition => 1.0,
                };
                (rect, opacity)
            }
        }
    }

    /// Advance the animation. Returns true if animation is still running.
    pub fn tick_animation(&mut self) -> bool {
        if let Some(ref mut a) = self.anim {
            let now = super::read_tsc_public();
            let elapsed = now.wrapping_sub(a.start_tsc);
            a.progress = (elapsed as f32 / a.duration_ticks as f32).min(1.0);
            if a.progress >= 1.0 {
                // Keep the animation struct so finish_closed_windows() can detect
                // completed Close animations. It will be cleaned up there.
                return false;
            }
            true
        } else {
            false
        }
    }
}
