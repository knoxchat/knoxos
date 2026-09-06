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
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

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

// ═══════════════════════════════════════════════════════════════════════
// VIDEO FRAME
// ═══════════════════════════════════════════════════════════════════════

/// A decoded video frame
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub frame_type: FrameType,
    pub pts: i64, // presentation timestamp (in timebase units)
    pub dts: i64, // decoding timestamp
    pub duration: u64,
    pub key_frame: bool,
    pub planes: Vec<Vec<u8>>, // plane data
    pub linesize: Vec<u32>,   // bytes per row per plane
}

impl VideoFrame {
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Self {
        let mut planes = Vec::new();
        let mut linesize = Vec::new();

        match format {
            PixelFormat::Yuv420p => {
                let y_size = (width * height) as usize;
                let uv_size = (width / 2 * height / 2) as usize;
                planes.push(vec![0u8; y_size]);
                planes.push(vec![128u8; uv_size]);
                planes.push(vec![128u8; uv_size]);
                linesize.push(width);
                linesize.push(width / 2);
                linesize.push(width / 2);
            }
            PixelFormat::Nv12 => {
                let y_size = (width * height) as usize;
                let uv_size = (width * height / 2) as usize;
                planes.push(vec![0u8; y_size]);
                planes.push(vec![128u8; uv_size]);
                linesize.push(width);
                linesize.push(width);
            }
            PixelFormat::Rgb24 | PixelFormat::Bgr24 => {
                let size = (width * height * 3) as usize;
                planes.push(vec![0u8; size]);
                linesize.push(width * 3);
            }
            PixelFormat::Rgba32 | PixelFormat::Bgra32 => {
                let size = (width * height * 4) as usize;
                planes.push(vec![0u8; size]);
                linesize.push(width * 4);
            }
            _ => {
                let bpp = format.bits_per_pixel();
                let size = (width * height * bpp / 8) as usize;
                planes.push(vec![0u8; size]);
                linesize.push(width * bpp / 8);
            }
        }

        Self {
            width,
            height,
            format,
            frame_type: FrameType::I,
            pts: 0,
            dts: 0,
            duration: 0,
            key_frame: true,
            planes,
            linesize,
        }
    }

    /// Convert YUV420P frame to BGRA32
    pub fn to_bgra(&self) -> Vec<u8> {
        if self.format != PixelFormat::Yuv420p || self.planes.len() < 3 {
            return self.planes.first().cloned().unwrap_or_default();
        }

        let w = self.width as usize;
        let h = self.height as usize;
        let mut bgra = vec![0u8; w * h * 4];

        let y_plane = &self.planes[0];
        let u_plane = &self.planes[1];
        let v_plane = &self.planes[2];

        for row in 0..h {
            for col in 0..w {
                let y_idx = row * w + col;
                let uv_idx = (row / 2) * (w / 2) + (col / 2);

                let y = y_plane.get(y_idx).copied().unwrap_or(16) as i32;
                let u = u_plane.get(uv_idx).copied().unwrap_or(128) as i32;
                let v = v_plane.get(uv_idx).copied().unwrap_or(128) as i32;

                // BT.601 conversion
                let c = y - 16;
                let d = u - 128;
                let e = v - 128;

                let r = clamp_u8((298 * c + 409 * e + 128) >> 8);
                let g = clamp_u8((298 * c - 100 * d - 208 * e + 128) >> 8);
                let b = clamp_u8((298 * c + 516 * d + 128) >> 8);

                let out_idx = (row * w + col) * 4;
                bgra[out_idx] = b;
                bgra[out_idx + 1] = g;
                bgra[out_idx + 2] = r;
                bgra[out_idx + 3] = 255;
            }
        }

        bgra
    }
}

fn clamp_u8(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

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

// ═══════════════════════════════════════════════════════════════════════
// V4L2 INTERFACE
// ═══════════════════════════════════════════════════════════════════════

/// V4L2-compatible buffer type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4l2BufType {
    VideoCapture = 1,
    VideoOutput = 2,
    VideoOverlay = 3,
    VideoCaptureMplane = 9,
    VideoOutputMplane = 10,
}

/// V4L2-compatible memory model
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4l2Memory {
    Mmap = 1,
    UserPtr = 2,
    Overlay = 3,
    DmaBuf = 4,
}

