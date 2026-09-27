use super::frame::VideoFrame;
use super::types::{FrameType, PixelFormat};

// ═══════════════════════════════════════════════════════════════════════
// MJPEG DECODER
// ═══════════════════════════════════════════════════════════════════════

/// JPEG markers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JpegMarker {
    Soi = 0xFFD8,  // Start of Image
    Eoi = 0xFFD9,  // End of Image
    Sof0 = 0xFFC0, // Start of Frame (Baseline DCT)
    Sof2 = 0xFFC2, // Progressive DCT
    Dht = 0xFFC4,  // Define Huffman Table
    Dqt = 0xFFDB,  // Define Quantization Table
    Dri = 0xFFDD,  // Define Restart Interval
    Sos = 0xFFDA,  // Start of Scan
    App0 = 0xFFE0, // JFIF
    App1 = 0xFFE1, // EXIF
    Com = 0xFFFE,  // Comment
}

pub struct MjpegDecoder {
    width: u32,
    height: u32,
    frame_num: u64,
}

impl MjpegDecoder {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            frame_num: 0,
        }
    }

    pub fn decode_frame(&mut self, data: &[u8]) -> Option<VideoFrame> {
        // Validate SOI marker
        if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
            return None;
        }

        // Parse markers to find SOF0 for dimensions
        let mut i = 2;
        while i + 3 < data.len() {
            if data[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = data[i + 1];
            let length = if i + 3 < data.len() {
                ((data[i + 2] as usize) << 8) | (data[i + 3] as usize)
            } else {
                break;
            };

            if marker == 0xC0 || marker == 0xC2 {
                // SOF0 or SOF2 — contains dimensions
                if i + 9 < data.len() {
                    let _precision = data[i + 4];
                    self.height = ((data[i + 5] as u32) << 8) | (data[i + 6] as u32);
                    self.width = ((data[i + 7] as u32) << 8) | (data[i + 8] as u32);
                }
                break;
            }

            i += 2 + length;
        }

        if self.width == 0 || self.height == 0 {
            self.width = 640;
            self.height = 480;
        }

        let mut frame = VideoFrame::new(self.width, self.height, PixelFormat::Yuv420p);
        frame.frame_type = FrameType::I;
        frame.key_frame = true;
        frame.pts = self.frame_num as i64;

        self.frame_num += 1;
        Some(frame)
    }
}
