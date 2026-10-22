// SPDX-License-Identifier: MIT
//! Integer geometry and HSLA color for Knox GPUI.
//!
//! Type names follow Zed GPUI (`Pixels`, `Point`, `Bounds`, `Hsla`, …) as a
//! design reference (Apache-2.0). This file is new `no_std` code: [`Pixels`]
//! is `i32` (software raster / integer flex), and HSLA conversion uses `libm`.

use core::fmt;
use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Packed BGRA8888 color (`b, g, r, a` memory order), matching the software FB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bgra {
    pub b: u8,
    pub g: u8,
    pub r: u8,
    pub a: u8,
}

impl Bgra {
    pub const TRANSPARENT: Self = Self {
        b: 0,
        g: 0,
        r: 0,
        a: 0,
    };

    /// Opaque RGB. Arguments are red, green, blue (not memory order).
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { b, g, r, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { b, g, r, a }
    }

    /// Memory order: blue, green, red, alpha.
    pub const fn to_bytes(self) -> [u8; 4] {
        [self.b, self.g, self.r, self.a]
    }

    pub const fn from_bytes(bytes: [u8; 4]) -> Self {
        Self {
            b: bytes[0],
            g: bytes[1],
            r: bytes[2],
            a: bytes[3],
        }
    }

    /// Multiply alpha by a 0..=255 coverage (straight alpha; RGB unchanged).
    pub const fn modulate_alpha(self, coverage: u8) -> Self {
        if coverage == 255 {
            return self;
        }
        if coverage == 0 || self.a == 0 {
            return Self::TRANSPARENT;
        }
        let a = ((self.a as u32 * coverage as u32 + 127) / 255) as u8;
        Self { a, ..self }
    }

    /// Porter-Duff source-over. `self` is the source, `dst` is the destination.
    ///
    /// Straight (non-premultiplied) BGRA. An opaque source replaces `dst`; a
    /// transparent destination yields `self` (including partial source alpha).
    pub fn over(self, dst: Self) -> Self {
        if self.a == 0 {
            return dst;
        }
        if self.a == 255 || dst.a == 0 {
            return self;
        }
        let sa = self.a as u32;
        let da = dst.a as u32;
        let inv = 255 - sa;
        let out_a = sa + (da * inv + 127) / 255;
        if out_a == 0 {
            return Self::TRANSPARENT;
        }
        let ch = |sc: u8, dc: u8| -> u8 {
            let src_term = sc as u32 * sa;
            let dst_term = (dc as u32 * da * inv + 127) / 255;
            ((src_term + dst_term + out_a / 2) / out_a) as u8
        };
        Self {
            b: ch(self.b, dst.b),
            g: ch(self.g, dst.g),
            r: ch(self.r, dst.r),
            a: out_a as u8,
        }
    }
}

/// Device pixels. Integer, not GPUI’s `f32` logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Pixels(pub i32);

impl Pixels {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> i32 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

/// `px(12)` — integer pixel constructor (GPUI’s `px` is `f32`).
pub const fn px(value: i32) -> Pixels {
    Pixels(value)
}

impl From<i32> for Pixels {
    fn from(value: i32) -> Self {
        Self(value)
    }
}

impl From<Pixels> for i32 {
    fn from(value: Pixels) -> Self {
        value.0
    }
}

impl fmt::Display for Pixels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}px", self.0)
    }
}

impl Add for Pixels {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Pixels {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for Pixels {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for Pixels {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Neg for Pixels {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Mul<i32> for Pixels {
    type Output = Self;
    fn mul(self, rhs: i32) -> Self {
        Self(self.0 * rhs)
    }
}

impl Mul<Pixels> for i32 {
    type Output = Pixels;
    fn mul(self, rhs: Pixels) -> Pixels {
        Pixels(self * rhs.0)
    }
}

impl Div<i32> for Pixels {
    type Output = Self;
    fn div(self, rhs: i32) -> Self {
        Self(self.0 / rhs)
    }
}

/// 2D point in [`Pixels`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: Pixels,
    pub y: Pixels,
}

impl Point {
    pub const ZERO: Self = Self {
        x: Pixels::ZERO,
        y: Pixels::ZERO,
    };

