/// Pixel formats and shared-memory pool/buffer types.
use alloc::vec::Vec;

use super::surface::DamageRect;

// ─── Pixel formats ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Argb8888,
    Xrgb8888,
    Rgb565,
    Bgra8888,
}

impl PixelFormat {
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            Self::Argb8888 | Self::Xrgb8888 | Self::Bgra8888 => 4,
            Self::Rgb565 => 2,
        }
    }
}

// ─── Shared Memory Buffer ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ShmBuffer {
    pub id: u32,
    pub data_offset: usize, // offset into SHM pool
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    pub pool_id: u32,
    pub released: bool,
    pub busy: bool,                      // buffer in use by GPU/display
    pub pending_damage: Vec<DamageRect>, // accumulated damage since last commit
    pub client_pid: u32,
}

impl ShmBuffer {
    pub fn new(
        id: u32,
        pool_id: u32,
        offset: usize,
        width: u32,
        height: u32,
        stride: u32,
        format: PixelFormat,
        client_pid: u32,
    ) -> Self {
        Self {
            id,
            pool_id,
            data_offset: offset,
            width,
            height,
            stride,
            format,
            released: false,
            busy: false,
            pending_damage: Vec::new(),
            client_pid,
        }
    }

    /// Get virtual address of buffer data
    pub fn data_ptr(&self, pool_base: u64) -> u64 {
        pool_base + self.data_offset as u64
    }

    /// Get size of buffer in bytes
    pub fn size_bytes(&self) -> usize {
        self.stride as usize * self.height as usize
    }
}

#[derive(Debug, Clone)]
pub struct ShmPool {
    pub id: u32,
    pub fd: i32, // memfd backing
    pub size: usize,
    pub base_addr: u64, // kernel virtual address of real allocated memory
    pub alloc_layout: Option<alloc::alloc::Layout>, // layout for deallocation
    pub client_pid: u32,
}
