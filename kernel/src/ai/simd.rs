//! Check CPU SIMD support at runtime

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Detected SIMD capability level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SimdLevel {
    Scalar = 0,
    Sse2 = 1,
    Sse42 = 2,
    Avx = 3,
    Avx2 = 4,
    Avx512 = 5,
}

static DETECTED_LEVEL: AtomicU8 = AtomicU8::new(0);
static DETECTION_DONE: AtomicBool = AtomicBool::new(false);

/// Detect SIMD capabilities from CPUID
pub fn detect() -> SimdLevel {
    if DETECTION_DONE.load(Ordering::Relaxed) {
        return SimdLevel::from(DETECTED_LEVEL.load(Ordering::Relaxed));
    }

    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    let mut level = SimdLevel::Scalar;

    if let Some(features) = cpuid.get_feature_info() {
        if features.has_sse2() {
            level = SimdLevel::Sse2;
        }
        if features.has_sse41() {
            level = SimdLevel::Sse42;
        }
        if features.has_avx() {
            level = SimdLevel::Avx;
        }
    }

    if let Some(ext) = cpuid.get_extended_feature_info() {
        if ext.has_avx2() {
            level = SimdLevel::Avx2;
        }
        if ext.has_avx512f() {
            level = SimdLevel::Avx512;
        }
    }

    DETECTED_LEVEL.store(level as u8, Ordering::Relaxed);
    DETECTION_DONE.store(true, Ordering::Relaxed);
    crate::serial_println!("[AI/SIMD] Detected SIMD level: {:?}", level);
    level
}

impl From<u8> for SimdLevel {
    fn from(v: u8) -> Self {
        match v {
            0 => SimdLevel::Scalar,
            1 => SimdLevel::Sse2,
            2 => SimdLevel::Sse42,
            3 => SimdLevel::Avx,
            4 => SimdLevel::Avx2,
            5 => SimdLevel::Avx512,
            _ => SimdLevel::Scalar,
        }
    }
}

// ─── SIMD-accelerated vector operations ────────────────────────────

/// SIMD dot product: a · b
/// Uses AVX2 (8-wide f32) when available, SSE2 (4-wide f32) fallback
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    let level = detect();

    match level {
        SimdLevel::Avx2 | SimdLevel::Avx512 => dot_product_avx2(a, b, len),
        SimdLevel::Sse2 | SimdLevel::Sse42 | SimdLevel::Avx => dot_product_sse2(a, b, len),
        SimdLevel::Scalar => dot_product_scalar(a, b, len),
    }
}

/// Scalar dot product (baseline)
fn dot_product_scalar(a: &[f32], b: &[f32], len: usize) -> f32 {
    let mut sum = 0.0f32;
    for i in 0..len {
        sum += a[i] * b[i];
    }
    sum
}

/// SSE2 dot product — process 4 floats at a time
fn dot_product_sse2(a: &[f32], b: &[f32], len: usize) -> f32 {
    #[cfg(target_feature = "avx2")]
    {
        #[cfg(target_arch = "x86_64")]
        use core::arch::x86_64::*;
        #[cfg(target_arch = "x86_64")]
        use core::arch::x86_64::*;
        let mut sum = 0.0f32;
        let chunks = len / 4;
        let remainder = len % 4;

        unsafe {
            let mut acc = _mm_setzero_ps();
            for i in 0..chunks {
                let va = _mm_loadu_ps(a.as_ptr().add(i * 4));
                let vb = _mm_loadu_ps(b.as_ptr().add(i * 4));
                let prod = _mm_mul_ps(va, vb);
                acc = _mm_add_ps(acc, prod);
            }
            // Horizontal sum: [a, b, c, d] -> a+b+c+d
            let hi = _mm_movehl_ps(acc, acc);
            let sum_lo = _mm_add_ps(acc, hi);
            let shuf = _mm_shuffle_ps(sum_lo, sum_lo, 1);
            let final_sum = _mm_add_ss(sum_lo, shuf);
            sum = _mm_cvtss_f32(final_sum);
        }

        // Handle remaining elements
        let start = chunks * 4;
        for i in 0..remainder {
            sum += a[start + i] * b[start + i];
        }
        sum
    }
    #[cfg(not(target_feature = "avx2"))]
    {
        dot_product_scalar(a, b, len)
    }
}

