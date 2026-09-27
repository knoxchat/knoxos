use super::frame::VideoFrame;
use super::types::{FrameType, PixelFormat};

// ═══════════════════════════════════════════════════════════════════════
// VP9 DECODER
// ═══════════════════════════════════════════════════════════════════════

/// VP9 frame header
#[derive(Debug, Clone)]
pub struct Vp9FrameHeader {
    pub profile: u8,
    pub show_existing_frame: bool,
    pub frame_type: u8, // 0 = key, 1 = inter
    pub show_frame: bool,
    pub error_resilient: bool,
    pub width: u32,
    pub height: u32,
    pub render_width: u32,
    pub render_height: u32,
    pub bit_depth: u8,
    pub color_space: u8,
}

pub struct Vp9Decoder {
    width: u32,
    height: u32,
    frame_num: u64,
    reference_frames: [Option<VideoFrame>; 8],
}

impl Vp9Decoder {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            frame_num: 0,
            reference_frames: Default::default(),
        }
    }

    pub fn decode_frame(&mut self, data: &[u8]) -> Option<VideoFrame> {
        let header = self.parse_frame_header(data)?;

        self.width = header.width;
        self.height = header.height;

        let mut frame = VideoFrame::new(self.width, self.height, PixelFormat::Yuv420p);
        frame.frame_type = if header.frame_type == 0 {
            FrameType::I
        } else {
            FrameType::P
        };
        frame.key_frame = header.frame_type == 0;
        frame.pts = self.frame_num as i64;

        self.frame_num += 1;
        Some(frame)
    }

    fn parse_frame_header(&self, data: &[u8]) -> Option<Vp9FrameHeader> {
        if data.len() < 3 {
            return None;
        }

        // VP9 frame marker check
        let marker = (data[0] >> 6) & 0x03;
        if marker != 2 {
            return None;
        }

        let profile = ((data[0] >> 4) & 0x01) | (((data[0] >> 5) & 0x01) << 1);
        let show_existing = (data[0] >> 3) & 0x01 != 0;

        if show_existing {
            return Some(Vp9FrameHeader {
                profile,
                show_existing_frame: true,
                frame_type: 1,
                show_frame: true,
                error_resilient: false,
                width: self.width,
                height: self.height,
                render_width: self.width,
                render_height: self.height,
                bit_depth: 8,
                color_space: 1,
            });
        }

        let frame_type = (data[0] >> 2) & 0x01;
        let show_frame = (data[0] >> 1) & 0x01 != 0;
        let error_resilient = data[0] & 0x01 != 0;

        Some(Vp9FrameHeader {
            profile,
            show_existing_frame: false,
            frame_type,
            show_frame,
            error_resilient,
            width: if self.width > 0 { self.width } else { 1920 },
            height: if self.height > 0 { self.height } else { 1080 },
            render_width: if self.width > 0 { self.width } else { 1920 },
            render_height: if self.height > 0 { self.height } else { 1080 },
            bit_depth: 8,
            color_space: 1,
        })
    }
}
