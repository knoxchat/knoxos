// Math functions (software float, no_std compatible)

/// fabs — absolute value of float
#[unsafe(no_mangle)]
pub extern "C" fn fabs(x: f64) -> f64 {
    if x < 0.0 { -x } else { x }
}

/// fabsf — absolute value of float (f32)
#[unsafe(no_mangle)]
pub extern "C" fn fabsf(x: f32) -> f32 {
    if x < 0.0 { -x } else { x }
}

/// sqrt — square root (Newton-Raphson)
#[unsafe(no_mangle)]
pub extern "C" fn sqrt(x: f64) -> f64 {
    if x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    let mut guess = x;
    let i = f64::to_bits(x);
    let i = 0x1FF7A3BEA91D9B1B_u64.wrapping_add(i >> 1);
    guess = f64::from_bits(i);
    for _ in 0..5 {
        guess = 0.5 * (guess + x / guess);
    }
    guess
}

/// sqrtf — square root (f32)
#[unsafe(no_mangle)]
pub extern "C" fn sqrtf(x: f32) -> f32 {
    if x < 0.0 {
        return f32::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    let mut guess = x;
    let i = f32::to_bits(x);
    let i = 0x1FBD1DF5_u32.wrapping_add(i >> 1);
    guess = f32::from_bits(i);
    for _ in 0..4 {
        guess = 0.5 * (guess + x / guess);
    }
    guess
}

/// floor — largest integer not greater than x
#[unsafe(no_mangle)]
pub extern "C" fn floor(x: f64) -> f64 {
    let i = x as i64;
    let f = i as f64;
    if x < f { f - 1.0 } else { f }
}

/// floorf
#[unsafe(no_mangle)]
pub extern "C" fn floorf(x: f32) -> f32 {
    let i = x as i32;
    let f = i as f32;
    if x < f { f - 1.0 } else { f }
}

/// ceil — smallest integer not less than x
#[unsafe(no_mangle)]
pub extern "C" fn ceil(x: f64) -> f64 {
    let i = x as i64;
    let f = i as f64;
    if x > f { f + 1.0 } else { f }
}

/// ceilf
#[unsafe(no_mangle)]
pub extern "C" fn ceilf(x: f32) -> f32 {
    let i = x as i32;
    let f = i as f32;
    if x > f { f + 1.0 } else { f }
}

/// round — round to nearest integer
#[unsafe(no_mangle)]
pub extern "C" fn round(x: f64) -> f64 {
    floor(x + 0.5)
}

/// roundf
#[unsafe(no_mangle)]
pub extern "C" fn roundf(x: f32) -> f32 {
    floorf(x + 0.5)
}

/// fmod — floating-point remainder
#[unsafe(no_mangle)]
pub extern "C" fn fmod(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return f64::NAN;
    }
    x - (x / y) as i64 as f64 * y
}

/// fmodf
#[unsafe(no_mangle)]
pub extern "C" fn fmodf(x: f32, y: f32) -> f32 {
    if y == 0.0 {
        return f32::NAN;
    }
    x - (x / y) as i32 as f32 * y
}

/// log — natural logarithm (series approximation)
#[unsafe(no_mangle)]
pub extern "C" fn log(x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NAN;
    }
    // Use the identity: ln(x) = ln(2) * log2(x)
    // log2(x) via bit manipulation + polynomial
    let bits = f64::to_bits(x);
    let exp = ((bits >> 52) & 0x7FF) as i64 - 1023;
    let mantissa = f64::from_bits((bits & 0x000FFFFFFFFFFFFF) | 0x3FF0000000000000);
    // Polynomial approximation for ln(m) where m in [1, 2)
    let m = mantissa - 1.0;
    let ln_m = m * (1.0 - m * (0.5 - m * (1.0 / 3.0 - m * 0.25)));
    ln_m + exp as f64 * core::f64::consts::LN_2
}

/// log10 — base-10 logarithm
#[unsafe(no_mangle)]
pub extern "C" fn log10(x: f64) -> f64 {
    log(x) * core::f64::consts::LOG10_E
}

/// pow — power function (simplified)
#[unsafe(no_mangle)]
pub extern "C" fn pow(base: f64, exp: f64) -> f64 {
    if exp == 0.0 {
        return 1.0;
    }
    if base == 0.0 {
        return 0.0;
    }
    // For integer exponents, use repeated multiplication
    let trunc_exp = exp as i64 as f64;
    if exp == trunc_exp && (if exp < 0.0 { -exp } else { exp }) < 100.0 {
        let n = exp as i64;
        let mut result = 1.0;
        let mut b = base;
        let mut e = n.unsigned_abs();
        while e > 0 {
            if e & 1 == 1 {
                result *= b;
            }
            b *= b;
            e >>= 1;
        }
        if n < 0 { 1.0 / result } else { result }
    } else {
        // exp(exp * ln(base))
        let lnb = log(base);
        exp_approx(exp * lnb)
    }
}

/// exp — exponential function (Taylor series)
fn exp_approx(x: f64) -> f64 {
    if x > 709.0 {
        return f64::INFINITY;
    }
    if x < -709.0 {
        return 0.0;
    }
    // Reduce: e^x = 2^k * e^r where r = x - k*ln(2)
    let k = {
        let v = x * core::f64::consts::LOG2_E;
        if v >= 0.0 {
            (v + 0.5) as i64
        } else {
            (v - 0.5) as i64
        }
    }; // 1/ln(2)
    let r = x - k as f64 * core::f64::consts::LN_2;
    // Taylor series for e^r (r is small)
    let mut term = 1.0;
    let mut sum = 1.0;
    for i in 1..=15 {
        term *= r / i as f64;
        sum += term;
    }
    // Multiply by 2^k
    let scale = f64::from_bits(((k + 1023) as u64) << 52);
    sum * scale
}

/// exp (exported)
#[unsafe(no_mangle)]
pub extern "C" fn exp(x: f64) -> f64 {
    exp_approx(x)
}

/// expf
#[unsafe(no_mangle)]
pub extern "C" fn expf(x: f32) -> f32 {
    exp_approx(x as f64) as f32
}
