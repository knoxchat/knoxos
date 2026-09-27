use alloc::vec::Vec;

use super::types::{BlendMode, SurfaceFormat};

/// GPU compositing commands
#[derive(Debug, Clone)]
pub enum GpuCompCmd {
    /// Create a GPU resource
    CreateSurface {
        id: u32,
        width: u32,
        height: u32,
        format: SurfaceFormat,
    },
    /// Destroy a GPU resource
    DestroySurface { id: u32 },
    /// Upload CPU data to GPU surface
    UploadData {
        surface_id: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        data_offset: usize,
    },
    /// Blit from one surface to another with blending
    Composite {
        src: u32,
        dst: u32,
        src_rect: (i32, i32, u32, u32),
        dst_rect: (i32, i32, u32, u32),
        blend: BlendMode,
        opacity: f32,
    },
    /// Fill a surface region with a solid color
    Fill {
        surface_id: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        color: u32,
    },
    /// Apply gaussian blur
    Blur { surface_id: u32, radius: u32 },
    /// Set scanout (present to display)
    SetScanout { surface_id: u32 },
    /// Flush pending operations
    Flush,
}

/// GPU compositing command buffer
#[derive(Debug)]
pub struct GpuCmdBuffer {
    pub commands: Vec<GpuCompCmd>,
    pub data_buffer: Vec<u8>,
}

impl GpuCmdBuffer {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            data_buffer: Vec::new(),
        }
    }

    pub fn push(&mut self, cmd: GpuCompCmd) {
        self.commands.push(cmd);
    }

    pub fn clear(&mut self) {
        self.commands.clear();
        self.data_buffer.clear();
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}
