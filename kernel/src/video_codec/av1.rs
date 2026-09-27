use crate::serial_println;

use super::bitstream::{BitReader, read_leb128};
use super::frame::VideoFrame;
use super::types::{FrameType, PixelFormat};

// ═══════════════════════════════════════════════════════════════════════
// AV1 DECODER
// ═══════════════════════════════════════════════════════════════════════

/// AV1 OBU (Open Bitstream Unit) types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Av1ObuType {
    Reserved = 0,
    SequenceHeader = 1,
    TemporalDelimiter = 2,
    FrameHeader = 3,
    TileGroup = 4,
    Metadata = 5,
    Frame = 6,
    RedundantFrameHeader = 7,
    TileList = 8,
    Padding = 15,
    Unknown,
}

impl Av1ObuType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Av1ObuType::Reserved,
            1 => Av1ObuType::SequenceHeader,
            2 => Av1ObuType::TemporalDelimiter,
            3 => Av1ObuType::FrameHeader,
            4 => Av1ObuType::TileGroup,
            5 => Av1ObuType::Metadata,
            6 => Av1ObuType::Frame,
            7 => Av1ObuType::RedundantFrameHeader,
            8 => Av1ObuType::TileList,
            15 => Av1ObuType::Padding,
            _ => Av1ObuType::Unknown,
        }
    }
}

/// AV1 sequence header
#[derive(Debug, Clone)]
pub struct Av1SequenceHeader {
    pub profile: u8,
    pub still_picture: bool,
    pub max_frame_width: u32,
    pub max_frame_height: u32,
    pub bit_depth: u8,
    pub monochrome: bool,
    pub color_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
    pub chroma_sample_position: u8,
    pub film_grain_params_present: bool,
}

pub struct Av1Decoder {
    seq_header: Option<Av1SequenceHeader>,
    frame_num: u64,
}

impl Av1Decoder {
    pub fn new() -> Self {
        Self {
            seq_header: None,
            frame_num: 0,
        }
    }

    /// Parse OBU from bitstream
    pub fn parse_obu(data: &[u8]) -> Option<(Av1ObuType, bool, usize, usize)> {
        if data.is_empty() {
            return None;
        }

        let obu_type = Av1ObuType::from_u8((data[0] >> 3) & 0x0F);
        let has_extension = (data[0] >> 2) & 0x01 != 0;
        let has_size = (data[0] >> 1) & 0x01 != 0;

        let mut offset = 1;
        if has_extension {
            offset += 1;
        }

        let obu_size = if has_size && offset < data.len() {
            // Read LEB128 size
            let (size, bytes_read) = read_leb128(&data[offset..]);
            offset += bytes_read;
            size as usize
        } else {
            data.len() - offset
        };

        Some((obu_type, has_extension, offset, obu_size))
    }

    pub fn decode_obu(&mut self, data: &[u8]) -> Option<VideoFrame> {
        let (obu_type, _, header_size, obu_size) = Self::parse_obu(data)?;

        match obu_type {
            Av1ObuType::SequenceHeader => {
                self.parse_sequence_header(&data[header_size..header_size + obu_size]);
                None
            }
            Av1ObuType::Frame | Av1ObuType::FrameHeader => {
                self.decode_frame_obu(&data[header_size..header_size + obu_size])
            }
            _ => None,
        }
    }

    fn parse_sequence_header(&mut self, data: &[u8]) {
        if data.len() < 4 {
            return;
        }

        let mut reader = BitReader::new(data);
        let profile = reader.read_bits(3) as u8;
        let still_picture = reader.read_bits(1) != 0;
        let _reduced_still_picture_header = reader.read_bits(1) != 0;

        self.seq_header = Some(Av1SequenceHeader {
            profile,
            still_picture,
            max_frame_width: 1920,
            max_frame_height: 1080,
            bit_depth: 8,
            monochrome: false,
            color_primaries: 1,
            transfer_characteristics: 1,
            matrix_coefficients: 1,
            chroma_sample_position: 0,
            film_grain_params_present: false,
        });

        serial_println!("[AV1] Sequence header: profile {}", profile);
    }

    fn decode_frame_obu(&mut self, _data: &[u8]) -> Option<VideoFrame> {
        let seq = self.seq_header.as_ref()?;
        let width = seq.max_frame_width;
        let height = seq.max_frame_height;

        let mut frame = VideoFrame::new(width, height, PixelFormat::Yuv420p);
        frame.pts = self.frame_num as i64;
        frame.key_frame = self.frame_num == 0;
        frame.frame_type = if frame.key_frame {
            FrameType::I
        } else {
            FrameType::P
        };

        self.frame_num += 1;
        Some(frame)
    }
}
