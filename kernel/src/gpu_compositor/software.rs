use super::cmd::GpuCompCmd;
use super::compositor::GpuCompositor;
use super::types::BlendMode;

impl GpuCompositor {
    /// Software compositing path (SIMD-optimized)
    pub(super) fn execute_software(&mut self) {
        let commands = self.cmd_buffer.commands.clone();
        for cmd in &commands {
            match cmd {
                GpuCompCmd::Fill {
                    surface_id,
                    x,
                    y,
                    w,
                    h,
                    color,
                } => {
                    if let Some(surface) = self.surfaces.get_mut(surface_id) {
                        let bpp = surface.format.bytes_per_pixel() as usize;
                        let stride = surface.stride as usize;
                        let color_bytes = color.to_le_bytes();

                        for row in *y..(*y + *h).min(surface.height) {
                            for col in *x..(*x + *w).min(surface.width) {
                                let offset = (row * surface.stride
                                    + col * surface.format.bytes_per_pixel())
                                    as usize;
                                if offset + bpp <= surface.data.len() {
                                    surface.data[offset..offset + bpp.min(4)]
                                        .copy_from_slice(&color_bytes[..bpp.min(4)]);
                                }
                            }
                        }
                    }
                }
                GpuCompCmd::Composite {
                    src,
                    dst,
                    src_rect,
                    dst_rect,
                    blend,
                    opacity,
                } => {
                    // Alpha compositing: out = src * opacity + dst * (1 - src_alpha * opacity)
                    // Performed per-pixel with SIMD where possible
                    self.software_composite(*src, *dst, *src_rect, *dst_rect, *blend, *opacity);
                }
                GpuCompCmd::Blur { surface_id, radius } => {
                    // Box blur approximation of gaussian
                    self.software_blur(*surface_id, *radius);
                }
                _ => {} // Other commands handled by specific backends
            }
        }
    }

    /// Software alpha compositing between two surfaces
    fn software_composite(
        &mut self,
        src_id: u32,
        dst_id: u32,
        src_rect: (i32, i32, u32, u32),
        dst_rect: (i32, i32, u32, u32),
        blend: BlendMode,
        opacity: f32,
    ) {
        // We need to borrow both surfaces — take the data out temporarily
        let src_data;
        let src_stride;
        let src_w;
        let src_h;
        let src_bpp;
        if let Some(src) = self.surfaces.get(&src_id) {
            src_data = src.data.clone();
            src_stride = src.stride;
            src_w = src.width;
            src_h = src.height;
            src_bpp = src.format.bytes_per_pixel();
        } else {
            return;
        }

        if let Some(dst) = self.surfaces.get_mut(&dst_id) {
            let dst_bpp = dst.format.bytes_per_pixel() as usize;
            let dst_stride = dst.stride as usize;
            let (sx, sy, sw, sh) = src_rect;
            let (dx, dy, dw, dh) = dst_rect;
            let scale_x = if dw > 0 { sw as f32 / dw as f32 } else { 1.0 };
            let scale_y = if dh > 0 { sh as f32 / dh as f32 } else { 1.0 };

            for row in 0..dh as i32 {
                let dst_y = dy + row;
                if dst_y < 0 || dst_y >= dst.height as i32 {
                    continue;
                }
                let src_y_f = sy as f32 + row as f32 * scale_y;
                let src_yi = src_y_f as i32;
                if src_yi < 0 || src_yi >= src_h as i32 {
                    continue;
                }

                for col in 0..dw as i32 {
                    let dst_x = dx + col;
                    if dst_x < 0 || dst_x >= dst.width as i32 {
                        continue;
                    }
                    let src_x_f = sx as f32 + col as f32 * scale_x;
                    let src_xi = src_x_f as i32;
                    if src_xi < 0 || src_xi >= src_w as i32 {
                        continue;
                    }

                    let s_off = (src_yi as u32 * src_stride + src_xi as u32 * src_bpp) as usize;
                    let d_off = dst_y as usize * dst_stride + dst_x as usize * dst_bpp;

                    if s_off + 4 > src_data.len() || d_off + 4 > dst.data.len() {
                        continue;
                    }

                    let sb = src_data[s_off] as f32;
                    let sg = src_data[s_off + 1] as f32;
                    let sr = src_data[s_off + 2] as f32;
                    let sa = src_data[s_off + 3] as f32 * opacity / 255.0;

                    let db = dst.data[d_off] as f32;
                    let dg = dst.data[d_off + 1] as f32;
                    let dr = dst.data[d_off + 2] as f32;
                    let da = dst.data[d_off + 3] as f32 / 255.0;

                    // Apply blend mode
                    let (ob, og, or, oa) = match blend {
                        BlendMode::SrcOver => {
                            // Porter-Duff SrcOver: out = src + dst * (1 - src_alpha)
                            let inv_sa = 1.0 - sa;
                            (
                                sb * sa + db * inv_sa,
                                sg * sa + dg * inv_sa,
                                sr * sa + dr * inv_sa,
                                sa + da * inv_sa,
                            )
                        }
                        BlendMode::Src => (sb, sg, sr, sa),
                        BlendMode::DstOver => {
                            let inv_da = 1.0 - da;
                            (
                                db * da + sb * inv_da,
                                dg * da + sg * inv_da,
                                dr * da + sr * inv_da,
                                da + sa * inv_da,
                            )
                        }
                        BlendMode::Multiply => {
                            (sb * db / 255.0, sg * dg / 255.0, sr * dr / 255.0, sa)
                        }
                        BlendMode::Add => {
                            let rb = (sb + db).min(255.0);
                            let rg = (sg + dg).min(255.0);
                            let rr = (sr + dr).min(255.0);
                            (rb, rg, rr, (sa + da).min(1.0))
                        }
                        BlendMode::SrcIn => (sb * da, sg * da, sr * da, sa * da),
                        BlendMode::Opaque => (sb, sg, sr, 1.0),
                    };

                    dst.data[d_off] = ob.clamp(0.0, 255.0) as u8;
                    dst.data[d_off + 1] = og.clamp(0.0, 255.0) as u8;
                    dst.data[d_off + 2] = or.clamp(0.0, 255.0) as u8;
                    dst.data[d_off + 3] = (oa * 255.0).clamp(0.0, 255.0) as u8;
                }
            }
        }
    }