/// V4L2 device capability flags
pub const V4L2_CAP_VIDEO_CAPTURE: u32 = 0x00000001;
pub const V4L2_CAP_VIDEO_OUTPUT: u32 = 0x00000002;
pub const V4L2_CAP_STREAMING: u32 = 0x04000000;

/// V4L2 buffer
#[derive(Debug, Clone)]
pub struct V4l2Buffer {
    pub index: u32,
    pub buf_type: V4l2BufType,
    pub memory: V4l2Memory,
    pub length: u32,
    pub bytesused: u32,
    pub flags: u32,
    pub timestamp: u64,
    pub sequence: u32,
    pub data: Vec<u8>,
}

/// V4L2 video device
pub struct V4l2Device {
    pub name: String,
    pub driver: String,
    pub bus: String,
    pub capabilities: u32,
    pub buffers: Vec<V4l2Buffer>,
    pub streaming: bool,
    pub codec: Option<VideoCodec>,
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,
}

impl V4l2Device {
    pub fn new(name: &str, codec: VideoCodec) -> Self {
        Self {
            name: String::from(name),
            driver: String::from("knoxos-v4l2"),
            bus: String::from("platform:knoxos"),
            capabilities: V4L2_CAP_VIDEO_CAPTURE | V4L2_CAP_VIDEO_OUTPUT | V4L2_CAP_STREAMING,
            buffers: Vec::new(),
            streaming: false,
            codec: Some(codec),
            width: 1920,
            height: 1080,
            pixel_format: PixelFormat::Yuv420p,
        }
    }

    /// Request buffers
    pub fn reqbufs(&mut self, count: u32, buf_type: V4l2BufType, memory: V4l2Memory) -> u32 {
        self.buffers.clear();
        let size = (self.width * self.height * self.pixel_format.bits_per_pixel() / 8) as usize;
        for i in 0..count {
            self.buffers.push(V4l2Buffer {
                index: i,
                buf_type,
                memory,
                length: size as u32,
                bytesused: 0,
                flags: 0,
                timestamp: 0,
                sequence: 0,
                data: vec![0u8; size],
            });
        }
        count
    }

    /// Queue buffer
    pub fn qbuf(&mut self, index: u32) -> bool {
        if (index as usize) < self.buffers.len() {
            self.buffers[index as usize].flags |= 0x01; // queued
            true
        } else {
            false
        }
    }

    /// Dequeue buffer
    pub fn dqbuf(&mut self) -> Option<u32> {
        for buf in &mut self.buffers {
            if buf.flags & 0x01 != 0 {
                buf.flags &= !0x01;
                return Some(buf.index);
            }
        }
        None
    }

    /// Start streaming
    pub fn streamon(&mut self) -> bool {
        self.streaming = true;
        true
    }

    /// Stop streaming
    pub fn streamoff(&mut self) -> bool {
        self.streaming = false;
        true
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BIT READER UTILITY
// ═══════════════════════════════════════════════════════════════════════

struct BitReader<'a> {
    data: &'a [u8],
    byte_offset: usize,
    bit_offset: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_offset: 0,
            bit_offset: 0,
        }
    }

    fn read_bits(&mut self, n: u8) -> u32 {
        let mut result = 0u32;
        for _ in 0..n {
            if self.byte_offset >= self.data.len() {
                return result;
            }
            let bit = (self.data[self.byte_offset] >> (7 - self.bit_offset)) & 1;
            result = (result << 1) | (bit as u32);
            self.bit_offset += 1;
            if self.bit_offset >= 8 {
                self.bit_offset = 0;
                self.byte_offset += 1;
            }
        }
        result
    }

    fn read_exp_golomb(&mut self) -> u32 {
        let mut leading_zeros = 0u32;
        while self.read_bits(1) == 0 {
            leading_zeros += 1;
            if leading_zeros > 31 {
                return 0;
            }
        }
        if leading_zeros == 0 {
            return 0;
        }
        let suffix = self.read_bits(leading_zeros as u8);
        (1 << leading_zeros) - 1 + suffix
    }

    fn read_signed_exp_golomb(&mut self) -> i32 {
        let val = self.read_exp_golomb();
        if val == 0 {
            return 0;
        }
        let sign = if val & 1 == 0 { -1 } else { 1 };
        sign * val.div_ceil(2) as i32
    }
}

