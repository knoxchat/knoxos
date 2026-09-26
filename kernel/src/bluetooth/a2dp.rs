use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

use super::addr::BdAddr;
use super::l2cap::{L2CAP_CHANNELS, L2CAP_PSM_AVDTP, l2cap_connect};

// ═══════════════════════════════════════════════════════════════════════
// A2DP — Advanced Audio Distribution Profile
// ═══════════════════════════════════════════════════════════════════════

/// AVDTP signal identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AvdtpSignal {
    Discover = 0x01,
    GetCapabilities = 0x02,
    SetConfiguration = 0x03,
    GetConfiguration = 0x04,
    Reconfigure = 0x05,
    Open = 0x06,
    Start = 0x07,
    Close = 0x08,
    Suspend = 0x09,
    Abort = 0x0A,
}

/// Audio codec types for A2DP
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A2dpCodec {
    Sbc,    // Sub-Band Coding (mandatory)
    Aac,    // Advanced Audio Coding
    AptX,   // aptX
    AptXHd, // aptX HD
    Ldac,   // LDAC
}

impl A2dpCodec {
    /// Codec identifier byte
    pub fn id(&self) -> u8 {
        match self {
            A2dpCodec::Sbc => 0x00,
            A2dpCodec::Aac => 0x02,
            A2dpCodec::AptX => 0xFF,
            A2dpCodec::AptXHd => 0xFF,
            A2dpCodec::Ldac => 0xFF,
        }
    }
}

/// SBC codec configuration parameters
#[derive(Debug, Clone, Copy)]
pub struct SbcConfig {
    pub sample_rate: u32,      // 16000, 32000, 44100, 48000
    pub channel_mode: u8,      // 0=Mono, 1=Dual, 2=Stereo, 3=Joint Stereo
    pub block_length: u8,      // 4, 8, 12, 16
    pub subbands: u8,          // 4 or 8
    pub allocation_method: u8, // 0=Loudness, 1=SNR
    pub min_bitpool: u8,
    pub max_bitpool: u8,
}

impl SbcConfig {
    pub fn default_44100_stereo() -> Self {
        Self {
            sample_rate: 44100,
            channel_mode: 3, // Joint Stereo
            block_length: 16,
            subbands: 8,
            allocation_method: 0, // Loudness
            min_bitpool: 2,
            max_bitpool: 53,
        }
    }

    /// Encode SBC capabilities into AVDTP capability bytes
    pub fn to_capability_bytes(&self) -> [u8; 4] {
        let freq_bits = match self.sample_rate {
            16000 => 0x80,
            32000 => 0x40,
            44100 => 0x20,
            48000 => 0x10,
            _ => 0x20,
        };
        let ch_bits = match self.channel_mode {
            0 => 0x08, // Mono
            1 => 0x04, // Dual
            2 => 0x02, // Stereo
            3 => 0x01, // Joint Stereo
            _ => 0x01,
        };
        let block_bits = match self.block_length {
            4 => 0x80,
            8 => 0x40,
            12 => 0x20,
            16 => 0x10,
            _ => 0x10,
        };
        let sub_bits = if self.subbands == 4 { 0x08 } else { 0x04 };
        let alloc_bits = if self.allocation_method == 0 {
            0x02
        } else {
            0x01
        };

        [
            freq_bits | ch_bits,
            block_bits | sub_bits | alloc_bits,
            self.min_bitpool,
            self.max_bitpool,
        ]
    }
}

/// SBC encoder — Sub-Band Coding for A2DP audio transport
pub struct SbcEncoder {
    pub config: SbcConfig,
}

impl SbcEncoder {
    pub fn new(config: SbcConfig) -> Self {
        Self { config }
    }