/// AVX2 dot product — process 8 floats at a time
fn dot_product_avx2(a: &[f32], b: &[f32], len: usize) -> f32 {
    #[cfg(target_feature = "avx2")]
    {
        #[cfg(target_arch = "x86_64")]
        use core::arch::x86_64::*;
        let mut sum = 0.0f32;
        let chunks = len / 8;
        let remainder = len % 8;

        unsafe {
            let mut acc = _mm256_setzero_ps();
            for i in 0..chunks {
                let va = _mm256_loadu_ps(a.as_ptr().add(i * 8));
                let vb = _mm256_loadu_ps(b.as_ptr().add(i * 8));
                acc = _mm256_fmadd_ps(va, vb, acc); // FMA: acc += va * vb
            }
            // Horizontal sum across 256-bit register
            let hi128 = _mm256_extractf128_ps(acc, 1);
            let lo128 = _mm256_castps256_ps128(acc);
            let sum128 = _mm_add_ps(lo128, hi128);
            let hi64 = _mm_movehl_ps(sum128, sum128);
            let sum64 = _mm_add_ps(sum128, hi64);
            let hi32 = _mm_shuffle_ps(sum64, sum64, 1);
            let final_sum = _mm_add_ss(sum64, hi32);
            sum = _mm_cvtss_f32(final_sum);
        }

        let start = chunks * 8;
        for i in 0..remainder {
            sum += a[start + i] * b[start + i];
        }
        sum
    }
    #[cfg(not(target_feature = "avx2"))]
    {
        dot_product_scalar(a, b, len)
    }
}

/// SIMD vector addition: result = a + b
pub fn vec_add(a: &[f32], b: &[f32]) -> Vec<f32> {
    let len = a.len().min(b.len());
    let mut result = Vec::with_capacity(len);

    #[cfg(target_feature = "avx2")]
    {
        let level = detect();
        if level as u8 >= SimdLevel::Avx2 as u8 {
            vec_add_avx2(a, b, &mut result, len);
            return result;
        }
    }

    // Scalar fallback
    for i in 0..len {
        result.push(a[i] + b[i]);
    }
    result
}

#[cfg(target_feature = "avx2")]
fn vec_add_avx2(a: &[f32], b: &[f32], result: &mut Vec<f32>, len: usize) {
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;
    let chunks = len / 8;
    let remainder = len % 8;

    unsafe {
        result.set_len(len);
        for i in 0..chunks {
            let va = _mm256_loadu_ps(a.as_ptr().add(i * 8));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i * 8));
            let vr = _mm256_add_ps(va, vb);
            _mm256_storeu_ps(result.as_mut_ptr().add(i * 8), vr);
        }
    }

    let start = chunks * 8;
    for i in 0..remainder {
        result[start + i] = a[start + i] + b[start + i];
    }
}

/// SIMD vector multiply: result = a * b (element-wise)
pub fn vec_mul(a: &[f32], b: &[f32]) -> Vec<f32> {
    let len = a.len().min(b.len());
    let mut result = Vec::with_capacity(len);

    #[cfg(target_feature = "avx2")]
    {
        let level = detect();
        if level as u8 >= SimdLevel::Avx2 as u8 {
            vec_mul_avx2(a, b, &mut result, len);
            return result;
        }
    }

    for i in 0..len {
        result.push(a[i] * b[i]);
    }
    result
}

#[cfg(target_feature = "avx2")]
fn vec_mul_avx2(a: &[f32], b: &[f32], result: &mut Vec<f32>, len: usize) {
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;
    let chunks = len / 8;
    let remainder = len % 8;

    unsafe {
        result.set_len(len);
        for i in 0..chunks {
            let va = _mm256_loadu_ps(a.as_ptr().add(i * 8));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i * 8));
            let vr = _mm256_mul_ps(va, vb);
            _mm256_storeu_ps(result.as_mut_ptr().add(i * 8), vr);
        }
    }

    let start = chunks * 8;
    for i in 0..remainder {
        result[start + i] = a[start + i] * b[start + i];
    }
}

/// SIMD scalar multiply: result = a * scalar
pub fn vec_scale(a: &[f32], scalar: f32) -> Vec<f32> {
    let len = a.len();
    let mut result: Vec<f32> = Vec::with_capacity(len);

    #[cfg(target_feature = "avx2")]
    {
        let level = detect();
        if level as u8 >= SimdLevel::Avx2 as u8 {
            #[cfg(target_arch = "x86_64")]
            use core::arch::x86_64::*;
            let chunks = len / 8;
            let remainder = len % 8;

            unsafe {
                result.set_len(len);
                let vs = _mm256_set1_ps(scalar);
                for i in 0..chunks {
                    let va = _mm256_loadu_ps(a.as_ptr().add(i * 8));
                    let vr = _mm256_mul_ps(va, vs);
                    _mm256_storeu_ps(result.as_mut_ptr().add(i * 8), vr);
                }
            }

            let start = chunks * 8;
            for i in 0..remainder {
                result[start + i] = a[start + i] * scalar;
            }
            return result;
        }
    }

    for &v in a {
        result.push(v * scalar);
    }
    result
}

/// SIMD-accelerated matrix multiply (row-major, M×K × K×N → M×N)
pub fn matmul_simd(a: &[f32], b: &[f32], m: usize, k: usize, n: usize) -> Vec<f32> {
    let mut c = alloc::vec![0.0f32; m * n];
    let level = detect();

    match level {
        SimdLevel::Avx2 | SimdLevel::Avx512 => {
            matmul_avx2(a, b, &mut c, m, k, n);
        }
        _ => {
            matmul_scalar(a, b, &mut c, m, k, n);
        }
    }
    c
}

