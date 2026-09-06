// SPDX-License-Identifier: MIT
//! Window blur/gaussian effect (item 8.16)
//!
//! Provides real-time gaussian blur for window backgrounds,
//! frosted glass effects, and transparency compositing.

use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

static BLURS_APPLIED: AtomicU64 = AtomicU64::new(0);

/// Blur radius presets
#[derive(Debug, Clone, Copy)]
pub enum BlurRadius {
    /// Light blur (4px) — subtle frosted glass
    Light,
    /// Medium blur (8px) — standard glass effect
    Medium,
    /// Heavy blur (16px) — strong frosted glass
    Heavy,
    /// Custom radius
    Custom(u32),
}

impl BlurRadius {
    pub fn pixels(&self) -> u32 {
        match self {
            BlurRadius::Light => 4,
            BlurRadius::Medium => 8,
            BlurRadius::Heavy => 16,
            BlurRadius::Custom(r) => *r,
        }
    }
}

/// Blur quality settings
#[derive(Debug, Clone, Copy)]
pub enum BlurQuality {
    /// Fast box blur (2 passes) — good for real-time
    Fast,
    /// Medium quality (3-pass box blur ≈ gaussian)
    Medium,
    /// High quality (true gaussian kernel)
    High,
}

/// Generate 1D Gaussian kernel
fn gaussian_kernel(radius: u32) -> Vec<f32> {
    let size = (radius * 2 + 1) as usize;
    let mut kernel = vec![0.0f32; size];
    let sigma = radius as f32 / 3.0;
    let sigma2 = 2.0 * sigma * sigma;
    let mut sum = 0.0f32;

    for i in 0..size {
        let x = i as f32 - radius as f32;
        let val = libm::expf(-(x * x) / sigma2);
        kernel[i] = val;
        sum += val;
    }

    // Normalize
    for k in kernel.iter_mut() {
        *k /= sum;
    }
    kernel
}

/// Apply a fast box blur (single pass, horizontal or vertical)
fn box_blur_pass(
    src: &[u32],
    dst: &mut [u32],
    width: usize,
    height: usize,
    radius: usize,
    horizontal: bool,
) {
    let diameter = radius * 2 + 1;
    let inv = 1.0 / diameter as f32;

    if horizontal {
        for y in 0..height {
            let mut r_sum: u32 = 0;
            let mut g_sum: u32 = 0;
            let mut b_sum: u32 = 0;
            let mut a_sum: u32 = 0;

            // Initialize window
            for x in 0..=radius.min(width - 1) {
                let px = src[y * width + x];
                r_sum += (px >> 16) & 0xFF;
                g_sum += (px >> 8) & 0xFF;
                b_sum += px & 0xFF;
                a_sum += (px >> 24) & 0xFF;
            }
            // Mirror for left edge
            for i in 1..=radius {
                if i < width {
                    let px = src[y * width]; // clamp to edge
                    r_sum += (px >> 16) & 0xFF;
                    g_sum += (px >> 8) & 0xFF;
                    b_sum += px & 0xFF;
                    a_sum += (px >> 24) & 0xFF;
                }
            }

            for x in 0..width {
                let r = ((r_sum as f32 * inv) as u32).min(255);
                let g = ((g_sum as f32 * inv) as u32).min(255);
                let b = ((b_sum as f32 * inv) as u32).min(255);
                let a = ((a_sum as f32 * inv) as u32).min(255);
                dst[y * width + x] = (a << 24) | (r << 16) | (g << 8) | b;

                // Slide window
                let add_x = (x + radius + 1).min(width - 1);
                let sub_x = if x > radius { x - radius - 1 } else { 0 };

                let add_px = src[y * width + add_x];
                let sub_px = src[y * width + sub_x];

                r_sum += ((add_px >> 16) & 0xFF) - ((sub_px >> 16) & 0xFF);
                g_sum += ((add_px >> 8) & 0xFF) - ((sub_px >> 8) & 0xFF);
                b_sum += (add_px & 0xFF) - (sub_px & 0xFF);
                a_sum += ((add_px >> 24) & 0xFF) - ((sub_px >> 24) & 0xFF);
            }
        }
    } else {
        for x in 0..width {
            let mut r_sum: u32 = 0;
            let mut g_sum: u32 = 0;
            let mut b_sum: u32 = 0;
            let mut a_sum: u32 = 0;

            for y in 0..=radius.min(height - 1) {
                let px = src[y * width + x];
                r_sum += (px >> 16) & 0xFF;
                g_sum += (px >> 8) & 0xFF;
                b_sum += px & 0xFF;
                a_sum += (px >> 24) & 0xFF;
            }
            for _i in 1..=radius {
                let px = src[x]; // top edge
                r_sum += (px >> 16) & 0xFF;
                g_sum += (px >> 8) & 0xFF;
                b_sum += px & 0xFF;
                a_sum += (px >> 24) & 0xFF;
            }

            for y in 0..height {
                let r = ((r_sum as f32 * inv) as u32).min(255);
                let g = ((g_sum as f32 * inv) as u32).min(255);
                let b = ((b_sum as f32 * inv) as u32).min(255);
                let a = ((a_sum as f32 * inv) as u32).min(255);
                dst[y * width + x] = (a << 24) | (r << 16) | (g << 8) | b;

                let add_y = (y + radius + 1).min(height - 1);
                let sub_y = if y > radius { y - radius - 1 } else { 0 };

                let add_px = src[add_y * width + x];
                let sub_px = src[sub_y * width + x];

                r_sum += ((add_px >> 16) & 0xFF) - ((sub_px >> 16) & 0xFF);
                g_sum += ((add_px >> 8) & 0xFF) - ((sub_px >> 8) & 0xFF);
                b_sum += (add_px & 0xFF) - (sub_px & 0xFF);
                a_sum += ((add_px >> 24) & 0xFF) - ((sub_px >> 24) & 0xFF);
            }
        }
    }
}