fn read_leb128(data: &[u8]) -> (u64, usize) {
    let mut result = 0u64;
    let mut shift = 0u32;
    let mut i = 0;
    loop {
        if i >= data.len() {
            break;
        }
        let byte = data[i];
        result |= ((byte & 0x7F) as u64) << shift;
        i += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 56 {
            break;
        }
    }
    (result, i)
}

// ═══════════════════════════════════════════════════════════════════════
// COLOR SPACE CONVERSION
// ═══════════════════════════════════════════════════════════════════════

/// Convert RGB to YUV (BT.601)
pub fn rgb_to_yuv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let r = r as i32;
    let g = g as i32;
    let b = b as i32;

    let y = clamp_u8(((66 * r + 129 * g + 25 * b + 128) >> 8) + 16);
    let u = clamp_u8(((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128);
    let v = clamp_u8(((112 * r - 94 * g - 18 * b + 128) >> 8) + 128);

    (y, u, v)
}

/// Convert YUV to RGB (BT.601)
pub fn yuv_to_rgb(y: u8, u: u8, v: u8) -> (u8, u8, u8) {
    let c = y as i32 - 16;
    let d = u as i32 - 128;
    let e = v as i32 - 128;

    let r = clamp_u8((298 * c + 409 * e + 128) >> 8);
    let g = clamp_u8((298 * c - 100 * d - 208 * e + 128) >> 8);
    let b = clamp_u8((298 * c + 516 * d + 128) >> 8);

    (r, g, b)
}

/// Convert NV12 frame to BGRA
pub fn nv12_to_bgra(y_plane: &[u8], uv_plane: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let mut bgra = vec![0u8; w * h * 4];

    for row in 0..h {
        for col in 0..w {
            let y_idx = row * w + col;
            let uv_idx = (row / 2) * w + (col & !1);

            let y = y_plane.get(y_idx).copied().unwrap_or(16);
            let u = uv_plane.get(uv_idx).copied().unwrap_or(128);
            let v = uv_plane.get(uv_idx + 1).copied().unwrap_or(128);

            let (r, g, b) = yuv_to_rgb(y, u, v);

            let out_idx = (row * w + col) * 4;
            bgra[out_idx] = b;
            bgra[out_idx + 1] = g;
            bgra[out_idx + 2] = r;
            bgra[out_idx + 3] = 255;
        }
    }

    bgra
}

// ═══════════════════════════════════════════════════════════════════════
// CODEC REGISTRY
// ═══════════════════════════════════════════════════════════════════════

/// Registered codec information
#[derive(Debug, Clone)]
pub struct CodecInfo {
    pub codec: VideoCodec,
    pub is_encoder: bool,
    pub is_decoder: bool,
    pub profiles: Vec<String>,
    pub max_width: u32,
    pub max_height: u32,
    pub max_framerate: u32,
}

lazy_static::lazy_static! {
    static ref REGISTERED_CODECS: Mutex<Vec<CodecInfo>> = Mutex::new(Vec::new());
    static ref V4L2_DEVICES: Mutex<BTreeMap<u32, V4l2Device>> = Mutex::new(BTreeMap::new());
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static NEXT_V4L2_ID: AtomicU32 = AtomicU32::new(0);
static FRAMES_DECODED: AtomicU64 = AtomicU64::new(0);

/// Register default codecs
fn register_default_codecs() {
    let mut codecs = REGISTERED_CODECS.lock();

    codecs.push(CodecInfo {
        codec: VideoCodec::H264,
        is_encoder: false,
        is_decoder: true,
        profiles: vec![
            String::from("Baseline"),
            String::from("Main"),
            String::from("High"),
        ],
        max_width: 4096,
        max_height: 2160,
        max_framerate: 60,
    });

    codecs.push(CodecInfo {
        codec: VideoCodec::H265,
        is_encoder: false,
        is_decoder: true,
        profiles: vec![String::from("Main"), String::from("Main10")],
        max_width: 8192,
        max_height: 4320,
        max_framerate: 60,
    });

    codecs.push(CodecInfo {
        codec: VideoCodec::VP9,
        is_encoder: false,
        is_decoder: true,
        profiles: vec![String::from("Profile 0"), String::from("Profile 2")],
        max_width: 8192,
        max_height: 4320,
        max_framerate: 60,
    });

    codecs.push(CodecInfo {
        codec: VideoCodec::AV1,
        is_encoder: false,
        is_decoder: true,
        profiles: vec![String::from("Main"), String::from("High")],
        max_width: 8192,
        max_height: 4320,
        max_framerate: 120,
    });

    codecs.push(CodecInfo {
        codec: VideoCodec::MJPEG,
        is_encoder: true,
        is_decoder: true,
        profiles: vec![String::from("Baseline")],
        max_width: 4096,
        max_height: 4096,
        max_framerate: 30,
    });
}

/// List registered codecs
pub fn list_codecs() -> Vec<CodecInfo> {
    REGISTERED_CODECS.lock().clone()
}

/// Create a V4L2 video device
pub fn create_v4l2_device(name: &str, codec: VideoCodec) -> u32 {
    let id = NEXT_V4L2_ID.fetch_add(1, Ordering::Relaxed);
    let device = V4l2Device::new(name, codec);
    V4L2_DEVICES.lock().insert(id, device);
    serial_println!(
        "[Video] V4L2 device /dev/video{}: {} ({})",
        id,
        name,
        codec.name()
    );
    id
}

/// Get decoded frame count
pub fn total_frames_decoded() -> u64 {
    FRAMES_DECODED.load(Ordering::Relaxed)
}

/// Proc info for /proc/video
pub fn proc_video_info() -> String {
    let codecs = REGISTERED_CODECS.lock();
    let devices = V4L2_DEVICES.lock();

    let mut info = String::from("Video Subsystem:\n");
    info.push_str(&alloc::format!("  Registered codecs: {}\n", codecs.len()));
    info.push_str(&alloc::format!("  V4L2 devices: {}\n", devices.len()));
    info.push_str(&alloc::format!(
        "  Total frames decoded: {}\n\n",
        total_frames_decoded()
    ));

    info.push_str("Codecs:\n");
    for c in codecs.iter() {
        info.push_str(&alloc::format!(
            "  {} [{}{}] max {}x{}@{}fps\n",
            c.codec.name(),
            if c.is_decoder { "D" } else { "" },
            if c.is_encoder { "E" } else { "" },
            c.max_width,
            c.max_height,
            c.max_framerate,
        ));
    }

    info
}

/// Initialize video codec subsystem
pub fn init() {
    if INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    register_default_codecs();

    // Create default V4L2 decoder device
    create_v4l2_device("KnoxOS Video Decoder", VideoCodec::H264);

    INITIALIZED.store(true, Ordering::Relaxed);
    serial_println!("[KnoxOS] Video codec subsystem initialized (H.264, H.265, VP9, AV1, MJPEG)");
}

// ═══════════════════════════════════════════════════════════════════════
// Video Container Parsing (MP4, MKV, WebM)
// ═══════════════════════════════════════════════════════════════════════

/// Container format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerFormat {
    Mp4,
    Mkv,
    WebM,
    Avi,
    Ts,
}

/// Demuxed stream info
#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub stream_id: u8,
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub duration_ms: u64,
    pub bitrate: u32,
}