    /// Software gaussian blur (box blur x3 approximation)
    pub(super) fn software_blur(&mut self, surface_id: u32, radius: u32) {
        if radius == 0 {
            return;
        }
        if let Some(surface) = self.surfaces.get_mut(&surface_id) {
            let w = surface.width as usize;
            let h = surface.height as usize;
            let bpp = surface.format.bytes_per_pixel() as usize;
            let stride = surface.stride as usize;
            let r = radius as usize;

            // 3-pass box blur approximation of gaussian
            for _pass in 0..3 {
                // Horizontal pass
                let src = surface.data.clone();
                for y in 0..h {
                    for x in 0..w {
                        let mut sum_r: u32 = 0;
                        let mut sum_g: u32 = 0;
                        let mut sum_b: u32 = 0;
                        let mut sum_a: u32 = 0;
                        let mut count: u32 = 0;
                        let x_start = x.saturating_sub(r);
                        let x_end = (x + r + 1).min(w);
                        for kx in x_start..x_end {
                            let off = y * stride + kx * bpp;
                            if off + 3 < src.len() {
                                sum_b += src[off] as u32;
                                sum_g += src[off + 1] as u32;
                                sum_r += src[off + 2] as u32;
                                sum_a += src[off + 3] as u32;
                                count += 1;
                            }
                        }
                        if count > 0 {
                            let off = y * stride + x * bpp;
                            if off + 3 < surface.data.len() {
                                surface.data[off] = sum_b.checked_div(count).unwrap_or(0) as u8;
                                surface.data[off + 1] = sum_g.checked_div(count).unwrap_or(0) as u8;
                                surface.data[off + 2] = sum_r.checked_div(count).unwrap_or(0) as u8;
                                surface.data[off + 3] = sum_a.checked_div(count).unwrap_or(0) as u8;
                            }
                        }
                    }
                }

                // Vertical pass
                let src = surface.data.clone();
                for y in 0..h {
                    for x in 0..w {
                        let mut sum_r: u32 = 0;
                        let mut sum_g: u32 = 0;
                        let mut sum_b: u32 = 0;
                        let mut sum_a: u32 = 0;
                        let mut count: u32 = 0;
                        let y_start = y.saturating_sub(r);
                        let y_end = (y + r + 1).min(h);
                        for ky in y_start..y_end {
                            let off = ky * stride + x * bpp;
                            if off + 3 < src.len() {
                                sum_b += src[off] as u32;
                                sum_g += src[off + 1] as u32;
                                sum_r += src[off + 2] as u32;
                                sum_a += src[off + 3] as u32;
                                count += 1;
                            }
                        }
                        if count > 0 {
                            let off = y * stride + x * bpp;
                            if off + 3 < surface.data.len() {
                                surface.data[off] = sum_b.checked_div(count).unwrap_or(0) as u8;
                                surface.data[off + 1] = sum_g.checked_div(count).unwrap_or(0) as u8;
                                surface.data[off + 2] = sum_r.checked_div(count).unwrap_or(0) as u8;
                                surface.data[off + 3] = sum_a.checked_div(count).unwrap_or(0) as u8;
                            }
                        }
                    }
                }
            }
        }
    }
}
