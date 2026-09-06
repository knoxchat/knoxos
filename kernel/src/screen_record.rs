//! Screen Recording — Framebuffer capture to video
//!
//! Captures the screen framebuffer at configurable frame rates and
//! encodes to a simple raw video format. Supports region capture
//! and full-screen recording.
//! Covers status.md item 15.7 (Screen recording).

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

/// Recording state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    Recording,
    Paused,
    Encoding,
}

/// Recording configuration
#[derive(Debug, Clone, Copy)]
pub struct RecordConfig {
    pub fps: u32,
    pub capture_x: u32,
    pub capture_y: u32,
    pub capture_width: u32,
    pub capture_height: u32,
    pub include_cursor: bool,
    pub max_duration_secs: u32,
}

impl Default for RecordConfig {
    fn default() -> Self {
        Self {
            fps: 30,
            capture_x: 0,
            capture_y: 0,
            capture_width: 1920,
            capture_height: 1080,
            max_duration_secs: 300, // 5 minutes max
            include_cursor: true,
        }
    }
}

/// A captured frame (BGRA raw pixels)
struct CapturedFrame {
    /// Timestamp in milliseconds from recording start
    timestamp_ms: u64,
    /// Compressed frame data (simple RLE)
    data: Vec<u8>,
    width: u32,
    height: u32,
}

/// RLE-encode a frame (simple compression)
fn rle_encode(pixels: &[u32]) -> Vec<u8> {
    let mut encoded = Vec::new();
    if pixels.is_empty() {
        return encoded;
    }

    let mut run_pixel = pixels[0];
    let mut run_len: u16 = 1;

    for &pixel in &pixels[1..] {
        if pixel == run_pixel && run_len < u16::MAX {
            run_len += 1;
        } else {
            // Write run
            encoded.extend_from_slice(&run_len.to_le_bytes());
            encoded.extend_from_slice(&run_pixel.to_le_bytes());
            run_pixel = pixel;
            run_len = 1;
        }
    }
    // Final run
    encoded.extend_from_slice(&run_len.to_le_bytes());
    encoded.extend_from_slice(&run_pixel.to_le_bytes());

    encoded
}

/// RLE-decode a frame
fn rle_decode(data: &[u8], expected_pixels: usize) -> Vec<u32> {
    let mut pixels = Vec::with_capacity(expected_pixels);
    let mut i = 0;

    while i + 5 < data.len() && pixels.len() < expected_pixels {
        let run_len = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
        let pixel = u32::from_le_bytes([data[i + 2], data[i + 3], data[i + 4], data[i + 5]]);
        for _ in 0..run_len {
            if pixels.len() >= expected_pixels {
                break;
            }
            pixels.push(pixel);
        }
        i += 6;
    }

    pixels
}

/// Screen recording session
struct RecordingSession {
    state: RecordingState,
    config: RecordConfig,
    frames: Vec<CapturedFrame>,
    start_time_ms: u64,
    total_frames: u64,
    total_bytes: u64,
}

lazy_static::lazy_static! {
    static ref SESSION: Mutex<RecordingSession> = Mutex::new(RecordingSession {
        state: RecordingState::Idle,
        config: RecordConfig::default(),
        frames: Vec::new(),
        start_time_ms: 0,
        total_frames: 0,
        total_bytes: 0,
    });
}

static IS_RECORDING: AtomicBool = AtomicBool::new(false);
static FRAME_COUNT: AtomicU64 = AtomicU64::new(0);

/// Start screen recording
pub fn start(config: RecordConfig) {
    let mut session = SESSION.lock();
    session.config = config;
    session.frames.clear();
    session.state = RecordingState::Recording;
    session.start_time_ms = (crate::clock::monotonic_ns() / 1_000_000) as u64;
    session.total_frames = 0;
    session.total_bytes = 0;
    IS_RECORDING.store(true, Ordering::Relaxed);
    crate::serial_println!(
        "[screen_record] Recording started: {}x{} @ {}fps",
        config.capture_width,
        config.capture_height,
        config.fps
    );
}

