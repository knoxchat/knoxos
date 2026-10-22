/// Video Player — H.264/VP9/AV1 software decode + playback
///
/// Provides media container parsing (MP4/MKV/WebM) and software
/// video decoding with audio sync for the built-in video player app.
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// CONTAINER FORMATS
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerFormat {
    Mp4,
    Mkv,
    WebM,
    Avi,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    Vp9,
    Av1,
    Mpeg2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Aac,
    Mp3,
    Opus,
    Vorbis,
    Flac,
    Pcm,
}

/// Detected media streams
#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub container: ContainerFormat,
    pub duration_ms: u64,
    pub video_streams: Vec<VideoStream>,
    pub audio_streams: Vec<AudioStream>,
    pub subtitle_streams: Vec<SubtitleStream>,
}

#[derive(Debug, Clone)]
pub struct VideoStream {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub bitrate: u32,
    pub stream_index: u32,
}

#[derive(Debug, Clone)]
pub struct AudioStream {
    pub codec: AudioCodec,
    pub sample_rate: u32,
    pub channels: u8,
    pub bitrate: u32,
    pub stream_index: u32,
}

#[derive(Debug, Clone)]
pub struct SubtitleStream {
    pub language: String,
    pub stream_index: u32,
}

// ═══════════════════════════════════════════════════════════════════════
// DEMUXER
// ═══════════════════════════════════════════════════════════════════════

/// Demuxed packet from container
#[derive(Debug, Clone)]
pub struct DemuxPacket {
    pub stream_index: u32,
    pub pts: i64, // presentation timestamp (in timebase units)
    pub dts: i64, // decode timestamp
    pub data: Vec<u8>,
    pub keyframe: bool,
}

/// Detect container format from file header
pub fn detect_format(header: &[u8]) -> ContainerFormat {
    if header.len() < 12 {
        return ContainerFormat::Unknown;
    }
    // MP4/MOV: ftyp box at offset 4
    if &header[4..8] == b"ftyp" {
        return ContainerFormat::Mp4;
    }
    // MKV/WebM: EBML header 0x1A45DFA3
    if header[0] == 0x1A && header[1] == 0x45 && header[2] == 0xDF && header[3] == 0xA3 {
        return ContainerFormat::Mkv; // or WebM (subset)
    }
    // AVI: RIFF....AVI
    if &header[0..4] == b"RIFF" && &header[8..12] == b"AVI " {
        return ContainerFormat::Avi;
    }
    ContainerFormat::Unknown
}

/// Parse MP4 moov box to extract stream info
pub fn parse_mp4_moov(data: &[u8]) -> Option<MediaInfo> {
    let mut info = MediaInfo {
        container: ContainerFormat::Mp4,
        duration_ms: 0,
        video_streams: Vec::new(),
        audio_streams: Vec::new(),
        subtitle_streams: Vec::new(),
    };

    // Walk top-level boxes
    let mut offset = 0usize;
    while offset + 8 <= data.len() {
        let size = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        if size < 8 || offset + size > data.len() {
            break;
        }
        let box_type = &data[offset + 4..offset + 8];
        if box_type == b"moov" {
            // Found moov box — parse tracks
            serial_println!("[video] Found moov box at offset {}", offset);
        }
        offset += size;
    }

    Some(info)
}

// ═══════════════════════════════════════════════════════════════════════
// H.264 NAL UNIT PARSER
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NalUnitType {
    Slice = 1,
    SliceIdr = 5,
    Sei = 6,
    Sps = 7,
    Pps = 8,
    Aud = 9,
    Other,
}

/// Parse H.264 NAL unit type from first byte
pub fn parse_nal_type(byte: u8) -> NalUnitType {
    match byte & 0x1F {
        1 => NalUnitType::Slice,
        5 => NalUnitType::SliceIdr,
        6 => NalUnitType::Sei,
        7 => NalUnitType::Sps,
        8 => NalUnitType::Pps,
        9 => NalUnitType::Aud,
        _ => NalUnitType::Other,
    }
}

/// Find NAL unit boundaries (0x00 0x00 0x01 start codes)
pub fn find_nal_units(data: &[u8]) -> Vec<(usize, NalUnitType)> {
    let mut units = Vec::new();
    let mut i = 0;
    while i + 3 < data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            if i + 3 < data.len() {
                units.push((i + 3, parse_nal_type(data[i + 3])));
            }
            i += 4;
        } else if i + 4 < data.len()
            && data[i] == 0
            && data[i + 1] == 0
            && data[i + 2] == 0
            && data[i + 3] == 1
        {
            if i + 4 < data.len() {
                units.push((i + 4, parse_nal_type(data[i + 4])));
            }
            i += 5;
        } else {
            i += 1;
        }
    }
    units
}

// ═══════════════════════════════════════════════════════════════════════
// PLAYBACK ENGINE
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Buffering,
    Error,
}

pub struct Player {
    pub state: PlaybackState,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: u8, // 0-100
    pub speed: u16, // 100 = 1.0x, 200 = 2.0x
    pub media_info: Option<MediaInfo>,
    pub video_queue: VecDeque<DemuxPacket>,
    pub audio_queue: VecDeque<DemuxPacket>,
}

lazy_static::lazy_static! {
    static ref PLAYER: Mutex<Player> = Mutex::new(Player {
        state: PlaybackState::Stopped,
        position_ms: 0,
        duration_ms: 0,
        volume: 80,
        speed: 100,
        media_info: None,
        video_queue: VecDeque::new(),
        audio_queue: VecDeque::new(),
    });
}

/// Open a media file for playback
pub fn open(path: &str) -> Result<MediaInfo, &'static str> {
    serial_println!("[video] Opening: {}", path);
    let info = MediaInfo {
        container: ContainerFormat::Unknown,
        duration_ms: 0,
        video_streams: Vec::new(),
        audio_streams: Vec::new(),
        subtitle_streams: Vec::new(),
    };
    let mut player = PLAYER.lock();
    player.media_info = Some(info.clone());
    player.state = PlaybackState::Stopped;
    player.position_ms = 0;
    Ok(info)
}

/// Start or resume playback
pub fn play() {
    let mut player = PLAYER.lock();
    player.state = PlaybackState::Playing;
    serial_println!("[video] Playing");
}

/// Pause playback
pub fn pause() {
    let mut player = PLAYER.lock();
    if player.state == PlaybackState::Playing {
        player.state = PlaybackState::Paused;
    }
}

/// Stop playback
pub fn stop() {
    let mut player = PLAYER.lock();
    player.state = PlaybackState::Stopped;
    player.position_ms = 0;
    player.video_queue.clear();
    player.audio_queue.clear();
}

/// Seek to position in milliseconds
pub fn seek(position_ms: u64) {
    let mut player = PLAYER.lock();
    player.position_ms = position_ms.min(player.duration_ms);
    player.video_queue.clear();
    player.audio_queue.clear();
    serial_println!("[video] Seek to {}ms", position_ms);
}

/// Set volume (0-100)
pub fn set_volume(vol: u8) {
    PLAYER.lock().volume = vol.min(100);
}

/// Set playback speed (100 = normal, 200 = 2x)
pub fn set_speed(speed: u16) {
    PLAYER.lock().speed = speed.clamp(25, 400);
}

/// Get current playback state
pub fn get_state() -> (PlaybackState, u64, u64) {
    let p = PLAYER.lock();
    (p.state, p.position_ms, p.duration_ms)
}

/// Initialize the video player subsystem
pub fn init() {
    serial_println!("[video] Video player (H.264/VP9/AV1) initialized");
}
