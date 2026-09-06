//! DPI-aware Pixel Coordinate System — Adapted from winit's DPI crate
//!
//! Provides proper logical ↔ physical pixel conversion with correct rounding,
//! using `libm::round` for `no_std`-compatible float→int conversion that rounds
//! away from zero (not truncating like `as i32` would).
//!
//! # Why This Matters for Font Rendering
//!
//! Without proper rounding, pixel positions drift by ±1px causing:
//!   - Misaligned glyph baselines (text looks jagged vertically)
//!   - Uneven character spacing (some gaps wider than others)
//!   - Sub-pixel shimmer when scrolling
//!
//! The winit DPI approach solves this by always rounding through `libm::round()`
//! instead of casting with `as`, which truncates toward zero.
//!
//! # Usage
//!
//! ```
//! // Scale a logical 38px title bar to physical pixels at 1.25× scale
//! let physical = LogicalUnit::new(38.0).to_physical::<u32>(1.25);
//! assert_eq!(physical, PhysicalUnit::new(48)); // round(47.5) = 48, not 47
//! ```///
/// Round f64 to nearest integer, rounding away from zero.
/// Uses libm::round for `no_std` compatibility — this is the same approach
/// as winit/dpi/src/libm.rs but using the libm crate we already depend on.
#[inline]
fn round(f: f64) -> f64 {
    libm::round(f)
}

/// Trait for pixel coordinate types that can be converted to/from f64.
/// Integer types round properly (via `libm::round`), float types pass through.
///
/// This is directly adapted from winit's `Pixel` trait.
pub trait Pixel: Copy + Into<f64> {
    fn from_f64(f: f64) -> Self;
    #[inline]
    fn cast<P: Pixel>(self) -> P {
        P::from_f64(self.into())
    }
}

// Integer pixel implementations — all use proper rounding
macro_rules! pixel_int_impl {
    ($($t:ty),*) => {$(
        impl Pixel for $t {
            #[inline]
            fn from_f64(f: f64) -> Self {
                round(f) as $t
            }
        }
    )*}
}

pixel_int_impl!(u8, u16, u32, i8, i16, i32);

impl Pixel for f32 {
    #[inline]
    fn from_f64(f: f64) -> Self {
        f as f32
    }
}
impl Pixel for f64 {
    #[inline]
    fn from_f64(f: f64) -> Self {
        f
    }
}

/// Validate that a scale factor is a positive, finite number.
#[inline]
pub fn validate_scale_factor(scale_factor: f64) -> bool {
    scale_factor.is_sign_positive() && scale_factor.is_normal()
}

// ═══════════════════════════════════════════════════════════════════════
// Logical / Physical Size
// ═══════════════════════════════════════════════════════════════════════

/// A size in logical (DPI-independent) pixels.
/// At scale 1.0, logical == physical. At scale 2.0, logical 10 == physical 20.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LogicalSize<P> {
    pub width: P,
    pub height: P,
}

impl<P> LogicalSize<P> {
    #[inline]
    pub const fn new(width: P, height: P) -> Self {
        LogicalSize { width, height }
    }
}

impl<P: Pixel> LogicalSize<P> {
    /// Convert to physical pixels using the given scale factor.
    #[inline]
    pub fn to_physical<X: Pixel>(&self, scale_factor: f64) -> PhysicalSize<X> {
        let width = self.width.into() * scale_factor;
        let height = self.height.into() * scale_factor;
        PhysicalSize::new(X::from_f64(width), X::from_f64(height))
    }

    /// Cast to a different pixel type.
    #[inline]
    pub fn cast<X: Pixel>(&self) -> LogicalSize<X> {
        LogicalSize {
            width: self.width.cast(),
            height: self.height.cast(),
        }
    }
}

/// A size in physical (device) pixels.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct PhysicalSize<P> {
    pub width: P,
    pub height: P,
}

impl<P> PhysicalSize<P> {
    #[inline]
    pub const fn new(width: P, height: P) -> Self {
        PhysicalSize { width, height }
    }
}

impl<P: Pixel> PhysicalSize<P> {
    /// Convert to logical pixels by dividing by the scale factor.
    #[inline]
    pub fn to_logical<X: Pixel>(&self, scale_factor: f64) -> LogicalSize<X> {
        let width = self.width.into() / scale_factor;
        let height = self.height.into() / scale_factor;
        LogicalSize::new(X::from_f64(width), X::from_f64(height))
    }

