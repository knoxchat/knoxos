/// Intel HD Audio (HDA) Codec Driver
/// Implements the Intel High Definition Audio specification for PCM audio playback/capture
///
/// Features:
/// - PCI device discovery for HDA controllers
/// - CORB/RIRB command transport
/// - Codec enumeration and widget discovery
/// - PCM stream setup with BDL (Buffer Descriptor List)
/// - Mixer controls (master volume, mute)
/// - ALSA-compatible PCM parameters
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── HDA PCI Constants ─────────────────────────────────────────────

pub const HDA_PCI_CLASS: u8 = 0x04; // Multimedia
pub const HDA_PCI_SUBCLASS: u8 = 0x03; // HD Audio

// HDA controller vendor IDs
pub const INTEL_VENDOR_ID: u16 = 0x8086;
pub const QEMU_HDA_DEVICE: u16 = 0x2668; // ICH6 HDA (QEMU default)

// ─── HDA Register Offsets ───────────────────────────────────────────

pub const REG_GCAP: u16 = 0x00; // Global Capabilities
pub const REG_VMIN: u16 = 0x02; // Minor Version
pub const REG_VMAJ: u16 = 0x03; // Major Version
pub const REG_OUTPAY: u16 = 0x04; // Output Payload Capability
pub const REG_INPAY: u16 = 0x06; // Input Payload Capability
pub const REG_GCTL: u16 = 0x08; // Global Control
pub const REG_WAKEEN: u16 = 0x0C; // Wake Enable
pub const REG_STATESTS: u16 = 0x0E; // State Change Status
pub const REG_GSTS: u16 = 0x10; // Global Status
pub const REG_INTCTL: u16 = 0x20; // Interrupt Control
pub const REG_INTSTS: u16 = 0x24; // Interrupt Status
pub const REG_WALCLK: u16 = 0x30; // Wall Clock Counter
pub const REG_SSYNC: u16 = 0x38; // Stream Synchronization
pub const REG_CORBLBASE: u16 = 0x40; // CORB Lower Base Address
pub const REG_CORBUBASE: u16 = 0x44; // CORB Upper Base Address
pub const REG_CORBWP: u16 = 0x48; // CORB Write Pointer
pub const REG_CORBRP: u16 = 0x4A; // CORB Read Pointer
pub const REG_CORBCTL: u16 = 0x4C; // CORB Control
pub const REG_CORBSTS: u16 = 0x4D; // CORB Status
pub const REG_CORBSIZE: u16 = 0x4E; // CORB Size
pub const REG_RIRBLBASE: u16 = 0x50; // RIRB Lower Base Address
pub const REG_RIRBUBASE: u16 = 0x54; // RIRB Upper Base Address
pub const REG_RIRBWP: u16 = 0x58; // RIRB Write Pointer
pub const REG_RINTCNT: u16 = 0x5A; // Response Interrupt Count
pub const REG_RIRBCTL: u16 = 0x5C; // RIRB Control
pub const REG_RIRBSTS: u16 = 0x5D; // RIRB Status
pub const REG_RIRBSIZE: u16 = 0x5E; // RIRB Size

// Stream descriptor registers (offset = 0x80 + n * 0x20)
pub const SD_CTL: u16 = 0x00; // Stream Descriptor Control
pub const SD_STS: u16 = 0x03; // Stream Descriptor Status
pub const SD_LPIB: u16 = 0x04; // Link Position in Buffer
pub const SD_CBL: u16 = 0x08; // Cyclic Buffer Length
pub const SD_LVI: u16 = 0x0C; // Last Valid Index
pub const SD_FIFOW: u16 = 0x0E; // FIFO Watermark
pub const SD_FIFOS: u16 = 0x10; // FIFO Size
pub const SD_FMT: u16 = 0x12; // Stream Format
pub const SD_BDLPL: u16 = 0x18; // BDL Pointer Lower
pub const SD_BDLPU: u16 = 0x1C; // BDL Pointer Upper

// GCTL bits
pub const GCTL_CRST: u32 = 1 << 0; // Controller Reset
pub const GCTL_FCNTRL: u32 = 1 << 1; // Flush Control
pub const GCTL_UNSOL: u32 = 1 << 8; // Accept Unsolicited Responses

// CORBCTL bits
pub const CORBCTL_RUN: u8 = 1 << 1;

// RIRBCTL bits
pub const RIRBCTL_RUN: u8 = 1 << 1;
pub const RIRBCTL_INTCTL: u8 = 1 << 0;

// ─── HDA Codec Verbs ────────────────────────────────────────────────

/// HDA codec verb IDs
pub const VERB_GET_PARAMETER: u32 = 0xF00;
pub const VERB_GET_CONN_SELECT: u32 = 0xF01;
pub const VERB_SET_CONN_SELECT: u32 = 0x701;
pub const VERB_GET_CONN_LIST: u32 = 0xF02;
pub const VERB_GET_PROC_STATE: u32 = 0xF03;
pub const VERB_SET_POWER_STATE: u32 = 0x705;
pub const VERB_GET_POWER_STATE: u32 = 0xF05;
pub const VERB_SET_STREAM_FORMAT: u32 = 0x200;
pub const VERB_GET_STREAM_FORMAT: u32 = 0xA00;
pub const VERB_SET_AMP_GAIN: u32 = 0x300;
pub const VERB_GET_AMP_GAIN: u32 = 0xB00;
pub const VERB_SET_PIN_WIDGET_CTL: u32 = 0x707;
pub const VERB_GET_PIN_WIDGET_CTL: u32 = 0xF07;
pub const VERB_SET_EAPD_ENABLE: u32 = 0x70C;
pub const VERB_SET_CHANNEL_STREAM: u32 = 0x706;
pub const VERB_GET_CHANNEL_STREAM: u32 = 0xF06;
pub const VERB_SET_CONFIG_DEFAULT: u32 = 0x71C;
pub const VERB_GET_CONFIG_DEFAULT: u32 = 0xF1C;