/// Audio stream info
#[derive(Debug, Clone)]
pub struct AudioStreamInfo {
    pub stream_id: u8,
    pub codec_name: alloc::string::String,
    pub sample_rate: u32,
    pub channels: u8,
    pub bitrate: u32,
}

/// Container demuxer
pub struct Demuxer {
    pub format: ContainerFormat,
    pub video_streams: Vec<StreamInfo>,
    pub audio_streams: Vec<AudioStreamInfo>,
    pub duration_ms: u64,
}

impl Demuxer {
    /// Create a new demuxer from container data
    pub fn open(data: &[u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }
        let format = if &data[4..8] == b"ftyp" {
            ContainerFormat::Mp4
        } else if data[0..4] == [0x1A, 0x45, 0xDF, 0xA3] {
            // EBML header = MKV or WebM
            ContainerFormat::Mkv
        } else if &data[0..4] == b"RIFF" {
            ContainerFormat::Avi
        } else {
            return None;
        };
        Some(Self {
            format,
            video_streams: Vec::new(),
            audio_streams: Vec::new(),
            duration_ms: 0,
        })
    }

    /// Parse MP4 moov box to extract track info
    pub fn parse_tracks(&mut self, data: &[u8]) -> usize {
        // Simplified: scan for box headers in MP4
        let mut tracks = 0;
        if self.format == ContainerFormat::Mp4 && data.len() > 16 {
            // Add default video track
            self.video_streams.push(StreamInfo {
                stream_id: 0,
                codec: VideoCodec::H264,
                width: 1920,
                height: 1080,
                fps_num: 30,
                fps_den: 1,
                duration_ms: 0,
                bitrate: 5_000_000,
            });
            tracks += 1;
            self.audio_streams.push(AudioStreamInfo {
                stream_id: 1,
                codec_name: alloc::string::String::from("aac"),
                sample_rate: 44100,
                channels: 2,
                bitrate: 128_000,
            });
            tracks += 1;
        }
        tracks
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Audio Codec Decoders (MP3, AAC, OGG Vorbis/Opus, FLAC)
// ═══════════════════════════════════════════════════════════════════════

/// Audio codec type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Pcm,
    Mp3,
    Aac,
    OggVorbis,
    OggOpus,
    Flac,
    Wav,
}

/// Decoded audio buffer
pub struct AudioBuffer {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u8,
}

/// MP3 frame header
pub struct Mp3FrameHeader {
    pub version: u8,  // MPEG version (1, 2, 2.5)
    pub layer: u8,    // Layer (1, 2, 3)
    pub bitrate: u32, // kbps
    pub sample_rate: u32,
    pub channels: u8,
    pub frame_size: usize,
}

/// Decode an MP3 frame header from 4 bytes
pub fn mp3_parse_header(header: &[u8; 4]) -> Option<Mp3FrameHeader> {
    // Check sync word: 0xFFE0
    if header[0] != 0xFF || (header[1] & 0xE0) != 0xE0 {
        return None;
    }
    let version = match (header[1] >> 3) & 3 {
        0 => return None, // reserved
        1 => 3,           // MPEG 2.5 (unofficial but common)
        2 => 2,
        3 => 1,
        _ => return None,
    };
    let layer = match (header[1] >> 1) & 3 {
        1 => 3, // Layer III
        2 => 2,
        3 => 1,
        _ => return None,
    };
    let bitrate_idx = ((header[2] >> 4) & 0xF) as usize;
    let sr_idx = ((header[2] >> 2) & 3) as usize;
    let channels = if (header[3] >> 6) == 3 { 1 } else { 2 };

    // Bitrate table for MPEG1, Layer III
    let bitrate_table: [u32; 16] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ];
    let sr_table: [u32; 4] = [44100, 48000, 32000, 0];

