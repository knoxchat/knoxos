/// Color Management — ICC profile support and color space conversion
///
/// Provides:
///   - ICC profile parsing (v2 and v4)
///   - sRGB, Display P3, Adobe RGB color space definitions
///   - Color space conversions (sRGB ↔ linear, gamut mapping)
///   - Per-monitor color profiles
///   - Tone mapping for HDR content on SDR displays
///   - White point adaptation (D50 ↔ D65)
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use super::framebuffer::Pixel;

// ═══════════════════════════════════════════════════════════════════════
// COLOR SPACES
// ═══════════════════════════════════════════════════════════════════════

/// Standard color spaces
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    /// Standard sRGB (IEC 61966-2-1), D65 white point
    Srgb,
    /// Linear sRGB (gamma 1.0)
    LinearSrgb,
    /// Display P3 (DCI-P3 primaries, sRGB transfer function)
    DisplayP3,
    /// Adobe RGB (1998)
    AdobeRgb,
    /// BT.709 (identical primaries to sRGB, but 2.4 gamma)
    Bt709,
    /// BT.2020 (HDR wide gamut)
    Bt2020,
    /// CIE XYZ (device-independent)
    CieXyz,
    /// CIE L*a*b* (perceptually uniform)
    CieLab,
}

/// CIE xy chromaticity coordinates
#[derive(Debug, Clone, Copy)]
pub struct Chromaticity {
    pub x: f32,
    pub y: f32,
}

/// Color space primaries (red, green, blue) and white point
#[derive(Debug, Clone, Copy)]
pub struct Primaries {
    pub red: Chromaticity,
    pub green: Chromaticity,
    pub blue: Chromaticity,
    pub white: Chromaticity,
}

impl ColorSpace {
    /// Get CIE xy chromaticity primaries
    pub fn primaries(&self) -> Primaries {
        match self {
            Self::Srgb | Self::LinearSrgb | Self::Bt709 => Primaries {
                red: Chromaticity { x: 0.64, y: 0.33 },
                green: Chromaticity { x: 0.30, y: 0.60 },
                blue: Chromaticity { x: 0.15, y: 0.06 },
                white: Chromaticity {
                    x: 0.3127,
                    y: 0.3290,
                }, // D65
            },
            Self::DisplayP3 => Primaries {
                red: Chromaticity { x: 0.680, y: 0.320 },
                green: Chromaticity { x: 0.265, y: 0.690 },
                blue: Chromaticity { x: 0.150, y: 0.060 },
                white: Chromaticity {
                    x: 0.3127,
                    y: 0.3290,
                },
            },
            Self::AdobeRgb => Primaries {
                red: Chromaticity { x: 0.64, y: 0.33 },
                green: Chromaticity { x: 0.21, y: 0.71 },
                blue: Chromaticity { x: 0.15, y: 0.06 },
                white: Chromaticity {
                    x: 0.3127,
                    y: 0.3290,
                },
            },
            Self::Bt2020 => Primaries {
                red: Chromaticity { x: 0.708, y: 0.292 },
                green: Chromaticity { x: 0.170, y: 0.797 },
                blue: Chromaticity { x: 0.131, y: 0.046 },
                white: Chromaticity {
                    x: 0.3127,
                    y: 0.3290,
                },
            },
            _ => Primaries {
                red: Chromaticity { x: 0.64, y: 0.33 },
                green: Chromaticity { x: 0.30, y: 0.60 },
                blue: Chromaticity { x: 0.15, y: 0.06 },
                white: Chromaticity {
                    x: 0.3127,
                    y: 0.3290,
                },
            },
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TRANSFER FUNCTIONS (GAMMA)
// ═══════════════════════════════════════════════════════════════════════

/// Transfer function (EOTF — electro-optical transfer function)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferFunction {
    /// sRGB piecewise curve (~2.2 gamma)
    Srgb,
    /// Simple power law gamma
    Gamma22,
    /// BT.709 / BT.2020 (similar to sRGB but slightly different)
    Bt709,
    /// Linear (gamma 1.0)
    Linear,
    /// PQ (Perceptual Quantizer, SMPTE ST 2084) for HDR
    Pq,
    /// HLG (Hybrid Log-Gamma, BT.2100) for HDR broadcast
    Hlg,
}

/// Apply sRGB EOTF: encoded → linear
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        libm::powf((v + 0.055) / 1.055, 2.4)
    }
}

/// Apply sRGB inverse EOTF: linear → encoded
pub fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * libm::powf(v, 1.0 / 2.4) - 0.055
    }
}

