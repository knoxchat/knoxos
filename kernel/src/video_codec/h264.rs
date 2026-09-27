use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::serial_println;

use super::bitstream::BitReader;
use super::frame::VideoFrame;
use super::types::{FrameType, PixelFormat};

// ═══════════════════════════════════════════════════════════════════════
// H.264 / AVC DECODER
// ═══════════════════════════════════════════════════════════════════════

/// H.264 NAL unit types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NaluType {
    Slice = 1,
    DataPartA = 2,
    DataPartB = 3,
    DataPartC = 4,
    Idr = 5,
    Sei = 6,
    Sps = 7,
    Pps = 8,
    AccessUnitDelimiter = 9,
    EndOfSequence = 10,
    EndOfStream = 11,
    FillerData = 12,
    SpsExtension = 13,
    Prefix = 14,
    SubsetSps = 15,
    Unknown,
}

impl NaluType {
    pub fn from_u8(v: u8) -> Self {
        match v & 0x1F {
            1 => NaluType::Slice,
            2 => NaluType::DataPartA,
            3 => NaluType::DataPartB,
            4 => NaluType::DataPartC,
            5 => NaluType::Idr,
            6 => NaluType::Sei,
            7 => NaluType::Sps,
            8 => NaluType::Pps,
            9 => NaluType::AccessUnitDelimiter,
            10 => NaluType::EndOfSequence,
            11 => NaluType::EndOfStream,
            12 => NaluType::FillerData,
            _ => NaluType::Unknown,
        }
    }
}

/// H.264 Sequence Parameter Set
#[derive(Debug, Clone)]
pub struct H264Sps {
    pub profile_idc: u8,
    pub level_idc: u8,
    pub sps_id: u32,
    pub chroma_format_idc: u32,
    pub bit_depth_luma: u32,
    pub bit_depth_chroma: u32,
    pub log2_max_frame_num: u32,
    pub pic_order_cnt_type: u32,
    pub log2_max_pic_order_cnt_lsb: u32,
    pub max_num_ref_frames: u32,
    pub pic_width_in_mbs: u32,
    pub pic_height_in_map_units: u32,
    pub frame_mbs_only_flag: bool,
    pub direct_8x8_inference_flag: bool,
    pub frame_crop: Option<[u32; 4]>, // left, right, top, bottom
}

impl H264Sps {
    pub fn width(&self) -> u32 {
        self.pic_width_in_mbs * 16
    }

    pub fn height(&self) -> u32 {
        self.pic_height_in_map_units * 16 * if self.frame_mbs_only_flag { 1 } else { 2 }
    }
}

/// H.264 Picture Parameter Set
#[derive(Debug, Clone)]
pub struct H264Pps {
    pub pps_id: u32,
    pub sps_id: u32,
    pub entropy_coding_mode_flag: bool, // true = CABAC, false = CAVLC
    pub bottom_field_pic_order_in_frame_present_flag: bool,
    pub num_slice_groups: u32,
    pub num_ref_idx_l0_default: u32,
    pub num_ref_idx_l1_default: u32,
    pub weighted_pred_flag: bool,
    pub weighted_bipred_idc: u8,
    pub pic_init_qp: i32,
    pub chroma_qp_index_offset: i32,
    pub deblocking_filter_control_present_flag: bool,
    pub constrained_intra_pred_flag: bool,
    pub redundant_pic_cnt_present_flag: bool,
    pub transform_8x8_mode_flag: bool,
}

/// H.264 decoder state
pub struct H264Decoder {
    sps_map: BTreeMap<u32, H264Sps>,
    pps_map: BTreeMap<u32, H264Pps>,
    current_sps: Option<u32>,
    current_pps: Option<u32>,
    frame_num: u64,
    poc: i64,
    reference_frames: Vec<VideoFrame>,
    max_ref_frames: usize,
}

impl H264Decoder {
    pub fn new() -> Self {
        Self {
            sps_map: BTreeMap::new(),
            pps_map: BTreeMap::new(),
            current_sps: None,
            current_pps: None,
            frame_num: 0,
            poc: 0,
            reference_frames: Vec::new(),
            max_ref_frames: 16,
        }
    }

