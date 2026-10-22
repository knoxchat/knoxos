/// USB Audio Class (UAC) — USB audio device driver
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// USB Audio Class subclass types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSubclass {
    Undefined = 0x00,
    AudioControl = 0x01,
    AudioStreaming = 0x02,
    MidiStreaming = 0x03,
}

/// Audio format types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Pcm,
    PcmFloat,
    Alaw,
    Mulaw,
}

/// Audio terminal types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalType {
    UsbStreaming,
    Speaker,
    Headphones,
    Microphone,
    HeadsetMic,
    LineIn,
    LineOut,
    SpdifOut,
}

/// An audio endpoint (isochronous)
#[derive(Debug, Clone)]
pub struct AudioEndpoint {
    pub direction_in: bool,
    pub address: u8,
    pub max_packet_size: u16,
    pub interval: u8,
    pub sample_rate: u32,
    pub channels: u8,
    pub bit_depth: u8,
    pub format: AudioFormat,
}

/// A detected USB audio device
#[derive(Debug, Clone)]
pub struct UsbAudioDevice {
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usb_address: u8,
    pub inputs: Vec<AudioEndpoint>,
    pub outputs: Vec<AudioEndpoint>,
    pub volume: u8, // 0-100
    pub muted: bool,
    pub active: bool,
}

lazy_static::lazy_static! {
    static ref AUDIO_DEVICES: Mutex<Vec<UsbAudioDevice>> = Mutex::new(Vec::new());
}

/// Probe USB devices for Audio Class interfaces
pub fn probe() -> usize {
    serial_println!("[usb-audio] Probing for USB audio devices");
    // Would iterate USB device list, check interface class 0x01
    0
}

/// Initialize a USB audio device
pub fn init_device(dev_addr: u8) -> Result<(), &'static str> {
    serial_println!("[usb-audio] Initializing device at address {}", dev_addr);
    // Parse Audio Control descriptors
    // Set up Audio Streaming interfaces
    // Configure sample rate
    Ok(())
}

/// Start audio streaming on an output endpoint
pub fn start_output(dev_idx: usize, endpoint_idx: usize) -> Result<(), &'static str> {
    let mut devices = AUDIO_DEVICES.lock();
    let dev = devices.get_mut(dev_idx).ok_or("Device not found")?;
    if endpoint_idx >= dev.outputs.len() {
        return Err("Endpoint not found");
    }
    let ep = &dev.outputs[endpoint_idx];
    serial_println!(
        "[usb-audio] Starting output: {}ch {}Hz {}bit",
        ep.channels,
        ep.sample_rate,
        ep.bit_depth
    );
    dev.active = true;
    Ok(())
}

/// Stop audio streaming
pub fn stop_output(dev_idx: usize) -> Result<(), &'static str> {
    let mut devices = AUDIO_DEVICES.lock();
    let dev = devices.get_mut(dev_idx).ok_or("Device not found")?;
    dev.active = false;
    Ok(())
}

/// Start audio capture from an input endpoint
pub fn start_input(dev_idx: usize, endpoint_idx: usize) -> Result<(), &'static str> {
    let mut devices = AUDIO_DEVICES.lock();
    let dev = devices.get_mut(dev_idx).ok_or("Device not found")?;
    if endpoint_idx >= dev.inputs.len() {
        return Err("Endpoint not found");
    }
    let ep = &dev.inputs[endpoint_idx];
    serial_println!(
        "[usb-audio] Starting input: {}ch {}Hz {}bit",
        ep.channels,
        ep.sample_rate,
        ep.bit_depth
    );
    dev.active = true;
    Ok(())
}

/// Send PCM audio data to the output endpoint (isochronous transfer)
pub fn write_pcm(dev_idx: usize, samples: &[u8]) -> Result<usize, &'static str> {
    let devices = AUDIO_DEVICES.lock();
    let dev = devices.get(dev_idx).ok_or("Device not found")?;
    if !dev.active {
        return Err("Not streaming");
    }
    // Would submit isochronous URB with PCM data
    Ok(samples.len())
}

/// Read captured PCM audio data from the input endpoint
pub fn read_pcm(dev_idx: usize, buffer: &mut [u8]) -> Result<usize, &'static str> {
    let devices = AUDIO_DEVICES.lock();
    let dev = devices.get(dev_idx).ok_or("Device not found")?;
    if !dev.active {
        return Err("Not streaming");
    }
    Ok(0) // would return captured data
}

/// Set volume for a USB audio device (0-100)
pub fn set_volume(dev_idx: usize, volume: u8) -> Result<(), &'static str> {
    let mut devices = AUDIO_DEVICES.lock();
    let dev = devices.get_mut(dev_idx).ok_or("Device not found")?;
    dev.volume = volume.min(100);
    serial_println!("[usb-audio] Volume set to {}%", dev.volume);
    // Would send SET_CUR to Feature Unit volume control
    Ok(())
}

/// Set mute state
pub fn set_mute(dev_idx: usize, muted: bool) -> Result<(), &'static str> {
    let mut devices = AUDIO_DEVICES.lock();
    let dev = devices.get_mut(dev_idx).ok_or("Device not found")?;
    dev.muted = muted;
    Ok(())
}

/// List all detected USB audio devices
pub fn list_devices() -> Vec<(String, bool, bool)> {
    AUDIO_DEVICES
        .lock()
        .iter()
        .map(|d| (d.name.clone(), d.active, d.muted))
        .collect()
}

pub fn init() {
    let count = probe();
    serial_println!(
        "[usb-audio] USB Audio Class driver initialized ({} devices)",
        count
    );
}
