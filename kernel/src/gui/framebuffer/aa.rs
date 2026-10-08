/// Integer square root scaled by 256 (8.8 fixed point result)
/// Input: value already scaled by 256 (i.e., compute sqrt(val/256)*256)
pub(super) fn isqrt_256(val_x256: u32) -> u32 {
    // We want sqrt(dist_sq) * 256 = sqrt(val_x256 / 256) * 256 = sqrt(val_x256 * 256)
    let n = val_x256 as u64 * 256;
    if n == 0 {
        return 0;
    }
    // Integer square root via binary digit-by-digit method
    let mut guess = n;
    let mut result = 0u64;
    let mut bit = 1u64 << 62;
    while bit > n {
        bit >>= 2;
    }
    while bit != 0 {
        if guess >= result + bit {
            guess -= result + bit;
            result = (result >> 1) + bit;
        } else {
            result >>= 1;
        }
        bit >>= 2;
    }
    // result is floor(sqrt(n)) where n = val_x256 * 256
    // We want this / 16 to get back to 8.8 scale... no wait.
    // val_x256 = dist_sq * 256. We want sqrt(dist_sq) * 256.
    // sqrt(val_x256) * sqrt(256) = sqrt(dist_sq) * 16 * 16 = sqrt(dist_sq)*256
    // So result = floor(sqrt(val_x256 * 256))
    // = floor(sqrt(dist_sq * 256 * 256)) = floor(sqrt(dist_sq) * 256) ✓
    result as u32
}

/// Compute anti-aliased coverage for a pixel at (px,py) inside a rounded rect.
/// Returns 0-255 where 255 = fully covered, 0 = fully outside.
pub(super) fn rounded_rect_coverage(
    px: i32,
    py: i32,
    rx: i32,
    ry: i32,
    rw: i32,
    rh: i32,
    r: i32,
) -> u8 {
    // Local coords within the rect
    let lx = px - rx;
    let ly = py - ry;

    // Quick reject: outside bounding box
    if lx < 0 || ly < 0 || lx >= rw || ly >= rh {
        return 0;
    }

    // Check if we're in a corner region
    let in_left = lx < r;
    let in_right = lx >= rw - r;
    let in_top = ly < r;
    let in_bottom = ly >= rh - r;

    if (!in_left && !in_right) || (!in_top && !in_bottom) {
        // Not in a corner: fully inside
        return 255;
    }

    // In a corner: compute distance from corner circle center
    let (ccx, ccy) = match (in_left, in_top) {
        (true, true) => (r, r),                     // top-left
        (false, true) => (rw - r - 1, r),           // top-right
        (true, false) => (r, rh - r - 1),           // bottom-left
        (false, false) => (rw - r - 1, rh - r - 1), // bottom-right
    };

    let dx = lx - ccx;
    let dy = ly - ccy;
    let dist_sq = dx * dx + dy * dy;
    let r_sq = r * r;

    if dist_sq <= (r - 1) * (r - 1) {
        // Fully inside the rounded corner
        255
    } else if dist_sq > (r + 1) * (r + 1) {
        // Fully outside the rounded corner
        0
    } else {
        // Anti-alias zone: compute smooth coverage
        // dist = sqrt(dist_sq), coverage = clamp(r + 0.5 - dist, 0, 1) * 255
        let dist_x16 = isqrt_x16(dist_sq as u32);
        let r_x16 = r as u32 * 16 + 8; // r + 0.5 in 4.4 fixed point
        if dist_x16 <= r_x16 {
            255
        } else {
            let overshoot = dist_x16 - r_x16; // in 1/16 units
            // 16 units = 1 pixel of falloff
            let coverage = 255u32.saturating_sub(overshoot * 16);
            coverage.min(255) as u8
        }
    }
}

/// Integer sqrt in 4.4 fixed point (result * 16) — public for use in icon drawing
pub fn isqrt_x16_pub(n: u32) -> u32 {
    isqrt_x16(n)
}

/// Integer sqrt in 4.4 fixed point (result * 16)
fn isqrt_x16(n: u32) -> u32 {
    if n == 0 {
        return 0;
    }
    // We want sqrt(n) * 16 = sqrt(n * 256)
    let val = n as u64 * 256;
    // Integer square root via Newton's method
    let mut guess = val;
    let mut result = 0u64;
    let mut bit = 1u64 << 62;
    while bit > val {
        bit >>= 2;
    }
    while bit != 0 {
        if guess >= result + bit {
            guess -= result + bit;
            result = (result >> 1) + bit;
        } else {
            result >>= 1;
        }
        bit >>= 2;
    }
    result as u32
}