    pub const fn new(x: Pixels, y: Pixels) -> Self {
        Self { x, y }
    }

    /// Convenience: `Point` from raw integer coordinates.
    pub const fn xy(x: i32, y: i32) -> Self {
        Self {
            x: Pixels(x),
            y: Pixels(y),
        }
    }

    pub fn min(self, other: Self) -> Self {
        Self {
            x: self.x.min(other.x),
            y: self.y.min(other.y),
        }
    }

    pub fn max(self, other: Self) -> Self {
        Self {
            x: self.x.max(other.x),
            y: self.y.max(other.y),
        }
    }
}

pub const fn point(x: Pixels, y: Pixels) -> Point {
    Point { x, y }
}

impl Add for Point {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Add<Size> for Point {
    type Output = Self;
    fn add(self, rhs: Size) -> Self {
        Self {
            x: self.x + rhs.width,
            y: self.y + rhs.height,
        }
    }
}

impl Sub for Point {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Sub<Size> for Point {
    type Output = Self;
    fn sub(self, rhs: Size) -> Self {
        Self {
            x: self.x - rhs.width,
            y: self.y - rhs.height,
        }
    }
}

impl AddAssign for Point {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

/// Width × height in [`Pixels`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Size {
    pub width: Pixels,
    pub height: Pixels,
}

impl Size {
    pub const ZERO: Self = Self {
        width: Pixels::ZERO,
        height: Pixels::ZERO,
    };

    pub const fn new(width: Pixels, height: Pixels) -> Self {
        Self { width, height }
    }

    pub const fn wh(width: i32, height: i32) -> Self {
        Self {
            width: Pixels(width),
            height: Pixels(height),
        }
    }

    pub const fn splat(value: Pixels) -> Self {
        Self {
            width: value,
            height: value,
        }
    }

    pub const fn is_empty(self) -> bool {
        self.width.0 <= 0 || self.height.0 <= 0
    }
}

pub const fn size(width: Pixels, height: Pixels) -> Size {
    Size { width, height }
}

impl Add for Size {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            width: self.width + rhs.width,
            height: self.height + rhs.height,
        }
    }
}

impl Sub for Size {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            width: self.width - rhs.width,
            height: self.height - rhs.height,
        }
    }
}

impl Mul<i32> for Size {
    type Output = Self;
    fn mul(self, rhs: i32) -> Self {
        Self {
            width: self.width * rhs,
            height: self.height * rhs,
        }
    }
}

/// Axis-aligned rectangle: origin (top-left) + size.
///
/// Hit tests are half-open: `[left, right) × [top, bottom)`, matching kernel `Rect`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Bounds {
    pub origin: Point,
    pub size: Size,
}

impl Bounds {
    pub const ZERO: Self = Self {
        origin: Point::ZERO,
        size: Size::ZERO,
    };

    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    pub const fn from_xywh(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            origin: Point::xy(x, y),
            size: Size::wh(width, height),
        }
    }

    pub fn from_corners(top_left: Point, bottom_right: Point) -> Self {
        Self {
            origin: top_left,
            size: Size {
                width: bottom_right.x - top_left.x,
                height: bottom_right.y - top_left.y,
            },
        }
    }

    pub fn centered_at(center: Point, size: Size) -> Self {
        Self {
            origin: Point {
                x: Pixels(center.x.0 - size.width.0 / 2),
                y: Pixels(center.y.0 - size.height.0 / 2),
            },
            size,
        }
    }

    pub const fn x(self) -> i32 {
        self.origin.x.0
    }

    pub const fn y(self) -> i32 {
        self.origin.y.0
    }

    pub const fn width(self) -> i32 {
        self.size.width.0
    }

    pub const fn height(self) -> i32 {
        self.size.height.0
    }

    pub const fn left(self) -> Pixels {
        self.origin.x
    }

    pub const fn top(self) -> Pixels {
        self.origin.y
    }

    pub fn right(self) -> Pixels {
        self.origin.x + self.size.width
    }

    pub fn bottom(self) -> Pixels {
        self.origin.y + self.size.height
    }

    pub fn bottom_right(self) -> Point {
        Point {
            x: self.right(),
            y: self.bottom(),
        }
    }

