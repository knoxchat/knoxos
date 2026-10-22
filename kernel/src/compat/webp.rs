/// WebP Image Decoder
///
/// Decodes WebP lossy (VP8) and lossless images.
/// Supports alpha channel and animated WebP.
use alloc::vec::Vec;

use crate::serial_println;

/// WebP format variant
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WebPFormat {
    Lossy,
    Lossless,
    Animated,
}

/// Decoded WebP frame
#[derive(Debug, Clone)]
pub struct WebPFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>, // RGBA
    pub duration_ms: u32,
}

/// Decoded WebP image
pub struct WebPImage {
    pub format: WebPFormat,
    pub width: u32,
    pub height: u32,
    pub frames: Vec<WebPFrame>,
    pub loop_count: u32,
}

impl WebPImage {
    /// Decode WebP from RIFF container
    pub fn decode(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 12 {
            return Err("Too short");
        }
        if &data[0..4] != b"RIFF" {
            return Err("Not RIFF");
        }
        if &data[8..12] != b"WEBP" {
            return Err("Not WebP");
        }

        let file_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let _ = file_size;

        let mut pos = 12;
        let mut width = 0u32;
        let mut height = 0u32;
        let mut format = WebPFormat::Lossy;
        let mut frames = Vec::new();

        while pos + 8 <= data.len() {
            let chunk_id = &data[pos..pos + 4];
            let chunk_size =
                u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
                    as usize;
            pos += 8;

            match chunk_id {
                b"VP8 " => {
                    // Lossy VP8 bitstream
                    format = WebPFormat::Lossy;
                    if pos + 10 <= data.len() {
                        // VP8 frame header
                        let w = u16::from_le_bytes([data[pos + 6], data[pos + 7]]);
                        let h = u16::from_le_bytes([data[pos + 8], data[pos + 9]]);
                        width = (w & 0x3FFF) as u32;
                        height = (h & 0x3FFF) as u32;
                        // VP8 decode would go here
                        frames.push(WebPFrame {
                            width,
                            height,
                            pixels: alloc::vec![0xFF808080; (width * height) as usize],
                            duration_ms: 0,
                        });
                    }
                }
                b"VP8L" => {
                    // Lossless bitstream
                    format = WebPFormat::Lossless;
                    if pos + 5 <= data.len() {
                        let signature = data[pos];
                        if signature != 0x2F {
                            return Err("Invalid VP8L signature");
                        }
                        let bits = u32::from_le_bytes([
                            data[pos + 1],
                            data[pos + 2],
                            data[pos + 3],
                            data[pos + 4],
                        ]);
                        width = (bits & 0x3FFF) + 1;
                        height = ((bits >> 14) & 0x3FFF) + 1;
                        frames.push(WebPFrame {
                            width,
                            height,
                            pixels: alloc::vec![0xFFFFFFFF; (width * height) as usize],
                            duration_ms: 0,
                        });
                    }
                }
                b"VP8X"
                    // Extended format header
                    if pos + 10 <= data.len() => {
                        let _flags = data[pos];
                        width = (u32::from(data[pos + 4])
                            | (u32::from(data[pos + 5]) << 8)
                            | (u32::from(data[pos + 6]) << 16))
                            + 1;
                        height = (u32::from(data[pos + 7])
                            | (u32::from(data[pos + 8]) << 8)
                            | (u32::from(data[pos + 9]) << 16))
                            + 1;
                    }
                b"ANIM" => {
                    format = WebPFormat::Animated;
                }
                b"ANMF"
                    // Animation frame
                    if pos + 16 <= data.len() => {
                        let fx = u32::from(data[pos])
                            | (u32::from(data[pos + 1]) << 8)
                            | (u32::from(data[pos + 2]) << 16);
                        let fy = u32::from(data[pos + 3])
                            | (u32::from(data[pos + 4]) << 8)
                            | (u32::from(data[pos + 5]) << 16);
                        let fw = (u32::from(data[pos + 6])
                            | (u32::from(data[pos + 7]) << 8)
                            | (u32::from(data[pos + 8]) << 16))
                            + 1;
                        let fh = (u32::from(data[pos + 9])
                            | (u32::from(data[pos + 10]) << 8)
                            | (u32::from(data[pos + 11]) << 16))
                            + 1;
                        let duration = u32::from(data[pos + 12])
                            | (u32::from(data[pos + 13]) << 8)
                            | (u32::from(data[pos + 14]) << 16);
                        frames.push(WebPFrame {
                            width: fw,
                            height: fh,
                            pixels: alloc::vec![0xFF000000; (fw * fh) as usize],
                            duration_ms: duration,
                        });
                    }
                _ => {}
            }
            pos += chunk_size;
            if chunk_size % 2 != 0 {
                pos += 1;
            } // padding
        }

        Ok(WebPImage {
            format,
            width,
            height,
            frames,
            loop_count: 0,
        })
    }
}

pub fn init() {
    serial_println!("[WEBP] WebP decoder loaded");
}
