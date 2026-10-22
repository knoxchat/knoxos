use alloc::string::String;
use spin::Mutex;

use super::hdr::ColorSpace;

/// ICC profile data
#[derive(Debug, Clone)]
pub struct IccProfile {
    pub name: String,
    pub color_space: ColorSpace,
    /// 3×3 matrix for color space conversion (row-major)
    pub matrix: [f32; 9],
    /// TRC (Tone Response Curve) gamma value
    pub gamma: f32,
    /// Profile size in bytes
    pub size: usize,
}

impl IccProfile {
    /// sRGB standard profile
    pub fn srgb() -> Self {
        Self {
            name: String::from("sRGB IEC61966-2.1"),
            color_space: ColorSpace::Srgb,
            matrix: [
                0.4124564, 0.3575761, 0.1804375, 0.2126729, 0.7151522, 0.0721750, 0.0193339,
                0.119_192, 0.9503041,
            ],
            gamma: 2.2,
            size: 0,
        }
    }

    /// Display P3 profile
    pub fn display_p3() -> Self {
        Self {
            name: String::from("Display P3"),
            color_space: ColorSpace::DciP3,
            matrix: [
                0.4865709, 0.2656677, 0.1982173, 0.2289746, 0.6917385, 0.0792869, 0.0000000,
                0.0451134, 1.0439444,
            ],
            gamma: 2.2,
            size: 0,
        }
    }

    /// Apply color profile transformation to a pixel (r,g,b as 0.0-1.0)
    pub fn transform(&self, r: f32, g: f32, b: f32) -> (f32, f32, f32) {
        let out_r = self.matrix[0] * r + self.matrix[1] * g + self.matrix[2] * b;
        let out_g = self.matrix[3] * r + self.matrix[4] * g + self.matrix[5] * b;
        let out_b = self.matrix[6] * r + self.matrix[7] * g + self.matrix[8] * b;
        (
            out_r.clamp(0.0, 1.0),
            out_g.clamp(0.0, 1.0),
            out_b.clamp(0.0, 1.0),
        )
    }
}

lazy_static::lazy_static! {
    pub static ref DISPLAY_PROFILE: Mutex<IccProfile> = Mutex::new(IccProfile::srgb());
}
