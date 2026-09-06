/// ALSA-Compatible Audio Subsystem
/// Provides a Linux ALSA-compatible audio mixer and PCM interface
///
/// Features:
/// - ALSA PCM device abstraction (playback/capture)
/// - Mixer controls (volume, mute, balance, bass, treble)
/// - Audio routing (source/sink selection)
/// - Sample format conversion
/// - DMA buffer management for audio streaming
/// - Multiple audio card support
/// - Control element interface (ALSA kcontrol equivalent)
/// - ALSA timer interface
/// - Interrupt-driven hw_ptr advancement (DMA simulation)
/// - Period-elapsed callbacks for real-time audio
/// - mmap-capable buffer management
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// DMA BUFFER & STREAMING ENGINE
// ═══════════════════════════════════════════════════════════════════════

/// DMA buffer descriptor — represents a physical DMA-capable buffer region
#[derive(Debug, Clone)]
pub struct DmaBuffer {
    /// Virtual address of the buffer
    pub vaddr: u64,
    /// Physical address (for hardware DMA)
    pub paddr: u64,
    /// Buffer size in bytes
    pub size: usize,
    /// Number of periods in this buffer
    pub periods: u32,
    /// Size of each period in bytes
    pub period_bytes: usize,
    /// Current hardware position (bytes)
    pub hw_pos: usize,
    /// Current application position (bytes)
    pub app_pos: usize,
    /// Whether the buffer is mapped for mmap
    pub mmapped: bool,
}

impl DmaBuffer {
    pub fn new(size: usize, periods: u32) -> Self {
        Self {
            vaddr: 0,
            paddr: 0,
            size,
            periods,
            period_bytes: size / periods as usize,
            hw_pos: 0,
            app_pos: 0,
            mmapped: false,
        }
    }

    /// Get available bytes for writing (playback) or reading (capture)
    pub fn avail_bytes(&self) -> usize {
        if self.app_pos >= self.hw_pos {
            self.size - (self.app_pos - self.hw_pos)
        } else {
            self.hw_pos - self.app_pos
        }
    }

    /// Advance hardware pointer by `bytes` (called from interrupt)
    pub fn advance_hw(&mut self, bytes: usize) {
        self.hw_pos = (self.hw_pos + bytes) % self.size;
    }

    /// Advance application pointer by `bytes`
    pub fn advance_app(&mut self, bytes: usize) {
        self.app_pos = (self.app_pos + bytes) % self.size;
    }

    /// Check if a period boundary has been crossed
    pub fn period_elapsed(&self, old_hw_pos: usize) -> bool {
        let old_period = old_hw_pos / self.period_bytes;
        let new_period = self.hw_pos / self.period_bytes;
        old_period != new_period
    }
}

/// Audio stream state for DMA streaming
#[derive(Debug, Clone)]
pub struct AudioStream {
    /// DMA buffer
    pub dma_buf: DmaBuffer,
    /// Frames played/captured since start
    pub frames_elapsed: u64,
    /// Interrupt count
    pub irq_count: u64,
    /// Underrun/overrun count
    pub xrun_count: u64,
    /// Callback registered for period elapsed
    pub period_callback: bool,
    /// Stream is actively consuming/producing audio
    pub active: bool,
    /// Timestamp of last period interrupt (ticks)
    pub last_irq_tick: u64,
    /// Target IRQ interval in timer ticks
    pub irq_interval_ticks: u64,
}

impl AudioStream {
    pub fn new(buffer_size: usize, periods: u32) -> Self {
        Self {
            dma_buf: DmaBuffer::new(buffer_size, periods),
            frames_elapsed: 0,
            irq_count: 0,
            xrun_count: 0,
            period_callback: false,
            active: false,
            last_irq_tick: 0,
            irq_interval_ticks: 0,
        }
    }
}

/// Global audio stream table (for interrupt-driven advancement)
static AUDIO_STREAMS: Mutex<BTreeMap<(u32, u32, u8), AudioStream>> = Mutex::new(BTreeMap::new());

