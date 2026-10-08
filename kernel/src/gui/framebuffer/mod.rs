/// Framebuffer - Direct pixel manipulation for the display
/// Provides a software framebuffer with double-buffering for tear-free rendering
use alloc::vec;
use alloc::vec::Vec;

mod aa;
mod blit;
mod clip;
mod damage;
mod fill;
mod gradient;
mod lines;
mod pixel;
mod plot;
mod present;
mod rect;
mod shapes;

pub use aa::isqrt_x16_pub;
pub use pixel::Pixel;
pub use rect::Rect;

/// Software framebuffer with double-buffering
pub struct FrameBuffer {
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub bytes_per_pixel: usize,
    /// Back buffer (we draw here, then present to HW framebuffer)
    pub buffer: Vec<u8>,
    /// Physical/virtual framebuffer address from bootloader
    pub framebuffer_addr: usize,
    /// Length of the HW framebuffer mapping
    pub framebuffer_len: usize,
    /// HW framebuffer bytes per pixel (may differ from back buffer)
    pub hw_bytes_per_pixel: usize,
    /// HW framebuffer stride in pixels
    pub hw_stride: usize,
    /// Whether we have a real HW framebuffer to present to
    pub use_hw_framebuffer: bool,
    /// Clip rectangle stack — drawing is restricted to the topmost clip rect.
    /// When empty, the entire framebuffer is the clip region.
    clip_stack: Vec<Rect>,
    /// Accumulated dirty region (union of all modified areas since last reset).
    /// Used for damage-tracking to present only changed pixels.
    dirty: Option<Rect>,
}

impl FrameBuffer {
    pub fn new(width: usize, height: usize) -> Self {
        let bpp = 4; // 32bpp BGRA internal format for easy alpha blending
        let pitch = width * bpp;
        let buffer_size = pitch * height;
        Self {
            width,
            height,
            pitch,
            bytes_per_pixel: bpp,
            buffer: vec![0u8; buffer_size],
            framebuffer_addr: 0,
            framebuffer_len: 0,
            hw_bytes_per_pixel: 0,
            hw_stride: 0,
            use_hw_framebuffer: false,
            clip_stack: Vec::new(),
            dirty: None,
        }
    }
}
