/// Animation — Easing functions and animation state helpers for smooth transitions.
///
/// ```ignore
/// let t = Animation::progress(ui, id, target_value, speed);
/// let alpha = ease_out_cubic(t);
/// ```
use crate::gui::id::Id;
use crate::gui::ui::{UI_MEMORY, Ui};

// ── Easing functions (input and output in 0.0..=1.0 range) ──────────────────

/// Linear interpolation (no easing).
pub fn ease_linear(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

/// Ease-in quadratic.
pub fn ease_in_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t
}

/// Ease-out quadratic.
pub fn ease_out_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * (2.0 - t)
}

/// Ease-in-out quadratic.
pub fn ease_in_out_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        2.0 * t * t
    } else {
        -1.0 + (4.0 - 2.0 * t) * t
    }
}

/// Ease-out cubic.
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let t1 = t - 1.0;
    t1 * t1 * t1 + 1.0
}

/// Ease-in cubic.
pub fn ease_in_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t
}

/// Ease-in-out cubic.
pub fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let t1 = 2.0 * t - 2.0;
        0.5 * t1 * t1 * t1 + 1.0
    }
}

/// Ease-out elastic (spring-like bounce).
pub fn ease_out_elastic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t == 0.0 || t == 1.0 {
        return t;
    }
    let p = 0.3f32;
    let s = p / 4.0;
    // Approximate: 2^(-10t) * sin((t - s) * 2π / p) + 1
    let power = pow_approx(2.0, -10.0 * t);
    let sin_val = sin_approx_f32((t - s) * core::f32::consts::TAU / p);
    power * sin_val + 1.0
}

/// Ease-out bounce.
pub fn ease_out_bounce(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 1.0 / 2.75 {
        7.5625 * t * t
    } else if t < 2.0 / 2.75 {
        let t = t - 1.5 / 2.75;
        7.5625 * t * t + 0.75
    } else if t < 2.5 / 2.75 {
        let t = t - 2.25 / 2.75;
        7.5625 * t * t + 0.9375
    } else {
        let t = t - 2.625 / 2.75;
        7.5625 * t * t + 0.984375
    }
}

// ── Integer-based animation helpers ─────────────────────────────────────────

/// Smoothly animate an i32 value toward a target using stored state.
/// Returns the current animated value.
/// `speed` is in 1/256 units (64 = 25% per frame, 256 = instant).
pub fn animate_i32(id: Id, target: i32, speed: i32) -> i32 {
    let key = id.with("anim_val");
    let mut mem = UI_MEMORY.lock();
    let current = mem.get_i32(key, target);

    if current == target {
        return current;
    }

    let diff = target - current;
    let step = (diff * speed) / 256;
    let step = if step == 0 {
        if diff > 0 { 1 } else { -1 }
    } else {
        step
    };

    let new_val = current + step;
    // Prevent overshoot
    let new_val = if (diff > 0 && new_val > target) || (diff < 0 && new_val < target) {
        target
    } else {
        new_val
    };

    mem.set_i32(key, new_val);
    new_val
}

/// Animate a boolean state as a 0..256 progress value.
/// Useful for fade-in / fade-out, expand / collapse.
/// Returns 0..=256 where 0 = fully off, 256 = fully on.
pub fn animate_bool(id: Id, target: bool, speed: i32) -> i32 {
    let target_val = if target { 256 } else { 0 };
    animate_i32(id.with("bool_anim"), target_val, speed)
}

/// Convert a 0..=256 animation progress to a 0.0..=1.0 float.
pub fn progress_to_f32(progress: i32) -> f32 {
    (progress.clamp(0, 256) as f32) / 256.0
}

/// Linear interpolation between two i32 values using 0..=256 progress.
pub fn lerp_i32(a: i32, b: i32, progress: i32) -> i32 {
    let p = progress.clamp(0, 256);
    a + ((b - a) * p) / 256
}

/// Linear interpolation between two u8 values using 0..=256 progress.
pub fn lerp_u8(a: u8, b: u8, progress: i32) -> u8 {
    let p = progress.clamp(0, 256);
    let result = a as i32 + ((b as i32 - a as i32) * p) / 256;
    result.clamp(0, 255) as u8
}

// ── Internal math helpers ───────────────────────────────────────────────────

fn sin_approx_f32(x: f32) -> f32 {
    let pi = core::f32::consts::PI;
    let mut x = x % (2.0 * pi);
    if x > pi {
        x -= 2.0 * pi;
    }
    if x < -pi {
        x += 2.0 * pi;
    }
    let x2 = x * x;
    let x3 = x2 * x;
    let x5 = x3 * x2;
    x - x3 / 6.0 + x5 / 120.0
}

fn pow_approx(base: f32, exp: f32) -> f32 {
    // Simple approximation: base^exp = e^(exp * ln(base))
    // For base=2: 2^x using integer part + interpolation
    if base <= 0.0 {
        return 0.0;
    }
    let x = exp * ln_approx(base);
    exp_approx(x)
}

fn ln_approx(x: f32) -> f32 {
    if x <= 0.0 {
        return -100.0;
    }
    // ln(x) ≈ (x-1) - (x-1)^2/2 + (x-1)^3/3 for x near 1
    // For larger ranges, reduce: ln(x) = ln(x/2^n) + n*ln(2)
    let ln2 = core::f32::consts::LN_2;
    let mut val = x;
    let mut n = 0i32;
    while val > 2.0 {
        val /= 2.0;
        n += 1;
    }
    while val < 0.5 {
        val *= 2.0;
        n -= 1;
    }
    let t = val - 1.0;
    let t2 = t * t;
    let t3 = t2 * t;
    let result = t - t2 / 2.0 + t3 / 3.0;
    result + n as f32 * ln2
}

fn exp_approx(x: f32) -> f32 {
    // e^x for small x using Taylor series
    let x = x.clamp(-20.0, 20.0);
    let mut result = 1.0f32;
    let mut term = 1.0f32;
    for i in 1..12 {
        term *= x / i as f32;
        result += term;
    }
    if result < 0.0 { 0.0 } else { result }
}
