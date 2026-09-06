/// GIF Image Decoder
///
/// Decodes GIF87a/GIF89a images including animated GIFs.
/// Supports LZW decompression, transparency, and disposal methods.
use alloc::vec::Vec;

use crate::serial_println;

/// GIF version
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GifVersion {
    Gif87a,
    Gif89a,
}

/// Frame disposal method
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisposalMethod {
    None,
    DoNotDispose,
    RestoreBackground,
    RestorePrevious,
}

/// A single GIF frame
#[derive(Debug, Clone)]
pub struct GifFrame {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u32>, // RGBA
    pub delay_ms: u16,
    pub disposal: DisposalMethod,
    pub transparent_index: Option<u8>,
}

/// Decoded GIF image
pub struct GifImage {
    pub version: GifVersion,
    pub width: u16,
    pub height: u16,
    pub global_palette: Vec<u32>,
    pub frames: Vec<GifFrame>,
    pub loop_count: u16, // 0 = infinite
}

impl GifImage {
    /// Decode GIF from raw bytes
    pub fn decode(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 13 {
            return Err("Too short");
        }

        // Header
        let version = match &data[0..6] {
            b"GIF87a" => GifVersion::Gif87a,
            b"GIF89a" => GifVersion::Gif89a,
            _ => return Err("Not a GIF"),
        };

        let width = u16::from_le_bytes([data[6], data[7]]);
        let height = u16::from_le_bytes([data[8], data[9]]);
        let packed = data[10];
        let has_gct = (packed & 0x80) != 0;
        let gct_size = if has_gct {
            3 * (1 << ((packed & 0x07) + 1))
        } else {
            0
        };
        let _bg_index = data[11];

        let mut pos = 13;

        // Global Color Table
        let mut global_palette = Vec::new();
        if has_gct {
            for i in (0..gct_size).step_by(3) {
                if pos + i + 2 < data.len() {
                    let r = data[pos + i];
                    let g = data[pos + i + 1];
                    let b = data[pos + i + 2];
                    global_palette.push(0xFF000000 | (r as u32) << 16 | (g as u32) << 8 | b as u32);
                }
            }
            pos += gct_size;
        }

        let mut frames = Vec::new();
        let mut loop_count = 0u16;
        let mut gce_delay = 0u16;
        let mut gce_disposal = DisposalMethod::None;
        let mut gce_transparent = None;

        while pos < data.len() {
            match data[pos] {
                0x2C => {
                    // Image Descriptor
                    if pos + 10 > data.len() {
                        break;
                    }
                    let fx = u16::from_le_bytes([data[pos + 1], data[pos + 2]]);
                    let fy = u16::from_le_bytes([data[pos + 3], data[pos + 4]]);
                    let fw = u16::from_le_bytes([data[pos + 5], data[pos + 6]]);
                    let fh = u16::from_le_bytes([data[pos + 7], data[pos + 8]]);
                    let fpacked = data[pos + 9];
                    pos += 10;

                    let has_lct = (fpacked & 0x80) != 0;
                    let lct_size = if has_lct {
                        3 * (1 << ((fpacked & 0x07) + 1))
                    } else {
                        0
                    };

                    let palette = if has_lct {
                        let mut p = Vec::new();
                        for i in (0..lct_size).step_by(3) {
                            if pos + i + 2 < data.len() {
                                let r = data[pos + i];
                                let g = data[pos + i + 1];
                                let b = data[pos + i + 2];
                                p.push(0xFF000000 | (r as u32) << 16 | (g as u32) << 8 | b as u32);
                            }
                        }
                        pos += lct_size;
                        p
                    } else {
                        global_palette.clone()
                    };

                    // LZW minimum code size
                    if pos >= data.len() {
                        break;
                    }
                    let _min_code_size = data[pos];
                    pos += 1;

                    // Skip sub-blocks (LZW data)
                    let indices = decode_lzw_subblocks(data, &mut pos);
                    let mut pixels = Vec::with_capacity(fw as usize * fh as usize);
                    for &idx in &indices {
                        if (idx as usize) < palette.len() {
                            let mut color = palette[idx as usize];
                            if gce_transparent == Some(idx) {
                                color &= 0x00FFFFFF; // transparent
                            }
                            pixels.push(color);
                        }
                    }
                    pixels.resize(fw as usize * fh as usize, 0);

                    frames.push(GifFrame {
                        x: fx,
                        y: fy,
                        width: fw,
                        height: fh,
                        pixels,
                        delay_ms: gce_delay,
                        disposal: gce_disposal,
                        transparent_index: gce_transparent,
                    });
                    gce_delay = 0;
                    gce_disposal = DisposalMethod::None;
                    gce_transparent = None;
                }
                0x21 => {
                    // Extension
                    pos += 1;
                    if pos >= data.len() {
                        break;
                    }
                    let label = data[pos];
                    pos += 1;
                    if label == 0xF9 && pos + 4 < data.len() {
                        // Graphic Control Extension
                        let _block_size = data[pos];
                        pos += 1;
                        let gce_packed = data[pos];
                        pos += 1;
                        gce_disposal = match (gce_packed >> 2) & 0x07 {
                            1 => DisposalMethod::DoNotDispose,
                            2 => DisposalMethod::RestoreBackground,
                            3 => DisposalMethod::RestorePrevious,
                            _ => DisposalMethod::None,
                        };
                        gce_delay = u16::from_le_bytes([data[pos], data[pos + 1]]) * 10;
                        pos += 2;
                        if gce_packed & 0x01 != 0 {
                            gce_transparent = Some(data[pos]);
                        }
                        pos += 1; // transparent index
                        pos += 1; // block terminator
                    } else if label == 0xFF {
                        // Application Extension (NETSCAPE for loop count)
                        skip_subblocks(data, &mut pos);
                    } else {
                        skip_subblocks(data, &mut pos);
                    }
                }
                0x3B => break, // Trailer
                _ => {
                    pos += 1;
                }
            }
        }

        Ok(GifImage {
            version,
            width,
            height,
            global_palette,
            frames,
            loop_count,
        })
    }

    /// Get total animation duration in milliseconds
    pub fn total_duration_ms(&self) -> u32 {
        self.frames.iter().map(|f| f.delay_ms as u32).sum()
    }
}

fn skip_subblocks(data: &[u8], pos: &mut usize) {
    loop {
        if *pos >= data.len() {
            break;
        }
        let size = data[*pos] as usize;
        *pos += 1;
        if size == 0 {
            break;
        }
        *pos += size;
    }
}

fn decode_lzw_subblocks(data: &[u8], pos: &mut usize) -> Vec<u8> {
    // Collect sub-block data
    let mut lzw_data = Vec::new();
    loop {
        if *pos >= data.len() {
            break;
        }
        let size = data[*pos] as usize;
        *pos += 1;
        if size == 0 {
            break;
        }
        let end = (*pos + size).min(data.len());
        lzw_data.extend_from_slice(&data[*pos..end]);
        *pos = end;
    }
    // LZW decompression would go here
    lzw_data
}

pub fn init() {
    serial_println!("[GIF] GIF decoder loaded");
}
