/// UI Scale System — Resolution-aware scaling for window decorations and widgets
///
/// Automatically computes a UI scale factor based on the current display resolution.
/// All window chrome (title bar, buttons, borders, grip handles) use these scaled
/// values so that controls remain fully functional and properly sized at any resolution
/// from 1024×768 through 3840×2160.
///
/// Inspired by winit's DPI handling and egui's `pixels_per_point`.
///
/// Scale tiers:
///   - 1024×768  .. 1366×768  → 1.0x  (base)
///   - 1440×900  .. 1920×1080 → 1.0x  (reference design resolution)
///   - 2560×1440              → 1.25x
///   - 2560×1600              → 1.3x
///   - 3840×2160              → 1.75x
use core::sync::atomic::{AtomicU32, Ordering};

/// UI scale factor stored as fixed-point 16.16 (65536 = 1.0x).
static UI_SCALE_FP: AtomicU32 = AtomicU32::new(65536);

/// Cached screen width for scale computation.
static SCREEN_W: AtomicU32 = AtomicU32::new(1920);
/// Cached screen height for scale computation.
static SCREEN_H: AtomicU32 = AtomicU32::new(1080);

// ─── Base (1.0x) design constants ────────────────────────────────────
// These are the pixel values at 1920×1080. Everything else is derived by
// multiplying by the scale factor.

/// Base title bar height at 1.0x
const BASE_TITLE_BAR_HEIGHT: u32 = 38;
/// Base window control button width at 1.0x
const BASE_BTN_WIDTH: u32 = 32;
/// Base window control button height at 1.0x
const BASE_BTN_HEIGHT: u32 = 26;
/// Base gap between buttons at 1.0x
const BASE_BTN_GAP: u32 = 2;
/// Base button right margin from window edge at 1.0x
const BASE_BTN_MARGIN_RIGHT: u32 = 6;
/// Base corner radius for Normal windows at 1.0x
const BASE_PORTAL_RADIUS: u32 = 12;
/// Base resize border grab zone at 1.0x
const BASE_RESIZE_BORDER: u32 = 5;
/// Base minimum window width at 1.0x
const BASE_MIN_WIDTH: u32 = 240;
/// Base minimum window height at 1.0x
const BASE_MIN_HEIGHT: u32 = 120;
/// Base glyph cross half-size at 1.0x
const BASE_GLYPH_HALF: u32 = 4;
/// Base minimize dash half-width at 1.0x
const BASE_DASH_HALF: u32 = 5;
/// Base maximize box half-size at 1.0x
const BASE_MAX_BOX_HALF: u32 = 4;
/// Minimum visible pixels on screen to prevent dragging off at 1.0x
const BASE_MIN_VISIBLE_PX: u32 = 40;
/// Taskbar height base at 1.0x
const BASE_TASKBAR_HEIGHT: u32 = 54;
/// Base title left padding at 1.0x
const BASE_TITLE_PAD_LEFT: u32 = 14;

/// Update the UI scale factor based on the current screen resolution.
/// Call this whenever the display resolution changes.
pub fn update_scale(width: u32, height: u32) {
    SCREEN_W.store(width, Ordering::Relaxed);
    SCREEN_H.store(height, Ordering::Relaxed);

    // Compute scale factor based on the effective DPI relative to 1920×1080.
    // We use the height as the primary driver since vertical space determines
    // how large controls feel. Width is used as a secondary signal for
    // ultra-wide or small displays.
    let scale_fp = compute_scale_fp(width, height);
    UI_SCALE_FP.store(scale_fp, Ordering::Relaxed);
}

/// Compute the fixed-point 16.16 scale factor for a given resolution.
fn compute_scale_fp(w: u32, h: u32) -> u32 {
    // Reference: 1920×1080 = 1.0x (65536)
    // Scale proportionally to vertical resolution.
    //
    // CRITICAL: We now scale DOWN for sub-1080p resolutions too.
    // Without this, switching from 1920×1080 → 1280×720 keeps UI elements
    // at the same absolute pixel size, which makes buttons overflow their
    // containers and hit-test areas misalign with rendered graphics.
    // This matches winit's behavior: scale_factor = screen_height / reference_height.
    let scale_1000 = if h < 768 {
        // Tiny screens: floor at 0.7x to keep things usable
        700u32
    } else if h < 1080 {
        // Sub-1080p: scale DOWN proportionally.
        // 768p → 0.71x, 900p → 0.83x, 1024p → 0.95x, 1080p → 1.0x
        (h as u64 * 1000 / 1080) as u32
    } else if h == 1080 {
        // Reference resolution: exactly 1.0x
        1000u32
    } else if h <= 1200 {
        // Gentle scale for 1200p (1.0x – 1.1x)
        1000 + (h - 1080) * 100 / 120 // 1000..1100
    } else if h <= 1440 {
        // 1.1x – 1.25x
        1100 + (h - 1200) * 150 / 240 // 1100..1250
    } else if h <= 1600 {
        // 1.25x – 1.35x
        1250 + (h - 1440) * 100 / 160 // 1250..1350
    } else if h <= 2160 {
        // 1.35x – 1.75x
        1350 + (h - 1600) * 400 / 560 // 1350..1750
    } else {
        // 4K+ : cap at 2.0x
        let s = 1750 + (h.saturating_sub(2160)) * 250 / 1000;
        s.min(2000)
    };

    // Convert from ×1000 to fixed-point 16.16
    (scale_1000 as u64 * 65536 / 1000) as u32
}