/// Simulate DMA interrupt — called from timer interrupt handler
/// Advances hw_ptr for all active streams based on sample rate
pub fn dma_tick(current_tick: u64) {
    let mut streams = AUDIO_STREAMS.lock();
    for ((card, dev, stream_dir), stream) in streams.iter_mut() {
        if !stream.active {
            continue;
        }

        // Check if enough time has passed for a period
        if stream.irq_interval_ticks == 0 {
            continue;
        }

        let elapsed = current_tick.wrapping_sub(stream.last_irq_tick);
        if elapsed >= stream.irq_interval_ticks {
            let old_hw_pos = stream.dma_buf.hw_pos;
            let advance = stream.dma_buf.period_bytes;

            stream.dma_buf.advance_hw(advance);
            stream.irq_count += 1;
            stream.last_irq_tick = current_tick;

            // Calculate frames in this period
            // (period_bytes / frame_size) — we approximate with 4 bytes/frame (16-bit stereo)
            let frames_per_period = advance / 4;
            stream.frames_elapsed += frames_per_period as u64;

            // Check for underrun (playback) or overrun (capture)
            if stream.dma_buf.avail_bytes() == 0 {
                stream.xrun_count += 1;
            }
        }
    }
}

/// Register a DMA audio stream
pub fn register_audio_stream(
    card_id: u32,
    device_id: u32,
    direction: u8,
    sample_rate: u32,
    channels: u32,
    period_size: u32,
) {
    let frame_size = 2 * channels as usize; // 16-bit samples
    let period_bytes = period_size as usize * frame_size;
    let buffer_bytes = period_bytes * 4; // 4 periods

    let mut stream = AudioStream::new(buffer_bytes, 4);

    // Calculate IRQ interval: period_size frames / sample_rate = seconds per period
    // At 18.2 Hz timer: ticks_per_period = period_duration * 18.2
    // e.g., 1024 frames @ 44100 Hz = 0.0232s = ~0.42 ticks
    // We'll use a minimum of 1 tick
    let period_duration_us = (period_size as u64 * 1_000_000) / sample_rate as u64;
    let ticks = core::cmp::max(1, period_duration_us / 54945); // 54945 us per PIT tick at 18.2Hz
    stream.irq_interval_ticks = ticks;

    AUDIO_STREAMS
        .lock()
        .insert((card_id, device_id, direction), stream);

    serial_println!(
        "[ALSA-DMA] Stream registered: card={} dev={} dir={} rate={} ch={} period={} irq_interval={}ticks",
        card_id,
        device_id,
        direction,
        sample_rate,
        channels,
        period_size,
        ticks
    );
}

/// Start DMA streaming for a device
pub fn start_dma_stream(card_id: u32, device_id: u32, direction: u8) {
    let mut streams = AUDIO_STREAMS.lock();
    if let Some(stream) = streams.get_mut(&(card_id, device_id, direction)) {
        stream.active = true;
        stream.last_irq_tick = crate::interrupts::get_ticks();
        serial_println!(
            "[ALSA-DMA] Stream started: card={} dev={}",
            card_id,
            device_id
        );
    }
}

/// Stop DMA streaming for a device
pub fn stop_dma_stream(card_id: u32, device_id: u32, direction: u8) {
    let mut streams = AUDIO_STREAMS.lock();
    if let Some(stream) = streams.get_mut(&(card_id, device_id, direction)) {
        stream.active = false;
        serial_println!(
            "[ALSA-DMA] Stream stopped: card={} dev={} frames={} irqs={} xruns={}",
            card_id,
            device_id,
            stream.frames_elapsed,
            stream.irq_count,
            stream.xrun_count
        );
    }
}

/// Get DMA stream status
pub fn dma_stream_status(
    card_id: u32,
    device_id: u32,
    direction: u8,
) -> Option<(u64, u64, u64, usize)> {
    let streams = AUDIO_STREAMS.lock();
    streams.get(&(card_id, device_id, direction)).map(|s| {
        (
            s.frames_elapsed,
            s.irq_count,
            s.xrun_count,
            s.dma_buf.avail_bytes(),
        )
    })
}

// ═══════════════════════════════════════════════════════════════════════
// AUDIO CARD
// ═══════════════════════════════════════════════════════════════════════

/// Unique audio card ID
static NEXT_CARD_ID: AtomicU32 = AtomicU32::new(0);

/// ALSA sound card
#[derive(Debug, Clone)]
pub struct SoundCard {
    pub id: u32,
    pub name: String,
    pub long_name: String,
    pub driver: String,
    pub mixer_name: String,
    pub components: String,
    pub pcm_devices: Vec<PcmDevice>,
    pub controls: Vec<MixerControl>,
}

/// Global sound card registry
static SOUND_CARDS: Mutex<Vec<SoundCard>> = Mutex::new(Vec::new());

