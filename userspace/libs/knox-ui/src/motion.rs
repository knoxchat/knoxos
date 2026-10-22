// SPDX-License-Identifier: MIT
//! Motion tokens, tween, and spring (GPUI kit motion *behavior*, `libm`).
//!
//! Reduced-motion sets duration to zero: values jump to the target.

use core::time::Duration;

use knox_gpui::{Pixels, px};

/// Kit `MotionTokens` policy shared by styled controls and the proof window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionTokens {
    pub duration_instant: Duration,
    pub duration_fast: Duration,
    pub duration_normal: Duration,
    pub duration_slow: Duration,
    pub easing_enter: Easing,
    pub easing_exit: Easing,
    pub easing_move: Easing,
    pub spring_control: Spring,
    pub spring_move: Spring,
    pub distance_short: Pixels,
    pub distance_medium: Pixels,
}

impl Default for MotionTokens {
    fn default() -> Self {
        Self {
            duration_instant: Duration::ZERO,
            duration_fast: Duration::from_millis(120),
            duration_normal: Duration::from_millis(180),
            duration_slow: Duration::from_millis(280),
            easing_enter: Easing::cubic_bezier(0.16, 1.0, 0.3, 1.0),
            easing_exit: Easing::cubic_bezier(0.4, 0.0, 1.0, 1.0),
            easing_move: Easing::cubic_bezier(0.2, 0.0, 0.0, 1.0),
            spring_control: Spring::new(Duration::from_millis(180)),
            spring_move: Spring::new(Duration::from_millis(280))
                .with_damping(0.85)
                .with_epsilon(0.1),
            distance_short: px(4),
            distance_medium: px(8),
        }
    }
}

impl MotionTokens {
    /// Duration used for a tween when reduced-motion is on (`0`).
    pub fn duration_or_zero(self, duration: Duration, reduce_motion: bool) -> Duration {
        if reduce_motion {
            Duration::ZERO
        } else {
            duration
        }
    }

    /// All timed motions collapse to a jump (D16.1).
    pub fn reduced() -> Self {
        let mut t = Self::default();
        t.duration_fast = Duration::ZERO;
        t.duration_normal = Duration::ZERO;
        t.duration_slow = Duration::ZERO;
        t
    }
}

/// CSS-like easing. Cubic-Bézier control x must be in `0..=1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier { x1: f32, y1: f32, x2: f32, y2: f32 },
}

impl Easing {
    pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self::CubicBezier { x1, y1, x2, y2 }
    }

    pub fn sample(self, progress: f32) -> f32 {
        let t = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Ease => bezier(0.25, 0.1, 0.25, 1.0, t),
            Self::EaseIn => bezier(0.42, 0.0, 1.0, 1.0, t),
            Self::EaseOut => bezier(0.0, 0.0, 0.58, 1.0, t),
            Self::EaseInOut => bezier(0.42, 0.0, 0.58, 1.0, t),
            Self::CubicBezier { x1, y1, x2, y2 } => bezier(x1, y1, x2, y2, t),
        }
    }
}

/// Unit cubic Bézier with P0=(0,0), P3=(1,1). `t` is x-progress (CSS).
fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let cx = 3.0 * x1;
    let bx = 3.0 * (x2 - x1) - cx;
    let ax = 1.0 - cx - bx;
    let cy = 3.0 * y1;
    let by = 3.0 * (y2 - y1) - cy;
    let ay = 1.0 - cy - by;
    let sample_x = |s: f32| ((ax * s + bx) * s + cx) * s;
    let sample_y = |s: f32| ((ay * s + by) * s + cy) * s;
    let slope_x = |s: f32| (3.0 * ax * s + 2.0 * bx) * s + cx;

    let mut s = t;
    for _ in 0..8 {
        let error = sample_x(s) - t;
        if libm::fabsf(error) < 1e-6 {
            return sample_y(s);
        }
        let slope = slope_x(s);
        if libm::fabsf(slope) < 1e-6 {
            break;
        }
        s = (s - error / slope).clamp(0.0, 1.0);
    }
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..32 {
        let x = sample_x(s);
        if libm::fabsf(x - t) < 1e-6 {
            break;
        }
        if x < t {
            low = s;
        } else {
            high = s;
        }
        s = (low + high) * 0.5;
    }
    sample_y(s)
}