/// Scale a base pixel value by the current UI scale factor.
/// Uses `libm::round()` for correct rounding at fractional scales (1.25×, 1.5×).
/// Returns the scaled value (always ≥ 1).
#[inline]
pub fn scale(base: u32) -> u32 {
    super::dpi::scale_pixel(base, scale_factor())
}

/// Scale a signed pixel value by the current UI scale factor.
/// Uses `libm::round()` for correct rounding.
#[inline]
pub fn scale_i(base: i32) -> i32 {
    super::dpi::scale_pixel_i(base, scale_factor())
}

/// Get the current UI scale as a percentage (100 = 1.0x, 125 = 1.25x, etc.)
pub fn scale_percent() -> u32 {
    let fp = UI_SCALE_FP.load(Ordering::Relaxed) as u64;
    (fp * 100 / 65536) as u32
}

/// Get the current scale factor as an f64 (1.0 = 100%, 1.25 = 125%, etc.)
/// This is the winit-compatible API — use this for DPI-aware calculations.
#[inline]
pub fn scale_factor() -> f64 {
    let fp = UI_SCALE_FP.load(Ordering::Relaxed);
    fp as f64 / 65536.0
}

/// Get the raw fixed-point 16.16 scale factor
pub fn scale_fp() -> u32 {
    UI_SCALE_FP.load(Ordering::Relaxed)
}

// ═══════════════════════════════════════════════════════════════════════
// SCALED DECORATION ACCESSORS
// ═══════════════════════════════════════════════════════════════════════
// These replace the old `pub const` values in window.rs. Every call site
// that previously used e.g. `TITLE_BAR_HEIGHT` now calls `title_bar_height()`.

/// Scaled title bar height
#[inline]
pub fn title_bar_height() -> u32 {
    scale(BASE_TITLE_BAR_HEIGHT)
}

/// Scaled window control button width
#[inline]
pub fn btn_width() -> u32 {
    scale(BASE_BTN_WIDTH)
}

/// Scaled window control button height
#[inline]
pub fn btn_height() -> u32 {
    scale(BASE_BTN_HEIGHT)
}

/// Scaled gap between window buttons
#[inline]
pub fn btn_gap() -> u32 {
    scale(BASE_BTN_GAP)
}

/// Scaled button right margin from window edge
#[inline]
pub fn btn_margin_right() -> u32 {
    scale(BASE_BTN_MARGIN_RIGHT)
}

/// Scaled corner radius for Normal-state windows
#[inline]
pub fn portal_radius() -> u32 {
    scale(BASE_PORTAL_RADIUS)
}

/// Scaled resize border grab zone
#[inline]
pub fn resize_border() -> i32 {
    scale(BASE_RESIZE_BORDER) as i32
}

/// Scaled minimum window width
#[inline]
pub fn min_width() -> u32 {
    scale(BASE_MIN_WIDTH)
}

/// Scaled minimum window height
#[inline]
pub fn min_height() -> u32 {
    scale(BASE_MIN_HEIGHT)
}

/// Scaled glyph half-size for close button cross
#[inline]
pub fn glyph_half() -> i32 {
    scale(BASE_GLYPH_HALF) as i32
}

/// Scaled minimize dash half-width
#[inline]
pub fn dash_half() -> i32 {
    scale(BASE_DASH_HALF) as i32
}

/// Scaled maximize box half-size
#[inline]
pub fn max_box_half() -> i32 {
    scale(BASE_MAX_BOX_HALF) as i32
}

/// Scaled minimum visible pixels
#[inline]
pub fn min_visible_px() -> i32 {
    scale(BASE_MIN_VISIBLE_PX) as i32
}

/// Scaled taskbar height
#[inline]
pub fn taskbar_height() -> u32 {
    scale(BASE_TASKBAR_HEIGHT)
}

/// Scaled title left padding
#[inline]
pub fn title_pad_left() -> i32 {
    scale(BASE_TITLE_PAD_LEFT) as i32
}
