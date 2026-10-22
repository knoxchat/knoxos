use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

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
