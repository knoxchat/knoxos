/// Fast inverse square root approximation (no_std compatible)
pub(crate) fn fast_sqrt(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    // Newton's method: start with rough estimate, iterate
    let mut guess = x;
    // Initial estimate using bit manipulation
    let i = f32::to_bits(x);
    let i = 0x1FBD1DF5 + (i >> 1); // magic constant for sqrt approx
    guess = f32::from_bits(i);
    // Two Newton-Raphson iterations for accuracy
    guess = 0.5 * (guess + x / guess);
    guess = 0.5 * (guess + x / guess);
    guess
}

/// Fast exponential approximation (no libm needed)
pub(crate) fn fast_exp(x: f32) -> f32 {
    // Schraudolph's algorithm: fast approximate exp
    if x > 88.0 {
        return f32::INFINITY;
    }
    if x < -88.0 {
        return 0.0;
    }

    // Use a polynomial approximation
    let x = x.clamp(-20.0, 20.0);
    let mut result = 1.0f32;
    let mut term = 1.0f32;
    for i in 1..=12 {
        term *= x / i as f32;
        result += term;
    }
    result
}

/// Fast tanh approximation: tanh(x) = (exp(2x) - 1) / (exp(2x) + 1)
pub(crate) fn fast_tanh(x: f32) -> f32 {
    if x > 10.0 {
        return 1.0;
    }
    if x < -10.0 {
        return -1.0;
    }
    let e2x = fast_exp(2.0 * x);
    (e2x - 1.0) / (e2x + 1.0)
}
