use alloc::vec;
use alloc::vec::Vec;

use super::util::clamp_u8;

// ═══════════════════════════════════════════════════════════════════════
// COLOR SPACE CONVERSION
// ═══════════════════════════════════════════════════════════════════════

/// Convert RGB to YUV (BT.601)
pub fn rgb_to_yuv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let r = r as i32;
    let g = g as i32;
    let b = b as i32;

    let y = clamp_u8(((66 * r + 129 * g + 25 * b + 128) >> 8) + 16);
    let u = clamp_u8(((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128);
    let v = clamp_u8(((112 * r - 94 * g - 18 * b + 128) >> 8) + 128);

    (y, u, v)
}

/// Convert YUV to RGB (BT.601)
pub fn yuv_to_rgb(y: u8, u: u8, v: u8) -> (u8, u8, u8) {
    let c = y as i32 - 16;
    let d = u as i32 - 128;
    let e = v as i32 - 128;

    let r = clamp_u8((298 * c + 409 * e + 128) >> 8);
    let g = clamp_u8((298 * c - 100 * d - 208 * e + 128) >> 8);
    let b = clamp_u8((298 * c + 516 * d + 128) >> 8);

    (r, g, b)
}

/// Convert NV12 frame to BGRA
pub fn nv12_to_bgra(y_plane: &[u8], uv_plane: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let mut bgra = vec![0u8; w * h * 4];

    for row in 0..h {
        for col in 0..w {
            let y_idx = row * w + col;
            let uv_idx = (row / 2) * w + (col & !1);

            let y = y_plane.get(y_idx).copied().unwrap_or(16);
            let u = uv_plane.get(uv_idx).copied().unwrap_or(128);
            let v = uv_plane.get(uv_idx + 1).copied().unwrap_or(128);

            let (r, g, b) = yuv_to_rgb(y, u, v);

            let out_idx = (row * w + col) * 4;
            bgra[out_idx] = b;
            bgra[out_idx + 1] = g;
            bgra[out_idx + 2] = r;
            bgra[out_idx + 3] = 255;
        }
    }

    bgra
}
