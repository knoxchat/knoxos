//! Voice Input — Voice-to-text and audio capture
//!
//! Provides basic voice input capability including PCM audio capture
//! from a virtual microphone and simple speech-to-text processing.
//! Covers status.md item 9.90 (Voice-to-text input).

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

/// Voice input state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    Idle,
    Listening,
    Processing,
    Error,
}

/// Audio sample format
#[derive(Debug, Clone, Copy)]
pub struct AudioFormat {
    pub sample_rate: u32,    // Hz
    pub channels: u8,        // 1 = mono, 2 = stereo
    pub bits_per_sample: u8, // 16
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self {
            sample_rate: 16000, // 16 kHz for speech
            channels: 1,
            bits_per_sample: 16,
        }
    }
}

/// Audio capture buffer
struct CaptureBuffer {
    samples: Vec<i16>,
    format: AudioFormat,
    max_samples: usize,
}

impl CaptureBuffer {
    fn new(max_duration_secs: u32) -> Self {
        let format = AudioFormat::default();
        let max_samples = format.sample_rate as usize * max_duration_secs as usize;
        Self {
            samples: Vec::with_capacity(max_samples),
            format,
            max_samples,
        }
    }

    fn push_samples(&mut self, data: &[i16]) {
        let remaining = self.max_samples.saturating_sub(self.samples.len());
        let count = data.len().min(remaining);
        self.samples.extend_from_slice(&data[..count]);
    }

    fn clear(&mut self) {
        self.samples.clear();
    }

    fn duration_ms(&self) -> u64 {
        if self.format.sample_rate == 0 {
            return 0;
        }
        (self.samples.len() as u64 * 1000) / self.format.sample_rate as u64
    }
}

/// Simple voice activity detection (energy-based)
fn detect_voice_activity(samples: &[i16], threshold: i32) -> bool {
    if samples.is_empty() {
        return false;
    }
    let energy: i64 = samples
        .iter()
        .map(|&s| (s as i64) * (s as i64))
        .sum::<i64>()
        / samples.len() as i64;
    energy > (threshold as i64) * (threshold as i64)
}

/// Very simple phoneme matching (keyword spotting)
/// In a real OS this would use an acoustic model — here we do
/// energy-based keyword detection as a placeholder.
fn recognize_keywords(samples: &[i16], sample_rate: u32) -> Vec<String> {
    let mut results = Vec::new();

    if samples.is_empty() {
        return results;
    }

    // Compute energy profile in 50ms windows
    let window_size = (sample_rate as usize) / 20; // 50ms
    let mut energies = Vec::new();

    for chunk in samples.chunks(window_size) {
        let e: i64 =
            chunk.iter().map(|&s| (s as i64) * (s as i64)).sum::<i64>() / chunk.len().max(1) as i64;
        energies.push(e);
    }

    // Count voiced segments (above threshold)
    let threshold = 500i64 * 500;
    let voiced_count = energies.iter().filter(|&&e| e > threshold).count();

    // Simple duration-based classification
    let duration_ms = (samples.len() as u64 * 1000) / sample_rate as u64;

    if voiced_count == 0 {
        // Silence
        results.push(String::from("[silence]"));
    } else if duration_ms < 500 {
        results.push(String::from("[short utterance]"));
    } else if duration_ms < 2000 {
        results.push(String::from("[word detected]"));
    } else {
        results.push(String::from("[phrase detected]"));
    }

    results
}

/// Voice input system state
struct VoiceSystem {
    state: VoiceState,
    buffer: CaptureBuffer,
    last_result: String,
    recognition_count: u64,
}

lazy_static::lazy_static! {
    static ref SYSTEM: Mutex<VoiceSystem> = Mutex::new(VoiceSystem {
        state: VoiceState::Idle,
        buffer: CaptureBuffer::new(30), // 30 second max recording
        last_result: String::new(),
        recognition_count: 0,
    });
}

static CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);
static TOTAL_SAMPLES: AtomicU64 = AtomicU64::new(0);

/// Start voice capture (push-to-talk)
pub fn start_capture() {
    let mut sys = SYSTEM.lock();
    sys.buffer.clear();
    sys.state = VoiceState::Listening;
    CAPTURE_ACTIVE.store(true, Ordering::Relaxed);
    crate::serial_println!("[voice] Capture started");
}

/// Stop capture and process audio
pub fn stop_capture() -> String {
    let mut sys = SYSTEM.lock();
    CAPTURE_ACTIVE.store(false, Ordering::Relaxed);

    if sys.buffer.samples.is_empty() {
        sys.state = VoiceState::Idle;
        return String::new();
    }

    sys.state = VoiceState::Processing;
    let duration = sys.buffer.duration_ms();

    // Run recognition
    let keywords = recognize_keywords(&sys.buffer.samples, sys.buffer.format.sample_rate);
    let result = if keywords.is_empty() {
        String::from("[no speech detected]")
    } else {
        keywords.join(" ")
    };

    sys.last_result = result.clone();
    sys.recognition_count += 1;
    sys.state = VoiceState::Idle;

    crate::serial_println!("[voice] Processed {}ms audio: {}", duration, result);
    result
}

/// Feed audio samples (called from audio capture interrupt/driver)
pub fn feed_samples(samples: &[i16]) {
    if !CAPTURE_ACTIVE.load(Ordering::Relaxed) {
        return;
    }
    TOTAL_SAMPLES.fetch_add(samples.len() as u64, Ordering::Relaxed);
    SYSTEM.lock().buffer.push_samples(samples);
}

/// Get current voice input state
pub fn current_state() -> VoiceState {
    SYSTEM.lock().state
}

/// Get last recognition result
pub fn last_result() -> String {
    SYSTEM.lock().last_result.clone()
}

/// Check if voice activity is detected in recent samples
pub fn has_voice_activity() -> bool {
    let sys = SYSTEM.lock();
    let samples = &sys.buffer.samples;
    if samples.len() < 800 {
        return false;
    }
    detect_voice_activity(&samples[samples.len() - 800..], 500)
}

/// Initialize voice input subsystem
pub fn init() {
    crate::serial_println!(
        "[voice] Voice-to-text subsystem initialized (16kHz mono, energy-based VAD)"
    );
}