/// Duration-based interpolation. Reduced-motion (or zero duration) jumps to `to`.
pub fn tween(
    from: f32,
    to: f32,
    elapsed: Duration,
    duration: Duration,
    easing: Easing,
    reduce_motion: bool,
) -> f32 {
    if reduce_motion || duration.is_zero() || elapsed >= duration {
        return to;
    }
    let t = elapsed.as_secs_f32() / duration.as_secs_f32();
    from + (to - from) * easing.sample(t)
}

/// Physical spring policy (kit `Spring`: response, damping ζ, epsilon).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    response: Duration,
    damping: f32,
    epsilon: f32,
}

impl Spring {
    /// Critically damped spring whose feel is the `response` period.
    pub const fn new(response: Duration) -> Self {
        Self {
            response,
            damping: 1.0,
            epsilon: 0.001,
        }
    }

    pub const fn with_damping(mut self, ratio: f32) -> Self {
        self.damping = ratio;
        self
    }

    pub const fn with_epsilon(mut self, epsilon: f32) -> Self {
        self.epsilon = epsilon;
        self
    }

    pub const fn epsilon(self) -> f32 {
        self.epsilon
    }

    pub const fn response(self) -> Duration {
        self.response
    }
}

/// Clock-driven f32 spring. First sample adopts the target; later retargets travel.
pub struct SpringDriver {
    policy: Spring,
    position: f32,
    velocity: f32,
    target: f32,
    last_ms: u64,
    primed: bool,
}

impl SpringDriver {
    pub fn new(policy: Spring) -> Self {
        Self {
            policy,
            position: 0.0,
            velocity: 0.0,
            target: 0.0,
            last_ms: 0,
            primed: false,
        }
    }

    /// Sample toward `target` at `now_ms` (host fake clock or kernel millis).
    pub fn sample(&mut self, target: f32, now_ms: u64, reduce_motion: bool) -> f32 {
        if !self.primed {
            self.primed = true;
            self.position = target;
            self.velocity = 0.0;
            self.target = target;
            self.last_ms = now_ms;
            return target;
        }

        if reduce_motion || self.policy.response.is_zero() {
            self.position = target;
            self.velocity = 0.0;
            self.target = target;
            self.last_ms = now_ms;
            return target;
        }

        let dt = now_ms.saturating_sub(self.last_ms) as f32 / 1000.0;
        self.last_ms = now_ms;
        if dt > 0.0 {
            let (p, v) = spring_step(
                self.position,
                self.velocity,
                self.target,
                dt,
                self.policy.response.as_secs_f32(),
                self.policy.damping,
            );
            self.position = p;
            self.velocity = v;
        }
        self.target = target;
        let eps = self.policy.epsilon;
        if libm::fabsf(self.position - target) < eps && libm::fabsf(self.velocity) < eps {
            self.position = target;
            self.velocity = 0.0;
        }
        self.position
    }
}

