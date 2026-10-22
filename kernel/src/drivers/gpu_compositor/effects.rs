use alloc::vec;

/// Visual effect types (software-emulated shader effects)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualEffect {
    /// Drop shadow behind windows
    DropShadow,
    /// Inner glow effect
    InnerGlow,
    /// Glass/frosted blur effect
    FrostedGlass,
    /// Bloom effect on bright areas
    Bloom,
    /// Vignette (dark corners)
    Vignette,
    /// Gaussian blur
    GaussianBlur,
}

/// Apply a drop shadow effect to a rectangular region
pub fn apply_drop_shadow(
    buffer: &mut [u32],
    stride: u32,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    shadow_radius: u32,
    shadow_color: u32,
    shadow_opacity: u8,
) {
    let sr = shadow_radius as i32;
    let shadow_a = shadow_opacity;

    for sy in -sr..h as i32 + sr {
        for sx in -sr..w as i32 + sr {
            let px = x + sx + sr / 2; // Shadow offset
            let py = y + sy + sr / 2;

            if px < 0 || py < 0 {
                continue;
            }
            let px = px as u32;
            let py = py as u32;

            // Skip if inside the window rectangle
            if sx >= 0 && sx < w as i32 && sy >= 0 && sy < h as i32 {
                continue;
            }

            // Distance to nearest edge
            let dx = if sx < 0 {
                -sx
            } else if sx >= w as i32 {
                sx - w as i32 + 1
            } else {
                0
            };
            let dy = if sy < 0 {
                -sy
            } else if sy >= h as i32 {
                sy - h as i32 + 1
            } else {
                0
            };
            let dist = libm::sqrtf((dx * dx + dy * dy) as f32);

            if dist > sr as f32 {
                continue;
            }

            // Gaussian falloff
            let sigma = sr as f32 / 2.5;
            let alpha = libm::expf(-(dist * dist) / (2.0 * sigma * sigma));
            let final_alpha = (alpha * shadow_a as f32) as u8;

            // Blend shadow into buffer
            let idx = (py * stride + px) as usize;
            if idx < buffer.len() {
                let bg = buffer[idx];
                let bg_r = (bg >> 16) & 0xFF;
                let bg_g = (bg >> 8) & 0xFF;
                let bg_b = bg & 0xFF;
                let sh_r = (shadow_color >> 16) & 0xFF;
                let sh_g = (shadow_color >> 8) & 0xFF;
                let sh_b = shadow_color & 0xFF;
                let a = final_alpha as u32;
                let ia = 255 - a;
                let r = (sh_r * a + bg_r * ia) / 255;
                let g = (sh_g * a + bg_g * ia) / 255;
                let b = (sh_b * a + bg_b * ia) / 255;
                buffer[idx] = (r << 16) | (g << 8) | b;
            }
        }
    }
}

/// Apply a bloom effect (bright areas glow)
pub fn apply_bloom(buffer: &mut [u32], width: u32, height: u32, threshold: u8, intensity: f32) {
    // Extract bright pixels
    let len = (width * height) as usize;
    let mut bright = vec![0u32; len];

    for i in 0..len {
        let pixel = buffer[i];
        let r = (pixel >> 16) & 0xFF;
        let g = (pixel >> 8) & 0xFF;
        let b = pixel & 0xFF;
        let luma = (r * 77 + g * 150 + b * 29) >> 8;
        if luma > threshold as u32 {
            bright[i] = pixel;
        }
    }

    // Simple box blur on bright pixels (2 passes)
    let radius = 3i32;
    let mut temp = vec![0u32; len];

    // Horizontal pass
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let mut r_sum = 0u32;
            let mut g_sum = 0u32;
            let mut b_sum = 0u32;
            let mut count = 0u32;
            for dx in -radius..=radius {
                let nx = x + dx;
                if nx >= 0 && nx < width as i32 {
                    let idx = (y as u32 * width + nx as u32) as usize;
                    let p = bright[idx];
                    r_sum += (p >> 16) & 0xFF;
                    g_sum += (p >> 8) & 0xFF;
                    b_sum += p & 0xFF;
                    count += 1;
                }
            }
            if count > 0 {
                let idx = (y as u32 * width + x as u32) as usize;
                temp[idx] = (r_sum.checked_div(count).unwrap_or(0) << 16)
                    | (g_sum.checked_div(count).unwrap_or(0) << 8)
                    | b_sum.checked_div(count).unwrap_or(0);
            }
        }
    }

    // Additive blend bloom back into original
    for i in 0..len {
        let orig = buffer[i];
        let bloom = temp[i];
        let r = (((orig >> 16) & 0xFF) + ((bloom >> 16) & 0xFF) as u32 * intensity as u32 / 100)
            .min(255);
        let g =
            (((orig >> 8) & 0xFF) + ((bloom >> 8) & 0xFF) as u32 * intensity as u32 / 100).min(255);
        let b = ((orig & 0xFF) + (bloom & 0xFF) as u32 * intensity as u32 / 100).min(255);
        buffer[i] = (r << 16) | (g << 8) | b;
    }
}