/// Register a new sound card
pub fn register_card(name: &str, driver: &str) -> u32 {
    let id = NEXT_CARD_ID.fetch_add(1, Ordering::SeqCst);
    let mut cards = SOUND_CARDS.lock();
    cards.push(SoundCard {
        id,
        name: String::from(name),
        long_name: alloc::format!("{} at 0x0000", name),
        driver: String::from(driver),
        mixer_name: alloc::format!("{} Mixer", name),
        components: String::new(),
        pcm_devices: Vec::new(),
        controls: Vec::new(),
    });
    serial_println!("[ALSA] Registered sound card {}: {}", id, name);
    id
}

/// Get number of registered cards
pub fn card_count() -> usize {
    SOUND_CARDS.lock().len()
}

/// Get card info
pub fn get_card_info(card_id: u32) -> Option<SoundCard> {
    let cards = SOUND_CARDS.lock();
    cards.iter().find(|c| c.id == card_id).cloned()
}

// ═══════════════════════════════════════════════════════════════════════
// PCM DEVICE
// ═══════════════════════════════════════════════════════════════════════

/// PCM stream direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmStream {
    Playback,
    Capture,
}

/// PCM state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmState {
    Open,
    Setup,
    Prepared,
    Running,
    Xrun, // Buffer overrun/underrun
    Draining,
    Paused,
    Suspended,
    Disconnected,
}

/// PCM sample format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    U8,
    S16Le,
    S16Be,
    S24Le,
    S24Be,
    S32Le,
    S32Be,
    F32Le,
    F32Be,
    MuLaw,
    ALaw,
    ImaAdpcm,
    S24Le3, // 3-byte packed 24-bit
}

impl SampleFormat {
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            SampleFormat::U8 | SampleFormat::MuLaw | SampleFormat::ALaw => 1,
            SampleFormat::S16Le | SampleFormat::S16Be | SampleFormat::ImaAdpcm => 2,
            SampleFormat::S24Le3 => 3,
            SampleFormat::S24Le
            | SampleFormat::S24Be
            | SampleFormat::S32Le
            | SampleFormat::S32Be
            | SampleFormat::F32Le
            | SampleFormat::F32Be => 4,
        }
    }

    pub fn is_signed(&self) -> bool {
        !matches!(self, SampleFormat::U8)
    }
}

/// PCM hardware parameters
#[derive(Debug, Clone)]
pub struct PcmHwParams {
    pub format: SampleFormat,
    pub channels: u32,
    pub rate: u32,        // Sample rate in Hz
    pub period_size: u32, // Frames per period
    pub periods: u32,     // Number of periods in buffer
    pub buffer_size: u32, // Total frames in buffer
    pub min_rate: u32,
    pub max_rate: u32,
    pub min_channels: u32,
    pub max_channels: u32,
}

impl PcmHwParams {
    pub fn default_playback() -> Self {
        Self {
            format: SampleFormat::S16Le,
            channels: 2,
            rate: 44100,
            period_size: 1024,
            periods: 4,
            buffer_size: 4096,
            min_rate: 8000,
            max_rate: 192000,
            min_channels: 1,
            max_channels: 8,
        }
    }

    pub fn frame_size(&self) -> usize {
        self.format.bytes_per_sample() * self.channels as usize
    }

    pub fn buffer_bytes(&self) -> usize {
        self.buffer_size as usize * self.frame_size()
    }

    pub fn period_bytes(&self) -> usize {
        self.period_size as usize * self.frame_size()
    }
}

/// PCM software parameters
#[derive(Debug, Clone)]
pub struct PcmSwParams {
    pub start_threshold: u32,
    pub stop_threshold: u32,
    pub avail_min: u32,
    pub silence_threshold: u32,
    pub silence_size: u32,
    pub boundary: u64,
}

impl PcmSwParams {
    pub fn default() -> Self {
        Self {
            start_threshold: 1,
            stop_threshold: 4096,
            avail_min: 1,
            silence_threshold: 0,
            silence_size: 0,
            boundary: 0x7FFFFFFF,
        }
    }
}

/// PCM device
#[derive(Debug, Clone)]
pub struct PcmDevice {
    pub card_id: u32,
    pub device_id: u32,
    pub name: String,
    pub stream: PcmStream,
    pub state: PcmState,
    pub hw_params: PcmHwParams,
    pub sw_params: PcmSwParams,
    pub hw_ptr: u64,     // Hardware pointer (frames)
    pub appl_ptr: u64,   // Application pointer (frames)
    pub buffer: Vec<u8>, // DMA buffer
    pub running: bool,
}

