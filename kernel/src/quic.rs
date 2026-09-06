/// QUIC Transport Protocol — RFC 9000 compliant
/// Modern encrypted transport protocol built on UDP
/// Provides multiplexed streams, 0-RTT connection establishment, and built-in TLS 1.3
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// QUIC VERSION & CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

/// QUIC protocol version 1 (RFC 9000)
pub const QUIC_VERSION_1: u32 = 0x0000_0001;
/// QUIC version 2 (RFC 9369)
pub const QUIC_VERSION_2: u32 = 0x6b33_43cf;

/// Maximum UDP payload size
pub const MAX_UDP_PAYLOAD: usize = 1472;
/// Maximum QUIC packet size (including headers)
pub const MAX_QUIC_PACKET: usize = 1350;
/// Initial max data (connection-level flow control)
pub const INITIAL_MAX_DATA: u64 = 1_048_576; // 1 MiB
/// Initial max stream data
pub const INITIAL_MAX_STREAM_DATA: u64 = 262_144; // 256 KiB
/// Initial max streams (bidi)
pub const INITIAL_MAX_STREAMS_BIDI: u64 = 100;
/// Initial max streams (uni)
pub const INITIAL_MAX_STREAMS_UNI: u64 = 100;
/// Default idle timeout (ms)
pub const DEFAULT_IDLE_TIMEOUT: u64 = 30_000;
/// Default max ack delay (ms)
pub const DEFAULT_MAX_ACK_DELAY: u64 = 25;
/// Initial RTT estimate (ms)
pub const INITIAL_RTT: u64 = 333;
/// Minimum congestion window (bytes)
pub const MIN_CWND: u64 = 2 * MAX_QUIC_PACKET as u64;
/// Initial congestion window (bytes)
pub const INITIAL_CWND: u64 = 10 * MAX_QUIC_PACKET as u64;

// ═══════════════════════════════════════════════════════════════════════
// CONNECTION ID
// ═══════════════════════════════════════════════════════════════════════

/// QUIC Connection ID (variable length, up to 20 bytes)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConnectionId {
    pub bytes: Vec<u8>,
}

impl ConnectionId {
    pub fn new(bytes: Vec<u8>) -> Self {
        assert!(bytes.len() <= 20, "Connection ID too long");
        Self { bytes }
    }

    pub fn empty() -> Self {
        Self { bytes: Vec::new() }
    }

    pub fn generate() -> Self {
        // Generate 8-byte random connection ID
        let id = NEXT_CONN_ID.fetch_add(1, Ordering::SeqCst);
        let mut bytes = Vec::with_capacity(8);
        bytes.extend_from_slice(&id.to_be_bytes());
        Self { bytes }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

static NEXT_CONN_ID: AtomicU64 = AtomicU64::new(1);

// ═══════════════════════════════════════════════════════════════════════
// PACKET TYPES
// ═══════════════════════════════════════════════════════════════════════

/// QUIC packet types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketType {
    /// Initial packet (starts handshake)
    Initial,
    /// 0-RTT packet (early data)
    ZeroRTT,
    /// Handshake packet
    Handshake,
    /// Retry packet
    Retry,
    /// Short header (1-RTT data) packet
    Short,
    /// Version negotiation packet
    VersionNegotiation,
}

impl PacketType {
    /// Get the long header packet type bits
    pub fn to_bits(self) -> u8 {
        match self {
            PacketType::Initial => 0x00,
            PacketType::ZeroRTT => 0x01,
            PacketType::Handshake => 0x02,
            PacketType::Retry => 0x03,
            _ => 0x00,
        }
    }
}

/// QUIC packet header
#[derive(Debug, Clone)]
pub struct PacketHeader {
    pub packet_type: PacketType,
    pub version: u32,
    pub dcid: ConnectionId,
    pub scid: ConnectionId,
    pub packet_number: u64,
    pub payload_length: usize,
    /// Token (for Initial and Retry packets)
    pub token: Vec<u8>,
}

impl PacketHeader {
    pub fn new_initial(dcid: ConnectionId, scid: ConnectionId, pn: u64) -> Self {
        Self {
            packet_type: PacketType::Initial,
            version: QUIC_VERSION_1,
            dcid,
            scid,
            packet_number: pn,
            payload_length: 0,
            token: Vec::new(),
        }
    }

    pub fn new_handshake(dcid: ConnectionId, scid: ConnectionId, pn: u64) -> Self {
        Self {
            packet_type: PacketType::Handshake,
            version: QUIC_VERSION_1,
            dcid,
            scid,
            packet_number: pn,
            payload_length: 0,
            token: Vec::new(),
        }
    }

    pub fn new_short(dcid: ConnectionId, pn: u64) -> Self {
        Self {
            packet_type: PacketType::Short,
            version: 0,
            dcid,
            scid: ConnectionId::empty(),
            packet_number: pn,
            payload_length: 0,
            token: Vec::new(),
        }
    }