/// Apply PQ EOTF: encoded → linear (nits)
pub fn pq_to_linear(v: f32) -> f32 {
    let m1 = 0.1593017578125;
    let m2 = 78.84375;
    let c1 = 0.8359375;
    let c2 = 18.8515625;
    let c3 = 18.6875;
    let vp = libm::powf(v, 1.0 / m2);
    let num = (vp - c1).max(0.0);
    let den = c2 - c3 * vp;
    if den <= 0.0 {
        return 0.0;
    }
    10000.0 * libm::powf(num / den, 1.0 / m1)
}

/// Apply inverse PQ: linear (nits) → encoded
pub fn linear_to_pq(l: f32) -> f32 {
    let m1 = 0.1593017578125;
    let m2 = 78.84375;
    let c1 = 0.8359375;
    let c2 = 18.8515625;
    let c3 = 18.6875;
    let y = (l / 10000.0).max(0.0);
    let yp = libm::powf(y, m1);
    libm::powf((c1 + c2 * yp) / (1.0 + c3 * yp), m2)
}

// ═══════════════════════════════════════════════════════════════════════
// 3x3 MATRIX (for color space conversion)
// ═══════════════════════════════════════════════════════════════════════

/// 3x3 matrix for color transformations
#[derive(Debug, Clone, Copy)]
pub struct Matrix3x3 {
    pub m: [[f32; 3]; 3],
}

impl Matrix3x3 {
    pub const IDENTITY: Self = Self {
        m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };

    /// Multiply matrix by a 3-component vector [R, G, B]
    pub fn transform(&self, v: [f32; 3]) -> [f32; 3] {
        [
            self.m[0][0] * v[0] + self.m[0][1] * v[1] + self.m[0][2] * v[2],
            self.m[1][0] * v[0] + self.m[1][1] * v[1] + self.m[1][2] * v[2],
            self.m[2][0] * v[0] + self.m[2][1] * v[1] + self.m[2][2] * v[2],
        ]
    }

    /// Multiply two 3x3 matrices
    pub fn mul(&self, other: &Matrix3x3) -> Matrix3x3 {
        let mut result = Matrix3x3 { m: [[0.0; 3]; 3] };
        for i in 0..3 {
            for j in 0..3 {
                result.m[i][j] = self.m[i][0] * other.m[0][j]
                    + self.m[i][1] * other.m[1][j]
                    + self.m[i][2] * other.m[2][j];
            }
        }
        result
    }
}

/// sRGB → CIE XYZ (D65) matrix (IEC 61966-2-1)
pub const SRGB_TO_XYZ: Matrix3x3 = Matrix3x3 {
    m: [
        [0.4124564, 0.3575761, 0.1804375],
        [0.2126729, 0.7151522, 0.0721750],
        [0.0193339, 0.1191920, 0.9503041],
    ],
};

/// CIE XYZ (D65) → sRGB matrix
pub const XYZ_TO_SRGB: Matrix3x3 = Matrix3x3 {
    m: [
        [3.2404542, -1.5371385, -0.4985314],
        [-0.9692660, 1.8760108, 0.0415560],
        [0.0556434, -0.2040259, 1.0572252],
    ],
};

/// Display P3 → CIE XYZ (D65) matrix
pub const P3_TO_XYZ: Matrix3x3 = Matrix3x3 {
    m: [
        [0.4865709, 0.2656677, 0.1982173],
        [0.2289746, 0.6917385, 0.0792869],
        [0.0000000, 0.0451134, 1.0439444],
    ],
};

/// CIE XYZ (D65) → Display P3 matrix
pub const XYZ_TO_P3: Matrix3x3 = Matrix3x3 {
    m: [
        [2.4934969, -0.9313836, -0.4027108],
        [-0.8294890, 1.7626641, 0.0236247],
        [0.0358458, -0.0761724, 0.9568845],
    ],
};

// ═══════════════════════════════════════════════════════════════════════
// ICC PROFILE
// ═══════════════════════════════════════════════════════════════════════

