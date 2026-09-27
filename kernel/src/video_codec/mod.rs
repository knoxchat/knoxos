/// Video Codec Support
///
/// Provides software video encoding/decoding for media playback and streaming.
/// Implements common video codecs used in Linux media applications.
///
/// Features:
///   - H.264/AVC decoder (baseline profile)
///   - H.265/HEVC decoder (main profile)
///   - VP8/VP9 decoder
///   - AV1 decoder (OBU parsing)
///   - MJPEG decoder
///   - V4L2-compatible interface
///   - Frame buffer management
///   - YUV ↔ RGB color space conversion
///   - Bitstream parsing (NALU, OBU)
///   - DMA-BUF integration stubs
///
/// Split into submodules for maintainability:
///   types     — Codec, pixel format, frame type, profiles
///   frame     — Decoded video frame and plane layout
///   bitstream — Bit reader and LEB128 helpers
///   h264      — H.264/AVC NALU parser and decoder
///   hevc      — H.265/HEVC NALU parser and decoder
///   vp9       — VP9 frame header parser and decoder
///   av1       — AV1 OBU parser and decoder
///   mjpeg     — MJPEG/JPEG marker parser and decoder
///   v4l2      — V4L2-compatible device and buffer interface
///   color     — YUV ↔ RGB conversion
///   registry  — Codec registration, V4L2 devices, init
///   container — MP4/MKV/WebM/AVI demuxer
///   audio     — MP3, AAC, OGG, FLAC parsers
///   image     — GIF, WebP, SVG decoders
mod audio;
mod av1;
mod bitstream;
mod color;
mod container;
mod frame;
mod h264;
mod hevc;
mod image;
mod mjpeg;
mod registry;
mod types;
mod util;
mod v4l2;
mod vp9;

pub use audio::*;
pub use av1::*;
pub use color::*;
pub use container::*;
pub use frame::*;
pub use h264::*;
pub use hevc::*;
pub use image::*;
pub use mjpeg::*;
pub use registry::*;
pub use types::*;
pub use v4l2::*;
pub use vp9::*;
