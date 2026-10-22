use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

use super::cid::ConnectionId;
use super::connection::QuicConnection;
use super::error::QuicError;
use super::listener::QuicListener;

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
