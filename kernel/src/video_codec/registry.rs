use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

use super::types::VideoCodec;
use super::v4l2::V4l2Device;

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