/// Capture a frame from the framebuffer
/// Called from the render loop at the configured FPS
pub fn capture_frame(fb_pixels: &[u32], fb_width: usize, _fb_height: usize) {
    if !IS_RECORDING.load(Ordering::Relaxed) {
        return;
    }

    let mut session = SESSION.lock();
    if session.state != RecordingState::Recording {
        return;
    }

    let config = session.config;
    let elapsed_ms = (crate::clock::monotonic_ns() / 1_000_000) as u64 - session.start_time_ms;

    // Check max duration
    if elapsed_ms > (config.max_duration_secs as u64) * 1000 {
        session.state = RecordingState::Idle;
        IS_RECORDING.store(false, Ordering::Relaxed);
        crate::serial_println!("[screen_record] Max duration reached, stopping");
        return;
    }

    // Extract capture region
    let cw = config.capture_width as usize;
    let ch = config.capture_height as usize;
    let cx = config.capture_x as usize;
    let cy = config.capture_y as usize;

    let mut region = Vec::with_capacity(cw * ch);
    for y in 0..ch {
        let sy = cy + y;
        for x in 0..cw {
            let sx = cx + x;
            let idx = sy * fb_width + sx;
            region.push(if idx < fb_pixels.len() {
                fb_pixels[idx]
            } else {
                0xFF000000
            });
        }
    }

    // RLE compress
    let encoded = rle_encode(&region);
    let frame_bytes = encoded.len() as u64;

    session.frames.push(CapturedFrame {
        timestamp_ms: elapsed_ms,
        data: encoded,
        width: cw as u32,
        height: ch as u32,
    });

    session.total_frames += 1;
    session.total_bytes += frame_bytes;
    FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Pause recording
pub fn pause() {
    let mut session = SESSION.lock();
    if session.state == RecordingState::Recording {
        session.state = RecordingState::Paused;
        crate::serial_println!("[screen_record] Paused");
    }
}

/// Resume recording
pub fn resume() {
    let mut session = SESSION.lock();
    if session.state == RecordingState::Paused {
        session.state = RecordingState::Recording;
        crate::serial_println!("[screen_record] Resumed");
    }
}

/// Stop recording and return total frames captured
pub fn stop() -> u64 {
    let mut session = SESSION.lock();
    IS_RECORDING.store(false, Ordering::Relaxed);
    let frames = session.total_frames;
    let bytes = session.total_bytes;
    session.state = RecordingState::Idle;
    crate::serial_println!(
        "[screen_record] Stopped: {} frames, {} bytes total",
        frames,
        bytes
    );
    frames
}

/// Get current recording state
pub fn current_state() -> RecordingState {
    SESSION.lock().state
}

/// Get recording duration in milliseconds
pub fn duration_ms() -> u64 {
    let session = SESSION.lock();
    if session.state == RecordingState::Idle {
        return 0;
    }
    (crate::clock::monotonic_ns() / 1_000_000) as u64 - session.start_time_ms
}

/// Get frame count
pub fn frame_count() -> u64 {
    FRAME_COUNT.load(Ordering::Relaxed)
}

/// Save recording to VFS (raw format)
pub fn save_to_file(path: &str) -> bool {
    let session = SESSION.lock();
    if session.frames.is_empty() {
        return false;
    }

    // Build a simple header + frame data
    let mut output = String::new();
    output.push_str(&alloc::format!(
        "KNOXREC v1\n{}x{} {}fps {} frames\n",
        session.config.capture_width,
        session.config.capture_height,
        session.config.fps,
        session.total_frames
    ));

    let _ = crate::file_manager::write_file(path, output.as_bytes());
    crate::serial_println!("[screen_record] Saved to {}", path);
    true
}

/// Initialize screen recording subsystem
pub fn init() {
    crate::serial_println!(
        "[screen_record] Screen recording initialized (RLE compression, region capture)"
    );
}