    pub fn center(self) -> Point {
        Point {
            x: Pixels(self.origin.x.0 + self.size.width.0 / 2),
            y: Pixels(self.origin.y.0 + self.size.height.0 / 2),
        }
    }

    pub const fn is_empty(self) -> bool {
        self.size.is_empty()
    }

    /// Half-open containment: the right and bottom edges are outside.
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.origin.x
            && p.x < self.origin.x + self.size.width
            && p.y >= self.origin.y
            && p.y < self.origin.y + self.size.height
    }

    pub fn intersects(self, other: Self) -> bool {
        self.origin.x < other.right()
            && self.right() > other.origin.x
            && self.origin.y < other.bottom()
            && self.bottom() > other.origin.y
    }

    /// Overlap. Empty (zero size) if the rectangles do not overlap.
    pub fn intersect(self, other: Self) -> Self {
        let origin = self.origin.max(other.origin);
        let br = self.bottom_right().min(other.bottom_right()).max(origin);
        Self::from_corners(origin, br)
    }

    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let origin = self.origin.min(other.origin);
        let br = self.bottom_right().max(other.bottom_right());
        Self::from_corners(origin, br)
    }

    /// Expand on all sides (`amount` may be negative to shrink).
    pub fn dilate(self, amount: Pixels) -> Self {
        Self {
            origin: Point {
                x: self.origin.x - amount,
                y: self.origin.y - amount,
            },
            size: Size {
                width: self.size.width + amount * 2,
                height: self.size.height + amount * 2,
            },
        }
    }

    pub fn inset(self, amount: Pixels) -> Self {
        self.dilate(-amount)
    }

    pub fn extend(self, edges: Edges) -> Self {
        Self {
            origin: Point {
                x: self.origin.x - edges.left,
                y: self.origin.y - edges.top,
            },
            size: Size {
                width: self.size.width + edges.left + edges.right,
                height: self.size.height + edges.top + edges.bottom,
            },
        }
    }

    pub fn translate(self, by: Point) -> Self {
        Self {
            origin: self.origin + by,
            size: self.size,
        }
    }
}

pub const fn bounds(origin: Point, size: Size) -> Bounds {
    Bounds { origin, size }
}

/// Top / right / bottom / left (padding, margin, border width).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Edges {
    pub top: Pixels,
    pub right: Pixels,
    pub bottom: Pixels,
    pub left: Pixels,
}

impl Edges {
    pub const ZERO: Self = Self {
        top: Pixels::ZERO,
        right: Pixels::ZERO,
        bottom: Pixels::ZERO,
        left: Pixels::ZERO,
    };

    pub const fn all(value: Pixels) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub const fn vh(vertical: Pixels, horizontal: Pixels) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    pub fn horizontal(self) -> Pixels {
        self.left + self.right
    }

    pub fn vertical(self) -> Pixels {
        self.top + self.bottom
    }

    pub fn is_zero(self) -> bool {
        self.top.is_zero() && self.right.is_zero() && self.bottom.is_zero() && self.left.is_zero()
    }

    pub fn max(self) -> Pixels {
        self.top.max(self.right).max(self.bottom).max(self.left)
    }
}

/// Per-corner values (typically border radii).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Corners {
    pub top_left: Pixels,
    pub top_right: Pixels,
    pub bottom_right: Pixels,
    pub bottom_left: Pixels,
}

impl Corners {
    pub const ZERO: Self = Self {
        top_left: Pixels::ZERO,
        top_right: Pixels::ZERO,
        bottom_right: Pixels::ZERO,
        bottom_left: Pixels::ZERO,
    };