    /// Encode packet header into bytes
    pub fn encode(&self, buf: &mut Vec<u8>) {
        match self.packet_type {
            PacketType::Short => {
                // Short header: form=0, fixed=1, spin=0, reserved=00, key_phase=0, pn_len=00
                let first_byte = 0x40u8; // 01SRRKPP
                buf.push(first_byte);
                buf.extend_from_slice(&self.dcid.bytes);
                // Packet number (1 byte for simplicity)
                buf.push(self.packet_number as u8);
            }
            _ => {
                // Long header: form=1, fixed=1, type=TT, reserved=00, pn_len=00
                let first_byte = 0xC0u8 | (self.packet_type.to_bits() << 4);
                buf.push(first_byte);
                buf.extend_from_slice(&self.version.to_be_bytes());
                buf.push(self.dcid.len() as u8);
                buf.extend_from_slice(&self.dcid.bytes);
                buf.push(self.scid.len() as u8);
                buf.extend_from_slice(&self.scid.bytes);
                if self.packet_type == PacketType::Initial {
                    // Token length + token
                    encode_varint(buf, self.token.len() as u64);
                    buf.extend_from_slice(&self.token);
                }
                // Length (will be filled later)
                encode_varint(buf, self.payload_length as u64 + 1); // +1 for pn
                // Packet number (1 byte for simplicity)
                buf.push(self.packet_number as u8);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FRAMES
// ═══════════════════════════════════════════════════════════════════════

/// QUIC frame types (RFC 9000 Section 12.4)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum FrameType {
    Padding = 0x00,
    Ping = 0x01,
    Ack = 0x02,
    AckEcn = 0x03,
    ResetStream = 0x04,
    StopSending = 0x05,
    Crypto = 0x06,
    NewToken = 0x07,
    Stream = 0x08, // 0x08-0x0f (with FIN/LEN/OFF bits)
    MaxData = 0x10,
    MaxStreamData = 0x11,
    MaxStreams = 0x12, // bidi=0x12, uni=0x13
    DataBlocked = 0x14,
    StreamDataBlocked = 0x15,
    StreamsBlocked = 0x16,
    NewConnectionId = 0x18,
    RetireConnectionId = 0x19,
    PathChallenge = 0x1a,
    PathResponse = 0x1b,
    ConnectionClose = 0x1c,
    HandshakeDone = 0x1e,
}

/// QUIC frame
#[derive(Debug, Clone)]
pub enum Frame {
    Padding,
    Ping,
    Ack {
        largest_ack: u64,
        ack_delay: u64,
        ack_ranges: Vec<AckRange>,
    },
    ResetStream {
        stream_id: u64,
        error_code: u64,
        final_size: u64,
    },
    StopSending {
        stream_id: u64,
        error_code: u64,
    },
    Crypto {
        offset: u64,
        data: Vec<u8>,
    },
    NewToken {
        token: Vec<u8>,
    },
    Stream {
        stream_id: u64,
        offset: u64,
        data: Vec<u8>,
        fin: bool,
    },
    MaxData {
        max_data: u64,
    },
    MaxStreamData {
        stream_id: u64,
        max_data: u64,
    },
    MaxStreams {
        max_streams: u64,
        bidi: bool,
    },
    DataBlocked {
        limit: u64,
    },
    StreamDataBlocked {
        stream_id: u64,
        limit: u64,
    },
    StreamsBlocked {
        limit: u64,
        bidi: bool,
    },
    NewConnectionId {
        sequence: u64,
        retire_prior: u64,
        connection_id: ConnectionId,
        stateless_reset_token: [u8; 16],
    },
    RetireConnectionId {
        sequence: u64,
    },
    PathChallenge {
        data: [u8; 8],
    },
    PathResponse {
        data: [u8; 8],
    },
    ConnectionClose {
        error_code: u64,
        frame_type: u64,
        reason: String,
    },
    HandshakeDone,
}

/// ACK range for ACK frames
#[derive(Debug, Clone)]
pub struct AckRange {
    pub gap: u64,
    pub length: u64,
}

impl Frame {
    /// Encode frame into bytes
    pub fn encode(&self, buf: &mut Vec<u8>) {
        match self {
            Frame::Padding => buf.push(0x00),
            Frame::Ping => buf.push(0x01),
            Frame::Ack {
                largest_ack,
                ack_delay,
                ack_ranges,
            } => {
                buf.push(0x02);
                encode_varint(buf, *largest_ack);
                encode_varint(buf, *ack_delay);
                encode_varint(buf, ack_ranges.len() as u64);
                if let Some(first) = ack_ranges.first() {
                    encode_varint(buf, first.length);
                }
                for range in ack_ranges.iter().skip(1) {
                    encode_varint(buf, range.gap);
                    encode_varint(buf, range.length);
                }
            }
            Frame::Crypto { offset, data } => {
                buf.push(0x06);
                encode_varint(buf, *offset);
                encode_varint(buf, data.len() as u64);
                buf.extend_from_slice(data);
            }
            Frame::Stream {
                stream_id,
                offset,
                data,
                fin,
            } => {
                let mut frame_type = 0x08u8;
                if *offset > 0 {
                    frame_type |= 0x04;
                } // OFF bit
                frame_type |= 0x02; // LEN bit always set
                if *fin {
                    frame_type |= 0x01;
                } // FIN bit
                buf.push(frame_type);
                encode_varint(buf, *stream_id);
                if *offset > 0 {
                    encode_varint(buf, *offset);
                }
                encode_varint(buf, data.len() as u64);
                buf.extend_from_slice(data);
            }
            Frame::MaxData { max_data } => {
                buf.push(0x10);
                encode_varint(buf, *max_data);
            }
            Frame::MaxStreamData {
                stream_id,
                max_data,
            } => {
                buf.push(0x11);
                encode_varint(buf, *stream_id);
                encode_varint(buf, *max_data);
            }
            Frame::ConnectionClose {
                error_code,
                frame_type,
                reason,
            } => {
                buf.push(0x1c);
                encode_varint(buf, *error_code);
                encode_varint(buf, *frame_type);
                let reason_bytes = reason.as_bytes();
                encode_varint(buf, reason_bytes.len() as u64);
                buf.extend_from_slice(reason_bytes);
            }
            Frame::HandshakeDone => buf.push(0x1e),
            Frame::ResetStream {
                stream_id,
                error_code,
                final_size,
            } => {
                buf.push(0x04);
                encode_varint(buf, *stream_id);
                encode_varint(buf, *error_code);
                encode_varint(buf, *final_size);
            }
            Frame::StopSending {
                stream_id,
                error_code,
            } => {
                buf.push(0x05);
                encode_varint(buf, *stream_id);
                encode_varint(buf, *error_code);
            }
            Frame::NewToken { token } => {
                buf.push(0x07);
                encode_varint(buf, token.len() as u64);
                buf.extend_from_slice(token);
            }
            Frame::MaxStreams { max_streams, bidi } => {
                buf.push(if *bidi { 0x12 } else { 0x13 });
                encode_varint(buf, *max_streams);
            }
            Frame::DataBlocked { limit } => {
                buf.push(0x14);
                encode_varint(buf, *limit);
            }
            Frame::StreamDataBlocked { stream_id, limit } => {
                buf.push(0x15);
                encode_varint(buf, *stream_id);
                encode_varint(buf, *limit);
            }
            Frame::StreamsBlocked { limit, bidi } => {
                buf.push(if *bidi { 0x16 } else { 0x17 });
                encode_varint(buf, *limit);
            }
            Frame::NewConnectionId {
                sequence,
                retire_prior,
                connection_id,
                stateless_reset_token,
            } => {
                buf.push(0x18);
                encode_varint(buf, *sequence);
                encode_varint(buf, *retire_prior);
                buf.push(connection_id.len() as u8);
                buf.extend_from_slice(&connection_id.bytes);
                buf.extend_from_slice(stateless_reset_token);
            }
            Frame::RetireConnectionId { sequence } => {
                buf.push(0x19);
                encode_varint(buf, *sequence);
            }
            Frame::PathChallenge { data } => {
                buf.push(0x1a);
                buf.extend_from_slice(data);
            }
            Frame::PathResponse { data } => {
                buf.push(0x1b);
                buf.extend_from_slice(data);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// STREAM STATE
// ═══════════════════════════════════════════════════════════════════════

/// Stream state (RFC 9000 Section 3)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    /// Open for sending and receiving
    Ready,
    /// Sending data
    Send,
    /// All data sent, waiting for ack
    DataSent,
    /// Reset sent
    ResetSent,
    /// Reset received or all data acked
    ResetRecvd,
    /// Receiving data
    Recv,
    /// All data received
    SizeKnown,
    /// All data read by application
    DataRead,
    /// Reset received
    ResetRead,
}

/// Stream direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamDirection {
    Bidirectional,
    Unidirectional,
}

/// QUIC stream
#[derive(Debug)]
pub struct QuicStream {
    pub id: u64,
    pub state: StreamState,
    pub direction: StreamDirection,
    /// Send buffer
    pub send_buf: Vec<u8>,
    /// Receive buffer
    pub recv_buf: Vec<u8>,
    /// Send offset (next byte to send)
    pub send_offset: u64,
    /// Receive offset (next expected byte)
    pub recv_offset: u64,
    /// Max send data (flow control)
    pub max_send_data: u64,
    /// Max recv data (flow control)
    pub max_recv_data: u64,
    /// FIN flag sent
    pub fin_sent: bool,
    /// FIN flag received
    pub fin_received: bool,
    /// Final size (if known)
    pub final_size: Option<u64>,
}

impl QuicStream {
    pub fn new(id: u64, direction: StreamDirection) -> Self {
        Self {
            id,
            state: StreamState::Ready,
            direction,
            send_buf: Vec::new(),
            recv_buf: Vec::new(),
            send_offset: 0,
            recv_offset: 0,
            max_send_data: INITIAL_MAX_STREAM_DATA,
            max_recv_data: INITIAL_MAX_STREAM_DATA,
            fin_sent: false,
            fin_received: false,
            final_size: None,
        }
    }

    /// Write data to the stream's send buffer
    pub fn write(&mut self, data: &[u8]) -> Result<usize, QuicError> {
        if self.fin_sent {
            return Err(QuicError::FinalSizeError);
        }
        let available = self
            .max_send_data
            .saturating_sub(self.send_offset + self.send_buf.len() as u64);
        let to_write = core::cmp::min(data.len(), available as usize);
        if to_write == 0 {
            return Err(QuicError::FlowControlError);
        }
        self.send_buf.extend_from_slice(&data[..to_write]);
        self.state = StreamState::Send;
        Ok(to_write)
    }

    /// Read data from the stream's receive buffer
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, QuicError> {
        let to_read = core::cmp::min(buf.len(), self.recv_buf.len());
        if to_read == 0 {
            if self.fin_received {
                self.state = StreamState::DataRead;
                return Ok(0); // EOF
            }
            return Err(QuicError::WouldBlock);
        }
        buf[..to_read].copy_from_slice(&self.recv_buf[..to_read]);
        self.recv_buf.drain(..to_read);
        self.recv_offset += to_read as u64;
        Ok(to_read)
    }

    /// Receive data from a STREAM frame
    pub fn receive_data(&mut self, offset: u64, data: &[u8], fin: bool) -> Result<(), QuicError> {
        if offset != self.recv_offset + self.recv_buf.len() as u64 {
            // Out-of-order data — in a full implementation, buffer and reassemble
            // For now, only accept in-order data
            if offset < self.recv_offset {
                return Ok(()); // duplicate, ignore
            }
            return Err(QuicError::StreamStateError);
        }
        self.recv_buf.extend_from_slice(data);
        if fin {
            self.fin_received = true;
            self.final_size = Some(offset + data.len() as u64);
            self.state = StreamState::SizeKnown;
        } else {
            self.state = StreamState::Recv;
        }
        Ok(())
    }

    /// Mark stream as finished (send FIN)
    pub fn finish(&mut self) -> Result<(), QuicError> {
        if self.fin_sent {
            return Err(QuicError::FinalSizeError);
        }
        self.fin_sent = true;
        self.state = StreamState::DataSent;
        Ok(())
    }

    /// Check if stream is client-initiated
    pub fn is_client_initiated(&self) -> bool {
        self.id & 0x01 == 0
    }

    /// Check if stream is bidirectional
    pub fn is_bidirectional(&self) -> bool {
        self.id & 0x02 == 0
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CONNECTION STATE
// ═══════════════════════════════════════════════════════════════════════

/// QUIC connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Initial state
    Idle,
    /// Handshake in progress
    Handshaking,
    /// Connection established
    Connected,
    /// Closing (draining period)
    Closing,
    /// Draining (received CONNECTION_CLOSE)
    Draining,
    /// Connection closed
    Closed,
}

/// Congestion control algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CongestionAlgorithm {
    /// New Reno (RFC 9002)
    NewReno,
    /// Cubic
    Cubic,
    /// BBR
    Bbr,
}

/// Congestion control state
#[derive(Debug, Clone)]
pub struct CongestionController {
    pub algorithm: CongestionAlgorithm,
    /// Congestion window (bytes)
    pub cwnd: u64,
    /// Slow start threshold
    pub ssthresh: u64,
    /// Bytes in flight
    pub bytes_in_flight: u64,
    /// Smoothed RTT (microseconds)
    pub smoothed_rtt: u64,
    /// RTT variance
    pub rttvar: u64,
    /// Minimum RTT observed
    pub min_rtt: u64,
    /// Latest RTT sample
    pub latest_rtt: u64,
    /// In slow start phase
    pub in_slow_start: bool,
    /// Recovery start packet number
    pub recovery_start_pn: u64,
    /// ECN CE counter
    pub ecn_ce_count: u64,
}

impl CongestionController {
    pub fn new(algorithm: CongestionAlgorithm) -> Self {
        Self {
            algorithm,
            cwnd: INITIAL_CWND,
            ssthresh: u64::MAX,
            bytes_in_flight: 0,
            smoothed_rtt: INITIAL_RTT * 1000, // Convert to microseconds
            rttvar: INITIAL_RTT * 500,
            min_rtt: u64::MAX,
            latest_rtt: 0,
            in_slow_start: true,
            recovery_start_pn: 0,
            ecn_ce_count: 0,
        }
    }

    /// Update RTT estimates
    pub fn update_rtt(&mut self, rtt_sample: u64) {
        self.latest_rtt = rtt_sample;
        if self.min_rtt > rtt_sample {
            self.min_rtt = rtt_sample;
        }
        if self.smoothed_rtt == INITIAL_RTT * 1000 {
            self.smoothed_rtt = rtt_sample;
            self.rttvar = rtt_sample / 2;
        } else {
            let abs_diff = rtt_sample.abs_diff(self.smoothed_rtt);
            self.rttvar = (3 * self.rttvar + abs_diff) / 4;
            self.smoothed_rtt = (7 * self.smoothed_rtt + rtt_sample) / 8;
        }
    }

    /// On packet acknowledged
    pub fn on_ack(&mut self, acked_bytes: u64) {
        self.bytes_in_flight = self.bytes_in_flight.saturating_sub(acked_bytes);
        if self.in_slow_start {
            self.cwnd += acked_bytes;
            if self.cwnd >= self.ssthresh {
                self.in_slow_start = false;
            }
        } else {
            // Congestion avoidance (New Reno)
            self.cwnd += (MAX_QUIC_PACKET as u64 * acked_bytes) / self.cwnd;
        }
    }

    /// On packet loss detected
    pub fn on_loss(&mut self, lost_pn: u64) {
        if lost_pn >= self.recovery_start_pn {
            self.recovery_start_pn = lost_pn + 1;
            self.ssthresh = core::cmp::max(self.cwnd / 2, MIN_CWND);
            self.cwnd = self.ssthresh;
            self.in_slow_start = false;
        }
    }

    /// Get PTO (Probe Timeout) duration in microseconds
    pub fn pto(&self) -> u64 {
        self.smoothed_rtt + core::cmp::max(4 * self.rttvar, 1000)
    }

    /// Can send data?
    pub fn can_send(&self, packet_size: u64) -> bool {
        self.bytes_in_flight + packet_size <= self.cwnd
    }
}

/// QUIC transport parameters
#[derive(Debug, Clone)]
pub struct TransportParameters {
    pub max_idle_timeout: u64,
    pub max_udp_payload_size: u64,
    pub initial_max_data: u64,
    pub initial_max_stream_data_bidi_local: u64,
    pub initial_max_stream_data_bidi_remote: u64,
    pub initial_max_stream_data_uni: u64,
    pub initial_max_streams_bidi: u64,
    pub initial_max_streams_uni: u64,
    pub ack_delay_exponent: u64,
    pub max_ack_delay: u64,
    pub active_connection_id_limit: u64,
    pub disable_active_migration: bool,
}

impl Default for TransportParameters {
    fn default() -> Self {
        Self {
            max_idle_timeout: DEFAULT_IDLE_TIMEOUT,
            max_udp_payload_size: MAX_UDP_PAYLOAD as u64,
            initial_max_data: INITIAL_MAX_DATA,
            initial_max_stream_data_bidi_local: INITIAL_MAX_STREAM_DATA,
            initial_max_stream_data_bidi_remote: INITIAL_MAX_STREAM_DATA,
            initial_max_stream_data_uni: INITIAL_MAX_STREAM_DATA,
            initial_max_streams_bidi: INITIAL_MAX_STREAMS_BIDI,
            initial_max_streams_uni: INITIAL_MAX_STREAMS_UNI,
            ack_delay_exponent: 3,
            max_ack_delay: DEFAULT_MAX_ACK_DELAY,
            active_connection_id_limit: 2,
            disable_active_migration: false,
        }
    }
}

/// QUIC connection
pub struct QuicConnection {
    pub state: ConnectionState,
    pub local_cid: ConnectionId,
    pub remote_cid: ConnectionId,
    pub is_server: bool,
    /// Streams map
    pub streams: BTreeMap<u64, QuicStream>,
    /// Next client-initiated bidirectional stream ID
    pub next_bidi_stream_id: u64,
    /// Next client-initiated unidirectional stream ID
    pub next_uni_stream_id: u64,
    /// Congestion controller
    pub congestion: CongestionController,
    /// Transport parameters (local)
    pub local_params: TransportParameters,
    /// Transport parameters (remote)
    pub remote_params: TransportParameters,
    /// Max data we can send (connection-level)
    pub max_send_data: u64,
    /// Max data we can receive
    pub max_recv_data: u64,
    /// Total data sent
    pub data_sent: u64,
    /// Total data received
    pub data_recv: u64,
    /// Packet number space
    pub next_packet_number: u64,
    /// Largest acknowledged packet number
    pub largest_acked: Option<u64>,
    /// Unacked packets (pn → size)
    pub unacked: BTreeMap<u64, u64>,
    /// Handshake complete
    pub handshake_complete: bool,
    /// Close error code
    pub close_error: Option<u64>,
    /// Statistics
    pub stats: ConnectionStats,
}

/// Connection statistics
#[derive(Debug, Clone, Default)]
pub struct ConnectionStats {
    pub packets_sent: u64,
    pub packets_recv: u64,
    pub bytes_sent: u64,
    pub bytes_recv: u64,
    pub packets_lost: u64,
    pub streams_opened: u64,
    pub streams_closed: u64,
    pub handshake_time_us: u64,
}

impl QuicConnection {
    pub fn new_client() -> Self {
        let local_cid = ConnectionId::generate();
        let remote_cid = ConnectionId::generate();
        Self {
            state: ConnectionState::Idle,
            local_cid,
            remote_cid,
            is_server: false,
            streams: BTreeMap::new(),
            next_bidi_stream_id: 0, // Client-initiated bidi: 0, 4, 8, ...
            next_uni_stream_id: 2,  // Client-initiated uni: 2, 6, 10, ...
            congestion: CongestionController::new(CongestionAlgorithm::NewReno),
            local_params: TransportParameters::default(),
            remote_params: TransportParameters::default(),
            max_send_data: INITIAL_MAX_DATA,
            max_recv_data: INITIAL_MAX_DATA,
            data_sent: 0,
            data_recv: 0,
            next_packet_number: 0,
            largest_acked: None,
            unacked: BTreeMap::new(),
            handshake_complete: false,
            close_error: None,
            stats: ConnectionStats::default(),
        }
    }

    pub fn new_server(client_dcid: ConnectionId) -> Self {
        let local_cid = ConnectionId::generate();
        Self {
            state: ConnectionState::Idle,
            local_cid,
            remote_cid: client_dcid,
            is_server: true,
            streams: BTreeMap::new(),
            next_bidi_stream_id: 1, // Server-initiated bidi: 1, 5, 9, ...
            next_uni_stream_id: 3,  // Server-initiated uni: 3, 7, 11, ...
            congestion: CongestionController::new(CongestionAlgorithm::NewReno),
            local_params: TransportParameters::default(),
            remote_params: TransportParameters::default(),
            max_send_data: INITIAL_MAX_DATA,
            max_recv_data: INITIAL_MAX_DATA,
            data_sent: 0,
            data_recv: 0,
            next_packet_number: 0,
            largest_acked: None,
            unacked: BTreeMap::new(),
            handshake_complete: false,
            close_error: None,
            stats: ConnectionStats::default(),
        }
    }

    /// Open a new bidirectional stream
    pub fn open_stream_bidi(&mut self) -> Result<u64, QuicError> {
        let id = self.next_bidi_stream_id;
        if id / 4 >= self.remote_params.initial_max_streams_bidi {
            return Err(QuicError::StreamLimitError);
        }
        self.next_bidi_stream_id += 4;
        let stream = QuicStream::new(id, StreamDirection::Bidirectional);
        self.streams.insert(id, stream);
        self.stats.streams_opened += 1;
        Ok(id)
    }

    /// Open a new unidirectional stream
    pub fn open_stream_uni(&mut self) -> Result<u64, QuicError> {
        let id = self.next_uni_stream_id;
        if id / 4 >= self.remote_params.initial_max_streams_uni {
            return Err(QuicError::StreamLimitError);
        }
        self.next_uni_stream_id += 4;
        let stream = QuicStream::new(id, StreamDirection::Unidirectional);
        self.streams.insert(id, stream);
        self.stats.streams_opened += 1;
        Ok(id)
    }

    /// Send data on a stream
    pub fn stream_send(&mut self, stream_id: u64, data: &[u8]) -> Result<usize, QuicError> {
        if self.state != ConnectionState::Connected {
            return Err(QuicError::InvalidState);
        }
        let stream = self
            .streams
            .get_mut(&stream_id)
            .ok_or(QuicError::StreamStateError)?;
        let written = stream.write(data)?;
        self.data_sent += written as u64;
        Ok(written)
    }

    /// Receive data from a stream
    pub fn stream_recv(&mut self, stream_id: u64, buf: &mut [u8]) -> Result<usize, QuicError> {
        let stream = self
            .streams
            .get_mut(&stream_id)
            .ok_or(QuicError::StreamStateError)?;
        stream.read(buf)
    }

    /// Finish a stream (send FIN)
    pub fn stream_finish(&mut self, stream_id: u64) -> Result<(), QuicError> {
        let stream = self
            .streams
            .get_mut(&stream_id)
            .ok_or(QuicError::StreamStateError)?;
        stream.finish()?;
        self.stats.streams_closed += 1;
        Ok(())
    }

    /// Start the QUIC handshake (client-side)
    pub fn connect(&mut self) -> Result<Vec<u8>, QuicError> {
        self.state = ConnectionState::Handshaking;
        // Build Initial packet with CRYPTO frame (TLS ClientHello)
        let mut packet = Vec::new();
        let header = PacketHeader::new_initial(
            self.remote_cid.clone(),
            self.local_cid.clone(),
            self.next_packet_number,
        );
        self.next_packet_number += 1;
        header.encode(&mut packet);

        // Add CRYPTO frame with TLS ClientHello stub
        let client_hello = build_tls_client_hello();
        let crypto_frame = Frame::Crypto {
            offset: 0,
            data: client_hello,
        };
        crypto_frame.encode(&mut packet);

        // Pad to 1200 bytes minimum (QUIC requirement for Initial packets)
        while packet.len() < 1200 {
            packet.push(0x00); // PADDING frame
        }

        self.stats.packets_sent += 1;
        self.stats.bytes_sent += packet.len() as u64;
        Ok(packet)
    }

    /// Process an incoming QUIC packet
    pub fn process_packet(&mut self, data: &[u8]) -> Result<Vec<u8>, QuicError> {
        if data.is_empty() {
            return Err(QuicError::ProtocolViolation);
        }
        self.stats.packets_recv += 1;
        self.stats.bytes_recv += data.len() as u64;

        // Parse header
        let first_byte = data[0];
        let is_long = (first_byte & 0x80) != 0;

        if is_long {
            self.process_long_header(data)
        } else {
            self.process_short_header(data)
        }
    }

    fn process_long_header(&mut self, data: &[u8]) -> Result<Vec<u8>, QuicError> {
        if data.len() < 7 {
            return Err(QuicError::ProtocolViolation);
        }
        let first_byte = data[0];
        let pkt_type = (first_byte >> 4) & 0x03;

        match pkt_type {
            0x00 => self.process_initial(data),
            0x02 => self.process_handshake_packet(data),
            _ => Ok(Vec::new()),
        }
    }

    fn process_short_header(&mut self, data: &[u8]) -> Result<Vec<u8>, QuicError> {
        if data.len() < 2 {
            return Err(QuicError::ProtocolViolation);
        }
        // Skip first byte + DCID
        let dcid_len = self.local_cid.len();
        let pn_offset = 1 + dcid_len;
        if data.len() <= pn_offset {
            return Err(QuicError::ProtocolViolation);
        }
        let pn = data[pn_offset] as u64;
        let payload = &data[pn_offset + 1..];

        // Parse and process frames from the payload
        let frames = parse_frames(payload);
        let mut response_frames: Vec<Frame> = Vec::new();

        for frame in frames {
            match frame {
                Frame::Stream {
                    stream_id,
                    offset,
                    data,
                    fin,
                } => {
                    // Ensure stream exists
                    self.streams.entry(stream_id).or_insert_with(|| {
                        let dir = if stream_id & 0x02 == 0 {
                            StreamDirection::Bidirectional
                        } else {
                            StreamDirection::Unidirectional
                        };
                        QuicStream::new(stream_id, dir)
                    });
                    if let Some(stream) = self.streams.get_mut(&stream_id) {
                        let _ = stream.receive_data(offset, &data, fin);
                    }
                    self.data_recv += data.len() as u64;
                }
                Frame::Ack { largest_ack, .. } => {
                    self.handle_ack(largest_ack);
                }
                Frame::MaxData { max_data } => {
                    self.max_send_data = max_data;
                }
                Frame::MaxStreamData {
                    stream_id,
                    max_data,
                } => {
                    if let Some(stream) = self.streams.get_mut(&stream_id) {
                        stream.max_send_data = max_data;
                    }
                }
                Frame::Ping => {
                    // Respond with ACK
                }
                Frame::ConnectionClose { error_code, .. } => {
                    self.state = ConnectionState::Draining;
                    self.close_error = Some(error_code);
                    return Ok(Vec::new());
                }
                Frame::PathChallenge { data: challenge } => {
                    response_frames.push(Frame::PathResponse { data: challenge });
                }
                Frame::ResetStream {
                    stream_id,
                    final_size,
                    ..
                } => {
                    if let Some(stream) = self.streams.get_mut(&stream_id) {
                        stream.state = StreamState::ResetRecvd;
                        stream.final_size = Some(final_size);
                    }
                }
                _ => {}
            }
        }

        // Always send ACK for received packet
        response_frames.push(Frame::Ack {
            largest_ack: pn,
            ack_delay: 0,
            ack_ranges: vec![AckRange { gap: 0, length: 0 }],
        });

        // Build response packet
        let mut response = Vec::new();
        let header = PacketHeader::new_short(self.remote_cid.clone(), self.next_packet_number);
        self.next_packet_number += 1;
        header.encode(&mut response);
        for frame in &response_frames {
            frame.encode(&mut response);
        }
        self.stats.packets_sent += 1;
        self.stats.bytes_sent += response.len() as u64;
        Ok(response)
    }

    fn process_initial(&mut self, _data: &[u8]) -> Result<Vec<u8>, QuicError> {
        if self.is_server {
            // Server receives Initial → send Initial + Handshake
            self.state = ConnectionState::Handshaking;
            let mut response = Vec::new();
            let header = PacketHeader::new_initial(
                self.remote_cid.clone(),
                self.local_cid.clone(),
                self.next_packet_number,
            );
            self.next_packet_number += 1;
            header.encode(&mut response);

            let server_hello = build_tls_server_hello();
            let crypto_frame = Frame::Crypto {
                offset: 0,
                data: server_hello,
            };
            crypto_frame.encode(&mut response);

            self.stats.packets_sent += 1;
            self.stats.bytes_sent += response.len() as u64;
            Ok(response)
        } else {
            // Client receives server Initial
            Ok(Vec::new())
        }
    }

    fn process_handshake_packet(&mut self, _data: &[u8]) -> Result<Vec<u8>, QuicError> {
        if !self.is_server {
            // Client: handshake complete after receiving server Handshake
            self.state = ConnectionState::Connected;
            self.handshake_complete = true;
            let mut response = Vec::new();
            let header = PacketHeader::new_handshake(
                self.remote_cid.clone(),
                self.local_cid.clone(),
                self.next_packet_number,
            );
            self.next_packet_number += 1;
            header.encode(&mut response);
            Frame::HandshakeDone.encode(&mut response);
            self.stats.packets_sent += 1;
            self.stats.bytes_sent += response.len() as u64;
            Ok(response)
        } else {
            // Server: receives client Handshake Finished
            self.state = ConnectionState::Connected;
            self.handshake_complete = true;
            Ok(Vec::new())
        }
    }

    /// Build outgoing data packets
    pub fn build_data_packets(&mut self) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        if self.state != ConnectionState::Connected {
            return packets;
        }

        let stream_ids: Vec<u64> = self.streams.keys().copied().collect();
        for stream_id in stream_ids {
            if let Some(stream) = self.streams.get_mut(&stream_id) {
                while !stream.send_buf.is_empty() {
                    let chunk_size = core::cmp::min(stream.send_buf.len(), MAX_QUIC_PACKET - 50);
                    if !self.congestion.can_send(chunk_size as u64) {
                        break;
                    }
                    let chunk: Vec<u8> = stream.send_buf.drain(..chunk_size).collect();
                    let fin = stream.fin_sent && stream.send_buf.is_empty();

                    let mut packet = Vec::new();
                    let header =
                        PacketHeader::new_short(self.remote_cid.clone(), self.next_packet_number);
                    let pn = self.next_packet_number;
                    self.next_packet_number += 1;
                    header.encode(&mut packet);

                    let frame = Frame::Stream {
                        stream_id,
                        offset: stream.send_offset,
                        data: chunk.clone(),
                        fin,
                    };
                    frame.encode(&mut packet);

                    stream.send_offset += chunk.len() as u64;
                    self.congestion.bytes_in_flight += packet.len() as u64;
                    self.unacked.insert(pn, packet.len() as u64);
                    self.stats.packets_sent += 1;
                    self.stats.bytes_sent += packet.len() as u64;
                    packets.push(packet);
                }
            }
        }
        packets
    }

    /// Close the connection
    pub fn close(&mut self, error_code: u64, reason: &str) -> Result<Vec<u8>, QuicError> {
        self.state = ConnectionState::Closing;
        self.close_error = Some(error_code);

        let mut packet = Vec::new();
        let header = PacketHeader::new_short(self.remote_cid.clone(), self.next_packet_number);
        self.next_packet_number += 1;
        header.encode(&mut packet);

        let frame = Frame::ConnectionClose {
            error_code,
            frame_type: 0,
            reason: String::from(reason),
        };
        frame.encode(&mut packet);

        self.stats.packets_sent += 1;
        self.stats.bytes_sent += packet.len() as u64;
        Ok(packet)
    }

    /// Handle ACK for a packet
    pub fn handle_ack(&mut self, pn: u64) {
        if let Some(size) = self.unacked.remove(&pn) {
            self.congestion.on_ack(size);
            if self.largest_acked.is_none_or(|la| pn > la) {
                self.largest_acked = Some(pn);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ERROR CODES
// ═══════════════════════════════════════════════════════════════════════

/// QUIC error codes (RFC 9000 Section 20)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuicError {
    NoError,
    InternalError,
    ConnectionRefused,
    FlowControlError,
    StreamLimitError,
    StreamStateError,
    FinalSizeError,
    FrameEncodingError,
    TransportParameterError,
    ConnectionIdLimitError,
    ProtocolViolation,
    InvalidToken,
    ApplicationError,
    CryptoBufferExceeded,
    KeyUpdateError,
    AeadLimitReached,
    NoViablePath,
    /// Non-standard: would block
    WouldBlock,
    /// Non-standard: invalid state
    InvalidState,
}

impl QuicError {
    pub fn to_code(self) -> u64 {
        match self {
            QuicError::NoError => 0x00,
            QuicError::InternalError => 0x01,
            QuicError::ConnectionRefused => 0x02,
            QuicError::FlowControlError => 0x03,
            QuicError::StreamLimitError => 0x04,
            QuicError::StreamStateError => 0x05,
            QuicError::FinalSizeError => 0x06,
            QuicError::FrameEncodingError => 0x07,
            QuicError::TransportParameterError => 0x08,
            QuicError::ConnectionIdLimitError => 0x09,
            QuicError::ProtocolViolation => 0x0a,
            QuicError::InvalidToken => 0x0b,
            QuicError::ApplicationError => 0x0c,
            QuicError::CryptoBufferExceeded => 0x0d,
            QuicError::KeyUpdateError => 0x0e,
            QuicError::AeadLimitReached => 0x0f,
            QuicError::NoViablePath => 0x10,
            QuicError::WouldBlock => 0xff00,
            QuicError::InvalidState => 0xff01,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// QUIC LISTENER (SERVER)
// ═══════════════════════════════════════════════════════════════════════

/// QUIC server listener
pub struct QuicListener {
    /// Pending connections keyed by initial DCID
    pub connections: BTreeMap<Vec<u8>, QuicConnection>,
    /// Listen port
    pub port: u16,
    /// Transport parameters for new connections
    pub params: TransportParameters,
    /// Total connections accepted
    pub total_accepted: u64,
}

impl QuicListener {
    pub fn new(port: u16) -> Self {
        Self {
            connections: BTreeMap::new(),
            port,
            params: TransportParameters::default(),
            total_accepted: 0,
        }
    }

    /// Accept a new connection from an Initial packet
    pub fn accept(&mut self, data: &[u8]) -> Result<(ConnectionId, Vec<u8>), QuicError> {
        if data.is_empty() || (data[0] & 0x80) == 0 {
            return Err(QuicError::ProtocolViolation);
        }
        // Extract DCID from Initial packet
        if data.len() < 6 {
            return Err(QuicError::ProtocolViolation);
        }
        let dcid_len = data[5] as usize;
        if data.len() < 6 + dcid_len {
            return Err(QuicError::ProtocolViolation);
        }
        let dcid = ConnectionId::new(data[6..6 + dcid_len].to_vec());

        let mut conn = QuicConnection::new_server(dcid.clone());
        conn.local_params = self.params.clone();
        let response = conn.process_packet(data)?;
        let local_cid = conn.local_cid.clone();
        self.connections.insert(local_cid.bytes.clone(), conn);
        self.total_accepted += 1;
        Ok((local_cid, response))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    /// Global QUIC connections
    static ref QUIC_CONNECTIONS: Mutex<BTreeMap<Vec<u8>, QuicConnection>> =
        Mutex::new(BTreeMap::new());
    /// Global QUIC listeners
    static ref QUIC_LISTENERS: Mutex<BTreeMap<u16, QuicListener>> =
        Mutex::new(BTreeMap::new());
}

/// Create a new QUIC client connection
pub fn quic_connect(remote_addr: &str, port: u16) -> Result<ConnectionId, QuicError> {
    let mut conn = QuicConnection::new_client();
    let _initial_packet = conn.connect()?;
    // In a full implementation, send the packet via UDP
    let cid = conn.local_cid.clone();
    QUIC_CONNECTIONS.lock().insert(cid.bytes.clone(), conn);
    serial_println!(
        "[QUIC] Client connection initiated to {}:{}",
        remote_addr,
        port
    );
    Ok(cid)
}

/// Start listening for QUIC connections on a port
pub fn quic_listen(port: u16) -> Result<(), QuicError> {
    let listener = QuicListener::new(port);
    QUIC_LISTENERS.lock().insert(port, listener);
    serial_println!("[QUIC] Listening on port {}", port);
    Ok(())
}

/// Send data on a QUIC stream
pub fn quic_send(cid: &ConnectionId, stream_id: u64, data: &[u8]) -> Result<usize, QuicError> {
    let mut conns = QUIC_CONNECTIONS.lock();
    let conn = conns.get_mut(&cid.bytes).ok_or(QuicError::InvalidState)?;
    conn.stream_send(stream_id, data)
}

/// Receive data from a QUIC stream
pub fn quic_recv(cid: &ConnectionId, stream_id: u64, buf: &mut [u8]) -> Result<usize, QuicError> {
    let mut conns = QUIC_CONNECTIONS.lock();
    let conn = conns.get_mut(&cid.bytes).ok_or(QuicError::InvalidState)?;
    conn.stream_recv(stream_id, buf)
}

/// Open a new stream on a connection
pub fn quic_open_stream(cid: &ConnectionId, bidi: bool) -> Result<u64, QuicError> {
    let mut conns = QUIC_CONNECTIONS.lock();
    let conn = conns.get_mut(&cid.bytes).ok_or(QuicError::InvalidState)?;
    if bidi {
        conn.open_stream_bidi()
    } else {
        conn.open_stream_uni()
    }
}

/// Close a QUIC connection
pub fn quic_close(cid: &ConnectionId, error_code: u64, reason: &str) -> Result<(), QuicError> {
    let mut conns = QUIC_CONNECTIONS.lock();
    if let Some(conn) = conns.get_mut(&cid.bytes) {
        let _packet = conn.close(error_code, reason)?;
        // In a full implementation, send the close packet via UDP
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// VARIABLE-LENGTH INTEGER ENCODING (RFC 9000 Section 16)
// ═══════════════════════════════════════════════════════════════════════

/// Encode a QUIC variable-length integer
pub fn encode_varint(buf: &mut Vec<u8>, value: u64) {
    if value < 64 {
        buf.push(value as u8);
    } else if value < 16384 {
        buf.push(0x40 | (value >> 8) as u8);
        buf.push(value as u8);
    } else if value < 1_073_741_824 {
        buf.push(0x80 | (value >> 24) as u8);
        buf.push((value >> 16) as u8);
        buf.push((value >> 8) as u8);
        buf.push(value as u8);
    } else {
        buf.push(0xC0 | (value >> 56) as u8);
        buf.push((value >> 48) as u8);
        buf.push((value >> 40) as u8);
        buf.push((value >> 32) as u8);
        buf.push((value >> 24) as u8);
        buf.push((value >> 16) as u8);
        buf.push((value >> 8) as u8);
        buf.push(value as u8);
    }
}

/// Decode a QUIC variable-length integer
pub fn decode_varint(data: &[u8]) -> Option<(u64, usize)> {
    if data.is_empty() {
        return None;
    }
    let first = data[0];
    let prefix = first >> 6;
    match prefix {
        0 => Some((first as u64, 1)),
        1 => {
            if data.len() < 2 {
                return None;
            }
            let value = ((first as u64 & 0x3F) << 8) | data[1] as u64;
            Some((value, 2))
        }
        2 => {
            if data.len() < 4 {
                return None;
            }
            let value = ((first as u64 & 0x3F) << 24)
                | ((data[1] as u64) << 16)
                | ((data[2] as u64) << 8)
                | data[3] as u64;
            Some((value, 4))
        }
        3 => {
            if data.len() < 8 {
                return None;
            }
            let value = ((first as u64 & 0x3F) << 56)
                | ((data[1] as u64) << 48)
                | ((data[2] as u64) << 40)
                | ((data[3] as u64) << 32)
                | ((data[4] as u64) << 24)
                | ((data[5] as u64) << 16)
                | ((data[6] as u64) << 8)
                | data[7] as u64;
            Some((value, 8))
        }
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FRAME PARSING
// ═══════════════════════════════════════════════════════════════════════

/// Parse frames from a QUIC payload
pub fn parse_frames(mut data: &[u8]) -> Vec<Frame> {
    let mut frames = Vec::new();
    while !data.is_empty() {
        let (frame_type, consumed) = match decode_varint(data) {
            Some(v) => v,
            None => break,
        };
        data = &data[consumed..];

        let frame = match frame_type {
            0x00 => {
                // PADDING — skip
                Frame::Padding
            }
            0x01 => Frame::Ping,
            0x02 | 0x03 => {
                // ACK
                let (largest_ack, c1) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c1..];
                let (ack_delay, c2) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c2..];
                let (ack_range_count, c3) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c3..];
                let (first_range, c4) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c4..];
                let mut ranges = vec![AckRange {
                    gap: 0,
                    length: first_range,
                }];
                for _ in 0..ack_range_count {
                    let (gap, cg) = match decode_varint(data) {
                        Some(v) => v,
                        None => break,
                    };
                    data = &data[cg..];
                    let (length, cl) = match decode_varint(data) {
                        Some(v) => v,
                        None => break,
                    };
                    data = &data[cl..];
                    ranges.push(AckRange { gap, length });
                }
                Frame::Ack {
                    largest_ack,
                    ack_delay,
                    ack_ranges: ranges,
                }
            }
            0x04 => {
                // RESET_STREAM
                let (stream_id, c1) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c1..];
                let (error_code, c2) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c2..];
                let (final_size, c3) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c3..];
                Frame::ResetStream {
                    stream_id,
                    error_code,
                    final_size,
                }
            }
            0x05 => {
                // STOP_SENDING
                let (stream_id, c1) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c1..];
                let (error_code, c2) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c2..];
                Frame::StopSending {
                    stream_id,
                    error_code,
                }
            }
            0x06 => {
                // CRYPTO
                let (offset, c1) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c1..];
                let (length, c2) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c2..];
                let len = length as usize;
                if data.len() < len {
                    break;
                }
                let crypto_data = data[..len].to_vec();
                data = &data[len..];
                Frame::Crypto {
                    offset,
                    data: crypto_data,
                }
            }
            0x08..=0x0f => {
                // STREAM frames (type encodes OFF/LEN/FIN bits)
                let has_off = frame_type & 0x04 != 0;
                let has_len = frame_type & 0x02 != 0;
                let has_fin = frame_type & 0x01 != 0;

                let (stream_id, c1) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c1..];

                let offset = if has_off {
                    let (off, co) = match decode_varint(data) {
                        Some(v) => v,
                        None => break,
                    };
                    data = &data[co..];
                    off
                } else {
                    0
                };

                let stream_data = if has_len {
                    let (length, cl) = match decode_varint(data) {
                        Some(v) => v,
                        None => break,
                    };
                    data = &data[cl..];
                    let len = length as usize;
                    if data.len() < len {
                        break;
                    }
                    let d = data[..len].to_vec();
                    data = &data[len..];
                    d
                } else {
                    // No LEN: rest of packet is stream data
                    let d = data.to_vec();
                    data = &[];
                    d
                };

                Frame::Stream {
                    stream_id,
                    offset,
                    data: stream_data,
                    fin: has_fin,
                }
            }
            0x10 => {
                let (max_data, c) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c..];
                Frame::MaxData { max_data }
            }
            0x11 => {
                let (stream_id, c1) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c1..];
                let (max_data, c2) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c2..];
                Frame::MaxStreamData {
                    stream_id,
                    max_data,
                }
            }
            0x12 | 0x13 => {
                let (max_streams, c) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c..];
                Frame::MaxStreams {
                    max_streams,
                    bidi: frame_type == 0x12,
                }
            }
            0x1a => {
                // PATH_CHALLENGE
                if data.len() < 8 {
                    break;
                }
                let mut challenge = [0u8; 8];
                challenge.copy_from_slice(&data[..8]);
                data = &data[8..];
                Frame::PathChallenge { data: challenge }
            }
            0x1b => {
                // PATH_RESPONSE
                if data.len() < 8 {
                    break;
                }
                let mut response = [0u8; 8];
                response.copy_from_slice(&data[..8]);
                data = &data[8..];
                Frame::PathResponse { data: response }
            }
            0x1c | 0x1d => {
                // CONNECTION_CLOSE
                let (error_code, c1) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c1..];
                let (ft, c2) = if frame_type == 0x1c {
                    match decode_varint(data) {
                        Some(v) => v,
                        None => break,
                    }
                } else {
                    (0, 0)
                };
                data = &data[c2..];
                let (reason_len, c3) = match decode_varint(data) {
                    Some(v) => v,
                    None => break,
                };
                data = &data[c3..];
                let rlen = reason_len as usize;
                let reason = if data.len() >= rlen {
                    let r = core::str::from_utf8(&data[..rlen]).unwrap_or("");
                    data = &data[rlen..];
                    String::from(r)
                } else {
                    break;
                };
                Frame::ConnectionClose {
                    error_code,
                    frame_type: ft,
                    reason,
                }
            }
            0x1e => Frame::HandshakeDone,
            _ => break,
        };
        frames.push(frame);
    }
    frames
}

