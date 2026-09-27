use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::cid::ConnectionId;
use super::connection::{QuicConnection, TransportParameters};
use super::error::QuicError;

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