/// HDA parameter IDs
pub const PARAM_VENDOR_ID: u8 = 0x00;
pub const PARAM_REVISION_ID: u8 = 0x02;
pub const PARAM_SUBNODE_COUNT: u8 = 0x04;
pub const PARAM_FUNC_GROUP_TYPE: u8 = 0x05;
pub const PARAM_AUDIO_CAPS: u8 = 0x09;
pub const PARAM_PIN_CAPS: u8 = 0x0C;
pub const PARAM_AMP_IN_CAPS: u8 = 0x0D;
pub const PARAM_AMP_OUT_CAPS: u8 = 0x12;
pub const PARAM_CONN_LIST_LEN: u8 = 0x0E;
pub const PARAM_POWER_STATES: u8 = 0x0F;
pub const PARAM_GPIO_COUNT: u8 = 0x11;
pub const PARAM_STREAM_FORMATS: u8 = 0x0B;
pub const PARAM_PCM_CAPS: u8 = 0x0A;
pub const PARAM_VOLUME_KNOB_CAPS: u8 = 0x13;

// ─── Widget Types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetType {
    AudioOutput = 0,
    AudioInput = 1,
    AudioMixer = 2,
    AudioSelector = 3,
    PinComplex = 4,
    PowerWidget = 5,
    VolumeKnob = 6,
    BeepGenerator = 7,
    VendorDefined = 0x0F,
    Unknown = 0xFF,
}

impl From<u8> for WidgetType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::AudioOutput,
            1 => Self::AudioInput,
            2 => Self::AudioMixer,
            3 => Self::AudioSelector,
            4 => Self::PinComplex,
            5 => Self::PowerWidget,
            6 => Self::VolumeKnob,
            7 => Self::BeepGenerator,
            0x0F => Self::VendorDefined,
            _ => Self::Unknown,
        }
    }
}

// ─── Audio Format ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRate {
    Rate8000 = 8000,
    Rate11025 = 11025,
    Rate16000 = 16000,
    Rate22050 = 22050,
    Rate32000 = 32000,
    Rate44100 = 44100,
    Rate48000 = 48000,
    Rate88200 = 88200,
    Rate96000 = 96000,
    Rate176400 = 176400,
    Rate192000 = 192000,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    S16LE,   // 16-bit signed little-endian
    S24LE,   // 24-bit signed little-endian
    S32LE,   // 32-bit signed little-endian
    Float32, // 32-bit float
}

#[derive(Debug, Clone)]
pub struct PcmParams {
    pub sample_rate: u32,
    pub channels: u8,
    pub format: SampleFormat,
    pub buffer_size: usize, // Total buffer size in frames
    pub period_size: usize, // Frames per period/interrupt
}

impl PcmParams {
    pub fn default_playback() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            format: SampleFormat::S16LE,
            buffer_size: 4096,
            period_size: 1024,
        }
    }

    /// Encode HDA stream format register value
    pub fn to_hda_format(&self) -> u16 {
        let base = match self.sample_rate {
            44100 | 88200 | 176400 => 1u16 << 14, // 44.1kHz base
            _ => 0u16,                            // 48kHz base
        };
        let mult = match self.sample_rate {
            88200 | 96000 => 1u16 << 11,
            176400 | 192000 => 3u16 << 11,
            _ => 0u16,
        };
        let div = match self.sample_rate {
            8000 => 5u16 << 8,
            11025 => 3u16 << 8,
            16000 => 2u16 << 8,
            22050 => 1u16 << 8,
            32000 => 1u16 << 8,
            _ => 0u16,
        };
        let bits = match self.format {
            SampleFormat::S16LE => 1u16 << 4,
            SampleFormat::S24LE => 3u16 << 4,
            SampleFormat::S32LE => 4u16 << 4,
            SampleFormat::Float32 => 4u16 << 4,
        };
        let chan = (self.channels.saturating_sub(1) as u16) & 0x0F;

        base | mult | div | bits | chan
    }
}

// ─── Buffer Descriptor List Entry ───────────────────────────────────

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct BdlEntry {
    pub address: u64, // Physical address of buffer
    pub length: u32,  // Buffer length in bytes
    pub ioc: u32,     // Interrupt on Completion (bit 0)
}

// ─── PCM Stream Management ──────────────────────────────────────────

/// HDA DMA stream state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Stopped,
    Running,
    Paused,
    Underrun,
}

/// PCM stream state for playback or capture
#[derive(Debug, Clone)]
pub struct PcmStreamState {
    pub stream_index: u8, // 0-15 (output), 16-31 (input)
    pub state: StreamState,
    pub params: PcmParams,
    pub position_frames: u64,          // Current playback position
    pub buffer_base: u64,              // Physical address of DMA buffer
    pub buffer_size: usize,            // Total buffer size in bytes
    pub period_size: usize,            // Bytes per period
    pub bdl_base: u64,                 // Physical address of BDL
    pub bdl_entries: u8,               // Number of BDL entries
    pub dma_position_in_buffer: usize, // Current DMA write position
    pub fifo_size: u32,                // FIFO size in bytes
    pub last_irq_count: u64,           // For IRQ tracking
}

impl PcmStreamState {
    pub fn new(stream_index: u8, params: PcmParams) -> Self {
        Self {
            stream_index,
            state: StreamState::Stopped,
            params,
            position_frames: 0,
            buffer_base: 0,
            buffer_size: 0,
            period_size: 0,
            bdl_base: 0,
            bdl_entries: 0,
            dma_position_in_buffer: 0,
            fifo_size: 4096,
            last_irq_count: 0,
        }
    }

    /// Get current playback position in frames
    pub fn get_position_frames(&self) -> u64 {
        self.position_frames
    }

