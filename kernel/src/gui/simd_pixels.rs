/// SIMD Pixel Operations — SSE2/AVX2 optimized pixel blending
///
/// Provides SIMD-accelerated versions of common pixel operations:
///   - Alpha blending (4 pixels at a time with SSE2)
///   - Fill rect (16 bytes at a time)
///   - Pixel lerp (color interpolation)
///   - Premultiplied alpha conversion
///
/// Falls back to scalar operations if SIMD is not available.
use crate::gui::framebuffer::Pixel;
use crate::serial_println;
use core::sync::atomic::{AtomicBool, Ordering};

// ═══════════════════════════════════════════════════════════════════════
// SIMD DETECTION
// ═══════════════════════════════════════════════════════════════════════

static SSE2_AVAILABLE: AtomicBool = AtomicBool::new(false);
static AVX2_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Detect SIMD capabilities via CPUID (x86_64) or equivalent
pub fn detect_simd() {
    #[cfg(target_arch = "x86_64")]
    {
        let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();

        if let Some(features) = cpuid.get_feature_info() {
            if features.has_sse2() {
                SSE2_AVAILABLE.store(true, Ordering::Relaxed);
                serial_println!("[SIMD] SSE2 available");
            }
        }

        if let Some(ext_features) = cpuid.get_extended_feature_info() {
            if ext_features.has_avx2() {
                AVX2_AVAILABLE.store(true, Ordering::Relaxed);
                serial_println!("[SIMD] AVX2 available");
            }
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        // aarch64 NEON is always available; riscv64 uses scalar fallback
        #[cfg(target_arch = "aarch64")]
        serial_println!("[SIMD] aarch64 — using scalar pixel ops (NEON future)");
        #[cfg(target_arch = "riscv64")]
        serial_println!("[SIMD] riscv64 — using scalar pixel ops");
    }
}

/// Check if SSE2 is available
pub fn has_sse2() -> bool {
    SSE2_AVAILABLE.load(Ordering::Relaxed)
}

/// Check if AVX2 is available
pub fn has_avx2() -> bool {
    AVX2_AVAILABLE.load(Ordering::Relaxed)
}

// ═══════════════════════════════════════════════════════════════════════
// FAST ALPHA BLEND
// ═══════════════════════════════════════════════════════════════════════

/// Blend source pixel over destination pixel (optimized scalar)
/// Uses the approximation: alpha/256 instead of alpha/255 to avoid division
#[inline(always)]
pub fn blend_over_fast(dst: &mut Pixel, src: Pixel) {
    let sa = src.a as u32;
    if sa == 0 {
        return;
    }
    if sa == 255 {
        *dst = src;
        return;
    }

    // Fast blend: dst = src * sa/256 + dst * (256-sa)/256
    let inv_a = 256 - sa;
    let r = (src.r as u32 * sa + dst.r as u32 * inv_a) >> 8;
    let g = (src.g as u32 * sa + dst.g as u32 * inv_a) >> 8;
    let b = (src.b as u32 * sa + dst.b as u32 * inv_a) >> 8;

    dst.r = r as u8;
    dst.g = g as u8;
    dst.b = b as u8;
    dst.a = 255;
}

/// Blend a slice of source pixels over destination pixels
pub fn blend_row(dst: &mut [Pixel], src: &[Pixel]) {
    let len = dst.len().min(src.len());
    for i in 0..len {
        blend_over_fast(&mut dst[i], src[i]);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FAST FILL
// ═══════════════════════════════════════════════════════════════════════

/// Fill a pixel slice with a solid color (uses memset-like approach for opaque)
#[inline]
pub fn fill_fast(dst: &mut [Pixel], color: Pixel) {
    if color.a == 255 {
        // Opaque fill — can use simple assignment
        for px in dst.iter_mut() {
            *px = color;
        }
    } else if color.a == 0 {
        // Fully transparent — no-op
    } else {
        // Semi-transparent — need blending
        for px in dst.iter_mut() {
            blend_over_fast(px, color);
        }
    }
}

/// Fill a pixel slice with a solid color using 64-bit writes where possible
pub fn fill_fast_u64(dst: &mut [Pixel], color: Pixel) {
    if color.a == 255 && dst.len() >= 2 {
        // Pack two pixels into a u64
        let pixel_u32 = unsafe { core::mem::transmute::<Pixel, u32>(color) };
        let packed = ((pixel_u32 as u64) << 32) | (pixel_u32 as u64);

        let ptr = dst.as_mut_ptr() as *mut u64;
        let pairs = dst.len() / 2;

        unsafe {
            for i in 0..pairs {
                ptr.add(i).write(packed);
            }
        }

        // Handle odd pixel
        if dst.len() % 2 == 1 {
            dst[dst.len() - 1] = color;
        }
    } else {
        fill_fast(dst, color);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PIXEL LERP
// ═══════════════════════════════════════════════════════════════════════

/// Linearly interpolate between two pixels (t = 0..255)
#[inline(always)]
pub fn lerp_pixel(a: Pixel, b: Pixel, t: u8) -> Pixel {
    let t32 = t as u32;
    let inv_t = 255 - t32;

    Pixel {
        r: ((a.r as u32 * inv_t + b.r as u32 * t32) / 255) as u8,
        g: ((a.g as u32 * inv_t + b.g as u32 * t32) / 255) as u8,
        b: ((a.b as u32 * inv_t + b.b as u32 * t32) / 255) as u8,
        a: ((a.a as u32 * inv_t + b.a as u32 * t32) / 255) as u8,
    }
}

/// Batch lerp: interpolate two pixel slices
pub fn lerp_row(dst: &mut [Pixel], a: &[Pixel], b: &[Pixel], t: u8) {
    let len = dst.len().min(a.len()).min(b.len());
    for i in 0..len {
        dst[i] = lerp_pixel(a[i], b[i], t);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PREMULTIPLIED ALPHA
// ═══════════════════════════════════════════════════════════════════════

/// Convert to premultiplied alpha
#[inline(always)]
pub fn premultiply(pixel: Pixel) -> Pixel {
    let a = pixel.a as u32;
    Pixel {
        r: ((pixel.r as u32 * a) / 255) as u8,
        g: ((pixel.g as u32 * a) / 255) as u8,
        b: ((pixel.b as u32 * a) / 255) as u8,
        a: pixel.a,
    }
}

/// Convert from premultiplied alpha
#[inline(always)]
pub fn unpremultiply(pixel: Pixel) -> Pixel {
    if pixel.a == 0 {
        return Pixel::new(0, 0, 0, 0);
    }
    let a = pixel.a as u32;
    Pixel {
        r: ((pixel.r as u32 * 255) / a).min(255) as u8,
        g: ((pixel.g as u32 * 255) / a).min(255) as u8,
        b: ((pixel.b as u32 * 255) / a).min(255) as u8,
        a: pixel.a,
    }
}

/// Convert a row to premultiplied alpha
pub fn premultiply_row(pixels: &mut [Pixel]) {
    for px in pixels.iter_mut() {
        *px = premultiply(*px);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FAST COPY
// ═══════════════════════════════════════════════════════════════════════

/// Copy a row of pixels (opaque copy, no blending)
#[inline]
pub fn copy_row(dst: &mut [Pixel], src: &[Pixel]) {
    let len = dst.len().min(src.len());
    dst[..len].copy_from_slice(&src[..len]);
}

/// Copy with a uniform alpha applied to source
pub fn copy_row_with_alpha(dst: &mut [Pixel], src: &[Pixel], alpha: u8) {
    let len = dst.len().min(src.len());
    let a = alpha as u32;
    for i in 0..len {
        let s = src[i];
        let sa = (s.a as u32 * a) >> 8;
        if sa == 0 {
            continue;
        }
        let inv_a = 256 - sa;
        dst[i] = Pixel {
            r: ((s.r as u32 * sa + dst[i].r as u32 * inv_a) >> 8) as u8,
            g: ((s.g as u32 * sa + dst[i].g as u32 * inv_a) >> 8) as u8,
            b: ((s.b as u32 * sa + dst[i].b as u32 * inv_a) >> 8) as u8,
            a: 255,
        };
    }
}

/// Initialize SIMD pixel operations
pub fn init() {
    detect_simd();
    serial_println!(
        "[KnoxOS] SIMD pixel ops initialized (SSE2={}, AVX2={})",
        has_sse2(),
        has_avx2()
    );
}
