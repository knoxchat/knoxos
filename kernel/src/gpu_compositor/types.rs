use alloc::vec;
use alloc::vec::Vec;

/// Format of a GPU surface
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceFormat {
    BGRA8888,
    RGBA8888,
    RGB888,
    RGB565,
    A8, // Alpha-only (for masks)
    R8, // Single-channel
}

impl SurfaceFormat {
    pub fn bytes_per_pixel(&self) -> u32 {
        match self {
            Self::BGRA8888 | Self::RGBA8888 => 4,
            Self::RGB888 => 3,
            Self::RGB565 => 2,
            Self::A8 | Self::R8 => 1,
        }
    }
}

/// A GPU-managed surface (texture) for compositing
#[derive(Debug)]
pub struct GpuSurface {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub format: SurfaceFormat,
    pub stride: u32,
    /// GPU resource handle (for VirtIO GPU / DRM GEM)
    pub gpu_handle: u64,
    /// CPU-accessible backing store
    pub data: Vec<u8>,
    /// Whether this surface has been uploaded to GPU
    pub gpu_synced: bool,
    /// Damage regions (x, y, w, h) pending upload
    pub damage: Vec<(u32, u32, u32, u32)>,
}

impl GpuSurface {
    pub fn new(id: u32, width: u32, height: u32, format: SurfaceFormat) -> Self {
        let stride = width * format.bytes_per_pixel();
        let size = (stride * height) as usize;
        Self {
            id,
            width,
            height,
            format,
            stride,
            gpu_handle: 0,
            data: vec![0u8; size],
            gpu_synced: false,
            damage: Vec::new(),
        }
    }

    /// Mark a rectangular region as dirty
    pub fn add_damage(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.damage.push((x, y, w, h));
        self.gpu_synced = false;
    }

    /// Clear all damage
    pub fn clear_damage(&mut self) {
        self.damage.clear();
        self.gpu_synced = true;
    }

    /// Get pixel data pointer for a given (x, y)
    pub fn pixel_offset(&self, x: u32, y: u32) -> usize {
        (y * self.stride + x * self.format.bytes_per_pixel()) as usize
    }

    /// Total byte size of the surface
    pub fn byte_size(&self) -> usize {
        (self.stride * self.height) as usize
    }
}

/// Blend mode for compositing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    /// Standard alpha-over compositing (Porter-Duff)
    SrcOver,
    /// Source replaces destination
    Src,
    /// Destination-over
    DstOver,
    /// Multiply blend
    Multiply,
    /// Additive blend
    Add,
    /// Source-in (mask by destination alpha)
    SrcIn,
    /// No blending (opaque copy)
    Opaque,
}

/// A compositing layer
#[derive(Debug)]
pub struct CompLayer {
    pub surface_id: u32,
    pub src_x: i32,
    pub src_y: i32,
    pub src_w: u32,
    pub src_h: u32,
    pub dst_x: i32,
    pub dst_y: i32,
    pub dst_w: u32,
    pub dst_h: u32,
    pub opacity: f32, // 0.0 - 1.0
    pub blend: BlendMode,
    pub z_order: i32,
    pub visible: bool,
    pub transform: Transform2D,
}

/// 2D affine transform
#[derive(Debug, Clone, Copy)]
pub struct Transform2D {
    /// 3x2 matrix [a, b, c, d, tx, ty]
    pub m: [f32; 6],
}

impl Transform2D {
    pub fn identity() -> Self {
        Self {
            m: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }
    }

    pub fn translate(tx: f32, ty: f32) -> Self {
        Self {
            m: [1.0, 0.0, 0.0, 1.0, tx, ty],
        }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            m: [sx, 0.0, 0.0, sy, 0.0, 0.0],
        }
    }

    pub fn rotate(angle: f32) -> Self {
        let c = libm::cosf(angle);
        let s = libm::sinf(angle);
        Self {
            m: [c, s, -s, c, 0.0, 0.0],
        }
    }

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.m[0] * x + self.m[2] * y + self.m[4],
            self.m[1] * x + self.m[3] * y + self.m[5],
        )
    }
}
