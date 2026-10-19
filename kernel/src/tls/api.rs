//! Global TLS session table and the id-based public session API.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use super::session::{TlsError, TlsSession, TlsState};
use super::x509::CertificateStore;

lazy_static::lazy_static! {
    /// Global TLS session table
    pub(super) static ref TLS_SESSIONS: Mutex<BTreeMap<u64, TlsSession>> = Mutex::new(BTreeMap::new());
    /// Certificate store
    pub(super) static ref CERT_STORE: Mutex<CertificateStore> = Mutex::new(CertificateStore::new());
}

// ─── Public API ────────────────────────────────────────────────────────

/// Create a new TLS client session
pub fn create_client_session(server_name: Option<&str>) -> u64 {
    let session = TlsSession::new_client(server_name);
    let id = session.id;
    TLS_SESSIONS.lock().insert(id, session);
    id
}

/// Create a new TLS server session
pub fn create_server_session() -> u64 {
    let session = TlsSession::new_server();
    let id = session.id;
    TLS_SESSIONS.lock().insert(id, session);
    id
}

/// Perform TLS handshake step (returns data to send to peer)
pub fn handshake(session_id: u64) -> Option<Vec<u8>> {
    let mut sessions = TLS_SESSIONS.lock();
    let session = sessions.get_mut(&session_id)?;

    match session.state {
        TlsState::Init if session.is_client => Some(session.build_client_hello()),
        TlsState::ClientHelloSent if !session.is_client => Some(session.build_server_hello()),
        TlsState::ServerHelloReceived => {
            session.derive_hs_traffic();
            None
        }
        _ => None,
    }
}

/// Process incoming TLS data
pub fn process(session_id: u64, data: &[u8]) -> Result<Vec<u8>, TlsError> {
    let mut sessions = TLS_SESSIONS.lock();
    let session = sessions
        .get_mut(&session_id)
        .ok_or(TlsError::InternalError)?;
    session.process_record(data)
}

/// Send application data over TLS
pub fn send(session_id: u64, data: &[u8]) -> Result<Vec<u8>, TlsError> {
    let mut sessions = TLS_SESSIONS.lock();
    let session = sessions
        .get_mut(&session_id)
        .ok_or(TlsError::InternalError)?;
    session.send(data)
}

/// Close a TLS session
pub fn close_session(session_id: u64) -> Option<Vec<u8>> {
    let mut sessions = TLS_SESSIONS.lock();
    let session = sessions.get_mut(&session_id)?;
    let alert = session.close();
    Some(alert)
}

/// Destroy a TLS session
pub fn destroy_session(session_id: u64) {
    TLS_SESSIONS.lock().remove(&session_id);
}