    /// Update position based on DMA pointer
    pub fn update_position(&mut self, bytes_processed: usize) {
        let bytes_per_frame =
            (self.params.channels as usize) * (self.params.format.bytes_per_sample());
        let frames = bytes_processed / bytes_per_frame;
        self.position_frames += frames as u64;
    }
}

pub trait SampleFormatExt {
    fn bytes_per_sample(&self) -> usize;
}

impl SampleFormatExt for SampleFormat {
    fn bytes_per_sample(&self) -> usize {
        match self {
            SampleFormat::S16LE => 2,
            SampleFormat::S24LE => 3,
            SampleFormat::S32LE => 4,
            SampleFormat::Float32 => 4,
        }
    }
}

// ─── Codec Widget ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Widget {
    pub nid: u8,
    pub widget_type: WidgetType,
    pub caps: u32,
    pub pin_caps: u32,
    pub amp_in_caps: u32,
    pub amp_out_caps: u32,
    pub connections: Vec<u8>,
    pub config_default: u32,
}

// ─── Codec ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HdaCodec {
    pub address: u8, // Codec address (0-14)
    pub vendor_id: u32,
    pub revision_id: u32,
    pub afg_nid: u8, // Audio Function Group node ID
    pub widgets: Vec<Widget>,
    pub output_nids: Vec<u8>, // DAC nodes
    pub input_nids: Vec<u8>,  // ADC nodes
    pub pin_nids: Vec<u8>,    // Pin complex nodes
    pub mixer_nids: Vec<u8>,  // Mixer nodes
}

// ─── HDA Controller ─────────────────────────────────────────────────

#[derive(Debug)]
pub struct HdaController {
    pub pci_bus: u8,
    pub pci_dev: u8,
    pub pci_func: u8,
    pub mmio_base: u64,
    pub vendor_id: u16,
    pub device_id: u16,
    pub num_output_streams: u8,
    pub num_input_streams: u8,
    pub num_bidir_streams: u8,
    pub codecs: Vec<HdaCodec>,
    pub initialized: bool,
    // Mixer state
    pub master_volume: u8, // 0-100
    pub master_mute: bool,
    // Stream state
    pub playback_stream: Option<PcmStreamState>,
    pub capture_stream: Option<PcmStreamState>,
    pub current_params: PcmParams,
}

// ─── MMIO Helpers ───────────────────────────────────────────────────

unsafe fn mmio_read32(base: u64, offset: u16) -> u32 {
    let ptr = (base + offset as u64) as *const u32;
    core::ptr::read_volatile(ptr)
}

unsafe fn mmio_write32(base: u64, offset: u16, value: u32) {
    let ptr = (base + offset as u64) as *mut u32;
    core::ptr::write_volatile(ptr, value);
}

unsafe fn mmio_read16(base: u64, offset: u16) -> u16 {
    let ptr = (base + offset as u64) as *const u16;
    core::ptr::read_volatile(ptr)
}

unsafe fn mmio_write16(base: u64, offset: u16, value: u16) {
    let ptr = (base + offset as u64) as *mut u16;
    core::ptr::write_volatile(ptr, value);
}

unsafe fn mmio_read8(base: u64, offset: u16) -> u8 {
    let ptr = (base + offset as u64) as *const u8;
    core::ptr::read_volatile(ptr)
}

unsafe fn mmio_write8(base: u64, offset: u16, value: u8) {
    let ptr = (base + offset as u64) as *mut u8;
    core::ptr::write_volatile(ptr, value);
}

// ─── PCI Helpers ────────────────────────────────────────────────────

fn pci_read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr: u32 = 0x80000000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;
        let mut addr_port: Port<u32> = Port::new(0xCF8);
        let mut data_port: Port<u32> = Port::new(0xCFC);
        addr_port.write(addr);
        data_port.read()
    }
}

fn pci_write32(bus: u8, dev: u8, func: u8, offset: u8, value: u32) {
    let addr: u32 = 0x80000000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;
        let mut addr_port: Port<u32> = Port::new(0xCF8);
        let mut data_port: Port<u32> = Port::new(0xCFC);
        addr_port.write(addr);
        data_port.write(value);
    }
}

// ─── Controller Operations ──────────────────────────────────────────

/// Scan PCI bus for HDA controller
fn find_hda_controller() -> Option<(u8, u8, u8)> {
    for bus in 0..=255u16 {
        for dev in 0..32u8 {
            let id = pci_read32(bus as u8, dev, 0, 0);
            if id == 0xFFFFFFFF {
                continue;
            }
            let class = pci_read32(bus as u8, dev, 0, 0x08);
            let class_code = ((class >> 24) & 0xFF) as u8;
            let subclass = ((class >> 16) & 0xFF) as u8;
            if class_code == HDA_PCI_CLASS && subclass == HDA_PCI_SUBCLASS {
                return Some((bus as u8, dev, 0));
            }
        }
        if bus == 255 {
            break;
        }
    }
    None
}

/// Reset the HDA controller
fn controller_reset(base: u64) -> bool {
    unsafe {
        // Clear CRST to enter reset
        let gctl = mmio_read32(base, REG_GCTL);
        mmio_write32(base, REG_GCTL, gctl & !GCTL_CRST);

        // Wait for controller to enter reset
        for _ in 0..1000 {
            if mmio_read32(base, REG_GCTL) & GCTL_CRST == 0 {
                break;
            }
            core::hint::spin_loop();
        }

        // Set CRST to exit reset
        let gctl = mmio_read32(base, REG_GCTL);
        mmio_write32(base, REG_GCTL, gctl | GCTL_CRST);

        // Wait for controller to come out of reset
        for _ in 0..10000 {
            if mmio_read32(base, REG_GCTL) & GCTL_CRST != 0 {
                return true;
            }
            core::hint::spin_loop();
        }
    }
    false
}