/// Global PCM device table
static PCM_DEVICES: Mutex<BTreeMap<(u32, u32, u8), PcmDevice>> = Mutex::new(BTreeMap::new()); // (card, device, stream)

/// Open a PCM device
pub fn pcm_open(card_id: u32, device_id: u32, stream: PcmStream) -> Result<(), &'static str> {
    let key = (card_id, device_id, stream as u8);
    let mut devices = PCM_DEVICES.lock();

    if devices.contains_key(&key) {
        return Err("Device already open");
    }

    let hw = PcmHwParams::default_playback();
    let buf_size = hw.buffer_bytes();

    devices.insert(
        key,
        PcmDevice {
            card_id,
            device_id,
            name: alloc::format!("hw:{},{}", card_id, device_id),
            stream,
            state: PcmState::Open,
            hw_params: hw,
            sw_params: PcmSwParams::default(),
            hw_ptr: 0,
            appl_ptr: 0,
            buffer: vec![0u8; buf_size],
            running: false,
        },
    );

    serial_println!(
        "[ALSA] Opened PCM hw:{},{} {:?}",
        card_id,
        device_id,
        stream
    );
    Ok(())
}

/// Set hardware parameters
pub fn pcm_set_params(
    card_id: u32,
    device_id: u32,
    stream: PcmStream,
    format: SampleFormat,
    channels: u32,
    rate: u32,
) -> Result<(), &'static str> {
    let key = (card_id, device_id, stream as u8);
    let mut devices = PCM_DEVICES.lock();

    let dev = devices.get_mut(&key).ok_or("Device not open")?;
    dev.hw_params.format = format;
    dev.hw_params.channels = channels;
    dev.hw_params.rate = rate;
    dev.buffer = vec![0u8; dev.hw_params.buffer_bytes()];
    dev.state = PcmState::Setup;

    serial_println!("[ALSA] PCM params: {}ch {}Hz {:?}", channels, rate, format);
    Ok(())
}

/// Prepare PCM for playback/capture
pub fn pcm_prepare(card_id: u32, device_id: u32, stream: PcmStream) -> Result<(), &'static str> {
    let key = (card_id, device_id, stream as u8);
    let mut devices = PCM_DEVICES.lock();

    let dev = devices.get_mut(&key).ok_or("Device not open")?;
    dev.hw_ptr = 0;
    dev.appl_ptr = 0;
    dev.state = PcmState::Prepared;

    serial_println!("[ALSA] PCM prepared");
    Ok(())
}

/// Start PCM stream
pub fn pcm_start(card_id: u32, device_id: u32, stream: PcmStream) -> Result<(), &'static str> {
    let key = (card_id, device_id, stream as u8);
    let mut devices = PCM_DEVICES.lock();

    let dev = devices.get_mut(&key).ok_or("Device not open")?;
    if dev.state != PcmState::Prepared && dev.state != PcmState::Paused {
        return Err("Invalid state for start");
    }
    dev.state = PcmState::Running;
    dev.running = true;

    serial_println!("[ALSA] PCM started");
    Ok(())
}

/// Stop PCM stream
pub fn pcm_stop(card_id: u32, device_id: u32, stream: PcmStream) -> Result<(), &'static str> {
    let key = (card_id, device_id, stream as u8);
    let mut devices = PCM_DEVICES.lock();

    let dev = devices.get_mut(&key).ok_or("Device not open")?;
    dev.state = PcmState::Setup;
    dev.running = false;

    serial_println!("[ALSA] PCM stopped");
    Ok(())
}

/// Write PCM data (playback)
pub fn pcm_write(card_id: u32, device_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    let key = (card_id, device_id, PcmStream::Playback as u8);
    let mut devices = PCM_DEVICES.lock();

    let dev = devices.get_mut(&key).ok_or("Device not open")?;
    if dev.state != PcmState::Running && dev.state != PcmState::Prepared {
        return Err("Device not running");
    }

    let frame_size = dev.hw_params.frame_size();
    let buffer_frames = dev.hw_params.buffer_size as u64;
    let frames_to_write = data.len() / frame_size;

    let mut written = 0;
    for i in 0..frames_to_write {
        let buf_pos = ((dev.appl_ptr % buffer_frames) as usize) * frame_size;
        let src_pos = i * frame_size;

        if src_pos + frame_size <= data.len() && buf_pos + frame_size <= dev.buffer.len() {
            dev.buffer[buf_pos..buf_pos + frame_size]
                .copy_from_slice(&data[src_pos..src_pos + frame_size]);
            dev.appl_ptr += 1;
            written += 1;
        }
    }

    // Auto-start if start_threshold reached
    if dev.state == PcmState::Prepared && dev.appl_ptr >= dev.sw_params.start_threshold as u64 {
        dev.state = PcmState::Running;
        dev.running = true;
    }

    Ok(written * frame_size)
}

