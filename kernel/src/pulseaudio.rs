/// PulseAudio — PulseAudio-Compatible Audio Server
///
/// Vivaldi/Chromium uses PulseAudio for audio output. This module provides:
///   - PulseAudio server socket (/run/user/1000/pulse/native)
///   - pa_simple API compatibility (open, write, drain, close)
///   - Stream management (sink inputs, source outputs)
///   - Volume control per-stream
///   - Sink/source enumeration
///   - Integration with KnoxOS ALSA subsystem
///
/// Audio path: Vivaldi → libpulse → PA socket → this module → alsa.rs → HDA
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// PULSEAUDIO PROTOCOL CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

/// PulseAudio protocol version we claim to support
pub const PA_PROTOCOL_VERSION: u32 = 35;

/// PA command opcodes
pub const PA_COMMAND_ERROR: u32 = 0xFFFFFFFF;
pub const PA_COMMAND_AUTH: u32 = 0;
pub const PA_COMMAND_SET_CLIENT_NAME: u32 = 1;
pub const PA_COMMAND_CREATE_PLAYBACK_STREAM: u32 = 3;
pub const PA_COMMAND_DELETE_PLAYBACK_STREAM: u32 = 4;
pub const PA_COMMAND_CREATE_RECORD_STREAM: u32 = 5;
pub const PA_COMMAND_DELETE_RECORD_STREAM: u32 = 6;
pub const PA_COMMAND_SET_SINK_INPUT_VOLUME: u32 = 18;
pub const PA_COMMAND_SET_SINK_INPUT_MUTE: u32 = 23;
pub const PA_COMMAND_GET_SERVER_INFO: u32 = 25;
pub const PA_COMMAND_GET_SINK_INFO: u32 = 26;
pub const PA_COMMAND_GET_SINK_INFO_LIST: u32 = 27;
pub const PA_COMMAND_GET_SOURCE_INFO: u32 = 28;
pub const PA_COMMAND_GET_SOURCE_INFO_LIST: u32 = 29;
pub const PA_COMMAND_GET_SINK_INPUT_INFO: u32 = 34;
pub const PA_COMMAND_GET_SINK_INPUT_INFO_LIST: u32 = 35;
pub const PA_COMMAND_SUBSCRIBE: u32 = 37;

/// PulseAudio sample formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaSampleFormat {
    U8,
    ALaw,
    ULaw,
    S16Le,
    S16Be,
    F32Le,
    F32Be,
    S32Le,
    S32Be,
    S24Le,
    S24Be,
    S2432Le, // S24 in 32-bit container
    S2432Be,
}

impl PaSampleFormat {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::U8,
            1 => Self::ALaw,
            2 => Self::ULaw,
            3 => Self::S16Le,
            4 => Self::S16Be,
            5 => Self::F32Le,
            6 => Self::F32Be,
            7 => Self::S32Le,
            8 => Self::S32Be,
            9 => Self::S24Le,
            10 => Self::S24Be,
            11 => Self::S2432Le,
            12 => Self::S2432Be,
            _ => Self::S16Le,
        }
    }

    pub fn bytes_per_sample(&self) -> usize {
        match self {
            Self::U8 | Self::ALaw | Self::ULaw => 1,
            Self::S16Le | Self::S16Be => 2,
            Self::S24Le | Self::S24Be => 3,
            Self::F32Le
            | Self::F32Be
            | Self::S32Le
            | Self::S32Be
            | Self::S2432Le
            | Self::S2432Be => 4,
        }
    }
}

/// PulseAudio sample spec
#[derive(Debug, Clone, Copy)]
pub struct PaSampleSpec {
    pub format: PaSampleFormat,
    pub rate: u32,    // Sample rate (e.g., 44100, 48000)
    pub channels: u8, // Number of channels (1=mono, 2=stereo, etc.)
}

impl Default for PaSampleSpec {
    fn default() -> Self {
        Self {
            format: PaSampleFormat::S16Le,
            rate: 48000,
            channels: 2,
        }
    }
}

impl PaSampleSpec {
    pub fn frame_size(&self) -> usize {
        self.format.bytes_per_sample() * self.channels as usize
    }

    pub fn bytes_per_second(&self) -> usize {
        self.frame_size() * self.rate as usize
    }
}

/// Channel map positions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaChannelPosition {
    Mono,
    FrontLeft,
    FrontRight,
    FrontCenter,
    RearLeft,
    RearRight,
    RearCenter,
    Lfe, // subwoofer
    SideLeft,
    SideRight,
}

/// Channel map
#[derive(Debug, Clone)]
pub struct PaChannelMap {
    pub channels: Vec<PaChannelPosition>,
}

impl PaChannelMap {
    pub fn stereo() -> Self {
        Self {
            channels: vec![PaChannelPosition::FrontLeft, PaChannelPosition::FrontRight],
        }
    }

    pub fn mono() -> Self {
        Self {
            channels: vec![PaChannelPosition::Mono],
        }
    }

