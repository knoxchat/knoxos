use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Animation easing function
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier,
    Spring,
    Bounce,
}

/// A running animation
#[derive(Debug, Clone)]
pub struct Animation {
    pub id: u64,
    /// Start value
    pub from: f32,
    /// End value
    pub to: f32,
    /// Current interpolated value
    pub current: f32,
    /// Duration in milliseconds
    pub duration_ms: u32,
    /// Elapsed time in milliseconds
    pub elapsed_ms: u32,
    /// Easing function
    pub easing: Easing,
    /// Whether the animation is complete
    pub done: bool,
    /// Callback identifier (for matching to UI property)
    pub target: String,
}

impl Animation {
    pub fn new(from: f32, to: f32, duration_ms: u32, easing: Easing, target: &str) -> Self {
        Self {
            id: ANIM_COUNTER.fetch_add(1, Ordering::Relaxed),
            from,
            to,
            current: from,
            duration_ms,
            elapsed_ms: 0,
            easing,
            done: false,
            target: String::from(target),
        }
    }

    /// Advance the animation by delta_ms
    pub fn tick(&mut self, delta_ms: u32) {
        if self.done {
            return;
        }
        self.elapsed_ms += delta_ms;
        if self.elapsed_ms >= self.duration_ms {
            self.elapsed_ms = self.duration_ms;
            self.done = true;
        }

        let t = self.elapsed_ms as f32 / self.duration_ms as f32;
        let eased = match self.easing {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0) * (-2.0 * t + 2.0) / 2.0
                }
            }
            Easing::CubicBezier => {
                // Approximate cubic-bezier(0.25, 0.1, 0.25, 1.0) — CSS default
                3.0 * (1.0 - t) * (1.0 - t) * t * 0.1 + 3.0 * (1.0 - t) * t * t * 1.0 + t * t * t
            }
            Easing::Spring => {
                // Damped spring oscillation
                let decay = libm::expf(-5.0 * t);
                1.0 - decay * libm::cosf(10.0 * t)
            }
            Easing::Bounce => {
                let t2 = 1.0 - t;
                if t2 < 1.0 / 2.75 {
                    1.0 - 7.5625 * t2 * t2
                } else if t2 < 2.0 / 2.75 {
                    let t3 = t2 - 1.5 / 2.75;
                    1.0 - (7.5625 * t3 * t3 + 0.75)
                } else if t2 < 2.5 / 2.75 {
                    let t3 = t2 - 2.25 / 2.75;
                    1.0 - (7.5625 * t3 * t3 + 0.9375)
                } else {
                    let t3 = t2 - 2.625 / 2.75;
                    1.0 - (7.5625 * t3 * t3 + 0.984375)
                }
            }
        };

        self.current = self.from + (self.to - self.from) * eased;
    }
}

static ANIM_COUNTER: AtomicU64 = AtomicU64::new(1);

lazy_static::lazy_static! {
    static ref ANIMATIONS: Mutex<Vec<Animation>> = Mutex::new(Vec::new());
}

/// Start a new animation
pub fn animate(from: f32, to: f32, duration_ms: u32, easing: Easing, target: &str) -> u64 {
    let anim = Animation::new(from, to, duration_ms, easing, target);
    let id = anim.id;
    ANIMATIONS.lock().push(anim);
    id
}

/// Tick all animations forward
pub fn tick_animations(delta_ms: u32) {
    let mut anims = ANIMATIONS.lock();
    for anim in anims.iter_mut() {
        anim.tick(delta_ms);
    }
    // Remove completed animations
    anims.retain(|a| !a.done);
}

/// Get the current value of an animation by target name
pub fn get_animation_value(target: &str) -> Option<f32> {
    let anims = ANIMATIONS.lock();
    anims.iter().find(|a| a.target == target).map(|a| a.current)
}

/// Check if any animations are running
pub fn has_active_animations() -> bool {
    !ANIMATIONS.lock().is_empty()
}