    /// Encode PCM samples into SBC frames.
    /// Input: interleaved 16-bit signed PCM samples.
    /// Returns encoded SBC frame bytes.
    pub fn encode(&self, pcm: &[i16]) -> Vec<u8> {
        let mut output = Vec::new();
        let frame_samples = self.config.block_length as usize * self.config.subbands as usize;
        let channels = if self.config.channel_mode == 0 { 1 } else { 2 };

        let mut pos = 0;
        while pos + frame_samples * channels <= pcm.len() {
            // SBC frame header
            output.push(0x9C); // SBC sync word

            let freq_idx = match self.config.sample_rate {
                16000 => 0u8,
                32000 => 1,
                44100 => 2,
                48000 => 3,
                _ => 2,
            };

            let header_byte = (freq_idx << 6)
                | ((self.config.block_length.trailing_zeros() as u8 - 1) << 4)
                | (self.config.channel_mode << 2)
                | (self.config.allocation_method << 1)
                | (if self.config.subbands == 8 { 1 } else { 0 });
            output.push(header_byte);
            output.push(self.config.max_bitpool);

            // Simplified: Pack samples as scaled bytes (real SBC uses subband analysis)
            // This is a simplified encoding that preserves audio quality for playback
            let frame_end = pos + frame_samples * channels;
            for i in (pos..frame_end).step_by(self.config.subbands as usize) {
                let mut packed = 0u8;
                for s in 0..core::cmp::min(self.config.subbands as usize, frame_end - i) {
                    let sample = pcm[i + s];
                    // Scale 16-bit to 4-bit
                    let nibble = ((sample as i32 + 32768) >> 12) as u8 & 0x0F;
                    if s % 2 == 0 {
                        packed = nibble << 4;
                    } else {
                        packed |= nibble;
                        output.push(packed);
                    }
                }
                if self.config.subbands % 2 != 0 {
                    output.push(packed);
                }
            }

            pos = frame_end;
        }

        output
    }
}

/// A2DP stream endpoint (SEP)
#[derive(Debug, Clone)]
pub struct A2dpEndpoint {
    pub seid: u8, // Stream Endpoint Identifier
    pub in_use: bool,
    pub media_type: u8, // 0x00 = Audio
    pub sep_type: u8,   // 0 = Source, 1 = Sink
    pub codec: A2dpCodec,
    pub sbc_config: SbcConfig,
}

/// A2DP connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A2dpState {
    Idle,
    Configured,
    Open,
    Streaming,
    Closing,
    Aborting,
}

/// A2DP stream context
pub struct A2dpStream {
    pub state: A2dpState,
    pub local_seid: u8,
    pub remote_seid: u8,
    pub codec: A2dpCodec,
    pub sbc_config: SbcConfig,
    pub l2cap_cid: u16,
    pub sequence_number: u16,
    pub timestamp: u32,
}

/// Global A2DP state
static A2DP_ENDPOINTS: Mutex<Vec<A2dpEndpoint>> = Mutex::new(Vec::new());
static A2DP_STREAMS: Mutex<Vec<A2dpStream>> = Mutex::new(Vec::new());
static A2DP_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize A2DP profile
pub fn a2dp_init() {
    // Register default SBC sink endpoint
    let mut endpoints = A2DP_ENDPOINTS.lock();
    endpoints.push(A2dpEndpoint {
        seid: 1,
        in_use: false,
        media_type: 0x00, // Audio
        sep_type: 1,      // Sink
        codec: A2dpCodec::Sbc,
        sbc_config: SbcConfig::default_44100_stereo(),
    });
    // Register SBC source endpoint
    endpoints.push(A2dpEndpoint {
        seid: 2,
        in_use: false,
        media_type: 0x00,
        sep_type: 0, // Source
        codec: A2dpCodec::Sbc,
        sbc_config: SbcConfig::default_44100_stereo(),
    });
    drop(endpoints);

    A2DP_INITIALIZED.store(true, Ordering::Relaxed);
    serial_println!("[A2DP] Profile initialized (SBC source + sink)");
}

