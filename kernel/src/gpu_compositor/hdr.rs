use spin::Mutex;

/// HDR metadata (SMPTE ST 2086 + CTA-861.3)
#[derive(Debug, Clone)]
pub struct HdrMetadata {
    /// Whether HDR is active
    pub enabled: bool,
    /// Color space
    pub color_space: ColorSpace,
    /// Maximum display luminance in nits
    pub max_luminance: f32,
    /// Minimum display luminance in nits
    pub min_luminance: f32,
    /// Maximum content light level
    pub max_cll: u16,
    /// Maximum frame average light level
    pub max_fall: u16,
    /// Display primaries (CIE xy coordinates)
    pub primaries: DisplayPrimaries,
    /// White point (CIE xy)
    pub white_point: (f32, f32),
    /// Transfer function
    pub eotf: TransferFunction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    Srgb,
    Bt2020,
    DciP3,
    AdobeRgb,
    Rec709,
}

#[derive(Debug, Clone)]
pub struct DisplayPrimaries {
    pub red: (f32, f32),
    pub green: (f32, f32),
    pub blue: (f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferFunction {
    Srgb,
    Pq,  // Perceptual Quantizer (ST 2084)
    Hlg, // Hybrid Log-Gamma
    Linear,
}

impl Default for HdrMetadata {
    fn default() -> Self {
        Self {
            enabled: false,
            color_space: ColorSpace::Srgb,
            max_luminance: 400.0,
            min_luminance: 0.1,
            max_cll: 1000,
            max_fall: 400,
            primaries: DisplayPrimaries {
                red: (0.64, 0.33),
                green: (0.30, 0.60),
                blue: (0.15, 0.06),
            },
            white_point: (0.3127, 0.3290), // D65
            eotf: TransferFunction::Srgb,
        }
    }
}

/// PQ (Perceptual Quantizer) EOTF for HDR10 (ST 2084)
/// Converts PQ signal [0,1] to linear luminance [0, 10000] nits  
pub fn pq_eotf(signal: f32) -> f32 {
    let m1: f32 = 0.159_301_76;
    let m2: f32 = 78.84375;
    let c1: f32 = 0.8359375;
    let c2: f32 = 18.851_563;
    let c3: f32 = 18.6875;

    let signal_pow = libm::powf(signal, 1.0 / m2);
    let num = (signal_pow - c1).max(0.0);
    let den = c2 - c3 * signal_pow;
    10000.0 * libm::powf(num / den, 1.0 / m1)
}

/// Tone-map HDR content to SDR display
pub fn tonemap_reinhard(luminance: f32, max_luminance: f32) -> f32 {
    luminance / (1.0 + luminance / max_luminance)
}

lazy_static::lazy_static! {
    pub static ref HDR_STATE: Mutex<HdrMetadata> = Mutex::new(HdrMetadata::default());
}