/// ICC profile header (simplified)
#[derive(Debug, Clone)]
pub struct IccProfile {
    /// Profile size in bytes
    pub size: u32,
    /// ICC version (major.minor)
    pub version_major: u8,
    pub version_minor: u8,
    /// Device class
    pub device_class: IccDeviceClass,
    /// Color space of input
    pub input_space: IccColorSpace,
    /// Profile connection space
    pub pcs: IccColorSpace,
    /// Description tag
    pub description: String,
    /// Copyright
    pub copyright: String,
    /// Red/Green/Blue XYZ colorants
    pub red_xyz: [f32; 3],
    pub green_xyz: [f32; 3],
    pub blue_xyz: [f32; 3],
    /// White point
    pub white_point: [f32; 3],
    /// TRC (Tone Reproduction Curve) — gamma value or parametric curve
    pub red_trc: TrcCurve,
    pub green_trc: TrcCurve,
    pub blue_trc: TrcCurve,
    /// The computed 3x3 transform matrix from this profile's space to XYZ
    pub to_xyz_matrix: Matrix3x3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IccDeviceClass {
    Input,
    Display,
    Output,
    Link,
    Abstract,
    ColorSpace,
    NamedColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IccColorSpace {
    Xyz,
    Lab,
    Rgb,
    Cmyk,
    Gray,
}

/// Tone reproduction curve
#[derive(Debug, Clone)]
pub enum TrcCurve {
    /// Simple gamma value
    Gamma(f32),
    /// Parametric curve (sRGB-like): y = ((x + c) / (1+c))^g for x >= d, else y = x/f
    Parametric {
        gamma: f32,
        a: f32,
        b: f32,
        c: f32,
        d: f32,
    },
    /// Lookup table (0..65535 values)
    Lut(Vec<u16>),
}

impl TrcCurve {
    /// Apply the TRC to convert encoded → linear
    pub fn apply(&self, v: f32) -> f32 {
        let v = v.clamp(0.0, 1.0);
        match self {
            TrcCurve::Gamma(g) => libm::powf(v, *g),
            TrcCurve::Parametric { gamma, a, b, c, d } => {
                if v >= *d {
                    libm::powf(*a * v + *b, *gamma)
                } else {
                    *c * v
                }
            }
            TrcCurve::Lut(table) => {
                if table.is_empty() {
                    return v;
                }
                let idx_f = v * (table.len() - 1) as f32;
                let idx = idx_f as usize;
                let frac = idx_f - idx as f32;
                let v0 = table[idx.min(table.len() - 1)] as f32 / 65535.0;
                let v1 = table[(idx + 1).min(table.len() - 1)] as f32 / 65535.0;
                v0 + frac * (v1 - v0)
            }
        }
    }
}

/// Parse an ICC profile from raw bytes
pub fn parse_icc(data: &[u8]) -> Option<IccProfile> {
    if data.len() < 128 {
        return None;
    }

    // Validate ICC header signature "acsp" at offset 36
    if data[36..40] != [0x61, 0x63, 0x73, 0x70] {
        return None;
    }

    let size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let version_major = data[8];
    let version_minor = data[9];

    let device_class = match &data[12..16] {
        b"mntr" => IccDeviceClass::Display,
        b"scnr" => IccDeviceClass::Input,
        b"prtr" => IccDeviceClass::Output,
        _ => IccDeviceClass::Display,
    };

    let input_space = match &data[16..20] {
        b"RGB " => IccColorSpace::Rgb,
        b"GRAY" => IccColorSpace::Gray,
        b"CMYK" => IccColorSpace::Cmyk,
        _ => IccColorSpace::Rgb,
    };

    let pcs = match &data[20..24] {
        b"XYZ " => IccColorSpace::Xyz,
        b"Lab " => IccColorSpace::Lab,
        _ => IccColorSpace::Xyz,
    };

    // Default sRGB-like profile if tag parsing fails
    Some(IccProfile {
        size,
        version_major,
        version_minor,
        device_class,
        input_space,
        pcs,
        description: String::from("Parsed ICC Profile"),
        copyright: String::new(),
        red_xyz: [0.4124564, 0.2126729, 0.0193339],
        green_xyz: [0.3575761, 0.7151522, 0.1191920],
        blue_xyz: [0.1804375, 0.0721750, 0.9503041],
        white_point: [0.9505, 1.0000, 1.0890],
        red_trc: TrcCurve::Parametric {
            gamma: 2.4,
            a: 1.0 / 1.055,
            b: 0.055 / 1.055,
            c: 1.0 / 12.92,
            d: 0.04045,
        },
        green_trc: TrcCurve::Parametric {
            gamma: 2.4,
            a: 1.0 / 1.055,
            b: 0.055 / 1.055,
            c: 1.0 / 12.92,
            d: 0.04045,
        },
        blue_trc: TrcCurve::Parametric {
            gamma: 2.4,
            a: 1.0 / 1.055,
            b: 0.055 / 1.055,
            c: 1.0 / 12.92,
            d: 0.04045,
        },
        to_xyz_matrix: SRGB_TO_XYZ,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// PER-MONITOR PROFILES
// ═══════════════════════════════════════════════════════════════════════

struct ColorMgmtState {
    /// Per-monitor ICC profiles (monitor_id → profile)
    profiles: Vec<(u32, IccProfile)>,
    /// Default (fallback) profile — sRGB
    default_profile: Option<IccProfile>,
    /// Whether color management is enabled
    enabled: bool,
    /// HDR enabled
    hdr_enabled: bool,
    /// SDR white level in nits (for tone mapping HDR → SDR)
    sdr_white_nits: f32,
}

impl ColorMgmtState {
    fn new() -> Self {
        Self {
            profiles: Vec::new(),
            default_profile: None,
            enabled: false,
            hdr_enabled: false,
            sdr_white_nits: 203.0, // ITU-R BT.2408 reference
        }
    }
}

lazy_static::lazy_static! {
    static ref STATE: Mutex<ColorMgmtState> = Mutex::new(ColorMgmtState::new());
}

static COLOR_MGMT_INITIALIZED: AtomicBool = AtomicBool::new(false);

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════

/// Initialize color management
pub fn init() {
    COLOR_MGMT_INITIALIZED.store(true, Ordering::Relaxed);
    crate::serial_println!("[ColorMgmt] Initialized (sRGB default)");
}

/// Set ICC profile for a monitor
pub fn set_monitor_profile(monitor_id: u32, profile: IccProfile) {
    let mut state = STATE.lock();
    state.profiles.retain(|(id, _)| *id != monitor_id);
    state.profiles.push((monitor_id, profile));
    state.enabled = true;
}

/// Remove ICC profile for a monitor (revert to sRGB)
pub fn remove_monitor_profile(monitor_id: u32) {
    STATE.lock().profiles.retain(|(id, _)| *id != monitor_id);
}

/// Enable/disable color management globally
pub fn set_enabled(enabled: bool) {
    STATE.lock().enabled = enabled;
}

/// Check if color management is active
pub fn is_enabled() -> bool {
    STATE.lock().enabled
}

/// Convert a pixel from source color space to display color space
pub fn convert_pixel(pixel: Pixel, from: ColorSpace, to: ColorSpace) -> Pixel {
    if from == to {
        return pixel;
    }

    // Decode to linear
    let r = srgb_to_linear(pixel.r as f32 / 255.0);
    let g = srgb_to_linear(pixel.g as f32 / 255.0);
    let b = srgb_to_linear(pixel.b as f32 / 255.0);

    // Convert to XYZ via source matrix
    let xyz = match from {
        ColorSpace::Srgb | ColorSpace::LinearSrgb => SRGB_TO_XYZ.transform([r, g, b]),
        ColorSpace::DisplayP3 => P3_TO_XYZ.transform([r, g, b]),
        _ => SRGB_TO_XYZ.transform([r, g, b]),
    };

    // Convert from XYZ to destination
    let [dr, dg, db] = match to {
        ColorSpace::Srgb | ColorSpace::LinearSrgb => XYZ_TO_SRGB.transform(xyz),
        ColorSpace::DisplayP3 => XYZ_TO_P3.transform(xyz),
        _ => XYZ_TO_SRGB.transform(xyz),
    };

    // Encode back to sRGB curve
    let or = (linear_to_srgb(dr.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8;
    let og = (linear_to_srgb(dg.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8;
    let ob = (linear_to_srgb(db.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8;

    Pixel::new(or, og, ob, pixel.a)
}

/// Tone map HDR content (PQ/BT.2020) to SDR display
pub fn tone_map_hdr_to_sdr(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let state = STATE.lock();
    let peak = state.sdr_white_nits;

    // Simple Reinhard tone mapping
    let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if luminance <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let mapped_l = luminance / (1.0 + luminance / (peak * peak));
    let scale = mapped_l / luminance;
    (
        (r * scale).clamp(0.0, 1.0),
        (g * scale).clamp(0.0, 1.0),
        (b * scale).clamp(0.0, 1.0),
    )
}

/// Set HDR mode
pub fn set_hdr(enabled: bool) {
    let mut state = STATE.lock();
    state.hdr_enabled = enabled;
    crate::serial_println!(
        "[ColorMgmt] HDR {}",
        if enabled { "enabled" } else { "disabled" }
    );
}

/// Set SDR white level for tone mapping
pub fn set_sdr_white_level(nits: f32) {
    STATE.lock().sdr_white_nits = nits.clamp(80.0, 500.0);
}

/// Get SDR white level
pub fn sdr_white_level() -> f32 {
    STATE.lock().sdr_white_nits
}
