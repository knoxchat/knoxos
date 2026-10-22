/// Color Blindness Filters — Protanopia, Deuteranopia, Tritanopia simulation + correction
use crate::serial_println;
use core::sync::atomic::{AtomicU8, Ordering};

/// Color blindness filter modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CbFilter {
    None = 0,
    Protanopia = 1,    // Red-blind
    Deuteranopia = 2,  // Green-blind
    Tritanopia = 3,    // Blue-blind
    Achromatopsia = 4, // Total color blindness (grayscale)
}

static ACTIVE_FILTER: AtomicU8 = AtomicU8::new(0);

/// Set the active color blindness filter
pub fn set_filter(filter: CbFilter) {
    ACTIVE_FILTER.store(filter as u8, Ordering::SeqCst);
    serial_println!("[a11y] Color blindness filter: {:?}", filter);
}

/// Get the active filter
pub fn get_filter() -> CbFilter {
    match ACTIVE_FILTER.load(Ordering::Relaxed) {
        1 => CbFilter::Protanopia,
        2 => CbFilter::Deuteranopia,
        3 => CbFilter::Tritanopia,
        4 => CbFilter::Achromatopsia,
        _ => CbFilter::None,
    }
}

/// Apply the active color blindness filter to an RGBA pixel.
///
/// Uses the Brettel/Viénot/Mollon simulation matrices (simplified fixed-point).
/// Returns the transformed (r, g, b) values.
pub fn apply(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    match get_filter() {
        CbFilter::None => (r, g, b),
        CbFilter::Protanopia => protanopia(r, g, b),
        CbFilter::Deuteranopia => deuteranopia(r, g, b),
        CbFilter::Tritanopia => tritanopia(r, g, b),
        CbFilter::Achromatopsia => {
            let gray = ((r as u16 * 299 + g as u16 * 587 + b as u16 * 114) / 1000) as u8;
            (gray, gray, gray)
        }
    }
}

/// Protanopia simulation (red-blind)
/// Approximate Viénot matrix (fixed-point x1000):
///   R' = 0.152 R + 1.053 G - 0.205 B
///   G' = 0.115 R + 0.786 G + 0.099 B
///   B' = -0.004 R - 0.048 G + 1.052 B
fn protanopia(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let ri = r as i32;
    let gi = g as i32;
    let bi = b as i32;
    let rp = (152 * ri + 1053 * gi - 205 * bi) / 1000;
    let gp = (115 * ri + 786 * gi + 99 * bi) / 1000;
    let bp = (-4 * ri - 48 * gi + 1052 * bi) / 1000;
    (clamp(rp), clamp(gp), clamp(bp))
}

/// Deuteranopia simulation (green-blind)
///   R' = 0.367 R + 0.861 G - 0.228 B
///   G' = 0.280 R + 0.673 G + 0.047 B
///   B' = -0.012 R + 0.043 G + 0.969 B
fn deuteranopia(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let ri = r as i32;
    let gi = g as i32;
    let bi = b as i32;
    let rp = (367 * ri + 861 * gi - 228 * bi) / 1000;
    let gp = (280 * ri + 673 * gi + 47 * bi) / 1000;
    let bp = (-12 * ri + 43 * gi + 969 * bi) / 1000;
    (clamp(rp), clamp(gp), clamp(bp))
}

/// Tritanopia simulation (blue-blind)
///   R' = 1.256 R - 0.077 G - 0.179 B
///   G' = -0.078 R + 0.931 G + 0.148 B
///   B' = 0.005 R + 0.691 G + 0.304 B
fn tritanopia(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let ri = r as i32;
    let gi = g as i32;
    let bi = b as i32;
    let rp = (1256 * ri - 77 * gi - 179 * bi) / 1000;
    let gp = (-78 * ri + 931 * gi + 148 * bi) / 1000;
    let bp = (5 * ri + 691 * gi + 304 * bi) / 1000;
    (clamp(rp), clamp(gp), clamp(bp))
}

#[inline]
fn clamp(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

/// Apply filter to a full scanline buffer (RGBA, 4 bytes per pixel)
pub fn apply_scanline(pixels: &mut [u8]) {
    if get_filter() == CbFilter::None {
        return;
    }
    let mut i = 0;
    while i + 3 < pixels.len() {
        let (r, g, b) = apply(pixels[i], pixels[i + 1], pixels[i + 2]);
        pixels[i] = r;
        pixels[i + 1] = g;
        pixels[i + 2] = b;
        // Alpha channel (pixels[i+3]) unchanged
        i += 4;
    }
}

pub fn init() {
    serial_println!("[a11y] Color blindness filters initialized");
}
