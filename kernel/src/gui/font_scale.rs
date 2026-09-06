//! Font Size Scaling — Arbitrary font size rendering
//!
//! Provides scaling from the base Hack bitmap fonts (10×20, 8×14) to
//! arbitrary pixel sizes using nearest-neighbor and bilinear interpolation.
//! Covers status.md item 7.22 (Font size scaling).

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Scaling algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleAlgorithm {
    /// Fast, pixelated — good for integer multiples
    NearestNeighbor,
    /// Smoother — good for fractional scales
    Bilinear,
}

/// A scaled glyph bitmap
#[derive(Debug, Clone)]
pub struct ScaledGlyph {
    pub width: usize,
    pub height: usize,
    /// Alpha coverage for each pixel (0–255)
    pub alpha: Vec<u8>,
}

/// Font scale configuration
#[derive(Debug, Clone, Copy)]
pub struct FontScaleConfig {
    /// Target height in pixels
    pub target_height: usize,
    /// Algorithm to use
    pub algorithm: ScaleAlgorithm,
    /// Bold weight adjustment (0.0–1.0)
    pub bold_weight: f32,
}

impl Default for FontScaleConfig {
    fn default() -> Self {
        Self {
            target_height: 20,
            algorithm: ScaleAlgorithm::NearestNeighbor,
            bold_weight: 0.0,
        }
    }
}

/// Predefined font sizes (common desktop sizes in pixels)
pub const FONT_SIZE_SMALL: usize = 11;
pub const FONT_SIZE_NORMAL: usize = 14;
pub const FONT_SIZE_MEDIUM: usize = 16;
pub const FONT_SIZE_LARGE: usize = 20;
pub const FONT_SIZE_XLARGE: usize = 24;
pub const FONT_SIZE_HUGE: usize = 32;
pub const FONT_SIZE_DISPLAY: usize = 48;

static SCALE_COUNT: AtomicU64 = AtomicU64::new(0);

lazy_static::lazy_static! {
    static ref CURRENT_CONFIG: Mutex<FontScaleConfig> = Mutex::new(FontScaleConfig::default());
}

/// Scale a glyph from source dimensions to target height
pub fn scale_glyph(
    src: &[u8],
    src_w: usize,
    src_h: usize,
    target_h: usize,
    algorithm: ScaleAlgorithm,
) -> ScaledGlyph {
    SCALE_COUNT.fetch_add(1, Ordering::Relaxed);

    let scale = target_h as f32 / src_h as f32;
    let target_w = ((src_w as f32) * scale) as usize;
    let target_w = target_w.max(1);
    let target_h = target_h.max(1);

    let mut alpha = Vec::with_capacity(target_w * target_h);

    match algorithm {
        ScaleAlgorithm::NearestNeighbor => {
            for y in 0..target_h {
                let sy = ((y as f32) / scale) as usize;
                let sy = sy.min(src_h - 1);
                for x in 0..target_w {
                    let sx = ((x as f32) / scale) as usize;
                    let sx = sx.min(src_w - 1);
                    let idx = sy * src_w + sx;
                    alpha.push(if idx < src.len() { src[idx] } else { 0 });
                }
            }
        }
        ScaleAlgorithm::Bilinear => {
            for y in 0..target_h {
                let fy = (y as f32) / scale;
                let sy0 = (fy as usize).min(src_h.saturating_sub(1));
                let sy1 = (sy0 + 1).min(src_h.saturating_sub(1));
                let fy_frac = fy - sy0 as f32;

                for x in 0..target_w {
                    let fx = (x as f32) / scale;
                    let sx0 = (fx as usize).min(src_w.saturating_sub(1));
                    let sx1 = (sx0 + 1).min(src_w.saturating_sub(1));
                    let fx_frac = fx - sx0 as f32;

                    let get = |sy: usize, sx: usize| -> f32 {
                        let idx = sy * src_w + sx;
                        if idx < src.len() {
                            src[idx] as f32
                        } else {
                            0.0
                        }
                    };

                    let top = get(sy0, sx0) * (1.0 - fx_frac) + get(sy0, sx1) * fx_frac;
                    let bot = get(sy1, sx0) * (1.0 - fx_frac) + get(sy1, sx1) * fx_frac;
                    let val = top * (1.0 - fy_frac) + bot * fy_frac;
                    alpha.push(val.clamp(0.0, 255.0) as u8);
                }
            }
        }
    }

    ScaledGlyph {
        width: target_w,
        height: target_h,
        alpha,
    }
}

/// Set the global font scale configuration
pub fn set_config(config: FontScaleConfig) {
    *CURRENT_CONFIG.lock() = config;
}

/// Get the current font scale configuration
pub fn get_config() -> FontScaleConfig {
    *CURRENT_CONFIG.lock()
}

/// Compute the character width for a given target height
pub fn char_width_for_height(target_h: usize, base_w: usize, base_h: usize) -> usize {
    let scale = target_h as f32 / base_h as f32;
    ((base_w as f32) * scale) as usize
}

/// Get total scale operations performed
pub fn scale_count() -> u64 {
    SCALE_COUNT.load(Ordering::Relaxed)
}

/// Initialize the font scaling subsystem
pub fn init() {
    crate::serial_println!(
        "[font_scale] Font size scaling initialized (sizes: {}–{}px)",
        FONT_SIZE_SMALL,
        FONT_SIZE_DISPLAY
    );
}
