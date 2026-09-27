use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::cid::ConnectionId;
use super::varint::{decode_varint, encode_varint};

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
