use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::serial_println;

use super::bitstream::BitReader;
use super::frame::VideoFrame;
use super::types::{FrameType, PixelFormat};

// ═══════════════════════════════════════════════════════════════════════
// H.265 / HEVC DECODER
// ═══════════════════════════════════════════════════════════════════════

/// H.265 NAL unit types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HevcNaluType {
    TrailN = 0,
    TrailR = 1,
    TsaN = 2,
    TsaR = 3,
    StsaN = 4,
    StsaR = 5,
    RadlN = 6,
    RadlR = 7,
    RaslN = 8,
    RaslR = 9,
    BlaWLp = 16,
    BlaWRadl = 17,
    BlaNLp = 18,
    IdrWRadl = 19,
    IdrNLp = 20,
    CraNut = 21,
    VpsNut = 32,
    SpsNut = 33,
    PpsNut = 34,
    AudNut = 35,
    EosNut = 36,
    EobNut = 37,
    FdNut = 38,
    PrefixSeiNut = 39,
    SuffixSeiNut = 40,
    Unknown,
}

impl HevcNaluType {
    pub fn from_u8(v: u8) -> Self {
        match (v >> 1) & 0x3F {
            0 => HevcNaluType::TrailN,
            1 => HevcNaluType::TrailR,
            19 => HevcNaluType::IdrWRadl,
            20 => HevcNaluType::IdrNLp,
            21 => HevcNaluType::CraNut,
            32 => HevcNaluType::VpsNut,
            33 => HevcNaluType::SpsNut,
            34 => HevcNaluType::PpsNut,
            35 => HevcNaluType::AudNut,
            39 => HevcNaluType::PrefixSeiNut,
            _ => HevcNaluType::Unknown,
        }
    }

    pub fn is_idr(&self) -> bool {
        matches!(self, HevcNaluType::IdrWRadl | HevcNaluType::IdrNLp)
    }

    pub fn is_irap(&self) -> bool {
        matches!(
            self,
            HevcNaluType::BlaWLp
                | HevcNaluType::BlaWRadl
                | HevcNaluType::BlaNLp
                | HevcNaluType::IdrWRadl
                | HevcNaluType::IdrNLp
                | HevcNaluType::CraNut
        )
    }
}

/// HEVC Video Parameter Set
#[derive(Debug, Clone)]
pub struct HevcVps {
    pub vps_id: u32,
    pub max_layers: u32,
    pub max_sub_layers: u32,
    pub temporal_id_nesting_flag: bool,
}

/// HEVC decoder state
pub struct HevcDecoder {
    vps_map: BTreeMap<u32, HevcVps>,
    width: u32,
    height: u32,
    frame_num: u64,
    reference_frames: Vec<VideoFrame>,
}

impl HevcDecoder {
    pub fn new() -> Self {
        Self {
            vps_map: BTreeMap::new(),
            width: 0,
            height: 0,
            frame_num: 0,
            reference_frames: Vec::new(),
        }
    }

    pub fn decode_nalu(&mut self, nalu: &[u8]) -> Option<VideoFrame> {
        if nalu.len() < 2 {
            return None;
        }

        let nalu_type = HevcNaluType::from_u8(nalu[0]);

        match nalu_type {
            HevcNaluType::VpsNut => {
                self.parse_vps(nalu);
                None
            }
            HevcNaluType::SpsNut => {
                self.parse_sps(nalu);
                None
            }
            HevcNaluType::PpsNut => None,
            _ if nalu_type.is_irap() => self.decode_slice(true),
            HevcNaluType::TrailN | HevcNaluType::TrailR => self.decode_slice(false),
            _ => None,
        }
    }

    fn parse_vps(&mut self, data: &[u8]) {
        if data.len() < 4 {
            return;
        }
        let vps = HevcVps {
            vps_id: ((data[2] >> 4) & 0x0F) as u32,
            max_layers: 1,
            max_sub_layers: 1,
            temporal_id_nesting_flag: true,
        };
        self.vps_map.insert(vps.vps_id, vps);
    }

    fn parse_sps(&mut self, data: &[u8]) {
        if data.len() < 8 {
            return;
        }
        // Simplified SPS parsing — read width/height from bitstream
        let mut reader = BitReader::new(&data[2..]);
        let _sps_id = reader.read_exp_golomb();
        let chroma_format_idc = reader.read_exp_golomb();
        if chroma_format_idc == 3 {
            let _separate_colour_plane = reader.read_bits(1);
        }
        self.width = reader.read_exp_golomb();
        self.height = reader.read_exp_golomb();

        if self.width == 0 {
            self.width = 1920;
        }
        if self.height == 0 {
            self.height = 1080;
        }

        serial_println!("[H.265] SPS: {}x{}", self.width, self.height);
    }

    fn decode_slice(&mut self, is_idr: bool) -> Option<VideoFrame> {
        if self.width == 0 || self.height == 0 {
            self.width = 1920;
            self.height = 1080;
        }

        let mut frame = VideoFrame::new(self.width, self.height, PixelFormat::Yuv420p);
        frame.frame_type = if is_idr { FrameType::I } else { FrameType::P };
        frame.key_frame = is_idr;
        frame.pts = self.frame_num as i64;

        self.frame_num += 1;
        Some(frame)
    }

    pub fn flush(&mut self) -> Vec<VideoFrame> {
        let frames = self.reference_frames.drain(..).collect();
        self.frame_num = 0;
        frames
    }
}