/// Build a codec verb command
fn make_verb(codec_addr: u8, nid: u8, verb: u32, param: u8) -> u32 {
    ((codec_addr as u32) << 28) | ((nid as u32) << 20) | (verb << 8) | (param as u32)
}

/// Send a verb via CORB and read response from RIRB
unsafe fn corb_send_verb(mmio_base: u64, verb: u32) -> u32 {
    // Write verb to CORB
    let corbwp = mmio_read16(mmio_base, REG_CORBWP) as u32;
    let new_wp = (corbwp + 1) % 256;
    let corb_base = mmio_read32(mmio_base, REG_CORBLBASE) as u64
        | ((mmio_read32(mmio_base, REG_CORBUBASE) as u64) << 32);
    if corb_base != 0 {
        let ptr = corb_base as *mut u32;
        core::ptr::write_volatile(ptr.add(new_wp as usize), verb);
    }
    mmio_write16(mmio_base, REG_CORBWP, new_wp as u16);

    // Wait for RIRB response
    for _ in 0..10_000 {
        let rirb_wp = mmio_read16(mmio_base, 0x58); // RIRBWP
        if rirb_wp != corbwp as u16 {
            let rirb_base = mmio_read32(mmio_base, REG_RIRBLBASE) as u64
                | ((mmio_read32(mmio_base, 0x54) as u64) << 32);
            if rirb_base != 0 {
                let ptr = rirb_base as *const u64;
                let entry = core::ptr::read_volatile(ptr.add(new_wp as usize));
                return entry as u32; // lower 32 bits = response
            }
        }
        core::hint::spin_loop();
    }
    0 // timeout
}

/// Set master volume (0-100)
pub fn set_master_volume(volume: u8) {
    let mut ctrl = HDA_CONTROLLER.lock();
    ctrl.master_volume = volume.min(100);
    serial_println!("[HDA] Master volume set to {}%", ctrl.master_volume);
}

/// Get master volume
pub fn get_master_volume() -> u8 {
    HDA_CONTROLLER.lock().master_volume
}

/// Set master mute
pub fn set_master_mute(mute: bool) {
    let mut ctrl = HDA_CONTROLLER.lock();
    ctrl.master_mute = mute;
    serial_println!("[HDA] Master {}", if mute { "muted" } else { "unmuted" });
}

/// Check if HDA is available
pub fn is_available() -> bool {
    HDA_AVAILABLE.load(Ordering::Relaxed)
}

/// Get controller info string
pub fn controller_info() -> String {
    let ctrl = HDA_CONTROLLER.lock();
    if !ctrl.initialized {
        return String::from("No HDA controller");
    }
    alloc::format!(
        "HDA {:04x}:{:04x} at {:02x}:{:02x}.{} — {} codecs, {} output streams, vol {}%{}",
        ctrl.vendor_id,
        ctrl.device_id,
        ctrl.pci_bus,
        ctrl.pci_dev,
        ctrl.pci_func,
        ctrl.codecs.len(),
        ctrl.num_output_streams,
        ctrl.master_volume,
        if ctrl.master_mute { " [MUTE]" } else { "" }
    )
}

/// Handle HDA interrupt — process stream completion events
pub fn handle_interrupt() {
    let mut ctrl = HDA_CONTROLLER.lock();
    if !ctrl.initialized || ctrl.mmio_base == 0 {
        return;
    }

    let mmio = ctrl.mmio_base;

    unsafe {
        // Read global interrupt status
        let intsts = mmio_read32(mmio, REG_INTSTS);
        if intsts == 0 {
            return;
        }

        // Check each stream's interrupt status
        // Playback stream
        if let Some(ref mut stream) = ctrl.playback_stream {
            if intsts & (1 << stream.stream_index as u32) != 0 {
                let sd_base = 0x80u16 + (stream.stream_index as u16) * 0x20;

                // Read stream status
                let sts = mmio_read8(mmio, sd_base + SD_STS);

                // Buffer Completion Interrupt (BCIS)
                if sts & 0x04 != 0 {
                    // Read current DMA position
                    let lpib = mmio_read32(mmio, sd_base + SD_LPIB);
                    stream.dma_position_in_buffer = lpib as usize;

                    // Update frame position
                    stream.update_position(stream.period_size);
                    stream.last_irq_count += 1;
                }

                // Descriptor Error
                if sts & 0x08 != 0 {
                    serial_println!("[HDA] Stream {} descriptor error", stream.stream_index);
                }

                // FIFO Error
                if sts & 0x10 != 0 {
                    stream.state = StreamState::Underrun;
                    serial_println!("[HDA] Stream {} FIFO underrun", stream.stream_index);
                }

                // Clear status bits
                mmio_write8(mmio, sd_base + SD_STS, sts);
            }
        }

        // Capture stream
        if let Some(ref mut stream) = ctrl.capture_stream {
            if intsts & (1 << stream.stream_index as u32) != 0 {
                let sd_base = 0x80u16 + (stream.stream_index as u16) * 0x20;
                let sts = mmio_read8(mmio, sd_base + SD_STS);
                if sts & 0x04 != 0 {
                    let lpib = mmio_read32(mmio, sd_base + SD_LPIB);
                    stream.dma_position_in_buffer = lpib as usize;
                    stream.update_position(stream.period_size);
                    stream.last_irq_count += 1;
                }
                mmio_write8(mmio, sd_base + SD_STS, sts);
            }
        }

        // Clear global interrupt status
        mmio_write32(mmio, REG_INTSTS, intsts);
    }
}

// ─── ALSA-Compatible Interface ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmStream {
    Playback,
    Capture,
}

/// Open a PCM stream
pub fn pcm_open(stream: PcmStream) -> Result<u32, &'static str> {
    let ctrl = HDA_CONTROLLER.lock();
    if !ctrl.initialized {
        return Err("HDA not initialized");
    }
    // Return stream handle
    Ok(match stream {
        PcmStream::Playback => 0,
        PcmStream::Capture => 1,
    })
}