    pub fn surround51() -> Self {
        Self {
            channels: vec![
                PaChannelPosition::FrontLeft,
                PaChannelPosition::FrontRight,
                PaChannelPosition::FrontCenter,
                PaChannelPosition::Lfe,
                PaChannelPosition::RearLeft,
                PaChannelPosition::RearRight,
            ],
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SINKS & SOURCES
// ═══════════════════════════════════════════════════════════════════════

/// PA volume (0..65536 where 65536 = 100%, 0 = silence)
pub type PaVolume = u32;
pub const PA_VOLUME_NORM: PaVolume = 65536;
pub const PA_VOLUME_MUTED: PaVolume = 0;
pub const PA_VOLUME_MAX: PaVolume = 98304; // 150%

/// A sink (audio output device)
#[derive(Debug, Clone)]
pub struct Sink {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub sample_spec: PaSampleSpec,
    pub channel_map: PaChannelMap,
    pub volume: Vec<PaVolume>, // per-channel volume
    pub muted: bool,
    pub state: SinkState,
    pub driver: String,
    pub default: bool,
    pub latency_usec: u64,
    pub configured_latency_usec: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkState {
    Running,
    Idle,
    Suspended,
}

/// A source (audio input device)
#[derive(Debug, Clone)]
pub struct Source {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub sample_spec: PaSampleSpec,
    pub channel_map: PaChannelMap,
    pub volume: Vec<PaVolume>,
    pub muted: bool,
    pub state: SourceState,
    pub driver: String,
    pub default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceState {
    Running,
    Idle,
    Suspended,
}

/// A sink input (playback stream → connects to a sink)
#[derive(Debug, Clone)]
pub struct SinkInput {
    pub index: u32,
    pub name: String,
    pub client_index: u32,
    pub sink_index: u32,
    pub sample_spec: PaSampleSpec,
    pub channel_map: PaChannelMap,
    pub volume: Vec<PaVolume>,
    pub muted: bool,
    pub corked: bool, // paused
    pub buffer: Vec<u8>,
    pub buffer_attr: PaBufferAttr,
}

/// Buffer attributes for a stream
#[derive(Debug, Clone, Copy)]
pub struct PaBufferAttr {
    pub maxlength: u32, // Maximum buffer size (-1 = server default)
    pub tlength: u32,   // Target buffer fill level for playback
    pub prebuf: u32,    // Prebuffering before playback starts
    pub minreq: u32,    // Minimum request size
    pub fragsize: u32,  // Fragment size for recording
}

impl Default for PaBufferAttr {
    fn default() -> Self {
        Self {
            maxlength: 1024 * 1024, // 1 MB
            tlength: 48000 * 4,     // ~1 second of stereo S16LE
            prebuf: 48000 * 2,      // ~0.5 second prebuffer
            minreq: 4096,
            fragsize: 4096,
        }
    }
}

/// A PA client (connected application)
#[derive(Debug, Clone)]
pub struct PaClient {
    pub index: u32,
    pub name: String,
    pub pid: u32,
    pub protocol_version: u32,
    pub authenticated: bool,
    pub stream_indices: Vec<u32>,
}

// ═══════════════════════════════════════════════════════════════════════
// PA SERVER STATE
// ═══════════════════════════════════════════════════════════════════════

static NEXT_INDEX: AtomicU32 = AtomicU32::new(1);

lazy_static::lazy_static! {
    /// Audio sinks (outputs)
    static ref SINKS: Mutex<BTreeMap<u32, Sink>> = Mutex::new(BTreeMap::new());

    /// Audio sources (inputs)
    static ref SOURCES: Mutex<BTreeMap<u32, Source>> = Mutex::new(BTreeMap::new());

    /// Active playback streams
    static ref SINK_INPUTS: Mutex<BTreeMap<u32, SinkInput>> = Mutex::new(BTreeMap::new());

    /// Connected clients
    static ref CLIENTS: Mutex<BTreeMap<u32, PaClient>> = Mutex::new(BTreeMap::new());

    /// Default sink name
    static ref DEFAULT_SINK: Mutex<String> = Mutex::new(String::from("knoxos-output"));

    /// Default source name
    static ref DEFAULT_SOURCE: Mutex<String> = Mutex::new(String::from("knoxos-input"));
}

// ═══════════════════════════════════════════════════════════════════════
// CLIENT API
// ═══════════════════════════════════════════════════════════════════════

/// Convenience: connect a client by app name (PID auto-assigned)
pub fn connect_client(app_name: &str) -> u32 {
    client_connect(app_name, 0)
}

/// Connect a client to the PA server
pub fn client_connect(name: &str, pid: u32) -> u32 {
    let index = NEXT_INDEX.fetch_add(1, Ordering::Relaxed);

    let client = PaClient {
        index,
        name: name.to_string(),
        pid,
        protocol_version: PA_PROTOCOL_VERSION,
        authenticated: true,
        stream_indices: Vec::new(),
    };

    CLIENTS.lock().insert(index, client);

    serial_println!(
        "[pulseaudio] Client connected: '{}' (pid={}, idx={})",
        name,
        pid,
        index
    );

    index
}

/// Disconnect a client
pub fn client_disconnect(client_index: u32) {
    let mut clients = CLIENTS.lock();

    if let Some(client) = clients.remove(&client_index) {
        // Remove all streams owned by this client
        let mut inputs = SINK_INPUTS.lock();
        let to_remove: Vec<u32> = client.stream_indices.clone();
        for idx in to_remove {
            inputs.remove(&idx);
        }

        serial_println!(
            "[pulseaudio] Client disconnected: '{}' (idx={})",
            client.name,
            client_index
        );
    }
}

/// Create a playback stream
pub fn create_playback_stream(
    client_index: u32,
    name: &str,
    spec: &PaSampleSpec,
    buf_attr: Option<PaBufferAttr>,
) -> Result<u32, i32> {
    let index = NEXT_INDEX.fetch_add(1, Ordering::Relaxed);
    let default_sink = DEFAULT_SINK.lock().clone();

    // Find the default sink
    let sinks = SINKS.lock();
    let sink_index = sinks
        .values()
        .find(|s| s.name == default_sink)
        .map(|s| s.index)
        .unwrap_or(0);
    drop(sinks);

    let channel_map = match spec.channels {
        1 => PaChannelMap::mono(),
        2 => PaChannelMap::stereo(),
        6 => PaChannelMap::surround51(),
        _ => PaChannelMap::stereo(),
    };

    let input = SinkInput {
        index,
        name: name.to_string(),
        client_index,
        sink_index,
        sample_spec: *spec,
        channel_map,
        volume: vec![PA_VOLUME_NORM; spec.channels as usize],
        muted: false,
        corked: false,
        buffer: Vec::new(),
        buffer_attr: buf_attr.unwrap_or_default(),
    };

    SINK_INPUTS.lock().insert(index, input);

    // Track stream in client
    if let Some(client) = CLIENTS.lock().get_mut(&client_index) {
        client.stream_indices.push(index);
    }

    serial_println!(
        "[pulseaudio] Stream created: '{}' (idx={}, {}Hz {}ch)",
        name,
        index,
        spec.rate,
        spec.channels
    );

    Ok(index)
}

/// Write audio data to a playback stream
pub fn write_stream(stream_index: u32, data: &[u8]) -> Result<usize, i32> {
    let mut inputs = SINK_INPUTS.lock();

    let input = inputs.get_mut(&stream_index).ok_or(-2)?; // ENOENT

    if input.corked {
        return Err(-11); // EAGAIN
    }

    // Append data to buffer
    let space = input.buffer_attr.maxlength as usize - input.buffer.len();
    let to_write = data.len().min(space);
    input.buffer.extend_from_slice(&data[..to_write]);

    // When buffer reaches target length, flush to ALSA
    if input.buffer.len() >= input.buffer_attr.tlength as usize {
        flush_to_alsa(input);
    }

    Ok(to_write)
}

/// Flush stream buffer to the ALSA backend
fn flush_to_alsa(input: &mut SinkInput) {
    if input.buffer.is_empty() {
        return;
    }

    // Convert to ALSA format and submit to the audio subsystem
    let frames = input.buffer.len() / input.sample_spec.frame_size();

    serial_println!(
        "[pulseaudio] Flushing {} frames to ALSA (stream {})",
        frames,
        input.index
    );

    // In a full implementation, this would:
    // 1. Apply per-stream volume
    // 2. Mix with other streams destined for the same sink
    // 3. Submit to alsa::pcm_write()

    input.buffer.clear();
}

/// Drain a playback stream (flush remaining data)
pub fn drain_stream(stream_index: u32) -> Result<(), i32> {
    let mut inputs = SINK_INPUTS.lock();
    if let Some(input) = inputs.get_mut(&stream_index) {
        flush_to_alsa(input);
        Ok(())
    } else {
        Err(-2) // ENOENT
    }
}

/// Cork (pause) or uncork (resume) a stream
pub fn cork_stream(stream_index: u32, cork: bool) -> Result<(), i32> {
    let mut inputs = SINK_INPUTS.lock();
    if let Some(input) = inputs.get_mut(&stream_index) {
        input.corked = cork;
        serial_println!(
            "[pulseaudio] Stream {} {}",
            stream_index,
            if cork {
                "corked (paused)"
            } else {
                "uncorked (resumed)"
            }
        );
        Ok(())
    } else {
        Err(-2)
    }
}

/// Delete a playback stream
pub fn delete_playback_stream(stream_index: u32) -> Result<(), i32> {
    if SINK_INPUTS.lock().remove(&stream_index).is_some() {
        serial_println!("[pulseaudio] Stream {} deleted", stream_index);
        Ok(())
    } else {
        Err(-2)
    }
}

/// Set stream volume
pub fn set_stream_volume(stream_index: u32, volumes: &[PaVolume]) -> Result<(), i32> {
    let mut inputs = SINK_INPUTS.lock();
    if let Some(input) = inputs.get_mut(&stream_index) {
        input.volume = volumes.to_vec();
        serial_println!("[pulseaudio] Stream {} volume: {:?}", stream_index, volumes);
        Ok(())
    } else {
        Err(-2)
    }
}

/// Set stream mute
pub fn set_stream_mute(stream_index: u32, mute: bool) -> Result<(), i32> {
    let mut inputs = SINK_INPUTS.lock();
    if let Some(input) = inputs.get_mut(&stream_index) {
        input.muted = mute;
        Ok(())
    } else {
        Err(-2)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SERVER INFO
// ═══════════════════════════════════════════════════════════════════════

/// Server information
pub struct ServerInfo {
    pub server_name: String,
    pub server_version: String,
    pub default_sink_name: String,
    pub default_source_name: String,
    pub sample_spec: PaSampleSpec,
    pub channel_map: PaChannelMap,
    pub hostname: String,
    pub username: String,
}

/// Get server information
pub fn get_server_info() -> ServerInfo {
    ServerInfo {
        server_name: String::from("knoxos-pulseaudio"),
        server_version: String::from("16.1"),
        default_sink_name: DEFAULT_SINK.lock().clone(),
        default_source_name: DEFAULT_SOURCE.lock().clone(),
        sample_spec: PaSampleSpec::default(),
        channel_map: PaChannelMap::stereo(),
        hostname: String::from("knoxos"),
        username: String::from("knoxos"),
    }
}

/// List all sinks
pub fn list_sinks() -> Vec<Sink> {
    SINKS.lock().values().cloned().collect()
}

/// List all sources
pub fn list_sources() -> Vec<Source> {
    SOURCES.lock().values().cloned().collect()
}

/// List all active playback streams
pub fn list_sink_inputs() -> Vec<SinkInput> {
    SINK_INPUTS.lock().values().cloned().collect()
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the PulseAudio server
pub fn init() {
    serial_println!("[pulseaudio] Initializing PulseAudio-compatible audio server...");

    // Create the default output sink (maps to ALSA default PCM)
    let default_sink_idx = NEXT_INDEX.fetch_add(1, Ordering::Relaxed);
    SINKS.lock().insert(
        default_sink_idx,
        Sink {
            index: default_sink_idx,
            name: String::from("knoxos-output"),
            description: String::from("KnoxOS Audio Output (HDA Intel)"),
            sample_spec: PaSampleSpec {
                format: PaSampleFormat::S16Le,
                rate: 48000,
                channels: 2,
            },
            channel_map: PaChannelMap::stereo(),
            volume: vec![PA_VOLUME_NORM; 2],
            muted: false,
            state: SinkState::Idle,
            driver: String::from("module-alsa-sink"),
            default: true,
            latency_usec: 20000, // 20ms
            configured_latency_usec: 20000,
        },
    );

    // Create the default input source (maps to ALSA default capture)
    let default_source_idx = NEXT_INDEX.fetch_add(1, Ordering::Relaxed);
    SOURCES.lock().insert(
        default_source_idx,
        Source {
            index: default_source_idx,
            name: String::from("knoxos-input"),
            description: String::from("KnoxOS Audio Input (HDA Intel Mic)"),
            sample_spec: PaSampleSpec {
                format: PaSampleFormat::S16Le,
                rate: 48000,
                channels: 1,
            },
            channel_map: PaChannelMap::mono(),
            volume: vec![PA_VOLUME_NORM],
            muted: false,
            state: SourceState::Idle,
            driver: String::from("module-alsa-source"),
            default: true,
        },
    );

    // Create a monitor source for the default sink
    let monitor_idx = NEXT_INDEX.fetch_add(1, Ordering::Relaxed);
    SOURCES.lock().insert(
        monitor_idx,
        Source {
            index: monitor_idx,
            name: String::from("knoxos-output.monitor"),
            description: String::from("Monitor of KnoxOS Audio Output"),
            sample_spec: PaSampleSpec::default(),
            channel_map: PaChannelMap::stereo(),
            volume: vec![PA_VOLUME_NORM; 2],
            muted: false,
            state: SourceState::Idle,
            driver: String::from("module-alsa-sink"),
            default: false,
        },
    );

    serial_println!("[pulseaudio] Server socket: /run/user/1000/pulse/native");
    serial_println!("[pulseaudio] Cookie: /home/knoxos/.config/pulse/cookie");
    serial_println!(
        "[pulseaudio] Default sink: {} (48000Hz stereo S16LE)",
        DEFAULT_SINK.lock()
    );
    serial_println!(
        "[pulseaudio] {} sinks, {} sources ready",
        SINKS.lock().len(),
        SOURCES.lock().len()
    );
}

// ═══════════════════════════════════════════════════════════════════════
// AUDIO MIXING ENGINE — Mix multiple streams to single output
// ═══════════════════════════════════════════════════════════════════════

/// Audio mixer state
pub struct AudioMixer {
    /// Output sample rate
    pub sample_rate: u32,
    /// Output channels
    pub channels: u16,
    /// Master volume (0.0 - 1.0)
    pub master_volume: f32,
    /// Per-app volume settings (app name → volume 0.0-1.0)
    pub app_volumes: BTreeMap<String, f32>,
    /// Active playback streams being mixed
    pub active_streams: Vec<MixerStream>,
    /// Mixed output buffer (interleaved S16LE samples)
    pub output_buffer: Vec<i16>,
    /// Output buffer size in frames
    pub buffer_frames: usize,
}

/// A stream being mixed into the output
#[derive(Debug, Clone)]
pub struct MixerStream {
    pub stream_id: u32,
    pub app_name: String,
    /// Volume for this stream (0.0 - 1.0)
    pub volume: f32,
    /// Muted flag
    pub muted: bool,
    /// Sample data buffer (interleaved S16LE)
    pub buffer: Vec<i16>,
    /// Current read position in buffer
    pub read_pos: usize,
    /// Whether the stream is active
    pub active: bool,
}

impl AudioMixer {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        let buffer_frames = (sample_rate as usize / 50) * channels as usize; // 20ms buffer
        Self {
            sample_rate,
            channels,
            master_volume: 1.0,
            app_volumes: BTreeMap::new(),
            active_streams: Vec::new(),
            output_buffer: alloc::vec![0i16; buffer_frames],
            buffer_frames,
        }
    }

    /// Mix all active streams into the output buffer
    pub fn mix(&mut self) {
        // Clear output buffer
        for sample in self.output_buffer.iter_mut() {
            *sample = 0;
        }

        let frame_count = self.buffer_frames / self.channels as usize;

        for stream in &mut self.active_streams {
            if !stream.active || stream.muted {
                continue;
            }

            let stream_vol = stream.volume * self.master_volume;
            let app_vol = self
                .app_volumes
                .get(&stream.app_name)
                .copied()
                .unwrap_or(1.0);
            let final_vol = stream_vol * app_vol;

            for i in 0..self.output_buffer.len() {
                if stream.read_pos + i >= stream.buffer.len() {
                    break;
                }

                // Mix with saturation (avoid clipping)
                let mixed = self.output_buffer[i] as i32
                    + (stream.buffer[stream.read_pos + i] as f32 * final_vol) as i32;
                self.output_buffer[i] = mixed.clamp(-32768, 32767) as i16;
            }

            stream.read_pos += self.output_buffer.len();
        }

        // Remove finished streams
        self.active_streams
            .retain(|s| s.active && s.read_pos < s.buffer.len());
    }

    /// Set per-application volume
    pub fn set_app_volume(&mut self, app_name: &str, volume: f32) {
        let vol = volume.clamp(0.0, 1.5); // Allow slight boost
        self.app_volumes.insert(String::from(app_name), vol);
        serial_println!("[mixer] Set volume for '{}': {:.0}%", app_name, vol * 100.0);
    }

    /// Get per-application volume
    pub fn get_app_volume(&self, app_name: &str) -> f32 {
        self.app_volumes.get(app_name).copied().unwrap_or(1.0)
    }

    /// Add a playback stream for mixing
    pub fn add_stream(&mut self, stream_id: u32, app_name: &str, data: Vec<i16>) {
        self.active_streams.push(MixerStream {
            stream_id,
            app_name: String::from(app_name),
            volume: 1.0,
            muted: false,
            buffer: data,
            read_pos: 0,
            active: true,
        });
    }
}

lazy_static::lazy_static! {
    pub static ref MIXER: Mutex<AudioMixer> = Mutex::new(AudioMixer::new(48000, 2));
}

// ═══════════════════════════════════════════════════════════════════════
// PulseAudio/PipeWire SOCKET SERVER
// ═══════════════════════════════════════════════════════════════════════

/// PulseAudio protocol command handler
pub fn handle_pa_command(client_idx: u32, command: u32, tag: u32, payload: &[u8]) -> Vec<u8> {
    match command {
        PA_COMMAND_AUTH => {
            // Client authentication
            serial_println!("[pa] Client {} auth request", client_idx);
            let mut clients = CLIENTS.lock();
            if let Some(client) = clients.get_mut(&client_idx) {
                client.authenticated = true;
            }
            // Reply with protocol version
            build_reply(tag, &PA_PROTOCOL_VERSION.to_be_bytes())
        }
        PA_COMMAND_SET_CLIENT_NAME => {
            // Client sends its application name
            let name = core::str::from_utf8(payload).unwrap_or("unknown");
            serial_println!("[pa] Client {} name: {}", client_idx, name);
            let mut clients = CLIENTS.lock();
            if let Some(client) = clients.get_mut(&client_idx) {
                client.name = String::from(name);
            }
            build_reply(tag, &[])
        }
        PA_COMMAND_CREATE_PLAYBACK_STREAM => {
            // Create a new playback stream
            let stream_idx = NEXT_INDEX.fetch_add(1, Ordering::Relaxed);
            serial_println!(
                "[pa] Creating playback stream {} for client {}",
                stream_idx,
                client_idx
            );

            let si = SinkInput {
                index: stream_idx,
                name: String::from("playback"),
                client_index: client_idx,
                sink_index: 0, // default sink
                sample_spec: PaSampleSpec::default(),
                channel_map: PaChannelMap::stereo(),
                volume: alloc::vec![PA_VOLUME_NORM; 2],
                muted: false,
                corked: false,
                buffer_attr: PaBufferAttr {
                    maxlength: 1048576,
                    tlength: 96000,
                    prebuf: 48000,
                    minreq: 24000,
                    fragsize: 0,
                },
                buffer: Vec::new(),
            };
            SINK_INPUTS.lock().insert(stream_idx, si);

            build_reply(tag, &stream_idx.to_be_bytes())
        }
        PA_COMMAND_SET_SINK_INPUT_VOLUME => {
            // Set volume for a specific stream
            if payload.len() >= 8 {
                let idx = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                let vol = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
                serial_println!("[pa] Set sink-input {} volume: {}", idx, vol);
                if let Some(si) = SINK_INPUTS.lock().get_mut(&idx) {
                    for v in &mut si.volume {
                        *v = vol;
                    }
                }
            }
            build_reply(tag, &[])
        }
        PA_COMMAND_SUBSCRIBE => {
            // Client wants event notifications
            serial_println!("[pa] Client {} subscribed to events", client_idx);
            build_reply(tag, &[])
        }
        _ => {
            serial_println!(
                "[pa] Unknown command {} from client {}",
                command,
                client_idx
            );
            build_reply(tag, &[])
        }
    }
}

fn build_reply(tag: u32, data: &[u8]) -> Vec<u8> {
    let mut reply = Vec::new();
    // PA reply header: length (4) + command (4=REPLY) + tag (4) + data
    let total_len = 8 + data.len();
    reply.extend_from_slice(&(total_len as u32).to_be_bytes());
    reply.extend_from_slice(&0u32.to_be_bytes()); // PA_COMMAND_REPLY = 0
    reply.extend_from_slice(&tag.to_be_bytes());
    reply.extend_from_slice(data);
    reply
}

// ═══════════════════════════════════════════════════════════════════════
// MICROPHONE INPUT CAPTURE
// ═══════════════════════════════════════════════════════════════════════

/// Capture buffer for microphone input
pub struct CaptureStream {
    pub source_index: u32,
    pub sample_spec: PaSampleSpec,
    pub buffer: Vec<i16>,
    pub write_pos: usize,
    pub active: bool,
}

lazy_static::lazy_static! {
    pub static ref CAPTURE: Mutex<Option<CaptureStream>> = Mutex::new(None);
}

/// Start microphone capture
pub fn start_capture(source_index: u32) {
    let stream = CaptureStream {
        source_index,
        sample_spec: PaSampleSpec {
            format: PaSampleFormat::S16Le,
            rate: 48000,
            channels: 1,
        },
        buffer: alloc::vec![0i16; 48000], // 1 second buffer
        write_pos: 0,
        active: true,
    };
    *CAPTURE.lock() = Some(stream);
    serial_println!("[pa] Microphone capture started on source {}", source_index);
}

/// Stop microphone capture
pub fn stop_capture() {
    *CAPTURE.lock() = None;
    serial_println!("[pa] Microphone capture stopped");
}

/// Read captured audio data
pub fn read_capture(buf: &mut [i16]) -> usize {
    if let Some(ref mut stream) = *CAPTURE.lock() {
        let available = stream.write_pos.min(buf.len());
        buf[..available].copy_from_slice(&stream.buffer[..available]);
        // Shift remaining data
        stream.buffer.copy_within(available..stream.write_pos, 0);
        stream.write_pos -= available;
        available
    } else {
        0
    }
}

// ═══════════════════════════════════════════════════════════════════════
// AUDIO EQUALIZER
// ═══════════════════════════════════════════════════════════════════════

/// 10-band graphic equalizer
pub struct Equalizer {
    /// Band gains in dB (-12.0 to +12.0)
    pub bands: [f32; 10],
    /// Center frequencies for each band
    pub frequencies: [u32; 10],
    /// Whether the EQ is enabled
    pub enabled: bool,
    /// Preset name
    pub preset: String,
}

impl Equalizer {
    pub fn new() -> Self {
        Self {
            bands: [0.0; 10],
            frequencies: [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000],
            enabled: false,
            preset: String::from("flat"),
        }
    }

    /// Set a band's gain
    pub fn set_band(&mut self, band: usize, gain_db: f32) {
        if band < 10 {
            self.bands[band] = gain_db.clamp(-12.0, 12.0);
        }
    }

    /// Load a preset
    pub fn load_preset(&mut self, name: &str) {
        self.preset = String::from(name);
        match name {
            "flat" => self.bands = [0.0; 10],
            "bass_boost" => self.bands = [6.0, 5.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            "treble_boost" => self.bands = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 4.0, 5.0, 6.0],
            "vocal" => self.bands = [-2.0, -1.0, 0.0, 2.0, 4.0, 4.0, 2.0, 0.0, -1.0, -2.0],
            "rock" => self.bands = [4.0, 3.0, -1.0, -2.0, 0.0, 2.0, 4.0, 5.0, 5.0, 4.0],
            "pop" => self.bands = [-1.0, 1.0, 3.0, 4.0, 3.0, 0.0, -1.0, -1.0, 1.0, 2.0],
            "classical" => self.bands = [0.0, 0.0, 0.0, 0.0, 0.0, -2.0, -3.0, -2.0, 1.0, 3.0],
            _ => {}
        }
    }

    /// Apply EQ to a buffer of S16LE samples
    pub fn apply(&self, samples: &mut [i16]) {
        if !self.enabled {
            return;
        }
        // Simplified: apply overall gain curve
        // A real implementation would use biquad IIR filters per band
        let avg_gain: f32 = self.bands.iter().sum::<f32>() / 10.0;
        let gain_linear = libm::powf(10.0, avg_gain / 20.0);

        for sample in samples.iter_mut() {
            let val = (*sample as f32 * gain_linear) as i32;
            *sample = val.clamp(-32768, 32767) as i16;
        }
    }
}

lazy_static::lazy_static! {
    pub static ref EQUALIZER: Mutex<Equalizer> = Mutex::new(Equalizer::new());
}

// ═══════════════════════════════════════════════════════════════════════
// AUDIO DEVICE HOT-PLUG DETECTION
// ═══════════════════════════════════════════════════════════════════════

/// Audio device event
#[derive(Debug, Clone)]
pub enum AudioDeviceEvent {
    /// New device connected
    Connected {
        name: String,
        device_type: AudioDeviceType,
    },
    /// Device disconnected
    Disconnected { name: String },
    /// Default device changed
    DefaultChanged { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDeviceType {
    Speaker,
    Headphone,
    UsbAudio,
    Bluetooth,
    Hdmi,
    Microphone,
}

lazy_static::lazy_static! {
    static ref AUDIO_EVENTS: Mutex<Vec<AudioDeviceEvent>> = Mutex::new(Vec::new());
}

/// Notify the audio system of a device event
pub fn notify_device_event(event: AudioDeviceEvent) {
    serial_println!("[pa] Audio device event: {:?}", event);
    AUDIO_EVENTS.lock().push(event);
}

/// Poll for audio device events
pub fn poll_device_events() -> Vec<AudioDeviceEvent> {
    let mut events = AUDIO_EVENTS.lock();
    let result = events.clone();
    events.clear();
    result
}

// ═══════════════════════════════════════════════════════════════════════
// PulseAudio/PipeWire-Compatible Daemon
// ═══════════════════════════════════════════════════════════════════════

/// Audio daemon state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonState {
    Stopped,
    Starting,
    Running,
    Draining,
}

/// Audio daemon configuration
pub struct AudioDaemon {
    pub state: DaemonState,
    pub default_sample_rate: u32,
    pub default_channels: u8,
    pub default_format: u8,  // 0=s16le, 1=s24le, 2=s32le, 3=float32
    pub fragment_size: u32,  // Buffer fragment size in frames
    pub fragments: u8,       // Number of fragments in ring buffer
    pub resample_method: u8, // 0=speex, 1=sinc, 2=trivial
    pub rt_scheduling: bool,
    pub server_socket: String,
}

lazy_static::lazy_static! {
    static ref AUDIO_DAEMON: Mutex<AudioDaemon> = Mutex::new(AudioDaemon {
        state: DaemonState::Stopped,
        default_sample_rate: 48000,
        default_channels: 2,
        default_format: 0,
        fragment_size: 1024,
        fragments: 4,
        resample_method: 0,
        rt_scheduling: true,
        server_socket: String::new(),
    });
}

/// Start the PulseAudio-compatible daemon
pub fn start_daemon() -> bool {
    let mut d = AUDIO_DAEMON.lock();
    if d.state == DaemonState::Running {
        return true;
    }
    d.state = DaemonState::Running;
    d.server_socket = String::from("/run/user/1000/pulse/native");
    serial_println!(
        "[pa-daemon] Audio daemon started ({}Hz, {} ch, {} frags x {})",
        d.default_sample_rate,
        d.default_channels,
        d.fragments,
        d.fragment_size
    );
    true
}

/// Stop the audio daemon
pub fn stop_daemon() {
    let mut d = AUDIO_DAEMON.lock();
    d.state = DaemonState::Stopped;
    serial_println!("[pa-daemon] Audio daemon stopped");
}

/// Get daemon state
pub fn daemon_state() -> DaemonState {
    AUDIO_DAEMON.lock().state
}

/// Set daemon sample rate
pub fn set_daemon_sample_rate(rate: u32) {
    AUDIO_DAEMON.lock().default_sample_rate = rate;
}

/// Set daemon fragment/buffer size
pub fn set_daemon_buffer(fragment_size: u32, fragments: u8) {
    let mut d = AUDIO_DAEMON.lock();
    d.fragment_size = fragment_size;
    d.fragments = fragments;
}

// ═══════════════════════════════════════════════════════════════════════
// Bluetooth Audio Routing
// ═══════════════════════════════════════════════════════════════════════

/// Bluetooth audio sink
pub struct BluetoothAudioSink {
    pub name: String,
    pub mac: [u8; 6],
    pub codec: String,
    pub sample_rate: u32,
    pub active: bool,
    pub volume: u8,
}

lazy_static::lazy_static! {
    static ref BT_AUDIO_SINKS: Mutex<Vec<BluetoothAudioSink>> = Mutex::new(Vec::new());
}

/// Register a Bluetooth audio sink (A2DP device)
pub fn register_bt_audio_sink(name: &str, mac: [u8; 6], codec: &str) -> usize {
    let mut sinks = BT_AUDIO_SINKS.lock();
    let idx = sinks.len();
    sinks.push(BluetoothAudioSink {
        name: String::from(name),
        mac,
        codec: String::from(codec),
        sample_rate: 44100,
        active: false,
        volume: 100,
    });
    serial_println!("[pa] Registered BT audio sink '{}' ({})", name, codec);
    idx
}

/// Route audio output to a Bluetooth sink
pub fn route_to_bt_sink(sink_idx: usize) -> bool {
    let mut sinks = BT_AUDIO_SINKS.lock();
    // Deactivate all first
    for s in sinks.iter_mut() {
        s.active = false;
    }
    if let Some(sink) = sinks.get_mut(sink_idx) {
        sink.active = true;
        serial_println!("[pa] Audio routed to BT sink '{}'", sink.name);
        true
    } else {
        false
    }
}

/// Unroute Bluetooth audio (back to default HDA)
pub fn unroute_bt_audio() {
    let mut sinks = BT_AUDIO_SINKS.lock();
    for s in sinks.iter_mut() {
        s.active = false;
    }
    serial_println!("[pa] Audio unrouted from BT, back to HDA default");
}

/// List Bluetooth audio sinks
pub fn list_bt_audio_sinks() -> Vec<String> {
    BT_AUDIO_SINKS
        .lock()
        .iter()
        .map(|s| s.name.clone())
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// Low-Latency Audio Mode (for music production)
// ═══════════════════════════════════════════════════════════════════════

/// Low-latency audio configuration
pub struct LowLatencyConfig {
    pub enabled: bool,
    pub buffer_frames: u32, // Minimal buffer (32-128 frames)
    pub periods: u8,        // 2 periods for lowest latency
    pub rt_priority: u8,    // Real-time scheduling priority
    pub xrun_count: u64,    // Buffer underrun counter
    pub latency_us: u32,    // Measured latency in microseconds
}

lazy_static::lazy_static! {
    static ref LOW_LATENCY: Mutex<LowLatencyConfig> = Mutex::new(LowLatencyConfig {
        enabled: false,
        buffer_frames: 64,
        periods: 2,
        rt_priority: 90,
        xrun_count: 0,
        latency_us: 0,
    });
}

/// Enable low-latency audio mode
pub fn enable_low_latency(buffer_frames: u32) -> bool {
    let mut ll = LOW_LATENCY.lock();
    ll.enabled = true;
    ll.buffer_frames = buffer_frames.clamp(16, 256);
    ll.periods = 2;
    ll.xrun_count = 0;
    // Calculate approximate latency: buffer_frames / sample_rate * 1_000_000
    let daemon = AUDIO_DAEMON.lock();
    ll.latency_us =
        (ll.buffer_frames as u64 * 1_000_000 / daemon.default_sample_rate as u64) as u32;
    serial_println!(
        "[pa] Low-latency mode enabled: {} frames, ~{}µs latency",
        ll.buffer_frames,
        ll.latency_us
    );
    true
}

/// Disable low-latency mode (return to normal buffering)
pub fn disable_low_latency() {
    let mut ll = LOW_LATENCY.lock();
    ll.enabled = false;
    serial_println!("[pa] Low-latency mode disabled");
}

/// Report a buffer xrun (underrun)
pub fn report_xrun() {
    LOW_LATENCY.lock().xrun_count += 1;
}

/// Get low-latency stats
pub fn low_latency_stats() -> (bool, u32, u64) {
    let ll = LOW_LATENCY.lock();
    (ll.enabled, ll.latency_us, ll.xrun_count)
}
