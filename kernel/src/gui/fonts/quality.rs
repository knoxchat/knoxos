// ═══════════════════════════════════════════════════════════════════════
// SUBPIXEL RENDERING MODE
// ═══════════════════════════════════════════════════════════════════════

/// LCD subpixel geometry — determines how R/G/B subpixels are ordered.
/// Standard LCD panels use RGB (left-to-right). Some panels use BGR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubpixelOrder {
    /// RGB subpixel order (most common: Dell, LG, Apple non-Retina, QEMU)
    Rgb,
    /// BGR subpixel order (some Samsung, older panels)
    Bgr,
    /// No subpixel rendering (use grayscale AA instead)
    /// Use for projectors, OLED, CRT, or when subpixels cause color fringing
    None,
}

/// Text rendering quality levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextQuality {
    /// Subpixel (ClearType-style) — 3× horizontal resolution, highest quality
    Subpixel,
    /// Grayscale antialiased (high quality, no color fringing)
    Grayscale,
    /// Legacy 1-bit rendering (fast fallback)
    Legacy,
}

/// Global rendering quality: 0=Subpixel, 1=Legacy, 2=Grayscale
/// Default to Grayscale — Subpixel (ClearType) causes visible color fringing
/// on virtual displays (QEMU/KVM), OLED, and non-RGB-stripe LCDs. Grayscale
/// AA is universally safe, crisp, and avoids the "blurry rainbow" artifacts.
static TEXT_QUALITY: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(2);

/// Global subpixel order: 0=RGB, 1=BGR, 2=None
static SUBPIXEL_ORDER: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Set the global text rendering quality
pub fn set_text_quality(quality: TextQuality) {
    let val = match quality {
        TextQuality::Subpixel => 0,
        TextQuality::Legacy => 1,
        TextQuality::Grayscale => 2,
    };
    TEXT_QUALITY.store(val, core::sync::atomic::Ordering::Relaxed);
}

/// Get the current text rendering quality
pub fn get_text_quality() -> TextQuality {
    match TEXT_QUALITY.load(core::sync::atomic::Ordering::Relaxed) {
        1 => TextQuality::Legacy,
        2 => TextQuality::Grayscale,
        _ => TextQuality::Subpixel,
    }
}

/// Set the LCD subpixel order for ClearType rendering
pub fn set_subpixel_order(order: SubpixelOrder) {
    let val = match order {
        SubpixelOrder::Rgb => 0,
        SubpixelOrder::Bgr => 1,
        SubpixelOrder::None => 2,
    };
    SUBPIXEL_ORDER.store(val, core::sync::atomic::Ordering::Relaxed);
}

/// Get the current subpixel order
pub fn get_subpixel_order() -> SubpixelOrder {
    match SUBPIXEL_ORDER.load(core::sync::atomic::Ordering::Relaxed) {
        1 => SubpixelOrder::Bgr,
        2 => SubpixelOrder::None,
        _ => SubpixelOrder::Rgb,
    }
}

/// Stem darkening factor. Default = 40 (~16%) for improved thin-stem
/// legibility on dark backgrounds, similar to macOS Core Text and
/// FreeType's auto-hinter stem darkening. Range: 0 (off) to 255 (max).
pub(super) static STEM_DARKENING: core::sync::atomic::AtomicU8 =
    core::sync::atomic::AtomicU8::new(25);

/// Set stem darkening factor (0-255)
pub fn set_stem_darkening(factor: u8) {
    STEM_DARKENING.store(factor, core::sync::atomic::Ordering::Relaxed);
}

#[inline(always)]
pub(super) fn apply_stem_darkening(coverage: u8) -> u8 {
    if coverage == 0 || coverage == 255 {
        return coverage;
    }
    let dark = STEM_DARKENING.load(core::sync::atomic::Ordering::Relaxed) as u32;
    if dark == 0 {
        return coverage;
    }
    let c = coverage as u32;
    let boost = dark * (255 - c) / 255;
    (c + boost).min(255) as u8
}