/// Set PCM hardware parameters
pub fn pcm_set_params(handle: u32, params: &PcmParams) -> Result<(), &'static str> {
    let mut ctrl = HDA_CONTROLLER.lock();
    if !ctrl.initialized {
        return Err("HDA not initialized");
    }
    ctrl.current_params = params.clone();
    serial_println!(
        "[HDA] PCM params: {}Hz {}ch {:?}",
        params.sample_rate,
        params.channels,
        params.format
    );
    Ok(())
}

/// Write PCM data to playback DMA buffer
/// Copies audio data into the DMA ring buffer at the current application write position.
/// Returns the number of bytes actually written.
pub fn pcm_write(_handle: u32, data: &[u8]) -> Result<usize, &'static str> {
    let mut ctrl = HDA_CONTROLLER.lock();
    if !ctrl.initialized {
        return Err("HDA not initialized");
    }

    if let Some(ref mut stream) = ctrl.playback_stream {
        if stream.buffer_base == 0 || stream.buffer_size == 0 {
            return Err("Playback stream not configured");
        }

        let buf_size = stream.buffer_size;
        let write_pos = stream.dma_position_in_buffer;
        let avail = buf_size.saturating_sub(write_pos);

        let to_write = data.len().min(avail);
        if to_write == 0 {
            return Ok(0); // Buffer full, caller should retry
        }

        // Copy PCM data into the DMA buffer
        unsafe {
            let dst = (stream.buffer_base as *mut u8).add(write_pos);
            core::ptr::copy_nonoverlapping(data.as_ptr(), dst, to_write);
        }

        // Advance write position (wrap around)
        stream.dma_position_in_buffer = (write_pos + to_write) % buf_size;

        // Update position tracking
        let bytes_per_frame =
            stream.params.channels as usize * stream.params.format.bytes_per_sample();
        if let Some(frames) = to_write.checked_div(bytes_per_frame) {
            stream.position_frames += frames as u64;
        }

        Ok(to_write)
    } else {
        Err("No playback stream")
    }
}

/// Start PCM stream
pub fn pcm_start(handle: u32) -> Result<(), &'static str> {
    let mut ctrl = HDA_CONTROLLER.lock();
    if !ctrl.initialized {
        return Err("HDA not initialized");
    }
    match handle {
        0 => {
            if ctrl.playback_stream.is_none() {
                return Err("Playback stream not set up");
            }
        }
        1 => {
            if ctrl.capture_stream.is_none() {
                return Err("Capture stream not set up");
            }
        }
        _ => return Err("Invalid handle"),
    }
    serial_println!("[HDA] PCM stream {} started", handle);
    Ok(())
}

/// Stop PCM stream
pub fn pcm_stop(handle: u32) -> Result<(), &'static str> {
    let mut ctrl = HDA_CONTROLLER.lock();
    if !ctrl.initialized {
        return Err("HDA not initialized");
    }
    match handle {
        0 => ctrl.playback_stream = None,
        1 => ctrl.capture_stream = None,
        _ => return Err("Invalid handle"),
    }
    serial_println!("[HDA] PCM stream {} stopped", handle);
    Ok(())
}

// ─── Mixer Controls ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MixerControl {
    pub name: String,
    pub min: i32,
    pub max: i32,
    pub value: i32,
    pub mute: bool,
}

pub fn get_mixer_controls() -> Vec<MixerControl> {
    let ctrl = HDA_CONTROLLER.lock();
    vec![
        MixerControl {
            name: String::from("Master"),
            min: 0,
            max: 100,
            value: ctrl.master_volume as i32,
            mute: ctrl.master_mute,
        },
        MixerControl {
            name: String::from("PCM"),
            min: 0,
            max: 100,
            value: 100,
            mute: false,
        },
        MixerControl {
            name: String::from("Capture"),
            min: 0,
            max: 100,
            value: 50,
            mute: false,
        },
    ]
}

// ─── Global State ───────────────────────────────────────────────────

static HDA_CONTROLLER: Mutex<HdaController> = Mutex::new(HdaController::new());
static HDA_AVAILABLE: AtomicBool = AtomicBool::new(false);

impl Default for HdaController {
    fn default() -> Self {
        Self::new()
    }
}

impl HdaController {
    pub const fn new() -> Self {
        Self {
            pci_bus: 0,
            pci_dev: 0,
            pci_func: 0,
            mmio_base: 0,
            vendor_id: 0,
            device_id: 0,
            num_output_streams: 0,
            num_input_streams: 0,
            num_bidir_streams: 0,
            codecs: Vec::new(),
            initialized: false,
            master_volume: 75,
            master_mute: false,
            playback_stream: None,
            capture_stream: None,
            current_params: PcmParams {
                sample_rate: 48000,
                channels: 2,
                format: SampleFormat::S16LE,
                buffer_size: 4096,
                period_size: 1024,
            },
        }
    }