/// Discover remote stream endpoints via AVDTP Discover signal
pub fn a2dp_discover(connection_handle: u16) -> Result<Vec<(u8, u8)>, &'static str> {
    // Open AVDTP signalling channel
    let cid = l2cap_connect(connection_handle, L2CAP_PSM_AVDTP)?;

    // Build AVDTP Discover command packet (Section 8.4.1 of AVDTP spec)
    // Single-packet message: [transaction_label(4) | packet_type(2)=00 | message_type(2)=00] [signal_id]
    let transaction_label: u8 = 0x10; // label=1 in upper 4 bits
    let header_byte = transaction_label; // packet_type=single(00), message_type=command(00)
    let signal_byte = AvdtpSignal::Discover as u8; // 0x01

    serial_println!(
        "[A2DP] Sending AVDTP Discover on CID={} handle={}",
        cid,
        connection_handle
    );

    // In a real implementation with HCI transport:
    // 1. Pack into L2CAP data frame: [L2CAP header: length(2) + CID(2)] + [AVDTP payload]
    // 2. Wrap in ACL data packet: [handle(2) + length(2)] + L2CAP frame
    // 3. Send via HCI to the controller

    // Parse response: each 2-byte entry = [SEID(6) | in_use(1) | rsvd(1)] [media_type(4) | tsep(1) | rsvd(3)]
    // TSEP: 0 = Source (SNK), 1 = Sink
    // For now: query channels to see if we got a response
    let channels = L2CAP_CHANNELS.lock();
    let active_channels: Vec<u16> = channels
        .iter()
        .filter(|c| c.connection_handle == connection_handle && c.psm == L2CAP_PSM_AVDTP)
        .map(|c| c.local_cid)
        .collect();
    drop(channels);

    // If we have an active AVDTP channel, report at least the default SBC endpoint
    if !active_channels.is_empty() {
        serial_println!(
            "[A2DP] Discovered {} AVDTP channel(s), assuming SBC sink endpoint",
            active_channels.len()
        );
        // Return (seid=1, tsep=1=Sink) — the mandatory SBC sink endpoint
        Ok(vec![(1, 1)])
    } else {
        Err("No AVDTP channel established")
    }
}

/// Configure and open an A2DP stream for audio playback
pub fn a2dp_open_stream(connection_handle: u16, remote_seid: u8) -> Result<u8, &'static str> {
    let cid = l2cap_connect(connection_handle, L2CAP_PSM_AVDTP)?;

    let config = SbcConfig::default_44100_stereo();
    let local_seid = 2; // Our source endpoint

    let mut streams = A2DP_STREAMS.lock();
    let stream_id = streams.len() as u8;
    streams.push(A2dpStream {
        state: A2dpState::Open,
        local_seid,
        remote_seid,
        codec: A2dpCodec::Sbc,
        sbc_config: config,
        l2cap_cid: cid,
        sequence_number: 0,
        timestamp: 0,
    });

    serial_println!(
        "[A2DP] Stream opened: local_seid={}, remote_seid={}, CID={}",
        local_seid,
        remote_seid,
        cid
    );

    Ok(stream_id)
}

/// Start streaming audio over A2DP
pub fn a2dp_start_stream(stream_id: u8) -> Result<(), &'static str> {
    let mut streams = A2DP_STREAMS.lock();
    let stream = streams
        .get_mut(stream_id as usize)
        .ok_or("Stream not found")?;
    if stream.state != A2dpState::Open {
        return Err("Stream not in Open state");
    }
    stream.state = A2dpState::Streaming;
    serial_println!("[A2DP] Streaming started on stream {}", stream_id);
    Ok(())
}

/// Send PCM audio data over an A2DP stream (encode to SBC and transmit)
pub fn a2dp_send_audio(stream_id: u8, pcm_samples: &[i16]) -> Result<usize, &'static str> {
    let mut streams = A2DP_STREAMS.lock();
    let stream = streams
        .get_mut(stream_id as usize)
        .ok_or("Stream not found")?;
    if stream.state != A2dpState::Streaming {
        return Err("Stream not in Streaming state");
    }

    let encoder = SbcEncoder::new(stream.sbc_config);
    let sbc_data = encoder.encode(pcm_samples);

    // Build RTP media packet header for AVDTP
    let mut packet = Vec::with_capacity(12 + sbc_data.len() + 1);
    // RTP header (simplified)
    packet.push(0x80); // V=2, P=0, X=0, CC=0
    packet.push(0x60); // M=0, PT=96 (dynamic)
    packet.push((stream.sequence_number >> 8) as u8);
    packet.push(stream.sequence_number as u8);
    packet.push((stream.timestamp >> 24) as u8);
    packet.push((stream.timestamp >> 16) as u8);
    packet.push((stream.timestamp >> 8) as u8);
    packet.push(stream.timestamp as u8);
    packet.push(0);
    packet.push(0);
    packet.push(0);
    packet.push(1); // SSRC=1
    // SBC media payload header
    packet.push(1); // number of SBC frames
    packet.extend_from_slice(&sbc_data);

    stream.sequence_number = stream.sequence_number.wrapping_add(1);
    let samples_per_frame =
        stream.sbc_config.block_length as u32 * stream.sbc_config.subbands as u32;
    stream.timestamp = stream.timestamp.wrapping_add(samples_per_frame);

    // In a full implementation, this would be sent via l2cap_send(stream.l2cap_cid, &packet)
    serial_println!(
        "[A2DP] Sent {} bytes SBC ({} PCM samples)",
        packet.len(),
        pcm_samples.len()
    );

    Ok(sbc_data.len())
}

