// ═══════════════════════════════════════════════════════════════════════
// sRGB <-> Linear Conversion (proper gamma ≈ 2.2)
// ═══════════════════════════════════════════════════════════════════════
//
// Real sRGB uses a piecewise function:
//   linear = srgb/12.92                   if srgb <= 0.04045
//   linear = ((srgb+0.055)/1.055)^2.4     otherwise
//
// We approximate with gamma 2.2 via a const-friendly integer power
// approximation. The key insight: gamma 2.2 ≈ x^2 * x^0.2.
// For 8-bit precision, a carefully built LUT is exact enough.
//
// These tables are the #1 factor in font rendering quality.
// Incorrect tables cause dark fringing halos (gamma too high)
// or washed-out blending (gamma too low).

/// sRGB [0..255] → Linear [0..255] using gamma ≈ 2.2
/// Built with: round(((i/255)^2.2) * 255)
static SRGB_TO_LINEAR: [u8; 256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2,
    3, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 7, 7, 7, 8, 8, 8, 9, 9, 9, 10, 10, 11, 11,
    11, 12, 12, 13, 13, 13, 14, 14, 15, 15, 16, 16, 17, 17, 18, 18, 19, 19, 20, 20, 21, 22, 22, 23,
    23, 24, 25, 25, 26, 26, 27, 28, 28, 29, 30, 30, 31, 32, 33, 33, 34, 35, 35, 36, 37, 38, 39, 39,
    40, 41, 42, 43, 43, 44, 45, 46, 47, 48, 49, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61,
    62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 73, 74, 75, 76, 77, 78, 79, 81, 82, 83, 84, 85, 87, 88,
    89, 90, 91, 93, 94, 95, 97, 98, 99, 100, 102, 103, 105, 106, 107, 109, 110, 111, 113, 114, 116,
    117, 119, 120, 121, 123, 124, 126, 127, 129, 130, 132, 133, 135, 137, 138, 140, 141, 143, 145,
    146, 148, 149, 151, 153, 154, 156, 158, 159, 161, 163, 165, 166, 168, 170, 172, 173, 175, 177,
    179, 181, 182, 184, 186, 188, 190, 192, 194, 196, 197, 199, 201, 203, 205, 207, 209, 211, 213,
    215, 217, 219, 221, 223, 225, 227, 229, 231, 234, 236, 238, 240, 242, 244, 246, 248, 251, 253,
    255,
];

/// Linear [0..255] → sRGB [0..255] using gamma ≈ 1/2.2
/// Built with: round(((i/255)^(1/2.2)) * 255)
static LINEAR_TO_SRGB: [u8; 256] = [
    0, 21, 28, 34, 39, 43, 46, 50, 53, 56, 59, 61, 64, 66, 68, 70, 72, 74, 76, 78, 80, 82, 84, 85,
    87, 89, 90, 92, 93, 95, 96, 98, 99, 101, 102, 103, 105, 106, 107, 109, 110, 111, 112, 114, 115,
    116, 117, 118, 119, 120, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135,
    136, 137, 138, 139, 140, 141, 142, 143, 144, 144, 145, 146, 147, 148, 149, 150, 151, 151, 152,
    153, 154, 155, 156, 156, 157, 158, 159, 160, 160, 161, 162, 163, 164, 164, 165, 166, 167, 167,
    168, 169, 170, 170, 171, 172, 173, 173, 174, 175, 175, 176, 177, 178, 178, 179, 180, 180, 181,
    182, 182, 183, 184, 184, 185, 186, 186, 187, 188, 188, 189, 190, 190, 191, 192, 192, 193, 194,
    194, 195, 195, 196, 197, 197, 198, 199, 199, 200, 200, 201, 202, 202, 203, 203, 204, 205, 205,
    206, 206, 207, 207, 208, 209, 209, 210, 210, 211, 212, 212, 213, 213, 214, 214, 215, 215, 216,
    217, 217, 218, 218, 219, 219, 220, 220, 221, 221, 222, 223, 223, 224, 224, 225, 225, 226, 226,
    227, 227, 228, 228, 229, 229, 230, 230, 231, 231, 232, 232, 233, 233, 234, 234, 235, 235, 236,
    236, 237, 237, 238, 238, 239, 239, 240, 240, 241, 241, 242, 242, 243, 243, 244, 244, 245, 245,
    246, 246, 247, 247, 248, 248, 249, 249, 249, 250, 250, 251, 251, 252, 252, 253, 253, 254, 254,
    255, 255,
];

/// Sharpen glyph alpha coverage to reduce AA fringe blur.
/// Applies a contrast-boost curve: pushes mid-alpha values toward 0 or 255.
/// This is the key to "macOS-quality" crisp text — Core Text uses a similar
/// technique internally (coverage quantization + stem contrast boosting).
///
/// The curve: out = clamp(((cov - 128) * SHARPEN_STRENGTH / 256) + cov, 0, 255)
/// At strength 96: a pixel at alpha 84 becomes ~48, at alpha 203 becomes ~238.
/// Higher values = crisper text but less smooth AA gradients.
pub(super) const COVERAGE_SHARPEN_STRENGTH: i32 = 128;

#[inline(always)]
pub(super) fn sharpen_coverage(cov: u8) -> u8 {
    if cov == 0 || cov == 255 {
        return cov;
    }
    let c = cov as i32;
    // Contrast-boost: push away from midpoint (128)
    let delta = (c - 128) * COVERAGE_SHARPEN_STRENGTH / 256;
    let result = c + delta;
    result.clamp(0, 255) as u8
}

#[inline(always)]
pub(super) fn gamma_blend_channel(fg: u8, bg: u8, alpha: u16) -> u8 {
    if alpha >= 255 {
        return fg;
    }
    if alpha == 0 {
        return bg;
    }
    // Blend in linear light space for correct color mixing
    let fg_lin = SRGB_TO_LINEAR[fg as usize] as u32;
    let bg_lin = SRGB_TO_LINEAR[bg as usize] as u32;
    let a = alpha as u32;
    let inv_a = 255 - a;
    let blended = (fg_lin * a + bg_lin * inv_a + 128) / 255;
    LINEAR_TO_SRGB[blended.min(255) as usize]
}

/// Public gamma-correct blend for a single channel (used by truetype.rs)
#[inline(always)]
pub fn gamma_blend_public(fg: u8, bg: u8, alpha: u32, inv_alpha: u32) -> u8 {
    let fg_lin = SRGB_TO_LINEAR[fg as usize] as u32;
    let bg_lin = SRGB_TO_LINEAR[bg as usize] as u32;
    let blended = (fg_lin * alpha + bg_lin * inv_alpha + 128) / 255;
    LINEAR_TO_SRGB[blended.min(255) as usize]
}
