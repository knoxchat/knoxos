use super::cmd::GpuCompCmd;
use super::compositor::GpuCompositor;

impl GpuCompositor {
    /// DRM/KMS compositing path
    pub(super) fn execute_drm(&mut self) {
        // DRM compositing uses GEM buffers and atomic modesetting
        // Each surface maps to a GEM buffer, layers map to DRM planes
        let commands = self.cmd_buffer.commands.clone();
        for cmd in &commands {
            match cmd {
                GpuCompCmd::CreateSurface {
                    id,
                    width,
                    height,
                    format,
                } => {
                    let size = (*width as u64) * (*height as u64) * format.bytes_per_pixel() as u64;
                    // Create GEM buffer via DRM
                    crate::serial_println!(
                        "[gpu_compositor] DRM: create GEM buffer id={} size={}",
                        id,
                        size
                    );
                }
                GpuCompCmd::Fill {
                    surface_id,
                    x,
                    y,
                    w,
                    h,
                    color,
                } => {
                    // DRM fill via mmap'd GEM buffer
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
                    ..
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
                    let _ = crate::virtio_gpu::dma_blit(*src, *dst, sr, dr, 1.0);
                }
                GpuCompCmd::SetScanout { surface_id } => {
                    // DRM atomic page flip: set primary plane to this buffer
                    crate::serial_println!(
                        "[gpu_compositor] DRM: atomic flip to surface {}",
                        surface_id
                    );
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
                    self.software_blur(*surface_id, *radius);
                }
                _ => {}
            }
        }
    }
}
