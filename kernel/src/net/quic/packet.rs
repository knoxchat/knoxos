use alloc::vec::Vec;

use super::cid::ConnectionId;
use super::constants::QUIC_VERSION_1;
use super::varint::encode_varint;

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