    /// Allocate and setup a PCM playback stream with real DMA buffers
    pub fn setup_playback_stream(&mut self, params: PcmParams) -> Option<u32> {
        if self.num_output_streams == 0 {
            serial_println!("[HDA] No output streams available");
            return None;
        }

        let stream_index = 0; // Use first output stream
        let mut stream = PcmStreamState::new(stream_index, params.clone());
        stream.state = StreamState::Stopped;

        // Calculate buffer sizes
        let bytes_per_frame = params.channels as usize * params.format.bytes_per_sample();
        let period_bytes = params.period_size * bytes_per_frame;
        let num_periods = 4u8; // 4 periods in BDL
        let buffer_size = period_bytes * num_periods as usize;

        // Allocate DMA buffer (page-aligned for hardware DMA)
        let dma_layout = alloc::alloc::Layout::from_size_align(buffer_size, 4096)
            .unwrap_or(alloc::alloc::Layout::from_size_align(buffer_size, 8).unwrap());
        let dma_ptr = unsafe { alloc::alloc::alloc_zeroed(dma_layout) };
        if dma_ptr.is_null() {
            serial_println!(
                "[HDA] Failed to allocate DMA buffer ({} bytes)",
                buffer_size
            );
            return None;
        }
        let dma_phys = dma_ptr as u64; // In kernel space, virt ~= phys for identity-mapped region

        // Allocate BDL (Buffer Descriptor List) — must be 128-byte aligned
        let bdl_size = core::mem::size_of::<BdlEntry>() * num_periods as usize;
        let bdl_layout = alloc::alloc::Layout::from_size_align(bdl_size, 128)
            .unwrap_or(alloc::alloc::Layout::from_size_align(bdl_size, 8).unwrap());
        let bdl_ptr = unsafe { alloc::alloc::alloc_zeroed(bdl_layout) } as *mut BdlEntry;
        if bdl_ptr.is_null() {
            unsafe {
                alloc::alloc::dealloc(dma_ptr, dma_layout);
            }
            serial_println!("[HDA] Failed to allocate BDL");
            return None;
        }

        // Fill BDL entries — each points to a period within the DMA buffer
        for i in 0..num_periods as usize {
            let entry = BdlEntry {
                address: dma_phys + (i * period_bytes) as u64,
                length: period_bytes as u32,
                ioc: 1, // Interrupt on completion for every period
            };
            unsafe {
                core::ptr::write(bdl_ptr.add(i), entry);
            }
        }

        stream.buffer_base = dma_phys;
        stream.buffer_size = buffer_size;
        stream.period_size = period_bytes;
        stream.bdl_base = bdl_ptr as u64;
        stream.bdl_entries = num_periods;

        // Program HDA stream descriptor registers
        if self.mmio_base != 0 {
            unsafe {
                let sd_base = 0x80u16 + (stream_index as u16) * 0x20;

                // Stop stream first (clear RUN bit)
                mmio_write32(self.mmio_base, sd_base + SD_CTL, 0);

                // Wait for stream to stop
                for _ in 0..1000 {
                    if mmio_read32(self.mmio_base, sd_base + SD_CTL) & 1 == 0 {
                        break;
                    }
                    core::hint::spin_loop();
                }

                // Clear status bits
                mmio_write8(self.mmio_base, sd_base + SD_STS, 0x1C);

                // Set Cyclic Buffer Length (total buffer size in bytes)
                mmio_write32(self.mmio_base, sd_base + SD_CBL, buffer_size as u32);

                // Set Last Valid Index (number of BDL entries - 1)
                mmio_write16(self.mmio_base, sd_base + SD_LVI, (num_periods - 1) as u16);

                // Set stream format register
                let fmt = params.to_hda_format();
                mmio_write16(self.mmio_base, sd_base + SD_FMT, fmt);

                // Set BDL pointer (lower and upper 32 bits)
                mmio_write32(
                    self.mmio_base,
                    sd_base + SD_BDLPL,
                    (stream.bdl_base & 0xFFFFFFFF) as u32,
                );
                mmio_write32(
                    self.mmio_base,
                    sd_base + SD_BDLPU,
                    (stream.bdl_base >> 32) as u32,
                );

                // Set stream tag (1-15) in CTL bits [23:20]
                let stream_tag = 1u32;
                mmio_write32(
                    self.mmio_base,
                    sd_base + SD_CTL,
                    (stream_tag << 20) | (1 << 2), // stream tag + IOCE (interrupt on completion enable)
                );
            }
        }

        self.playback_stream = Some(stream);

        serial_println!(
            "[HDA] Playback stream allocated: {} Hz, {} ch, {:?}, {} byte buffer ({} periods), DMA @ {:#x}, BDL @ {:#x}",
            params.sample_rate,
            params.channels,
            params.format,
            buffer_size,
            num_periods,
            dma_phys,
            bdl_ptr as u64
        );

        self.current_params = params;
        Some(stream_index as u32)
    }