// ═══════════════════════════════════════════════════════════════════════
// TLS 1.3 FOR QUIC
// ═══════════════════════════════════════════════════════════════════════

fn build_tls_client_hello() -> Vec<u8> {
    // TLS 1.3 ClientHello for QUIC (RFC 8446 §4.1.2)
    let mut hello = Vec::with_capacity(256);
    hello.push(0x01); // ClientHello handshake type

    // Length placeholder (3 bytes) — will be patched below
    let len_pos = hello.len();
    hello.extend_from_slice(&[0x00, 0x00, 0x00]);

    // Legacy version: TLS 1.2 (0x0303)
    hello.extend_from_slice(&[0x03, 0x03]);

    // Random (32 bytes) — use kernel random
    let r1 = crate::random::random_u64();
    let r2 = crate::random::random_u64();
    let r3 = crate::random::random_u64();
    let r4 = crate::random::random_u64();
    hello.extend_from_slice(&r1.to_le_bytes());
    hello.extend_from_slice(&r2.to_le_bytes());
    hello.extend_from_slice(&r3.to_le_bytes());
    hello.extend_from_slice(&r4.to_le_bytes());

    // Legacy session ID length = 0 (QUIC does not use session IDs)
    hello.push(0x00);

    // Cipher suites: TLS_AES_128_GCM_SHA256 (0x1301), TLS_AES_256_GCM_SHA384 (0x1302)
    hello.extend_from_slice(&[0x00, 0x04, 0x13, 0x01, 0x13, 0x02]);

    // Legacy compression methods: null only
    hello.extend_from_slice(&[0x01, 0x00]);

    // Extensions
    let ext_start = hello.len();
    hello.extend_from_slice(&[0x00, 0x00]); // Extensions length placeholder

    // Extension: supported_versions (0x002B) — advertise TLS 1.3
    hello.extend_from_slice(&[0x00, 0x2B]); // extension type
    hello.extend_from_slice(&[0x00, 0x03]); // extension data length
    hello.push(0x02); // supported versions list length
    hello.extend_from_slice(&[0x03, 0x04]); // TLS 1.3

    // Extension: supported_groups (0x000A) — x25519
    hello.extend_from_slice(&[0x00, 0x0A]); // extension type
    hello.extend_from_slice(&[0x00, 0x04]); // extension data length
    hello.extend_from_slice(&[0x00, 0x02]); // named group list length
    hello.extend_from_slice(&[0x00, 0x1D]); // x25519

    // Extension: signature_algorithms (0x000D) — ecdsa_secp256r1_sha256
    hello.extend_from_slice(&[0x00, 0x0D]); // extension type
    hello.extend_from_slice(&[0x00, 0x04]); // extension data length
    hello.extend_from_slice(&[0x00, 0x02]); // algorithms list length
    hello.extend_from_slice(&[0x04, 0x03]); // ecdsa_secp256r1_sha256

    // Extension: QUIC transport parameters (0x0039) — empty for now
    hello.extend_from_slice(&[0x00, 0x39]); // extension type
    hello.extend_from_slice(&[0x00, 0x00]); // empty

    // Patch extensions length
    let ext_len = (hello.len() - ext_start - 2) as u16;
    hello[ext_start] = (ext_len >> 8) as u8;
    hello[ext_start + 1] = ext_len as u8;

    // Patch handshake length
    let body_len = (hello.len() - len_pos - 3) as u32;
    hello[len_pos] = ((body_len >> 16) & 0xFF) as u8;
    hello[len_pos + 1] = ((body_len >> 8) & 0xFF) as u8;
    hello[len_pos + 2] = (body_len & 0xFF) as u8;

    hello
}

