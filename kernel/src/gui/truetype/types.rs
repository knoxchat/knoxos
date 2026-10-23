//! Glyph outline and rasterized bitmap types shared by the parser and rasterizer.
use alloc::vec::Vec;

/// A single glyph outline with contours
#[derive(Clone)]
pub struct GlyphOutline {
    pub advance_width: u16,
    pub lsb: i16,
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
    pub contours: Vec<Contour>,
}

/// A closed contour (sequence of on-curve and off-curve points)
#[derive(Clone)]
pub struct Contour {
    pub points: Vec<GlyphPoint>,
}

#[derive(Clone, Copy)]
pub struct GlyphPoint {
    pub x: i32,
    pub y: i32,
    pub on_curve: bool,
}

/// Rasterized glyph bitmap
pub struct GlyphBitmap {
    pub width: usize,
    pub height: usize,
    pub bearing_x: i32,
    pub bearing_y: i32,
    pub advance: i32,
    /// Alpha coverage values (0-255), row-major
    pub pixels: Vec<u8>,
}