/// Apply blur to a pixel buffer (BGRA packed u32)
///
/// Uses 3-pass box blur which closely approximates Gaussian blur.
pub fn blur_region(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    radius: BlurRadius,
    quality: BlurQuality,
) {
    let r = radius.pixels() as usize;
    if r == 0 || width == 0 || height == 0 {
        return;
    }

    let passes = match quality {
        BlurQuality::Fast => 2,
        BlurQuality::Medium => 3,
        BlurQuality::High => 4,
    };

    let len = width * height;
    let mut temp = vec![0u32; len];

    for _ in 0..passes {
        // Horizontal pass
        box_blur_pass(pixels, &mut temp, width, height, r, true);
        // Vertical pass
        box_blur_pass(&temp, pixels, width, height, r, false);
    }

    BLURS_APPLIED.fetch_add(1, Ordering::Relaxed);
}

/// Apply frosted glass effect: blur + tint overlay
pub fn frosted_glass(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    tint_color: u32,
    tint_alpha: u8,
) {
    // First blur
    blur_region(
        pixels,
        width,
        height,
        BlurRadius::Medium,
        BlurQuality::Medium,
    );

    // Then apply tint
    let tr = ((tint_color >> 16) & 0xFF) as u16;
    let tg = ((tint_color >> 8) & 0xFF) as u16;
    let tb = (tint_color & 0xFF) as u16;
    let ta = tint_alpha as u16;
    let inv_a = 255 - ta;

    for px in pixels.iter_mut() {
        let r = ((*px >> 16) & 0xFF) as u16;
        let g = ((*px >> 8) & 0xFF) as u16;
        let b = (*px & 0xFF) as u16;
        let a = ((*px >> 24) & 0xFF) as u16;

        let nr = ((r * inv_a + tr * ta) / 255).min(255);
        let ng = ((g * inv_a + tg * ta) / 255).min(255);
        let nb = ((b * inv_a + tb * ta) / 255).min(255);
        let na = a.max(ta).min(255);

        *px = ((na as u32) << 24) | ((nr as u32) << 16) | ((ng as u32) << 8) | (nb as u32);
    }
}

/// Blur a specific rectangle within a larger framebuffer
pub fn blur_rect_in_buffer(
    buffer: &mut [u32],
    buf_width: usize,
    buf_height: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    radius: BlurRadius,
) {
    if x + w > buf_width || y + h > buf_height || w == 0 || h == 0 {
        return;
    }

    // Extract the rectangle
    let mut region = vec![0u32; w * h];
    for row in 0..h {
        let src_start = (y + row) * buf_width + x;
        let dst_start = row * w;
        region[dst_start..dst_start + w].copy_from_slice(&buffer[src_start..src_start + w]);
    }

    // Blur it
    blur_region(&mut region, w, h, radius, BlurQuality::Medium);

    // Write back
    for row in 0..h {
        let src_start = row * w;
        let dst_start = (y + row) * buf_width + x;
        buffer[dst_start..dst_start + w].copy_from_slice(&region[src_start..src_start + w]);
    }
}

pub fn stats() -> u64 {
    BLURS_APPLIED.load(Ordering::Relaxed)
}

/// Initialize the blur subsystem
pub fn init() {
    crate::serial_println!("[blur] gaussian blur engine initialized");
}