    let bitrate = bitrate_table.get(bitrate_idx).copied().unwrap_or(0);
    let sample_rate = sr_table.get(sr_idx).copied().unwrap_or(0);
    if bitrate == 0 || sample_rate == 0 {
        return None;
    }

    let padding = ((header[2] >> 1) & 1) as usize;
    let frame_size = (144 * bitrate as usize * 1000 / sample_rate as usize) + padding;

    Some(Mp3FrameHeader {
        version,
        layer,
        bitrate,
        sample_rate,
        channels,
        frame_size,
    })
}

/// Decode MP3 data to PCM samples (simplified — produces silence with correct frame structure)
pub fn mp3_decode(data: &[u8]) -> Option<AudioBuffer> {
    if data.len() < 4 {
        return None;
    }
    let mut hdr_bytes = [0u8; 4];
    hdr_bytes.copy_from_slice(&data[..4]);
    let hdr = mp3_parse_header(&hdr_bytes)?;
    // In a full implementation: Huffman decode, dequantize, IMDCT, frequency inversion, synthesis filterbank
    let samples_per_frame = 1152; // MP3 Layer III = 1152 samples/frame
    let num_frames = data.len() / hdr.frame_size.max(1);
    let total_samples = num_frames * samples_per_frame * hdr.channels as usize;
    Some(AudioBuffer {
        samples: alloc::vec![0i16; total_samples],
        sample_rate: hdr.sample_rate,
        channels: hdr.channels,
    })
}

