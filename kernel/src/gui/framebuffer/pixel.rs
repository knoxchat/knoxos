/// RGBA color
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Pixel {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn from_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as u8,
            g: ((hex >> 8) & 0xFF) as u8,
            b: (hex & 0xFF) as u8,
            a: 255,
        }
    }

    pub const fn from_hex_alpha(hex: u32) -> Self {
        Self {
            r: ((hex >> 24) & 0xFF) as u8,
            g: ((hex >> 16) & 0xFF) as u8,
            b: ((hex >> 8) & 0xFF) as u8,
            a: (hex & 0xFF) as u8,
        }
    }

    /// Blend this pixel over another (alpha compositing)
    /// Uses fast (a*b + 128) >> 8 approximation for /255 (max error: 1 LSB)
    pub fn blend_over(self, below: Pixel) -> Pixel {
        if self.a == 255 {
            return self;
        }
        if self.a == 0 {
            return below;
        }
        let alpha = self.a as u16;
        let inv_alpha = 255 - alpha;
        Pixel {
            r: ((self.r as u16 * alpha + below.r as u16 * inv_alpha + 128) >> 8) as u8,
            g: ((self.g as u16 * alpha + below.g as u16 * inv_alpha + 128) >> 8) as u8,
            b: ((self.b as u16 * alpha + below.b as u16 * inv_alpha + 128) >> 8) as u8,
            a: 255,
        }
    }

    /// Linearly interpolate between two colors
    /// Uses fast (a*b + 128) >> 8 approximation for /255
    pub fn lerp(a: Pixel, b: Pixel, t: u8) -> Pixel {
        let t16 = t as u16;
        let inv_t = 255 - t16;
        Pixel {
            r: ((a.r as u16 * inv_t + b.r as u16 * t16 + 128) >> 8) as u8,
            g: ((a.g as u16 * inv_t + b.g as u16 * t16 + 128) >> 8) as u8,
            b: ((a.b as u16 * inv_t + b.b as u16 * t16 + 128) >> 8) as u8,
            a: 255,
        }
    }

    /// Multiply alpha of this pixel by a factor (0-255)
    pub fn with_alpha(self, alpha: u8) -> Pixel {
        Pixel::new(
            self.r,
            self.g,
            self.b,
            ((self.a as u16 * alpha as u16 + 128) >> 8) as u8,
        )
    }
}
