//! USB Audio Class (UAC 1.0) — SET_CUR sample rate + isochronous PCM loopback.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// USB Audio Class subclass
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSubclass {
    Undefined = 0x00,
    AudioControl = 0x01,
    AudioStreaming = 0x02,
    MidiStreaming = 0x03,
}

/// SET_CUR sampling-frequency control (CS_SAM_FREQ = 0x01) is 3 bytes LE.
pub fn parse_set_cur_freq(data: &[u8]) -> Option<u32> {
    if data.len() < 3 {
        return None;
    }
    Some(u32::from(data[0]) | (u32::from(data[1]) << 8) | (u32::from(data[2]) << 16))
}

pub fn encode_set_cur_freq(hz: u32) -> [u8; 3] {
    [hz as u8, (hz >> 8) as u8, (hz >> 16) as u8]
}

fn rate_supported(hz: u32) -> bool {
    matches!(hz, 44100 | 48000 | 96000)
}

#[derive(Debug, Clone)]
pub struct UsbAudioDevice {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u8,
    pub bit_depth: u8,
    pub volume: u8,
    pub muted: bool,
    pub active: bool,
    pcm_ring: Vec<u8>,
}

lazy_static::lazy_static! {
    static ref AUDIO_DEVICES: Mutex<Vec<UsbAudioDevice>> = Mutex::new(Vec::new());
}

pub fn probe_speaker(name: &str) -> usize {
    let mut devices = AUDIO_DEVICES.lock();
    devices.push(UsbAudioDevice {
        name: String::from(name),
        sample_rate: 48000,
        channels: 2,
        bit_depth: 16,
        volume: 100,
        muted: false,
        active: false,
        pcm_ring: Vec::new(),
    });
    devices.len() - 1
}

pub fn set_sample_rate(dev_idx: usize, data: &[u8]) -> Result<u32, &'static str> {
    let hz = parse_set_cur_freq(data).ok_or("short SET_CUR")?;
    if !rate_supported(hz) {
        return Err("unsupported rate");
    }
    let mut devices = AUDIO_DEVICES.lock();
    let dev = devices.get_mut(dev_idx).ok_or("Device not found")?;
    dev.sample_rate = hz;
    Ok(hz)
}

pub fn start_output(dev_idx: usize) -> Result<(), &'static str> {
    let mut devices = AUDIO_DEVICES.lock();
    let dev = devices.get_mut(dev_idx).ok_or("Device not found")?;
    dev.active = true;
    Ok(())
}

/// Isochronous PCM write — stored for capture-side loopback.
pub fn write_pcm(dev_idx: usize, samples: &[u8]) -> Result<usize, &'static str> {
    let mut devices = AUDIO_DEVICES.lock();
    let dev = devices.get_mut(dev_idx).ok_or("Device not found")?;
    if !dev.active {
        return Err("Not streaming");
    }
    if dev.muted {
        return Ok(samples.len());
    }
    dev.pcm_ring.extend_from_slice(samples);
    Ok(samples.len())
}

pub fn read_pcm(dev_idx: usize, buffer: &mut [u8]) -> Result<usize, &'static str> {
    let mut devices = AUDIO_DEVICES.lock();
    let dev = devices.get_mut(dev_idx).ok_or("Device not found")?;
    if !dev.active {
        return Err("Not streaming");
    }
    let n = buffer.len().min(dev.pcm_ring.len());
    buffer[..n].copy_from_slice(&dev.pcm_ring[..n]);
    dev.pcm_ring.drain(..n);
    Ok(n)
}

pub const GATE_USB_AUDIO_MARKER: &str = "GATE_USB audio";

pub fn audio_self_test() -> bool {
    let idx = probe_speaker("UAC Speaker");
    let cur = encode_set_cur_freq(48000);
    match set_sample_rate(idx, &cur) {
        Ok(48000) => {}
        Ok(hz) => {
            serial_println!("[USB-Audio] GATE_USB FAILED rate={}", hz);
            return false;
        }
        Err(e) => {
            serial_println!("[USB-Audio] GATE_USB FAILED: {}", e);
            return false;
        }
    }
    if start_output(idx).is_err() {
        serial_println!("[USB-Audio] GATE_USB FAILED: start");
        return false;
    }
    // 16-bit stereo: one frame L=0x1122 R=0x3344
    let pcm = [0x22u8, 0x11, 0x44, 0x33];
    if write_pcm(idx, &pcm) != Ok(4) {
        serial_println!("[USB-Audio] GATE_USB FAILED: write");
        return false;
    }
    let mut out = [0u8; 4];
    match read_pcm(idx, &mut out) {
        Ok(4) if out == pcm => {
            serial_println!(
                "[USB-Audio] {} (SET_CUR 48 kHz + iso PCM loopback)",
                GATE_USB_AUDIO_MARKER
            );
            true
        }
        Ok(n) => {
            serial_println!("[USB-Audio] GATE_USB FAILED n={} {:x?}", n, out);
            false
        }
        Err(e) => {
            serial_println!("[USB-Audio] GATE_USB FAILED: {}", e);
            false
        }
    }
}