/// Read PCM data (capture)
pub fn pcm_read(card_id: u32, device_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let key = (card_id, device_id, PcmStream::Capture as u8);
    let mut devices = PCM_DEVICES.lock();

    let dev = devices.get_mut(&key).ok_or("Device not open")?;
    if dev.state != PcmState::Running {
        return Err("Device not running");
    }

    let frame_size = dev.hw_params.frame_size();
    let buffer_frames = dev.hw_params.buffer_size as u64;
    let frames_to_read = buf.len() / frame_size;

    let mut read_count = 0;
    for i in 0..frames_to_read {
        if dev.appl_ptr >= dev.hw_ptr {
            break; // No data available
        }
        let buf_pos = ((dev.appl_ptr % buffer_frames) as usize) * frame_size;
        let dst_pos = i * frame_size;

        if dst_pos + frame_size <= buf.len() && buf_pos + frame_size <= dev.buffer.len() {
            buf[dst_pos..dst_pos + frame_size]
                .copy_from_slice(&dev.buffer[buf_pos..buf_pos + frame_size]);
            dev.appl_ptr += 1;
            read_count += 1;
        }
    }

    Ok(read_count * frame_size)
}

/// Close PCM device
pub fn pcm_close(card_id: u32, device_id: u32, stream: PcmStream) -> Result<(), &'static str> {
    let key = (card_id, device_id, stream as u8);
    let mut devices = PCM_DEVICES.lock();
    devices.remove(&key);
    serial_println!("[ALSA] PCM closed");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// MIXER CONTROLS
// ═══════════════════════════════════════════════════════════════════════

/// Mixer control type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlType {
    Boolean,
    Integer,
    Integer64,
    Enumerated,
    Bytes,
    Iec958,
}

/// Mixer control interface
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlInterface {
    Card,
    Hwdep,
    Mixer,
    Pcm,
    Rawmidi,
    Timer,
    Sequencer,
}

/// Mixer control element
#[derive(Debug, Clone)]
pub struct MixerControl {
    pub id: u32,
    pub name: String,
    pub iface: ControlInterface,
    pub control_type: ControlType,
    pub access: u32, // SNDRV_CTL_ELEM_ACCESS_*
    pub count: u32,  // Number of values
    pub min: i64,
    pub max: i64,
    pub step: i64,
    pub values: Vec<i64>,
    pub enum_items: Vec<String>,
}

/// Control access flags
pub const CTL_ELEM_ACCESS_READ: u32 = 1 << 0;
pub const CTL_ELEM_ACCESS_WRITE: u32 = 1 << 1;
pub const CTL_ELEM_ACCESS_VOLATILE: u32 = 1 << 2;
pub const CTL_ELEM_ACCESS_TLV_READ: u32 = 1 << 4;
pub const CTL_ELEM_ACCESS_READWRITE: u32 = CTL_ELEM_ACCESS_READ | CTL_ELEM_ACCESS_WRITE;

/// Global mixer control registry
static MIXER_CONTROLS: Mutex<BTreeMap<(u32, u32), MixerControl>> = Mutex::new(BTreeMap::new()); // (card_id, ctl_id)

/// Register a mixer control
pub fn register_control(
    card_id: u32,
    name: &str,
    control_type: ControlType,
    min: i64,
    max: i64,
    initial: i64,
) -> u32 {
    static NEXT_CTL_ID: AtomicU32 = AtomicU32::new(0);
    let id = NEXT_CTL_ID.fetch_add(1, Ordering::SeqCst);

    let mut controls = MIXER_CONTROLS.lock();
    controls.insert(
        (card_id, id),
        MixerControl {
            id,
            name: String::from(name),
            iface: ControlInterface::Mixer,
            control_type,
            access: CTL_ELEM_ACCESS_READWRITE,
            count: 1,
            min,
            max,
            step: 1,
            values: vec![initial],
            enum_items: Vec::new(),
        },
    );

    serial_println!("[ALSA] Registered control '{}' on card {}", name, card_id);
    id
}