    /// Parse NAL units from Annex B bytestream
    pub fn find_nalu_boundaries(data: &[u8]) -> Vec<(usize, usize)> {
        let mut nalus = Vec::new();
        let mut i = 0;
        let mut start = 0;
        let mut found_start = false;

        while i < data.len() {
            // Look for start codes: 0x000001 or 0x00000001
            if i + 2 < data.len() && data[i] == 0 && data[i + 1] == 0 {
                if data[i + 2] == 1 {
                    if found_start {
                        nalus.push((start, i));
                    }
                    start = i + 3;
                    found_start = true;
                    i += 3;
                    continue;
                } else if i + 3 < data.len() && data[i + 2] == 0 && data[i + 3] == 1 {
                    if found_start {
                        nalus.push((start, i));
                    }
                    start = i + 4;
                    found_start = true;
                    i += 4;
                    continue;
                }
            }
            i += 1;
        }
        if found_start {
            nalus.push((start, data.len()));
        }
        nalus
    }

    /// Decode a single NAL unit
    pub fn decode_nalu(&mut self, nalu: &[u8]) -> Option<VideoFrame> {
        if nalu.is_empty() {
            return None;
        }

        let nalu_type = NaluType::from_u8(nalu[0]);
        let _nal_ref_idc = (nalu[0] >> 5) & 0x03;

        match nalu_type {
            NaluType::Sps => {
                self.parse_sps(nalu);
                None
            }
            NaluType::Pps => {
                self.parse_pps(nalu);
                None
            }
            NaluType::Idr | NaluType::Slice => self.decode_slice(nalu, nalu_type == NaluType::Idr),
            NaluType::Sei => {
                // SEI messages can be logged/ignored
                None
            }
            _ => None,
        }
    }

    fn parse_sps(&mut self, data: &[u8]) {
        if data.len() < 4 {
            return;
        }
        let mut reader = BitReader::new(&data[1..]);

        let profile_idc = reader.read_bits(8) as u8;
        let _constraint_flags = reader.read_bits(8);
        let level_idc = reader.read_bits(8) as u8;
        let sps_id = reader.read_exp_golomb();

        let chroma_format_idc = if profile_idc >= 100 {
            reader.read_exp_golomb()
        } else {
            1
        };

        let bit_depth_luma = if profile_idc >= 100 {
            reader.read_exp_golomb() + 8
        } else {
            8
        };

        let bit_depth_chroma = if profile_idc >= 100 {
            reader.read_exp_golomb() + 8
        } else {
            8
        };

        if profile_idc >= 100 {
            let _qpprime_y_zero_transform_bypass = reader.read_bits(1);
            let seq_scaling_matrix_present = reader.read_bits(1);
            if seq_scaling_matrix_present != 0 {
                let count = if chroma_format_idc != 3 { 8 } else { 12 };
                for _ in 0..count {
                    let present = reader.read_bits(1);
                    if present != 0 {
                        // Skip scaling list
                    }
                }
            }
        }

        let log2_max_frame_num = reader.read_exp_golomb() + 4;
        let pic_order_cnt_type = reader.read_exp_golomb();

        let log2_max_pic_order_cnt_lsb = if pic_order_cnt_type == 0 {
            reader.read_exp_golomb() + 4
        } else {
            0
        };

        let max_num_ref_frames = reader.read_exp_golomb();
        let _gaps_in_frame_num_allowed = reader.read_bits(1);
        let pic_width_in_mbs = reader.read_exp_golomb() + 1;
        let pic_height_in_map_units = reader.read_exp_golomb() + 1;
        let frame_mbs_only_flag = reader.read_bits(1) != 0;

        let direct_8x8_inference_flag = if !frame_mbs_only_flag {
            let _mb_adaptive_frame_field = reader.read_bits(1);
            reader.read_bits(1) != 0
        } else {
            reader.read_bits(1) != 0
        };

        let frame_crop = if reader.read_bits(1) != 0 {
            Some([
                reader.read_exp_golomb(),
                reader.read_exp_golomb(),
                reader.read_exp_golomb(),
                reader.read_exp_golomb(),
            ])
        } else {
            None
        };

        let sps = H264Sps {
            profile_idc,
            level_idc,
            sps_id,
            chroma_format_idc,
            bit_depth_luma,
            bit_depth_chroma,
            log2_max_frame_num,
            pic_order_cnt_type,
            log2_max_pic_order_cnt_lsb,
            max_num_ref_frames,
            pic_width_in_mbs,
            pic_height_in_map_units,
            frame_mbs_only_flag,
            direct_8x8_inference_flag,
            frame_crop,
        };

        serial_println!(
            "[H.264] SPS #{}: {}x{}, profile {}, level {}.{}",
            sps_id,
            sps.width(),
            sps.height(),
            profile_idc,
            level_idc / 10,
            level_idc % 10
        );

        self.current_sps = Some(sps_id);
        self.sps_map.insert(sps_id, sps);
    }