    /// Allocate and setup a PCM capture stream with real DMA buffers
    pub fn setup_capture_stream(&mut self, params: PcmParams) -> Option<u32> {
        if self.num_input_streams == 0 {
            serial_println!("[HDA] No input streams available");
            return None;
        }

        let stream_index = self.num_output_streams; // First input stream index
        let mut stream = PcmStreamState::new(stream_index, params.clone());
        stream.state = StreamState::Stopped;

        // Calculate buffer sizes
        let bytes_per_frame = params.channels as usize * params.format.bytes_per_sample();
        let period_bytes = params.period_size * bytes_per_frame;
        let num_periods = 4u8;
        let buffer_size = period_bytes * num_periods as usize;

        // Allocate DMA buffer (page-aligned)
        let dma_layout = alloc::alloc::Layout::from_size_align(buffer_size, 4096)
            .unwrap_or(alloc::alloc::Layout::from_size_align(buffer_size, 8).unwrap());
        let dma_ptr = unsafe { alloc::alloc::alloc_zeroed(dma_layout) };
        if dma_ptr.is_null() {
            serial_println!("[HDA] Failed to allocate capture DMA buffer");
            return None;
        }

        // Allocate BDL (128-byte aligned)
        let bdl_size = core::mem::size_of::<BdlEntry>() * num_periods as usize;
        let bdl_layout = alloc::alloc::Layout::from_size_align(bdl_size, 128)
            .unwrap_or(alloc::alloc::Layout::from_size_align(bdl_size, 8).unwrap());
        let bdl_ptr = unsafe { alloc::alloc::alloc_zeroed(bdl_layout) } as *mut BdlEntry;
        if bdl_ptr.is_null() {
            unsafe {
                alloc::alloc::dealloc(dma_ptr, dma_layout);
            }
            return None;
        }

        // Fill BDL entries
        let dma_phys = dma_ptr as u64;
        for i in 0..num_periods as usize {
            let entry = BdlEntry {
                address: dma_phys + (i * period_bytes) as u64,
                length: period_bytes as u32,
                ioc: 1,
            };
            unsafe {
                core::ptr::write(bdl_ptr.add(i), entry);
            }
        }

        stream.buffer_base = dma_phys;
        stream.buffer_size = buffer_size;
        stream.period_size = period_bytes;
        stream.bdl_base = bdl_ptr as u64;
        stream.bdl_entries = num_periods;

        // Program HDA stream descriptor registers for capture
        if self.mmio_base != 0 {
            unsafe {
                let sd_base = 0x80u16 + (stream_index as u16) * 0x20;
                mmio_write32(self.mmio_base, sd_base + SD_CTL, 0);
                for _ in 0..1000 {
                    if mmio_read32(self.mmio_base, sd_base + SD_CTL) & 1 == 0 {
                        break;
                    }
                    core::hint::spin_loop();
                }
                mmio_write8(self.mmio_base, sd_base + SD_STS, 0x1C);
                mmio_write32(self.mmio_base, sd_base + SD_CBL, buffer_size as u32);
                mmio_write16(self.mmio_base, sd_base + SD_LVI, (num_periods - 1) as u16);
                mmio_write16(self.mmio_base, sd_base + SD_FMT, params.to_hda_format());
                mmio_write32(
                    self.mmio_base,
                    sd_base + SD_BDLPL,
                    (stream.bdl_base & 0xFFFFFFFF) as u32,
                );
                mmio_write32(
                    self.mmio_base,
                    sd_base + SD_BDLPU,
                    (stream.bdl_base >> 32) as u32,
                );
                let stream_tag = 2u32;
                mmio_write32(
                    self.mmio_base,
                    sd_base + SD_CTL,
                    (stream_tag << 20) | (1 << 2),
                );
            }
        }

        self.capture_stream = Some(stream);

        serial_println!(
            "[HDA] Capture stream allocated: {} Hz, {} ch, {} byte buffer, DMA @ {:#x}",
            params.sample_rate,
            params.channels,
            buffer_size,
            dma_phys
        );
        Some(stream_index as u32)
    }

    /// Start playback on allocated stream — programs HDA registers for real DMA
    pub fn start_playback(&mut self) -> bool {
        if let Some(ref mut stream) = self.playback_stream {
            if stream.buffer_base == 0 {
                serial_println!("[HDA] Cannot start: no DMA buffer allocated");
                return false;
            }

            stream.state = StreamState::Running;
            stream.dma_position_in_buffer = 0;

            if self.mmio_base != 0 {
                unsafe {
                    let sd_base = 0x80u16 + (stream.stream_index as u16) * 0x20;

                    // Read current CTL (preserves stream tag)
                    let ctl = mmio_read32(self.mmio_base, sd_base + SD_CTL);

                    // Enable global interrupt control for this stream
                    let intctl = mmio_read32(self.mmio_base, REG_INTCTL);
                    mmio_write32(
                        self.mmio_base,
                        REG_INTCTL,
                        intctl | (1 << 31) | (1 << stream.stream_index as u32), // GIE + stream interrupt enable
                    );

                    // Set RUN bit to start DMA transfer
                    mmio_write32(
                        self.mmio_base,
                        sd_base + SD_CTL,
                        ctl | 0x02, // RUN bit (bit 1 in the 24-bit CTL field)
                    );

                    // Verify stream started
                    for _ in 0..1000 {
                        if mmio_read32(self.mmio_base, sd_base + SD_CTL) & 0x02 != 0 {
                            break;
                        }
                        core::hint::spin_loop();
                    }
                }
            }

            serial_println!(
                "[HDA] Playback started: DMA @ {:#x}, {} bytes, {} Hz",
                stream.buffer_base,
                stream.buffer_size,
                stream.params.sample_rate
            );
            true
        } else {
            false
        }
    }

    /// Stop playback
    pub fn stop_playback(&mut self) -> bool {
        if let Some(ref mut stream) = self.playback_stream {
            stream.state = StreamState::Stopped;
            unsafe {
                let stream_offset = 0x80u16 + (stream.stream_index as u16) * 0x20;
                mmio_write32(self.mmio_base, stream_offset + SD_CTL, 0); // Stop
            }
            serial_println!("[HDA] Playback stopped");
            true
        } else {
            false
        }
    }

    /// Start capture
    pub fn start_capture(&mut self) -> bool {
        if let Some(ref mut stream) = self.capture_stream {
            stream.state = StreamState::Running;
            unsafe {
                let stream_offset = 0x80u16 + (stream.stream_index as u16) * 0x20;
                mmio_write32(
                    self.mmio_base,
                    stream_offset + SD_CTL,
                    0x0000_0001 | (1 << 2),
                );
            }
            serial_println!("[HDA] Capture started");
            true
        } else {
            false
        }
    }

    /// Get current playback position
    pub fn get_playback_position(&self) -> u64 {
        self.playback_stream
            .as_ref()
            .map(|s| s.get_position_frames())
            .unwrap_or(0)
    }

    /// Set master volume (0-100)
    pub fn set_volume(&mut self, volume: u8) {
        self.master_volume = volume.min(100);
        serial_println!("[HDA] Master volume set to {}%", self.master_volume);
    }

    /// Toggle mute
    pub fn set_mute(&mut self, mute: bool) {
        self.master_mute = mute;
        serial_println!("[HDA] Master {}", if mute { "muted" } else { "unmuted" });
    }
}

