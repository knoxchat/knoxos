use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::types::{PixelFormat, VideoCodec};

// ═══════════════════════════════════════════════════════════════════════
// V4L2 INTERFACE
// ═══════════════════════════════════════════════════════════════════════

/// V4L2-compatible buffer type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4l2BufType {
    VideoCapture = 1,
    VideoOutput = 2,
    VideoOverlay = 3,
    VideoCaptureMplane = 9,
    VideoOutputMplane = 10,
}

/// V4L2-compatible memory model
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4l2Memory {
    Mmap = 1,
    UserPtr = 2,
    Overlay = 3,
    DmaBuf = 4,
}

/// V4L2 device capability flags
pub const V4L2_CAP_VIDEO_CAPTURE: u32 = 0x00000001;
pub const V4L2_CAP_VIDEO_OUTPUT: u32 = 0x00000002;
pub const V4L2_CAP_STREAMING: u32 = 0x04000000;

/// V4L2 buffer
#[derive(Debug, Clone)]
pub struct V4l2Buffer {
    pub index: u32,
    pub buf_type: V4l2BufType,
    pub memory: V4l2Memory,
    pub length: u32,
    pub bytesused: u32,
    pub flags: u32,
    pub timestamp: u64,
    pub sequence: u32,
    pub data: Vec<u8>,
}

/// V4L2 video device
pub struct V4l2Device {
    pub name: String,
    pub driver: String,
    pub bus: String,
    pub capabilities: u32,
    pub buffers: Vec<V4l2Buffer>,
    pub streaming: bool,
    pub codec: Option<VideoCodec>,
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,
}

impl V4l2Device {
    pub fn new(name: &str, codec: VideoCodec) -> Self {
        Self {
            name: String::from(name),
            driver: String::from("knoxos-v4l2"),
            bus: String::from("platform:knoxos"),
            capabilities: V4L2_CAP_VIDEO_CAPTURE | V4L2_CAP_VIDEO_OUTPUT | V4L2_CAP_STREAMING,
            buffers: Vec::new(),
            streaming: false,
            codec: Some(codec),
            width: 1920,
            height: 1080,
            pixel_format: PixelFormat::Yuv420p,
        }
    }

    /// Request buffers
    pub fn reqbufs(&mut self, count: u32, buf_type: V4l2BufType, memory: V4l2Memory) -> u32 {
        self.buffers.clear();
        let size = (self.width * self.height * self.pixel_format.bits_per_pixel() / 8) as usize;
        for i in 0..count {
            self.buffers.push(V4l2Buffer {
                index: i,
                buf_type,
                memory,
                length: size as u32,
                bytesused: 0,
                flags: 0,
                timestamp: 0,
                sequence: 0,
                data: vec![0u8; size],
            });
        }
        count
    }

    /// Queue buffer
    pub fn qbuf(&mut self, index: u32) -> bool {
        if (index as usize) < self.buffers.len() {
            self.buffers[index as usize].flags |= 0x01; // queued
            true
        } else {
            false
        }
    }

    /// Dequeue buffer
    pub fn dqbuf(&mut self) -> Option<u32> {
        for buf in &mut self.buffers {
            if buf.flags & 0x01 != 0 {
                buf.flags &= !0x01;
                return Some(buf.index);
            }
        }
        None
    }

    /// Start streaming
    pub fn streamon(&mut self) -> bool {
        self.streaming = true;
        true
    }

    /// Stop streaming
    pub fn streamoff(&mut self) -> bool {
        self.streaming = false;
        true
    }
}