    pub const fn all(value: Pixels) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }

    /// Round only the left (leading) corners — button-group start.
    pub const fn left(value: Pixels) -> Self {
        Self {
            top_left: value,
            bottom_left: value,
            top_right: Pixels::ZERO,
            bottom_right: Pixels::ZERO,
        }
    }

    /// Round only the right (trailing) corners — button-group end.
    pub const fn right(value: Pixels) -> Self {
        Self {
            top_right: value,
            bottom_right: value,
            top_left: Pixels::ZERO,
            bottom_left: Pixels::ZERO,
        }
    }

    pub const fn top(value: Pixels) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_left: Pixels::ZERO,
            bottom_right: Pixels::ZERO,
        }
    }

    pub const fn bottom(value: Pixels) -> Self {
        Self {
            bottom_left: value,
            bottom_right: value,
            top_left: Pixels::ZERO,
            top_right: Pixels::ZERO,
        }
    }

    pub fn max(self) -> Pixels {
        self.top_left
            .max(self.top_right)
            .max(self.bottom_right)
            .max(self.bottom_left)
    }

    /// True when every corner is a sharp 90° (no radius).
    pub const fn is_sharp(self) -> bool {
        self.top_left.0 <= 0
            && self.top_right.0 <= 0
            && self.bottom_right.0 <= 0
            && self.bottom_left.0 <= 0
    }

    /// CSS-style fit: scale radii down so adjacent corners do not overflow
    /// `size`. Negative radii become zero.
    pub fn clamp_to(self, size: Size) -> Self {
        let w = size.width.0.max(0) as f32;
        let h = size.height.0.max(0) as f32;
        let tl = self.top_left.0.max(0) as f32;
        let tr = self.top_right.0.max(0) as f32;
        let br = self.bottom_right.0.max(0) as f32;
        let bl = self.bottom_left.0.max(0) as f32;
        let mut scale = 1.0f32;
        if tl + tr > w && tl + tr > 0.0 {
            scale = scale.min(w / (tl + tr));
        }
        if bl + br > w && bl + br > 0.0 {
            scale = scale.min(w / (bl + br));
        }
        if tl + bl > h && tl + bl > 0.0 {
            scale = scale.min(h / (tl + bl));
        }
        if tr + br > h && tr + br > 0.0 {
            scale = scale.min(h / (tr + br));
        }
        Self {
            top_left: Pixels(libm::floorf(tl * scale) as i32),
            top_right: Pixels(libm::floorf(tr * scale) as i32),
            bottom_right: Pixels(libm::floorf(br * scale) as i32),
            bottom_left: Pixels(libm::floorf(bl * scale) as i32),
        }
    }
}

/// HSLA color. Each channel is in `0.0..=1.0` (hue is a turn, not degrees).
///
/// Conversion to packed BGRA follows the CSS Color Module Level 3 HSL algorithm
/// (public spec), with `libm` for wrap/round on `no_std`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hsla {
    pub h: f32,
    pub s: f32,
    pub l: f32,
    pub a: f32,
}

impl PartialEq for Hsla {
    fn eq(&self, other: &Self) -> bool {
        self.h.total_cmp(&other.h).is_eq()
            && self.s.total_cmp(&other.s).is_eq()
            && self.l.total_cmp(&other.l).is_eq()
            && self.a.total_cmp(&other.a).is_eq()
    }
}

/// Construct an [`Hsla`], clamping each channel to `0.0..=1.0`.
pub const fn hsla(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    Hsla {
        h: clamp01(h),
        s: clamp01(s),
        l: clamp01(l),
        a: clamp01(a),
    }
}

impl Hsla {
    pub const TRANSPARENT: Self = Self {
        h: 0.0,
        s: 0.0,
        l: 0.0,
        a: 0.0,
    };
    pub const BLACK: Self = Self {
        h: 0.0,
        s: 0.0,
        l: 0.0,
        a: 1.0,
    };
    pub const WHITE: Self = Self {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 1.0,
    };
    pub const RED: Self = Self {
        h: 0.0,
        s: 1.0,
        l: 0.5,
        a: 1.0,
    };
    pub const GREEN: Self = Self {
        h: 1.0 / 3.0,
        s: 1.0,
        l: 0.5,
        a: 1.0,
    };
    pub const BLUE: Self = Self {
        h: 2.0 / 3.0,
        s: 1.0,
        l: 0.5,
        a: 1.0,
    };
    pub const YELLOW: Self = Self {
        h: 1.0 / 6.0,
        s: 1.0,
        l: 0.5,
        a: 1.0,
    };
    pub const CYAN: Self = Self {
        h: 0.5,
        s: 1.0,
        l: 0.5,
        a: 1.0,
    };
    pub const MAGENTA: Self = Self {
        h: 5.0 / 6.0,
        s: 1.0,
        l: 0.5,
        a: 1.0,
    };