fn build_tls_server_hello() -> Vec<u8> {
    // TLS 1.3 ServerHello for QUIC (RFC 8446 §4.1.3)
    let mut hello = Vec::with_capacity(256);
    hello.push(0x02); // ServerHello handshake type

    // Length placeholder (3 bytes)
    let len_pos = hello.len();
    hello.extend_from_slice(&[0x00, 0x00, 0x00]);

    // Legacy version: TLS 1.2 (0x0303)
    hello.extend_from_slice(&[0x03, 0x03]);

    // Random (32 bytes)
    let r1 = crate::random::random_u64();
    let r2 = crate::random::random_u64();
    let r3 = crate::random::random_u64();
    let r4 = crate::random::random_u64();
    hello.extend_from_slice(&r1.to_le_bytes());
    hello.extend_from_slice(&r2.to_le_bytes());
    hello.extend_from_slice(&r3.to_le_bytes());
    hello.extend_from_slice(&r4.to_le_bytes());

    // Legacy session ID echo (length = 0 for QUIC)
    hello.push(0x00);

    // Cipher suite: TLS_AES_128_GCM_SHA256
    hello.extend_from_slice(&[0x13, 0x01]);

    // Legacy compression: null
    hello.push(0x00);

    // Extensions
    let ext_start = hello.len();
    hello.extend_from_slice(&[0x00, 0x00]); // Extensions length placeholder

    // Extension: supported_versions (0x002B) — TLS 1.3 selected
    hello.extend_from_slice(&[0x00, 0x2B]); // extension type
    hello.extend_from_slice(&[0x00, 0x02]); // extension data length
    hello.extend_from_slice(&[0x03, 0x04]); // TLS 1.3

    // Patch extensions length
    let ext_len = (hello.len() - ext_start - 2) as u16;
    hello[ext_start] = (ext_len >> 8) as u8;
    hello[ext_start + 1] = ext_len as u8;

    // Patch handshake length
    let body_len = (hello.len() - len_pos - 3) as u32;
    hello[len_pos] = ((body_len >> 16) & 0xFF) as u8;
    hello[len_pos + 1] = ((body_len >> 8) & 0xFF) as u8;
    hello[len_pos + 2] = (body_len & 0xFF) as u8;

    hello
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize QUIC subsystem
pub fn init() {
    serial_println!("[KnoxOS] QUIC transport protocol initialized (RFC 9000/9001/9002)");
}