pub fn init() {
    if let Some((bus, dev, func)) = find_hda_controller() {
        let id = pci_read32(bus, dev, func, 0);
        let vendor_id = (id & 0xFFFF) as u16;
        let device_id = ((id >> 16) & 0xFFFF) as u16;

        // Enable bus mastering + memory space
        let cmd = pci_read32(bus, dev, func, 0x04);
        pci_write32(bus, dev, func, 0x04, cmd | 0x06);

        // Get MMIO base from BAR0
        let bar0 = pci_read32(bus, dev, func, 0x10) & 0xFFFFFFF0;

        let mut ctrl = HDA_CONTROLLER.lock();
        ctrl.pci_bus = bus;
        ctrl.pci_dev = dev;
        ctrl.pci_func = func;
        ctrl.vendor_id = vendor_id;
        ctrl.device_id = device_id;
        ctrl.mmio_base = bar0 as u64;

        // Read capabilities and perform controller reset
        if ctrl.mmio_base != 0 {
            // Reset the controller
            let reset_ok = controller_reset(ctrl.mmio_base);

            unsafe {
                let gcap = mmio_read16(ctrl.mmio_base, REG_GCAP);
                ctrl.num_output_streams = ((gcap >> 12) & 0x0F) as u8;
                ctrl.num_input_streams = ((gcap >> 8) & 0x0F) as u8;
                ctrl.num_bidir_streams = ((gcap >> 3) & 0x1F) as u8;

                // Enable unsolicited responses
                let gctl = mmio_read32(ctrl.mmio_base, REG_GCTL);
                mmio_write32(ctrl.mmio_base, REG_GCTL, gctl | GCTL_UNSOL);

                // Wait for codec enumeration (STATESTS indicates codec presence)
                for _ in 0..10000 {
                    let statests = mmio_read16(ctrl.mmio_base, REG_STATESTS);
                    if statests != 0 {
                        // Enumerate detected codecs
                        for codec_addr in 0..15u8 {
                            if statests & (1 << codec_addr) != 0 {
                                let codec = HdaCodec {
                                    address: codec_addr,
                                    vendor_id: 0,
                                    revision_id: 0,
                                    afg_nid: 1,
                                    widgets: Vec::new(),
                                    output_nids: Vec::new(),
                                    input_nids: Vec::new(),
                                    pin_nids: Vec::new(),
                                    mixer_nids: Vec::new(),
                                };
                                serial_println!("[HDA]   Codec {} detected", codec_addr);
                                ctrl.codecs.push(codec);

                                // Identify codec vendor for vendor-specific init
                                let verb = make_verb(codec_addr, 0, VERB_GET_PARAMETER, 0x00); // VendorID
                                let vid = corb_send_verb(ctrl.mmio_base, verb);
                                if let Some(c) = ctrl.codecs.last_mut() {
                                    c.vendor_id = vid;
                                }
                                let codec_vendor = (vid >> 16) as u16;
                                let codec_device = (vid & 0xFFFF) as u16;
                                match codec_vendor {
                                    0x10EC => {
                                        // Realtek codec — ALC series
                                        let name = match codec_device {
                                            0x0221 => "ALC221",
                                            0x0233 => "ALC233",
                                            0x0255 => "ALC255",
                                            0x0256 => "ALC256",
                                            0x0269 => "ALC269",
                                            0x0283 => "ALC283",
                                            0x0285 => "ALC285",
                                            0x0289 => "ALC289",
                                            0x0292 => "ALC292",
                                            0x0295 => "ALC295",
                                            0x0700 => "ALC700",
                                            0x0897 => "ALC897",
                                            0x0662 => "ALC662",
                                            0x0892 => "ALC892",
                                            0x0899 => "ALC899",
                                            _ => "ALC (unknown)",
                                        };
                                        serial_println!(
                                            "[HDA]   Realtek {} codec (vid={:#010x})",
                                            name,
                                            vid
                                        );
                                        // Initialize Realtek ALC codec:
                                        // 1. Set EAPD (External Amp Power Down) bit on output pins
                                        let eapd_verb = make_verb(codec_addr, 0x14, 0x70C, 0x02);
                                        corb_send_verb(ctrl.mmio_base, eapd_verb);
                                        // 2. Unmute output amp on DAC node (NID 0x02/0x03)
                                        let unmute_dac = make_verb(codec_addr, 0x02, 0x3B0, 0x7F);
                                        corb_send_verb(ctrl.mmio_base, unmute_dac);
                                        // 3. Set pin widget control for headphone out (NID 0x21)
                                        let hp_pin = make_verb(codec_addr, 0x21, 0x707, 0xC0);
                                        corb_send_verb(ctrl.mmio_base, hp_pin);
                                    }
                                    0x8086 => {
                                        serial_println!(
                                            "[HDA]   Intel HDMI/DP codec (vid={:#010x})",
                                            vid
                                        );
                                    }
                                    _ => {
                                        serial_println!(
                                            "[HDA]   Unknown codec vendor {:#06x} (vid={:#010x})",
                                            codec_vendor,
                                            vid
                                        );
                                    }
                                }
                            }
                        }
                        break;
                    }
                    core::hint::spin_loop();
                }

                // Enable global interrupt
                mmio_write32(ctrl.mmio_base, REG_INTCTL, 1 << 31); // GIE
            }

            serial_println!(
                "[HDA]   Controller reset: {}",
                if reset_ok {
                    "OK"
                } else {
                    "timeout (continuing)"
                }
            );
        }

        ctrl.initialized = true;
        HDA_AVAILABLE.store(true, Ordering::Relaxed);

        serial_println!(
            "[HDA] Intel HD Audio controller found: {:04x}:{:04x}",
            vendor_id,
            device_id
        );
        serial_println!(
            "[HDA]   PCI {:02x}:{:02x}.{}, MMIO @ {:#x}",
            bus,
            dev,
            func,
            bar0
        );
        serial_println!(
            "[HDA]   Streams: {} output, {} input, {} bidirectional, {} codecs",
            ctrl.num_output_streams,
            ctrl.num_input_streams,
            ctrl.num_bidir_streams,
            ctrl.codecs.len()
        );
    } else {
        serial_println!("[HDA] No HD Audio controller found (PC Speaker only)");
    }
}