    pub const fn new(h: f32, s: f32, l: f32, a: f32) -> Self {
        hsla(h, s, l, a)
    }

    pub fn is_transparent(self) -> bool {
        self.a <= 0.0
    }

    pub fn is_opaque(self) -> bool {
        self.a >= 1.0
    }

    pub fn alpha(self, a: f32) -> Self {
        Self {
            a: clamp01(a),
            ..self
        }
    }

    pub fn opacity(self, factor: f32) -> Self {
        Self {
            a: clamp01(self.a * clamp01(factor)),
            ..self
        }
    }

    /// Raise lightness (`amount` in 0..=1 turns of L).
    pub fn lighten(self, amount: f32) -> Self {
        Self {
            l: clamp01(self.l + amount),
            ..self
        }
    }

    /// Lower lightness (`amount` in 0..=1 turns of L).
    pub fn darken(self, amount: f32) -> Self {
        Self {
            l: clamp01(self.l - amount),
            ..self
        }
    }

    /// Convert packed 8-bit sRGB into HSLA (hue in turns).
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self::from_rgba(r, g, b, 255)
    }

    pub fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        let r = r as f32 / 255.0;
        let g = g as f32 / 255.0;
        let b = b as f32 / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let l = (max + min) * 0.5;
        let a = a as f32 / 255.0;
        if delta == 0.0 {
            return Self {
                h: 0.0,
                s: 0.0,
                l,
                a,
            };
        }
        let denom = 1.0 - libm::fabsf(2.0 * l - 1.0);
        let s = if denom == 0.0 { 0.0 } else { delta / denom };
        let h6 = if max == r {
            rem_euclid_f32((g - b) / delta, 6.0)
        } else if max == g {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        };
        Self {
            h: h6 / 6.0,
            s,
            l,
            a,
        }
    }

    /// Packed BGRA8888 (`b, g, r, a` memory order), matching the software FB.
    pub fn to_bgra(self) -> Bgra {
        let (r, g, b) = hsl_to_rgb(self.h, self.s, self.l);
        Bgra {
            b: channel_u8(b),
            g: channel_u8(g),
            r: channel_u8(r),
            a: channel_u8(self.a),
        }
    }
}

impl From<Hsla> for Bgra {
    fn from(color: Hsla) -> Self {
        color.to_bgra()
    }
}

const fn clamp01(v: f32) -> f32 {
    if v < 0.0 {
        0.0
    } else if v > 1.0 {
        1.0
    } else {
        v
    }
}

fn wrap01(h: f32) -> f32 {
    let h = h - libm::floorf(h);
    if h >= 1.0 { 0.0 } else { h }
}

fn rem_euclid_f32(x: f32, rhs: f32) -> f32 {
    let r = libm::fmodf(x, rhs);
    if r < 0.0 { r + rhs } else { r }
}

/// CSS Color Module Level 3 `HOW TO RETURN hsl.to.rgb`.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let h = wrap01(h);
    let s = clamp01(s);
    let l = clamp01(l);
    let m2 = if l <= 0.5 {
        l * (s + 1.0)
    } else {
        l + s - l * s
    };
    let m1 = l * 2.0 - m2;
    (
        hue_to_rgb(m1, m2, h + 1.0 / 3.0),
        hue_to_rgb(m1, m2, h),
        hue_to_rgb(m1, m2, h - 1.0 / 3.0),
    )
}

fn hue_to_rgb(m1: f32, m2: f32, h: f32) -> f32 {
    let h = wrap01(h);
    if h * 6.0 < 1.0 {
        m1 + (m2 - m1) * h * 6.0
    } else if h * 2.0 < 1.0 {
        m2
    } else if h * 3.0 < 2.0 {
        m1 + (m2 - m1) * (2.0 / 3.0 - h) * 6.0
    } else {
        m1
    }
}

fn channel_u8(v: f32) -> u8 {
    let rounded = libm::roundf(clamp01(v) * 255.0);
    if rounded < 0.0 {
        0
    } else if rounded > 255.0 {
        255
    } else {
        rounded as u8
    }
}

#[cfg(test)]
mod tests {
    use super::{Bgra, Bounds, Corners, Edges, Hsla, Point, Size, bounds, hsla, point, px, size};