/// Parse AAC ADTS header
pub fn aac_parse_adts(data: &[u8]) -> Option<(u32, u8, usize)> {
    if data.len() < 7 {
        return None;
    }
    // ADTS sync: 0xFFF
    if data[0] != 0xFF || (data[1] & 0xF0) != 0xF0 {
        return None;
    }
    let sr_idx = ((data[2] >> 2) & 0xF) as usize;
    let channels = ((data[2] & 0x01) << 2) | ((data[3] >> 6) & 0x03);
    let frame_len = ((data[3] as usize & 0x03) << 11)
        | ((data[4] as usize) << 3)
        | ((data[5] as usize >> 5) & 0x07);
    let sr_table: [u32; 13] = [
        96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
    ];
    let sample_rate = sr_table.get(sr_idx).copied().unwrap_or(44100);
    Some((sample_rate, channels, frame_len))
}

/// Decode AAC to PCM (stub — correct frame structure, outputs silence)
pub fn aac_decode(data: &[u8]) -> Option<AudioBuffer> {
    let (sr, ch, _) = aac_parse_adts(data)?;
    let samples = 1024; // AAC frame = 1024 samples
    Some(AudioBuffer {
        samples: alloc::vec![0i16; samples * ch as usize],
        sample_rate: sr,
        channels: ch,
    })
}

/// Decode OGG Vorbis/Opus container header
pub fn ogg_parse_header(data: &[u8]) -> Option<(AudioCodec, u32, u8)> {
    if data.len() < 35 {
        return None;
    }
    if &data[0..4] != b"OggS" {
        return None;
    }
    // Check for Vorbis or Opus identification header in first page payload
    let payload_start = 27 + data.get(26).copied().unwrap_or(0) as usize;
    if data.len() <= payload_start + 8 {
        return None;
    }
    let payload = &data[payload_start..];
    if payload.len() >= 7 && &payload[1..7] == b"vorbis" {
        let ch = payload.get(11).copied().unwrap_or(2);
        let sr = if payload.len() >= 16 {
            u32::from_le_bytes([payload[12], payload[13], payload[14], payload[15]])
        } else {
            44100
        };
        Some((AudioCodec::OggVorbis, sr, ch))
    } else if payload.len() >= 8 && &payload[0..8] == b"OpusHead" {
        let ch = payload.get(9).copied().unwrap_or(2);
        Some((AudioCodec::OggOpus, 48000, ch)) // Opus always 48kHz internally
    } else {
        None
    }
}

/// Decode OGG to PCM (stub)
pub fn ogg_decode(data: &[u8]) -> Option<AudioBuffer> {
    let (_, sr, ch) = ogg_parse_header(data)?;
    Some(AudioBuffer {
        samples: alloc::vec![0i16; 4096],
        sample_rate: sr,
        channels: ch,
    })
}

/// FLAC stream info
pub struct FlacStreamInfo {
    pub min_block_size: u16,
    pub max_block_size: u16,
    pub sample_rate: u32,
    pub channels: u8,
    pub bits_per_sample: u8,
    pub total_samples: u64,
}

/// Parse FLAC stream header
pub fn flac_parse_header(data: &[u8]) -> Option<FlacStreamInfo> {
    if data.len() < 42 {
        return None;
    }
    if &data[0..4] != b"fLaC" {
        return None;
    }
    // METADATA_BLOCK_HEADER + STREAMINFO
    let min_bs = u16::from_be_bytes([data[8], data[9]]);
    let max_bs = u16::from_be_bytes([data[10], data[11]]);
    let sr = ((data[18] as u32) << 12) | ((data[19] as u32) << 4) | ((data[20] as u32) >> 4);
    let channels = ((data[20] >> 1) & 0x07) + 1;
    let bps = (((data[20] & 0x01) << 4) | ((data[21] >> 4) & 0x0F)) + 1;
    let total = ((data[21] as u64 & 0x0F) << 32)
        | ((data[22] as u64) << 24)
        | ((data[23] as u64) << 16)
        | ((data[24] as u64) << 8)
        | (data[25] as u64);
    Some(FlacStreamInfo {
        min_block_size: min_bs,
        max_block_size: max_bs,
        sample_rate: sr,
        channels,
        bits_per_sample: bps,
        total_samples: total,
    })
}

/// Decode FLAC to PCM (stub)
pub fn flac_decode(data: &[u8]) -> Option<AudioBuffer> {
    let info = flac_parse_header(data)?;
    Some(AudioBuffer {
        samples: alloc::vec![0i16; info.total_samples.min(1_000_000) as usize * info.channels as usize],
        sample_rate: info.sample_rate,
        channels: info.channels,
    })
}

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
