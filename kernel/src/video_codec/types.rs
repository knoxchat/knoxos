// ═══════════════════════════════════════════════════════════════════════
// CODEC TYPES
// ═══════════════════════════════════════════════════════════════════════

/// Supported video codecs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    H264, // MPEG-4 AVC
    H265, // HEVC
    VP8,
    VP9,
    AV1,
    MJPEG,
    MPEG2,
    Raw,
}

impl VideoCodec {
    pub fn name(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "H.264/AVC",
            VideoCodec::H265 => "H.265/HEVC",
            VideoCodec::VP8 => "VP8",
            VideoCodec::VP9 => "VP9",
            VideoCodec::AV1 => "AV1",
            VideoCodec::MJPEG => "MJPEG",
            VideoCodec::MPEG2 => "MPEG-2",
            VideoCodec::Raw => "Raw",
        }
    }

    pub fn fourcc(&self) -> u32 {
        match self {
            VideoCodec::H264 => fourcc(b"H264"),
            VideoCodec::H265 => fourcc(b"H265"),
            VideoCodec::VP8 => fourcc(b"VP80"),
            VideoCodec::VP9 => fourcc(b"VP90"),
            VideoCodec::AV1 => fourcc(b"AV01"),
            VideoCodec::MJPEG => fourcc(b"MJPG"),
            VideoCodec::MPEG2 => fourcc(b"MPG2"),
            VideoCodec::Raw => fourcc(b"RAWV"),
        }
    }
}

const fn fourcc(s: &[u8; 4]) -> u32 {
    (s[0] as u32) | ((s[1] as u32) << 8) | ((s[2] as u32) << 16) | ((s[3] as u32) << 24)
}

/// Pixel format for decoded frames
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Yuv420p, // Planar YUV 4:2:0
    Yuv422p, // Planar YUV 4:2:2
    Yuv444p, // Planar YUV 4:4:4
    Nv12,    // Semi-planar NV12 (Y + interleaved UV)
    Nv21,    // Semi-planar NV21 (Y + interleaved VU)
    Rgb24,   // Packed RGB (3 bytes/pixel)
    Bgr24,   // Packed BGR
    Rgba32,  // Packed RGBA (4 bytes/pixel)
    Bgra32,  // Packed BGRA
    Yuyv,    // Packed YUYV 4:2:2
    Uyvy,    // Packed UYVY 4:2:2
}

impl PixelFormat {
    pub fn bits_per_pixel(&self) -> u32 {
        match self {
            PixelFormat::Yuv420p => 12,
            PixelFormat::Yuv422p | PixelFormat::Yuyv | PixelFormat::Uyvy => 16,
            PixelFormat::Yuv444p | PixelFormat::Rgb24 | PixelFormat::Bgr24 => 24,
            PixelFormat::Nv12 | PixelFormat::Nv21 => 12,
            PixelFormat::Rgba32 | PixelFormat::Bgra32 => 32,
        }
    }

    pub fn plane_count(&self) -> usize {
        match self {
            PixelFormat::Yuv420p | PixelFormat::Yuv422p | PixelFormat::Yuv444p => 3,
            PixelFormat::Nv12 | PixelFormat::Nv21 => 2,
            _ => 1,
        }
    }
}

/// Video frame type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    I, // Intra (keyframe)
    P, // Predicted
    B, // Bi-directional
    S, // Switching
}

/// Decoder profile
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H264Profile {
    Baseline,
    Main,
    High,
    High10,
    High422,
    High444,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H265Profile {
    Main,
    Main10,
    MainStillPicture,
    Rext,
}
