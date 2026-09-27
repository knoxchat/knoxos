use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::cmd::{GpuCmdBuffer, GpuCompCmd};
use super::types::{CompLayer, GpuSurface, SurfaceFormat};

/// The GPU-accelerated compositor state
#[derive(Debug)]
pub struct GpuCompositor {
    /// All managed surfaces
    pub surfaces: BTreeMap<u32, GpuSurface>,
    /// Next surface ID
    next_id: u32,
    /// Compositing layers (ordered by z_order)
    pub layers: Vec<CompLayer>,
    /// Output scanout surface
    pub scanout_id: u32,
    /// Display dimensions
    pub display_width: u32,
    pub display_height: u32,
    /// Backend type
    pub backend: CompBackend,
    /// Frame counter
    pub frame_count: u64,
    /// Pending command buffer
    pub cmd_buffer: GpuCmdBuffer,
    /// Performance stats
    pub stats: CompStats,
}

/// Compositing backend
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompBackend {
    /// Software rendering (SIMD-optimized)
    Software,
    /// VirtIO GPU 2D
    VirtioGpu2D,
    /// VirtIO GPU 3D (Virgl)
    VirtioGpu3D,
    /// DRM/KMS with GEM buffers
    DrmKms,
}

/// Compositor performance statistics
#[derive(Debug, Default)]
pub struct CompStats {
    pub total_frames: u64,
    pub total_composites: u64,
    pub total_uploads: u64,
    pub total_blurs: u64,
    pub gpu_submit_count: u64,
    pub last_frame_time_us: u64,
    pub avg_frame_time_us: u64,
    pub peak_frame_time_us: u64,
}

impl GpuCompositor {
    pub fn new(width: u32, height: u32, backend: CompBackend) -> Self {
        let mut comp = Self {
            surfaces: BTreeMap::new(),
            next_id: 1,
            layers: Vec::new(),
            scanout_id: 0,
            display_width: width,
            display_height: height,
            backend,
            frame_count: 0,
            cmd_buffer: GpuCmdBuffer::new(),
            stats: CompStats::default(),
        };

        // Create the scanout surface
        let scanout = comp.create_surface(width, height, SurfaceFormat::BGRA8888);
        comp.scanout_id = scanout;

        comp
    }

    /// Create a new GPU surface
    pub fn create_surface(&mut self, width: u32, height: u32, format: SurfaceFormat) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let surface = GpuSurface::new(id, width, height, format);

        // Submit GPU create command
        self.cmd_buffer.push(GpuCompCmd::CreateSurface {
            id,
            width,
            height,
            format,
        });

        self.surfaces.insert(id, surface);
        id
    }

    /// Destroy a surface
    pub fn destroy_surface(&mut self, id: u32) {
        self.surfaces.remove(&id);
        self.cmd_buffer.push(GpuCompCmd::DestroySurface { id });
    }

    /// Upload dirty regions of a surface to GPU
    pub fn upload_surface(&mut self, id: u32) {
        if let Some(surface) = self.surfaces.get_mut(&id) {
            if surface.gpu_synced {
                return;
            }

            for &(x, y, w, h) in &surface.damage.clone() {
                self.cmd_buffer.push(GpuCompCmd::UploadData {
                    surface_id: id,
                    x,
                    y,
                    w,
                    h,
                    data_offset: surface.pixel_offset(x, y),
                });
                self.stats.total_uploads += 1;
            }
            surface.clear_damage();
        }
    }

    /// Add a compositing layer
    pub fn add_layer(&mut self, layer: CompLayer) {
        self.layers.push(layer);
        // Keep sorted by z_order
        self.layers.sort_by_key(|l| l.z_order);
    }

    /// Remove a layer by surface ID
    pub fn remove_layer(&mut self, surface_id: u32) {
        self.layers.retain(|l| l.surface_id != surface_id);
    }

    /// Composite all layers to the scanout surface
    pub fn composite_frame(&mut self) {
        let start_tsc = read_tsc();

        // Clear scanout
        self.cmd_buffer.push(GpuCompCmd::Fill {
            surface_id: self.scanout_id,
            x: 0,
            y: 0,
            w: self.display_width,
            h: self.display_height,
            color: 0xFF1A1A2E, // Default background
        });

        // Composite each visible layer
        for layer in &self.layers {
            if !layer.visible {
                continue;
            }

            self.cmd_buffer.push(GpuCompCmd::Composite {
                src: layer.surface_id,
                dst: self.scanout_id,
                src_rect: (layer.src_x, layer.src_y, layer.src_w, layer.src_h),
                dst_rect: (layer.dst_x, layer.dst_y, layer.dst_w, layer.dst_h),
                blend: layer.blend,
                opacity: layer.opacity,
            });
            self.stats.total_composites += 1;
        }

        // Present
        self.cmd_buffer.push(GpuCompCmd::SetScanout {
            surface_id: self.scanout_id,
        });
        self.cmd_buffer.push(GpuCompCmd::Flush);

        // Execute the command buffer
        self.execute_commands();

        self.frame_count += 1;
        self.stats.total_frames += 1;

        let end_tsc = read_tsc();
        let frame_time = (end_tsc.saturating_sub(start_tsc)) / 3000; // Approximate µs
        self.stats.last_frame_time_us = frame_time;
        if frame_time > self.stats.peak_frame_time_us {
            self.stats.peak_frame_time_us = frame_time;
        }
        // Running average
        self.stats.avg_frame_time_us =
            (self.stats.avg_frame_time_us * (self.stats.total_frames - 1) + frame_time)
                / self.stats.total_frames;
    }

    /// Execute the pending GPU command buffer
    fn execute_commands(&mut self) {
        match self.backend {
            CompBackend::Software => self.execute_software(),
            CompBackend::VirtioGpu2D => self.execute_virtio_2d(),
            CompBackend::VirtioGpu3D => self.execute_virtio_3d(),
            CompBackend::DrmKms => self.execute_drm(),
        }
        self.cmd_buffer.clear();
        self.stats.gpu_submit_count += 1;
    }

    /// Apply blur to a surface region (for glassmorphism)
    pub fn blur_surface(&mut self, surface_id: u32, radius: u32) {
        self.cmd_buffer
            .push(GpuCompCmd::Blur { surface_id, radius });
        self.stats.total_blurs += 1;
    }

    /// Get compositor statistics
    pub fn get_stats(&self) -> &CompStats {
        &self.stats
    }
}

fn read_tsc() -> u64 {
    crate::arch_compat::read_tsc()
}
