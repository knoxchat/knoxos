use alloc::vec;
use alloc::vec::Vec;

use super::types::{FrameType, PixelFormat};
use super::util::clamp_u8;

// ═══════════════════════════════════════════════════════════════════════
// VIDEO FRAME
// ═══════════════════════════════════════════════════════════════════════

/// A decoded video frame
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub frame_type: FrameType,
    pub pts: i64, // presentation timestamp (in timebase units)
    pub dts: i64, // decoding timestamp
    pub duration: u64,
    pub key_frame: bool,
    pub planes: Vec<Vec<u8>>, // plane data
    pub linesize: Vec<u32>,   // bytes per row per plane
}

impl VideoFrame {
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Self {
        let mut planes = Vec::new();
        let mut linesize = Vec::new();

        match format {
            PixelFormat::Yuv420p => {
                let y_size = (width * height) as usize;
                let uv_size = (width / 2 * height / 2) as usize;
                planes.push(vec![0u8; y_size]);
                planes.push(vec![128u8; uv_size]);
                planes.push(vec![128u8; uv_size]);
                linesize.push(width);
                linesize.push(width / 2);
                linesize.push(width / 2);
            }
            PixelFormat::Nv12 => {
                let y_size = (width * height) as usize;
                let uv_size = (width * height / 2) as usize;
                planes.push(vec![0u8; y_size]);
                planes.push(vec![128u8; uv_size]);
                linesize.push(width);
                linesize.push(width);
            }
            PixelFormat::Rgb24 | PixelFormat::Bgr24 => {
                let size = (width * height * 3) as usize;
                planes.push(vec![0u8; size]);
                linesize.push(width * 3);
            }
            PixelFormat::Rgba32 | PixelFormat::Bgra32 => {
                let size = (width * height * 4) as usize;
                planes.push(vec![0u8; size]);
                linesize.push(width * 4);
            }
            _ => {
                let bpp = format.bits_per_pixel();
                let size = (width * height * bpp / 8) as usize;
                planes.push(vec![0u8; size]);
                linesize.push(width * bpp / 8);
            }
        }

        Self {
            width,
            height,
            format,
            frame_type: FrameType::I,
            pts: 0,
            dts: 0,
            duration: 0,
            key_frame: true,
            planes,
            linesize,
        }
    }

    /// Convert YUV420P frame to BGRA32
    pub fn to_bgra(&self) -> Vec<u8> {
        if self.format != PixelFormat::Yuv420p || self.planes.len() < 3 {
            return self.planes.first().cloned().unwrap_or_default();
        }

        let w = self.width as usize;
        let h = self.height as usize;
        let mut bgra = vec![0u8; w * h * 4];

        let y_plane = &self.planes[0];
        let u_plane = &self.planes[1];
        let v_plane = &self.planes[2];

        for row in 0..h {
            for col in 0..w {
                let y_idx = row * w + col;
                let uv_idx = (row / 2) * (w / 2) + (col / 2);

                let y = y_plane.get(y_idx).copied().unwrap_or(16) as i32;
                let u = u_plane.get(uv_idx).copied().unwrap_or(128) as i32;
                let v = v_plane.get(uv_idx).copied().unwrap_or(128) as i32;

                // BT.601 conversion
                let c = y - 16;
                let d = u - 128;
                let e = v - 128;

                let r = clamp_u8((298 * c + 409 * e + 128) >> 8);
                let g = clamp_u8((298 * c - 100 * d - 208 * e + 128) >> 8);
                let b = clamp_u8((298 * c + 516 * d + 128) >> 8);

                let out_idx = (row * w + col) * 4;
                bgra[out_idx] = b;
                bgra[out_idx + 1] = g;
                bgra[out_idx + 2] = r;
                bgra[out_idx + 3] = 255;
            }
        }

        bgra
    }
}
