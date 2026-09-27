use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::cid::ConnectionId;
use super::congestion::{CongestionAlgorithm, CongestionController};
use super::constants::{
    DEFAULT_IDLE_TIMEOUT, DEFAULT_MAX_ACK_DELAY, INITIAL_MAX_DATA, INITIAL_MAX_STREAM_DATA,
    INITIAL_MAX_STREAMS_BIDI, INITIAL_MAX_STREAMS_UNI, MAX_QUIC_PACKET, MAX_UDP_PAYLOAD,
};
use super::error::QuicError;
use super::frame::{AckRange, Frame, parse_frames};
use super::packet::PacketHeader;
use super::stream::{QuicStream, StreamDirection, StreamState};
use super::tls::{build_tls_client_hello, build_tls_server_hello};

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