/// Register a stereo volume control
pub fn register_volume_control(card_id: u32, name: &str, max_db: i64) -> u32 {
    static NEXT_CTL_ID2: AtomicU32 = AtomicU32::new(100);
    let id = NEXT_CTL_ID2.fetch_add(1, Ordering::SeqCst);

    let initial = max_db * 75 / 100; // 75% default volume
    let mut controls = MIXER_CONTROLS.lock();
    controls.insert(
        (card_id, id),
        MixerControl {
            id,
            name: String::from(name),
            iface: ControlInterface::Mixer,
            control_type: ControlType::Integer,
            access: CTL_ELEM_ACCESS_READWRITE | CTL_ELEM_ACCESS_TLV_READ,
            count: 2, // Stereo
            min: 0,
            max: max_db,
            step: 1,
            values: vec![initial, initial], // Left + Right
            enum_items: Vec::new(),
        },
    );

    id
}

/// Register an enumerated control (e.g., input source selector)
pub fn register_enum_control(card_id: u32, name: &str, items: &[&str], initial: usize) -> u32 {
    static NEXT_CTL_ID3: AtomicU32 = AtomicU32::new(200);
    let id = NEXT_CTL_ID3.fetch_add(1, Ordering::SeqCst);

    let mut controls = MIXER_CONTROLS.lock();
    controls.insert(
        (card_id, id),
        MixerControl {
            id,
            name: String::from(name),
            iface: ControlInterface::Mixer,
            control_type: ControlType::Enumerated,
            access: CTL_ELEM_ACCESS_READWRITE,
            count: 1,
            min: 0,
            max: items.len() as i64 - 1,
            step: 1,
            values: vec![initial as i64],
            enum_items: items.iter().map(|s| String::from(*s)).collect(),
        },
    );

    id
}

/// Get control value
pub fn control_get(card_id: u32, ctl_id: u32) -> Option<Vec<i64>> {
    let controls = MIXER_CONTROLS.lock();
    controls.get(&(card_id, ctl_id)).map(|c| c.values.clone())
}

/// Set control value
pub fn control_set(card_id: u32, ctl_id: u32, values: &[i64]) -> Result<(), &'static str> {
    let mut controls = MIXER_CONTROLS.lock();
    let ctl = controls
        .get_mut(&(card_id, ctl_id))
        .ok_or("Control not found")?;

    if ctl.access & CTL_ELEM_ACCESS_WRITE == 0 {
        return Err("Control is read-only");
    }

    for (i, &val) in values.iter().enumerate() {
        if i >= ctl.values.len() {
            break;
        }
        let clamped = val.clamp(ctl.min, ctl.max);
        ctl.values[i] = clamped;
    }

    serial_println!("[ALSA] Control '{}' set to {:?}", ctl.name, ctl.values);
    Ok(())
}

/// List all controls for a card
pub fn list_controls(card_id: u32) -> Vec<MixerControl> {
    let controls = MIXER_CONTROLS.lock();
    controls
        .iter()
        .filter(|((cid, _), _)| *cid == card_id)
        .map(|(_, c)| c.clone())
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// ALSA TIMER
// ═══════════════════════════════════════════════════════════════════════

/// ALSA timer type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerClass {
    None,
    Slave,
    Global,
    Card,
    Pcm,
}

/// ALSA timer instance
#[derive(Debug, Clone)]
pub struct AlsaTimer {
    pub id: u32,
    pub name: String,
    pub class: TimerClass,
    pub card: i32,
    pub device: i32,
    pub subdevice: i32,
    pub resolution_ns: u64,
    pub ticks: u64,
}

static ALSA_TIMERS: Mutex<Vec<AlsaTimer>> = Mutex::new(Vec::new());

/// Register a timer
pub fn register_timer(name: &str, class: TimerClass, resolution_ns: u64) -> u32 {
    static NEXT_TIMER_ID: AtomicU32 = AtomicU32::new(0);
    let id = NEXT_TIMER_ID.fetch_add(1, Ordering::SeqCst);
    let mut timers = ALSA_TIMERS.lock();
    timers.push(AlsaTimer {
        id,
        name: String::from(name),
        class,
        card: -1,
        device: -1,
        subdevice: -1,
        resolution_ns,
        ticks: 0,
    });
    id
}

// ═══════════════════════════════════════════════════════════════════════
// SAMPLE FORMAT CONVERSION
// ═══════════════════════════════════════════════════════════════════════