    #[test]
    fn pixels_arithmetic() {
        assert_eq!(px(10) + px(5), px(15));
        assert_eq!(px(10) - px(3), px(7));
        assert_eq!(px(4) * 3, px(12));
        assert_eq!(2 * px(7), px(14));
        assert_eq!(px(9) / 2, px(4));
        assert_eq!(-px(3), px(-3));
        assert_eq!(i32::from(px(8)), 8);
    }

    #[test]
    fn point_and_size_offset() {
        let p = Point::xy(4, 6);
        assert_eq!(p + Size::wh(2, 3), Point::xy(6, 9));
        assert_eq!(p + point(px(1), px(-1)), Point::xy(5, 5));
        assert_eq!(Size::splat(px(4)) * 2, Size::wh(8, 8));
        assert!(Size::wh(0, 10).is_empty());
        assert!(Size::wh(10, -1).is_empty());
    }

    #[test]
    fn bounds_half_open_contains() {
        let b = Bounds::from_xywh(8, 8, 16, 16);
        assert!(b.contains(Point::xy(8, 8)));
        assert!(b.contains(Point::xy(23, 23)));
        assert!(!b.contains(Point::xy(24, 8)));
        assert!(!b.contains(Point::xy(8, 24)));
        assert!(!b.contains(Point::xy(7, 8)));
        assert_eq!(b.right(), px(24));
        assert_eq!(b.bottom(), px(24));
        assert_eq!(b.center(), Point::xy(16, 16));
        assert_eq!(bounds(point(px(8), px(8)), size(px(16), px(16))), b);
    }

    #[test]
    fn bounds_intersect_union_inset() {
        let a = Bounds::from_xywh(0, 0, 10, 10);
        let b = Bounds::from_xywh(5, 5, 10, 10);
        assert!(a.intersects(b));
        assert_eq!(a.intersect(b), Bounds::from_xywh(5, 5, 5, 5));
        assert_eq!(a.union(b), Bounds::from_xywh(0, 0, 15, 15));

        let miss = Bounds::from_xywh(20, 20, 4, 4);
        assert!(!a.intersects(miss));
        assert!(a.intersect(miss).is_empty());

        assert_eq!(a.dilate(px(2)), Bounds::from_xywh(-2, -2, 14, 14));
        assert_eq!(a.inset(px(2)), Bounds::from_xywh(2, 2, 6, 6));
        assert_eq!(
            a.extend(Edges::vh(px(1), px(3))),
            Bounds::from_xywh(-3, -1, 16, 12)
        );
        assert_eq!(
            a.translate(Point::xy(4, -2)),
            Bounds::from_xywh(4, -2, 10, 10)
        );
        assert_eq!(
            Bounds::centered_at(Point::xy(10, 10), Size::wh(8, 6)),
            Bounds::from_xywh(6, 7, 8, 6)
        );
    }

    #[test]
    fn edges_and_corners() {
        let e = Edges::all(px(3));
        assert_eq!(e.horizontal(), px(6));
        assert_eq!(e.vertical(), px(6));
        let c = Corners::all(px(4));
        assert_eq!(c.max(), px(4));
        assert!(!c.is_sharp());
        assert!(Corners::ZERO.is_sharp());
        assert_eq!(
            c,
            Corners {
                top_left: px(4),
                top_right: px(4),
                bottom_right: px(4),
                bottom_left: px(4),
            }
        );
        assert_eq!(Corners::all(px(4)).clamp_to(Size::wh(16, 16)), c);
        assert_eq!(
            Corners::all(px(10)).clamp_to(Size::wh(10, 10)),
            Corners::all(px(5))
        );
        assert!(Corners::all(px(8)).clamp_to(Size::wh(1, 1)).is_sharp());
    }

    #[test]
    fn bgra_modulate_and_over() {
        let red = Bgra::rgb(255, 0, 0);
        assert_eq!(red.modulate_alpha(255), red);
        assert_eq!(red.modulate_alpha(0), Bgra::TRANSPARENT);
        assert_eq!(red.modulate_alpha(128), Bgra::rgba(255, 0, 0, 128));
        assert_eq!(red.over(Bgra::TRANSPARENT), red);
        assert_eq!(Bgra::TRANSPARENT.over(red), red);
        assert_eq!(red.over(Bgra::rgb(0, 255, 0)), red);
        let fringe = Bgra::rgba(255, 0, 0, 128);
        assert_eq!(fringe.over(Bgra::TRANSPARENT), fringe);
    }