fn spring_step(pos: f32, vel: f32, target: f32, dt: f32, response_s: f32, zeta: f32) -> (f32, f32) {
    let omega = core::f32::consts::TAU / response_s.max(1e-6);
    let x0 = pos - target;
    let v0 = vel;
    if zeta < 1.0 - 1e-4 {
        let wd = omega * libm::sqrtf((1.0 - zeta * zeta).max(0.0));
        let exp = libm::expf(-zeta * omega * dt);
        let a = x0;
        let b = if wd > 1e-8 {
            (v0 + zeta * omega * x0) / wd
        } else {
            0.0
        };
        let c = libm::cosf(wd * dt);
        let s = libm::sinf(wd * dt);
        let x = exp * (a * c + b * s);
        let v = -zeta * omega * x + exp * (-a * wd * s + b * wd * c);
        (target + x, v)
    } else if zeta > 1.0 + 1e-4 {
        let disc = libm::sqrtf(zeta * zeta - 1.0);
        let s1 = -omega * (zeta - disc);
        let s2 = -omega * (zeta + disc);
        let denom = s1 - s2;
        let a = if libm::fabsf(denom) > 1e-8 {
            (v0 - s2 * x0) / denom
        } else {
            0.0
        };
        let b = x0 - a;
        let e1 = libm::expf(s1 * dt);
        let e2 = libm::expf(s2 * dt);
        let x = a * e1 + b * e2;
        let v = a * s1 * e1 + b * s2 * e2;
        (target + x, v)
    } else {
        let exp = libm::expf(-omega * dt);
        let a = x0;
        let b = v0 + omega * x0;
        let x = (a + b * dt) * exp;
        let v = (b - omega * (a + b * dt)) * exp;
        (target + x, v)
    }
}

/// Opacity + Y enter sample used by the live proof window (D7.4).
pub fn enter_opacity_y(
    elapsed: Duration,
    tokens: &MotionTokens,
    reduce_motion: bool,
) -> (f32, i32, bool) {
    let opacity = tween(
        0.0,
        1.0,
        elapsed,
        tokens.duration_slow,
        tokens.easing_enter,
        reduce_motion,
    );
    let y = tween(
        tokens.distance_medium.get() as f32,
        0.0,
        elapsed,
        tokens.duration_slow,
        tokens.easing_enter,
        reduce_motion,
    );
    let done = reduce_motion || elapsed >= tokens.duration_slow || opacity >= 0.999;
    (opacity, y as i32, done)
}

/// Opacity + Y exit sample for GPUI window minimize (D10.3).
pub fn exit_opacity_y(
    elapsed: Duration,
    tokens: &MotionTokens,
    reduce_motion: bool,
) -> (f32, i32, bool) {
    let (enter_op, enter_y, done) = enter_opacity_y(elapsed, tokens, reduce_motion);
    (1.0 - enter_op, tokens.distance_medium.get() - enter_y, done)
}

#[cfg(test)]
mod tests {
    use super::{Easing, Spring, SpringDriver, tween};
    use core::time::Duration;

    /// D7.3 done-when: host fake-clock 0→1 spring; reduced-motion jumps.
    #[test]
    fn spring_zero_to_one_on_fake_clock() {
        let mut spring = SpringDriver::new(Spring::new(Duration::from_millis(180)));
        assert_eq!(spring.sample(0.0, 0, false), 0.0);

        let at_retarget = spring.sample(1.0, 0, false);
        assert!(
            at_retarget < 0.05,
            "retarget at the same tick stays near 0, got {at_retarget}"
        );

        let mid = spring.sample(1.0, 50, false);
        assert!(
            mid > 0.15 && mid < 0.9,
            "50ms of a 180ms spring should be mid-travel, got {mid}"
        );

        let end = spring.sample(1.0, 2000, false);
        assert!(
            (end - 1.0).abs() < 0.02,
            "settled spring must reach 1, got {end}"
        );
    }

    #[test]
    fn reduced_motion_jumps_to_target() {
        let mut spring = SpringDriver::new(Spring::new(Duration::from_millis(280)));
        assert_eq!(spring.sample(0.0, 0, true), 0.0);
        assert_eq!(spring.sample(1.0, 0, true), 1.0);
        assert_eq!(spring.sample(1.0, 40, true), 1.0);

        let jumped = tween(
            0.0,
            1.0,
            Duration::from_millis(0),
            Duration::from_millis(280),
            Easing::EaseOut,
            true,
        );
        assert_eq!(jumped, 1.0);
    }

    #[test]
    fn enter_easing_endpoints() {
        let e = Easing::cubic_bezier(0.16, 1.0, 0.3, 1.0);
        assert!((e.sample(0.0) - 0.0).abs() < 1e-5);
        assert!((e.sample(1.0) - 1.0).abs() < 1e-5);
        assert!(e.sample(0.25) > 0.5, "enter curve is front-loaded");
    }
}