/// Convert samples between formats
pub fn convert_samples(src: &[u8], src_fmt: SampleFormat, dst_fmt: SampleFormat) -> Vec<u8> {
    if src_fmt == dst_fmt {
        return src.to_vec();
    }

    let src_bps = src_fmt.bytes_per_sample();
    let dst_bps = dst_fmt.bytes_per_sample();
    let num_samples = src.len() / src_bps;
    let mut dst = vec![0u8; num_samples * dst_bps];

    for i in 0..num_samples {
        // Convert to f32 intermediate
        let value: f32 = match src_fmt {
            SampleFormat::U8 => (src[i] as f32 - 128.0) / 128.0,
            SampleFormat::S16Le => {
                let s = i16::from_le_bytes([src[i * 2], src[i * 2 + 1]]);
                s as f32 / 32768.0
            }
            SampleFormat::S32Le => {
                let s = i32::from_le_bytes([
                    src[i * 4],
                    src[i * 4 + 1],
                    src[i * 4 + 2],
                    src[i * 4 + 3],
                ]);
                s as f32 / 2147483648.0
            }
            _ => 0.0,
        };

        // Convert from f32 intermediate
        match dst_fmt {
            SampleFormat::U8 => {
                dst[i] = ((value * 128.0) + 128.0).clamp(0.0, 255.0) as u8;
            }
            SampleFormat::S16Le => {
                let s = (value * 32767.0).clamp(-32768.0, 32767.0) as i16;
                let bytes = s.to_le_bytes();
                dst[i * 2] = bytes[0];
                dst[i * 2 + 1] = bytes[1];
            }
            SampleFormat::S32Le => {
                let s = (value * 2147483647.0).clamp(-2147483648.0, 2147483647.0) as i32;
                let bytes = s.to_le_bytes();
                dst[i * 4] = bytes[0];
                dst[i * 4 + 1] = bytes[1];
                dst[i * 4 + 2] = bytes[2];
                dst[i * 4 + 3] = bytes[3];
            }
            _ => {}
        }
    }

    dst
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize ALSA subsystem with default card and controls
pub fn init() {
    serial_println!("[ALSA] Initializing ALSA-compatible audio subsystem");

    // Register default HDA sound card
    let card_id = register_card("HDA Intel PCH", "snd_hda_intel");

    // Register standard mixer controls
    register_volume_control(card_id, "Master Playback Volume", 100);
    register_control(
        card_id,
        "Master Playback Switch",
        ControlType::Boolean,
        0,
        1,
        1,
    );
    register_volume_control(card_id, "PCM Playback Volume", 100);
    register_control(
        card_id,
        "PCM Playback Switch",
        ControlType::Boolean,
        0,
        1,
        1,
    );
    register_volume_control(card_id, "Headphone Playback Volume", 100);
    register_control(
        card_id,
        "Headphone Playback Switch",
        ControlType::Boolean,
        0,
        1,
        1,
    );
    register_volume_control(card_id, "Speaker Playback Volume", 100);
    register_control(
        card_id,
        "Speaker Playback Switch",
        ControlType::Boolean,
        0,
        1,
        1,
    );
    register_volume_control(card_id, "Capture Volume", 100);
    register_control(card_id, "Capture Switch", ControlType::Boolean, 0, 1, 1);
    register_volume_control(card_id, "Mic Boost Volume", 30);
    register_enum_control(
        card_id,
        "Input Source",
        &["Mic", "Front Mic", "Line", "CD"],
        0,
    );
    register_enum_control(card_id, "Auto-Mute Mode", &["Disabled", "Enabled"], 1);
    register_control(card_id, "Loopback Mixing", ControlType::Boolean, 0, 1, 0);

    // Register system timer
    register_timer("system", TimerClass::Global, 1_000_000); // 1ms resolution

    serial_println!(
        "[ALSA] Audio subsystem initialized ({} controls)",
        list_controls(card_id).len()
    );
}

/// Generate /proc/asound/cards output
pub fn proc_asound_cards() -> String {
    let cards = SOUND_CARDS.lock();
    let mut output = String::new();
    for card in cards.iter() {
        output.push_str(&alloc::format!(
            " {}: {} - {}\n",
            card.id,
            card.name,
            card.long_name
        ));
        output.push_str(&alloc::format!("                      {}\n", card.driver));
    }
    output
}

/// Generate /proc/asound/devices output
pub fn proc_asound_devices() -> String {
    let cards = SOUND_CARDS.lock();
    let mut output = String::new();
    for card in cards.iter() {
        output.push_str(&alloc::format!("  {}:  : control\n", card.id));
        output.push_str(&alloc::format!("  {}-0: digital audio playback\n", card.id));
        output.push_str(&alloc::format!("  {}-0: digital audio capture\n", card.id));
        output.push_str(&alloc::format!("  {}-0: hardware dependent\n", card.id));
    }
    output
}

// ═══════════════════════════════════════════════════════════════════════
// AUDIO MIXING ENGINE
// ═══════════════════════════════════════════════════════════════════════

/// Mix multiple PCM audio streams into a single output buffer.
///
/// All input streams must be signed 16-bit PCM, same sample rate and
/// channel count. Mixing is additive with clamping to prevent overflow.
///
/// * `streams` — Slice of PCM data buffers (each &\[i16\] is one source)
/// * `output`  — Destination buffer (must be at least as long as shortest input)
/// * `volumes` — Per-stream volume scaling in 0..256 (256 = unity gain)
pub fn mix_pcm_s16(streams: &[&[i16]], output: &mut [i16], volumes: &[u16]) {
    let frames = streams
        .iter()
        .map(|s| s.len())
        .min()
        .unwrap_or(0)
        .min(output.len());

    for i in 0..frames {
        let mut accum: i32 = 0;
        for (idx, stream) in streams.iter().enumerate() {
            let vol = *volumes.get(idx).unwrap_or(&256) as i32;
            accum += (stream[i] as i32 * vol) >> 8; // scale by volume
        }
        // Clamp to i16 range
        output[i] = accum.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }
}

/// Software audio mixer — collects PCM data from multiple producers and
/// renders a single mixed output for the hardware sink.
pub struct AudioMixer {
    /// Active playback streams (indexed by stream id)
    pub streams: BTreeMap<u32, MixerStream>,
    /// Next stream ID
    next_id: u32,
    /// Master volume (0..256)
    pub master_volume: u16,
    /// Output sample rate
    pub sample_rate: u32,
    /// Output channels
    pub channels: u16,
}

/// A single stream feeding into the mixer
pub struct MixerStream {
    pub id: u32,
    /// Ring buffer of i16 PCM samples
    pub buffer: Vec<i16>,
    /// Read position in buffer
    pub read_pos: usize,
    /// Write position in buffer
    pub write_pos: usize,
    /// Per-stream volume (0..256)
    pub volume: u16,
    /// Whether this stream is actively playing
    pub active: bool,
}

impl AudioMixer {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            streams: BTreeMap::new(),
            next_id: 1,
            master_volume: 256,
            sample_rate,
            channels,
        }
    }

    /// Open a new playback stream, returning its id
    pub fn open_stream(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.streams.insert(
            id,
            MixerStream {
                id,
                buffer: vec![0i16; self.sample_rate as usize * self.channels as usize],
                read_pos: 0,
                write_pos: 0,
                volume: 256,
                active: true,
            },
        );
        id
    }

    /// Write PCM samples into a stream's ring buffer
    pub fn write_stream(&mut self, id: u32, data: &[i16]) -> usize {
        if let Some(stream) = self.streams.get_mut(&id) {
            let cap = stream.buffer.len();
            let mut written = 0;
            for &sample in data {
                stream.buffer[stream.write_pos % cap] = sample;
                stream.write_pos += 1;
                written += 1;
            }
            written
        } else {
            0
        }
    }

    /// Close and remove a stream
    pub fn close_stream(&mut self, id: u32) {
        self.streams.remove(&id);
    }

    /// Render mixed output: read from all active streams, mix, apply master volume
    pub fn render(&mut self, output: &mut [i16]) {
        // Zero output first
        for sample in output.iter_mut() {
            *sample = 0;
        }

        for stream in self.streams.values_mut() {
            if !stream.active {
                continue;
            }
            let cap = stream.buffer.len();
            for out_sample in output.iter_mut() {
                let s = stream.buffer[stream.read_pos % cap] as i32;
                let vol = stream.volume as i32;
                *out_sample = ((*out_sample as i32) + ((s * vol) >> 8))
                    .clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                stream.read_pos += 1;
            }
        }

        // Apply master volume
        let mv = self.master_volume as i32;
        for sample in output.iter_mut() {
            *sample = ((*sample as i32 * mv) >> 8).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        }
    }
}

lazy_static::lazy_static! {
    /// Global software audio mixer
    pub static ref AUDIO_MIXER: Mutex<AudioMixer> = Mutex::new(AudioMixer::new(48000, 2));
}