    /// Cast to a different pixel type.
    #[inline]
    pub fn cast<X: Pixel>(&self) -> PhysicalSize<X> {
        PhysicalSize {
            width: self.width.cast(),
            height: self.height.cast(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Logical / Physical Position
// ═══════════════════════════════════════════════════════════════════════

/// A position in logical (DPI-independent) pixels.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LogicalPosition<P> {
    pub x: P,
    pub y: P,
}

impl<P> LogicalPosition<P> {
    #[inline]
    pub const fn new(x: P, y: P) -> Self {
        LogicalPosition { x, y }
    }
}

impl<P: Pixel> LogicalPosition<P> {
    /// Convert to physical pixels using the given scale factor.
    #[inline]
    pub fn to_physical<X: Pixel>(&self, scale_factor: f64) -> PhysicalPosition<X> {
        let x = self.x.into() * scale_factor;
        let y = self.y.into() * scale_factor;
        PhysicalPosition::new(X::from_f64(x), X::from_f64(y))
    }

    /// Cast to a different pixel type.
    #[inline]
    pub fn cast<X: Pixel>(&self) -> LogicalPosition<X> {
        LogicalPosition {
            x: self.x.cast(),
            y: self.y.cast(),
        }
    }
}

/// A position in physical (device) pixels.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PhysicalPosition<P> {
    pub x: P,
    pub y: P,
}

impl<P> PhysicalPosition<P> {
    #[inline]
    pub const fn new(x: P, y: P) -> Self {
        PhysicalPosition { x, y }
    }
}

impl<P: Pixel> PhysicalPosition<P> {
    /// Convert to logical pixels by dividing by the scale factor.
    #[inline]
    pub fn to_logical<X: Pixel>(&self, scale_factor: f64) -> LogicalPosition<X> {
        let x = self.x.into() / scale_factor;
        let y = self.y.into() / scale_factor;
        LogicalPosition::new(X::from_f64(x), X::from_f64(y))
    }

    /// Cast to a different pixel type.
    #[inline]
    pub fn cast<X: Pixel>(&self) -> PhysicalPosition<X> {
        PhysicalPosition {
            x: self.x.cast(),
            y: self.y.cast(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Logical Unit (single value)
// ═══════════════════════════════════════════════════════════════════════

/// A single logical pixel measurement (width, height, margin, etc.)
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LogicalUnit<P>(pub P);

impl<P> LogicalUnit<P> {
    #[inline]
    pub const fn new(v: P) -> Self {
        LogicalUnit(v)
    }
}

impl<P: Pixel> LogicalUnit<P> {
    /// Convert to physical pixels.
    #[inline]
    pub fn to_physical<X: Pixel>(&self, scale_factor: f64) -> PhysicalUnit<X> {
        PhysicalUnit(X::from_f64(self.0.into() * scale_factor))
    }

    /// Cast to a different pixel type.
    #[inline]
    pub fn cast<X: Pixel>(&self) -> LogicalUnit<X> {
        LogicalUnit(self.0.cast())
    }
}

/// A single physical pixel measurement.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PhysicalUnit<P>(pub P);

impl<P> PhysicalUnit<P> {
    #[inline]
    pub const fn new(v: P) -> Self {
        PhysicalUnit(v)
    }
}

impl<P: Pixel> PhysicalUnit<P> {
    /// Convert to logical pixels.
    #[inline]
    pub fn to_logical<X: Pixel>(&self, scale_factor: f64) -> LogicalUnit<X> {
        LogicalUnit(X::from_f64(self.0.into() / scale_factor))
    }

    /// Cast to a different pixel type.
    #[inline]
    pub fn cast<X: Pixel>(&self) -> PhysicalUnit<X> {
        PhysicalUnit(self.0.cast())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Helper: DPI-aware pixel rounding
// ═══════════════════════════════════════════════════════════════════════

/// Scale an integer pixel value by a floating-point scale factor, with correct rounding.
/// This is the replacement for the old fixed-point `(base * fp + 32768) >> 16` approach.
///
/// Uses `libm::round()` instead of `as u32` truncation, which is critical for:
///   - Even glyph spacing at fractional scales (1.25×, 1.5×)
///   - Correct baseline alignment across a line of text
///   - Consistent button/margin sizing (no ±1px jitter)
#[inline]
pub fn scale_pixel(base: u32, scale_factor: f64) -> u32 {
    let scaled = base as f64 * scale_factor;
    (round(scaled) as u32).max(1)
}

/// Scale a signed integer pixel value with correct rounding.
#[inline]
pub fn scale_pixel_i(base: i32, scale_factor: f64) -> i32 {
    let scaled = base as f64 * scale_factor;
    round(scaled) as i32
}

/// Convert a physical pixel value to a logical pixel value, properly rounded.
#[inline]
pub fn to_logical(physical: u32, scale_factor: f64) -> u32 {
    if scale_factor <= 0.0 {
        return physical;
    }
    round(physical as f64 / scale_factor) as u32
}

/// Convert a physical signed pixel value to logical.
#[inline]
pub fn to_logical_i(physical: i32, scale_factor: f64) -> i32 {
    if scale_factor <= 0.0 {
        return physical;
    }
    round(physical as f64 / scale_factor) as i32
}
