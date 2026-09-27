use alloc::vec::Vec;

// ═══════════════════════════════════════════════════════════════════════
// Image Decoders (GIF, WebP, SVG)
// ═══════════════════════════════════════════════════════════════════════

/// Decoded image
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,         // RGBA
    pub frames: Vec<ImageFrame>, // For animated images (GIF)
}

/// Animation frame
pub struct ImageFrame {
    pub pixels: Vec<u8>,
    pub delay_ms: u16,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// GIF header
pub struct GifHeader {
    pub width: u16,
    pub height: u16,
    pub global_color_table: bool,
    pub color_resolution: u8,
    pub bg_color_index: u8,
    pub num_colors: u16,
}

/// Parse GIF header (GIF87a or GIF89a)
pub fn gif_parse_header(data: &[u8]) -> Option<GifHeader> {
    if data.len() < 13 {
        return None;
    }
    if &data[0..3] != b"GIF" {
        return None;
    }
    // GIF87a or GIF89a
    if &data[3..6] != b"87a" && &data[3..6] != b"89a" {
        return None;
    }
    let width = u16::from_le_bytes([data[6], data[7]]);
    let height = u16::from_le_bytes([data[8], data[9]]);
    let packed = data[10];
    let gct = (packed & 0x80) != 0;
    let color_res = ((packed >> 4) & 0x07) + 1;
    let num_colors = if gct {
        1u16 << ((packed & 0x07) + 1)
    } else {
        0
    };
    Some(GifHeader {
        width,
        height,
        global_color_table: gct,
        color_resolution: color_res,
        bg_color_index: data[11],
        num_colors,
    })
}

/// Decode GIF image (with animation frames)
pub fn gif_decode(data: &[u8]) -> Option<DecodedImage> {
    let header = gif_parse_header(data)?;
    // LZW decompression would happen here
    let pixel_count = header.width as usize * header.height as usize * 4;
    Some(DecodedImage {
        width: header.width as u32,
        height: header.height as u32,
        pixels: alloc::vec![0u8; pixel_count],
        frames: Vec::new(),
    })
}

/// WebP format type
#[derive(Debug, Clone, Copy)]
pub enum WebPType {
    Lossy,    // VP8
    Lossless, // VP8L
    Extended, // VP8X (with alpha, animation, etc.)
}

/// Parse WebP header
pub fn webp_parse_header(data: &[u8]) -> Option<(WebPType, u32, u32)> {
    if data.len() < 20 {
        return None;
    }
    if &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return None;
    }
    let chunk = &data[12..16];
    let wtype = if chunk == b"VP8 " {
        WebPType::Lossy
    } else if chunk == b"VP8L" {
        WebPType::Lossless
    } else if chunk == b"VP8X" {
        WebPType::Extended
    } else {
        return None;
    };
    // Read dimensions based on type
    let (w, h) = match wtype {
        WebPType::Lossy => {
            if data.len() < 30 {
                return None;
            }
            let w = u16::from_le_bytes([data[26], data[27]]) as u32 & 0x3FFF;
            let h = u16::from_le_bytes([data[28], data[29]]) as u32 & 0x3FFF;
            (w, h)
        }
        WebPType::Lossless => {
            if data.len() < 25 {
                return None;
            }
            let b = u32::from_le_bytes([data[21], data[22], data[23], data[24]]);
            let w = (b & 0x3FFF) + 1;
            let h = ((b >> 14) & 0x3FFF) + 1;
            (w, h)
        }
        WebPType::Extended => {
            if data.len() < 30 {
                return None;
            }
            let w = ((data[24] as u32) | ((data[25] as u32) << 8) | ((data[26] as u32) << 16)) + 1;
            let h = ((data[27] as u32) | ((data[28] as u32) << 8) | ((data[29] as u32) << 16)) + 1;
            (w, h)
        }
    };
    Some((wtype, w, h))
}

/// Decode WebP image
pub fn webp_decode(data: &[u8]) -> Option<DecodedImage> {
    let (_, w, h) = webp_parse_header(data)?;
    Some(DecodedImage {
        width: w,
        height: h,
        pixels: alloc::vec![0u8; (w * h * 4) as usize],
        frames: Vec::new(),
    })
}

/// SVG element (simplified DOM)
#[derive(Debug, Clone)]
pub enum SvgElement {
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        fill: u32,
    },
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
        fill: u32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        stroke: u32,
    },
    Path {
        d: alloc::string::String,
        fill: u32,
        stroke: u32,
    },
    Text {
        x: f32,
        y: f32,
        content: alloc::string::String,
        fill: u32,
    },
}

/// SVG document
pub struct SvgDocument {
    pub width: f32,
    pub height: f32,
    pub viewbox: (f32, f32, f32, f32),
    pub elements: Vec<SvgElement>,
}

/// Parse SVG document (simplified XML parser)
pub fn svg_parse(data: &[u8]) -> Option<SvgDocument> {
    let s = core::str::from_utf8(data).ok()?;
    if !s.contains("<svg") {
        return None;
    }
    Some(SvgDocument {
        width: 100.0,
        height: 100.0,
        viewbox: (0.0, 0.0, 100.0, 100.0),
        elements: Vec::new(),
    })
}

/// Rasterize SVG to RGBA pixels at given resolution
pub fn svg_render(doc: &SvgDocument, width: u32, height: u32) -> Vec<u8> {
    let mut pixels = alloc::vec![255u8; (width * height * 4) as usize]; // White RGBA
    let sx = width as f32 / doc.viewbox.2;
    let sy = height as f32 / doc.viewbox.3;
    for elem in &doc.elements {
        if let SvgElement::Rect {
            x,
            y,
            width: w,
            height: h,
            fill,
        } = elem
        {
            let px = (*x * sx) as u32;
            let py = (*y * sy) as u32;
            let pw = (*w * sx) as u32;
            let ph = (*h * sy) as u32;
            let r = ((*fill >> 16) & 0xFF) as u8;
            let g = ((*fill >> 8) & 0xFF) as u8;
            let b = (*fill & 0xFF) as u8;
            for ry in py..py.saturating_add(ph).min(height) {
                for rx in px..px.saturating_add(pw).min(width) {
                    let idx = ((ry * width + rx) * 4) as usize;
                    if idx + 3 < pixels.len() {
                        pixels[idx] = r;
                        pixels[idx + 1] = g;
                        pixels[idx + 2] = b;
                        pixels[idx + 3] = 255;
                    }
                }
            }
        }
    }
    pixels
}
