use super::cmd::GpuCompCmd;
use super::compositor::GpuCompositor;
use super::types::SurfaceFormat;

impl GpuCompositor {
    /// VirtIO GPU 2D compositing path
    pub(super) fn execute_virtio_2d(&mut self) {
        let commands = self.cmd_buffer.commands.clone();
        for cmd in &commands {
            match cmd {
                GpuCompCmd::CreateSurface {
                    id,
                    width,
                    height,
                    format,
                } => {
                    let fmt_id = match format {
                        SurfaceFormat::BGRA8888 => 1,
                        SurfaceFormat::RGBA8888 => 67,
                        SurfaceFormat::RGB565 => 2,
                        _ => 1,
                    };
                    crate::virtio_gpu::create_2d_resource(*id, *width, *height, fmt_id);
                }
                GpuCompCmd::UploadData {
                    surface_id,
                    x,
                    y,
                    w,
                    h,
                    ..
                } => {
                    crate::virtio_gpu::transfer_to_host_2d_rect(*surface_id, *x, *y, *w, *h);
                }
                GpuCompCmd::Fill {
                    surface_id,
                    x,
                    y,
                    w,
                    h,
                    color,
                } => {
                    let rect = crate::virtio_gpu::Rect {
                        x: *x,
                        y: *y,
                        width: *w,
                        height: *h,
                    };
                    let _ = crate::virtio_gpu::accelerated_fill(*surface_id, rect, *color);
                }
                GpuCompCmd::Composite {
                    src,
                    dst,
                    src_rect,
                    dst_rect,
                    blend,
                    opacity,
                } => {
                    let sr = crate::virtio_gpu::Rect {
                        x: src_rect.0 as u32,
                        y: src_rect.1 as u32,
                        width: src_rect.2,
                        height: src_rect.3,
                    };
                    let dr = crate::virtio_gpu::Rect {
                        x: dst_rect.0 as u32,
                        y: dst_rect.1 as u32,
                        width: dst_rect.2,
                        height: dst_rect.3,
                    };
                    let alpha = (*opacity * 255.0) as u8;
                    let _ = crate::virtio_gpu::composite_blit(*src, *dst, sr, dr, alpha);
                }
                GpuCompCmd::SetScanout { surface_id } => {
                    crate::virtio_gpu::set_scanout_rect(
                        0,
                        *surface_id,
                        0,
                        0,
                        self.display_width,
                        self.display_height,
                    );
                }
                GpuCompCmd::Flush => {
                    crate::virtio_gpu::flush_resource(
                        self.scanout_id,
                        0,
                        0,
                        self.display_width,
                        self.display_height,
                    );
                }
                GpuCompCmd::Blur { surface_id, radius } => {
                    // No hardware blur in VirtIO 2D — fall back to software
                    self.software_blur(*surface_id, *radius);
                }
                _ => {}
            }
        }
    }

    /// VirtIO GPU 3D (Virgl) compositing path
    pub(super) fn execute_virtio_3d(&mut self) {
        // Use Virgl 3D context for GPU-accelerated compositing
        let commands = self.cmd_buffer.commands.clone();
        for cmd in &commands {
            match cmd {
                GpuCompCmd::CreateSurface {
                    id, width, height, ..
                } => {
                    crate::virtio_gpu::create_3d_resource(*id, *width, *height, 1, 0, 0);
                }
                GpuCompCmd::Composite {
                    src,
                    dst,
                    src_rect,
                    dst_rect,
                    blend,
                    opacity,
                } => {
                    // Submit 3D draw commands for blending via Virgl
                    let sr = crate::virtio_gpu::Rect {
                        x: src_rect.0 as u32,
                        y: src_rect.1 as u32,
                        width: src_rect.2,
                        height: src_rect.3,
                    };
                    let dr = crate::virtio_gpu::Rect {
                        x: dst_rect.0 as u32,
                        y: dst_rect.1 as u32,
                        width: dst_rect.2,
                        height: dst_rect.3,
                    };
                    let _ = crate::virtio_gpu::dma_blit(*src, *dst, sr, dr, *opacity);
                }
                GpuCompCmd::Fill {
                    surface_id,
                    x,
                    y,
                    w,
                    h,
                    color,
                } => {
                    let rect = crate::virtio_gpu::Rect {
                        x: *x,
                        y: *y,
                        width: *w,
                        height: *h,
                    };
                    let _ = crate::virtio_gpu::accelerated_fill(*surface_id, rect, *color);
                }
                GpuCompCmd::SetScanout { surface_id } => {
                    crate::virtio_gpu::set_scanout_rect(
                        0,
                        *surface_id,
                        0,
                        0,
                        self.display_width,
                        self.display_height,
                    );
                }
                GpuCompCmd::Flush => {
                    crate::virtio_gpu::flush_resource(
                        self.scanout_id,
                        0,
                        0,
                        self.display_width,
                        self.display_height,
                    );
                }
                GpuCompCmd::Blur { surface_id, radius } => {
                    // Submit gaussian blur shader via 3D pipeline
                    // For now, fall back to software blur
                    self.software_blur(*surface_id, *radius);
                }
                _ => {}
            }
        }
    }
}