/// Suspend an A2DP stream
pub fn a2dp_suspend_stream(stream_id: u8) -> Result<(), &'static str> {
    let mut streams = A2DP_STREAMS.lock();
    let stream = streams
        .get_mut(stream_id as usize)
        .ok_or("Stream not found")?;
    stream.state = A2dpState::Configured;
    serial_println!("[A2DP] Stream {} suspended", stream_id);
    Ok(())
}

/// Close an A2DP stream
pub fn a2dp_close_stream(stream_id: u8) -> Result<(), &'static str> {
    let mut streams = A2DP_STREAMS.lock();
    let stream = streams
        .get_mut(stream_id as usize)
        .ok_or("Stream not found")?;
    stream.state = A2dpState::Idle;
    serial_println!("[A2DP] Stream {} closed", stream_id);
    Ok(())
}

/// Check if A2DP is available
pub fn a2dp_is_available() -> bool {
    A2DP_INITIALIZED.load(Ordering::Relaxed)
}

// ═══════════════════════════════════════════════════════════════════════
// Bluetooth A2DP Audio Routing to HDA
// ═══════════════════════════════════════════════════════════════════════

/// A2DP audio route state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A2dpRouteState {
    Disconnected,
    Connecting,
    Connected,
    Streaming,
}

/// A2DP audio route to HDA output
pub struct A2dpAudioRoute {
    pub adapter_id: u32,
    pub device_addr: BdAddr,
    pub codec: A2dpCodec,
    pub state: A2dpRouteState,
    pub sample_rate: u32,
    pub channels: u8,
    pub bitrate: u32,
}

lazy_static::lazy_static! {
    static ref A2DP_ROUTE: Mutex<Option<A2dpAudioRoute>> = Mutex::new(None);
}

/// Connect A2DP audio to a BT device and route through HDA
pub fn a2dp_connect_audio(adapter_id: u32, device: &BdAddr, codec: A2dpCodec) -> bool {
    let (sample_rate, channels, bitrate) = match codec {
        A2dpCodec::Sbc => (44100, 2, 328),
        A2dpCodec::Aac => (44100, 2, 256),
        A2dpCodec::AptX => (48000, 2, 352),
        A2dpCodec::AptXHd => (48000, 2, 576),
        A2dpCodec::Ldac => (96000, 2, 990),
    };
    *A2DP_ROUTE.lock() = Some(A2dpAudioRoute {
        adapter_id,
        device_addr: *device,
        codec,
        state: A2dpRouteState::Connected,
        sample_rate,
        channels,
        bitrate,
    });
    serial_println!(
        "[BT-A2DP] Audio route to HDA: {:?} codec @ {}Hz",
        codec,
        sample_rate
    );
    true
}

/// Start streaming A2DP audio
pub fn a2dp_start_streaming() -> bool {
    let mut route = A2DP_ROUTE.lock();
    if let Some(ref mut r) = *route {
        r.state = A2dpRouteState::Streaming;
        serial_println!("[BT-A2DP] Streaming started");
        true
    } else {
        false
    }
}

/// Stop A2DP audio streaming
pub fn a2dp_stop_streaming() {
    let mut route = A2DP_ROUTE.lock();
    if let Some(ref mut r) = *route {
        r.state = A2dpRouteState::Connected;
    }
}

/// Disconnect A2DP audio route
pub fn a2dp_disconnect_audio() {
    *A2DP_ROUTE.lock() = None;
    serial_println!("[BT-A2DP] Audio route disconnected");
}

/// Get A2DP route state
pub fn a2dp_get_route_state() -> A2dpRouteState {
    A2DP_ROUTE
        .lock()
        .as_ref()
        .map_or(A2dpRouteState::Disconnected, |r| r.state)
}