fn matmul_scalar(a: &[f32], b: &[f32], c: &mut [f32], m: usize, k: usize, n: usize) {
    for i in 0..m {
        for j in 0..n {
            let mut sum = 0.0f32;
            for l in 0..k {
                sum += a[i * k + l] * b[l * n + j];
            }
            c[i * n + j] = sum;
        }
    }
}

#[cfg(target_feature = "avx2")]
fn matmul_avx2(a: &[f32], b: &[f32], c: &mut [f32], m: usize, k: usize, n: usize) {
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    for i in 0..m {
        let row_a = &a[i * k..(i + 1) * k];
        let row_c = &mut c[i * n..(i + 1) * n];

        // Process 8 columns at a time
        let col_chunks = n / 8;
        let col_rem = n % 8;

        for jc in 0..col_chunks {
            unsafe {
                let mut acc = _mm256_setzero_ps();
                for l in 0..k {
                    let va = _mm256_set1_ps(row_a[l]);
                    let vb = _mm256_loadu_ps(b.as_ptr().add(l * n + jc * 8));
                    acc = _mm256_fmadd_ps(va, vb, acc);
                }
                _mm256_storeu_ps(row_c.as_mut_ptr().add(jc * 8), acc);
            }
        }

        // Remainder columns (scalar)
        let col_start = col_chunks * 8;
        for j in col_start..col_start + col_rem {
            let mut sum = 0.0f32;
            for l in 0..k {
                sum += row_a[l] * b[l * n + j];
            }
            row_c[j] = sum;
        }
    }
}

#[cfg(not(target_feature = "avx2"))]
fn matmul_avx2(a: &[f32], b: &[f32], c: &mut [f32], m: usize, k: usize, n: usize) {
    matmul_scalar(a, b, c, m, k, n);
}

/// SIMD-accelerated ReLU: max(0, x)
pub fn relu_simd(data: &[f32]) -> Vec<f32> {
    let len = data.len();
    let mut result: Vec<f32> = Vec::with_capacity(len);

    #[cfg(target_feature = "avx2")]
    {
        let level = detect();
        if level as u8 >= SimdLevel::Avx2 as u8 {
            #[cfg(target_arch = "x86_64")]
            use core::arch::x86_64::*;
            let chunks = len / 8;
            let remainder = len % 8;

            unsafe {
                result.set_len(len);
                let vzero = _mm256_setzero_ps();
                for i in 0..chunks {
                    let v = _mm256_loadu_ps(data.as_ptr().add(i * 8));
                    let r = _mm256_max_ps(v, vzero);
                    _mm256_storeu_ps(result.as_mut_ptr().add(i * 8), r);
                }
            }

            let start = chunks * 8;
            for i in 0..remainder {
                result[start + i] = if data[start + i] > 0.0 {
                    data[start + i]
                } else {
                    0.0
                };
            }
            return result;
        }
    }

    for &v in data {
        result.push(if v > 0.0 { v } else { 0.0 });
    }
    result
}

/// SIMD-accelerated softmax
pub fn softmax_simd(logits: &[f32]) -> Vec<f32> {
    let len = logits.len();
    if len == 0 {
        return Vec::new();
    }

    // Find max (for numerical stability)
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    // exp(x - max)
    let mut exps = Vec::with_capacity(len);
    for &v in logits {
        exps.push(super::fast_exp(v - max));
    }

    // Sum
    let sum: f32 = exps.iter().sum();

    // Normalize (SIMD scalar divide)
    vec_scale(&exps, 1.0 / sum)
}

/// SIMD-accelerated layer norm: (x - mean) / sqrt(var + eps) * gamma + beta
pub fn layer_norm(x: &[f32], gamma: &[f32], beta: &[f32], eps: f32) -> Vec<f32> {
    let len = x.len();
    if len == 0 {
        return Vec::new();
    }

    // Compute mean
    let sum: f32 = x.iter().sum();
    let mean = sum / len as f32;

    // Compute variance
    let var: f32 = x.iter().map(|&v| (v - mean) * (v - mean)).sum::<f32>() / len as f32;
    let inv_std = 1.0 / super::fast_sqrt(var + eps);

    // Normalize and apply affine
    let mut result = Vec::with_capacity(len);
    for i in 0..len {
        let normalized = (x[i] - mean) * inv_std;
        let g = if i < gamma.len() { gamma[i] } else { 1.0 };
        let b = if i < beta.len() { beta[i] } else { 0.0 };
        result.push(normalized * g + b);
    }
    result
}

/// SIMD-accelerated RMS norm (LLaMA-style): x / sqrt(mean(x^2) + eps) * gamma
pub fn rms_norm(x: &[f32], gamma: &[f32], eps: f32) -> Vec<f32> {
    let len = x.len();
    if len == 0 {
        return Vec::new();
    }

    // mean(x^2) using SIMD dot product
    let sum_sq = dot_product(x, x);
    let rms = super::fast_sqrt(sum_sq / len as f32 + eps);
    let inv_rms = 1.0 / rms;

    let mut result = Vec::with_capacity(len);
    for i in 0..len {
        let g = if i < gamma.len() { gamma[i] } else { 1.0 };
        result.push(x[i] * inv_rms * g);
    }
    result
}