    fn parse_pps(&mut self, data: &[u8]) {
        if data.len() < 2 {
            return;
        }
        let mut reader = BitReader::new(&data[1..]);

        let pps_id = reader.read_exp_golomb();
        let sps_id = reader.read_exp_golomb();
        let entropy_coding_mode_flag = reader.read_bits(1) != 0;
        let bottom_field_pic_order_in_frame_present_flag = reader.read_bits(1) != 0;
        let num_slice_groups = reader.read_exp_golomb() + 1;

        let num_ref_idx_l0_default = reader.read_exp_golomb() + 1;
        let num_ref_idx_l1_default = reader.read_exp_golomb() + 1;
        let weighted_pred_flag = reader.read_bits(1) != 0;
        let weighted_bipred_idc = reader.read_bits(2) as u8;
        let pic_init_qp = reader.read_signed_exp_golomb() + 26;
        let _pic_init_qs = reader.read_signed_exp_golomb() + 26;
        let chroma_qp_index_offset = reader.read_signed_exp_golomb();
        let deblocking_filter_control_present_flag = reader.read_bits(1) != 0;
        let constrained_intra_pred_flag = reader.read_bits(1) != 0;
        let redundant_pic_cnt_present_flag = reader.read_bits(1) != 0;

        let pps = H264Pps {
            pps_id,
            sps_id,
            entropy_coding_mode_flag,
            bottom_field_pic_order_in_frame_present_flag,
            num_slice_groups,
            num_ref_idx_l0_default,
            num_ref_idx_l1_default,
            weighted_pred_flag,
            weighted_bipred_idc,
            pic_init_qp,
            chroma_qp_index_offset,
            deblocking_filter_control_present_flag,
            constrained_intra_pred_flag,
            redundant_pic_cnt_present_flag,
            transform_8x8_mode_flag: false,
        };

        self.current_pps = Some(pps_id);
        self.pps_map.insert(pps_id, pps);
    }

    fn decode_slice(&mut self, _data: &[u8], is_idr: bool) -> Option<VideoFrame> {
        let sps_id = self.current_sps?;
        let sps = self.sps_map.get(&sps_id)?;

        let width = sps.width();
        let height = sps.height();

        // Create a decoded frame (in a real decoder, this would do block-level decoding)
        let mut frame = VideoFrame::new(width, height, PixelFormat::Yuv420p);
        frame.frame_type = if is_idr { FrameType::I } else { FrameType::P };
        frame.key_frame = is_idr;
        frame.pts = self.frame_num as i64;
        frame.dts = self.frame_num as i64;

        if is_idr {
            self.reference_frames.clear();
        }

        // Store as reference
        if self.reference_frames.len() >= self.max_ref_frames {
            self.reference_frames.remove(0);
        }
        self.reference_frames.push(frame.clone());

        self.frame_num += 1;
        Some(frame)
    }

    /// Flush decoder (return any buffered frames)
    pub fn flush(&mut self) -> Vec<VideoFrame> {
        let frames = self.reference_frames.drain(..).collect();
        self.frame_num = 0;
        self.poc = 0;
        frames
    }

    /// Reset decoder
    pub fn reset(&mut self) {
        self.sps_map.clear();
        self.pps_map.clear();
        self.current_sps = None;
        self.current_pps = None;
        self.frame_num = 0;
        self.poc = 0;
        self.reference_frames.clear();
    }
}