    /// CSS-known HSL primaries → packed BGRA (memory order `b,g,r,a`).
    #[test]
    fn known_hsla_converts_to_bgra() {
        let cases = [
            (Hsla::RED, Bgra::rgb(255, 0, 0), [0, 0, 255, 255]),
            (Hsla::GREEN, Bgra::rgb(0, 255, 0), [0, 255, 0, 255]),
            (Hsla::BLUE, Bgra::rgb(0, 0, 255), [255, 0, 0, 255]),
            (Hsla::YELLOW, Bgra::rgb(255, 255, 0), [0, 255, 255, 255]),
            (Hsla::CYAN, Bgra::rgb(0, 255, 255), [255, 255, 0, 255]),
            (Hsla::MAGENTA, Bgra::rgb(255, 0, 255), [255, 0, 255, 255]),
            (Hsla::BLACK, Bgra::rgb(0, 0, 0), [0, 0, 0, 255]),
            (Hsla::WHITE, Bgra::rgb(255, 255, 255), [255, 255, 255, 255]),
            (
                hsla(0.0, 0.0, 0.5, 1.0),
                Bgra::rgb(128, 128, 128),
                [128, 128, 128, 255],
            ),
            (
                hsla(0.0, 1.0, 0.25, 1.0),
                Bgra::rgb(128, 0, 0),
                [0, 0, 128, 255],
            ),
            (
                hsla(0.0, 1.0, 0.5, 0.5),
                Bgra::rgba(255, 0, 0, 128),
                [0, 0, 255, 128],
            ),
            (Hsla::TRANSPARENT, Bgra::TRANSPARENT, [0, 0, 0, 0]),
        ];

        for (color, expected, bytes) in cases {
            let bgra = color.to_bgra();
            assert_eq!(bgra, expected, "hsla={color:?}");
            assert_eq!(bgra.to_bytes(), bytes, "hsla={color:?} memory order");
        }
    }

    #[test]
    fn hsla_hue_wraps_full_turn_to_red() {
        // h = 1.0 is a full turn, same as 0 (red). Direct field, not clamped ctor.
        let wrapped = Hsla {
            h: 1.0,
            s: 1.0,
            l: 0.5,
            a: 1.0,
        };
        assert_eq!(wrapped.to_bgra(), Bgra::rgb(255, 0, 0));
        assert_eq!(hsla(1.25, 1.0, 0.5, 1.0).to_bgra(), Bgra::rgb(255, 0, 0));
    }

    #[test]
    fn hsla_constructor_clamps() {
        let c = hsla(-1.0, 2.0, 2.0, 2.0);
        assert_eq!(c, hsla(0.0, 1.0, 1.0, 1.0));
        assert_eq!(c.to_bgra(), Bgra::rgb(255, 255, 255));
    }

    #[test]
    fn from_rgb_primaries_roundtrip_bgra() {
        for (r, g, b) in [
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (255, 255, 255),
            (0, 0, 0),
        ] {
            assert_eq!(
                Hsla::from_rgb(r, g, b).to_bgra(),
                Bgra::rgb(r, g, b),
                "rgb({r},{g},{b})"
            );
        }
    }

    #[test]
    fn aurora_coral_from_rgb_to_bgra() {
        // Theme accent (kernel `theme.rs` Aurora Dark `accent_primary`).
        let coral = Hsla::from_rgb(232, 121, 100).to_bgra();
        assert_eq!(coral, Bgra::rgb(232, 121, 100));
        assert_eq!(coral.to_bytes(), [100, 121, 232, 255]);
    }

    #[test]
    fn rgba_swap_cannot_satisfy_red_golden() {
        let red = Hsla::RED.to_bgra();
        assert_eq!(red.to_bytes(), [0, 0, 255, 255]);
        assert_ne!(red.to_bytes(), [255, 0, 0, 255]);
    }
}
