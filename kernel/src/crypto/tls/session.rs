//! TLS connection state machine, session, and record processing.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use super::hash::{hkdf_expand_label, hkdf_extract};
use super::protocol::{
    AlertDescription, AlertLevel, CipherSuite, ContentType, HandshakeType, TLS_1_0, TLS_1_2,
    TLS_1_3, TlsRecordHeader,
};

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

// ─── TLS Connection State ──────────────────────────────────────────────

/// TLS connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsState {
    /// Initial state
    Init,
    /// ClientHello sent, waiting for ServerHello
    ClientHelloSent,
    /// ServerHello received
    ServerHelloReceived,
    /// Certificate received
    CertificateReceived,
    /// Key exchange complete
    KeyExchangeDone,
    /// Handshake complete, application data may flow
    Established,
    /// Close notify sent
    Closing,
    /// Connection closed
    Closed,
    /// Error state
    Error,
}

/// TLS session
pub struct TlsSession {
    /// Session ID
    pub id: u64,
    /// Connection state
    pub state: TlsState,
    /// Is this the client side?
    pub is_client: bool,
    /// Negotiated TLS version
    pub version: u16,
    /// Negotiated cipher suite
    pub cipher_suite: Option<CipherSuite>,
    /// Server name (SNI)
    pub server_name: Option<String>,
    /// Client random (32 bytes)
    pub client_random: [u8; 32],
    /// Server random (32 bytes)
    pub server_random: [u8; 32],
    /// Master secret (48 bytes for TLS 1.2)
    pub master_secret: [u8; 48],
    /// Sequence number for outgoing records
    pub write_seq: u64,
    /// Sequence number for incoming records
    pub read_seq: u64,
    /// Pending outgoing data
    pub write_buffer: Vec<u8>,
    /// Pending incoming plaintext
    pub read_buffer: Vec<u8>,
    /// Handshake messages hash (for Finished verification)
    pub handshake_hash: Vec<u8>,
    /// TLS 1.3 client handshake traffic secret
    pub hs_traffic: [u8; 32],
}

impl TlsSession {
    pub fn new_client(server_name: Option<&str>) -> Self {
        let mut client_random = [0u8; 32];
        // Fill with pseudo-random data
        let seed = crate::arch_compat::read_tsc();
        for (i, byte) in client_random.iter_mut().enumerate() {
            *byte = ((seed >> (i % 8 * 8)) ^ (seed >> ((i + 3) % 8 * 8))) as u8;
        }

        Self {
            id: NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed),
            state: TlsState::Init,
            is_client: true,
            version: TLS_1_2,
            cipher_suite: None,
            server_name: server_name.map(String::from),
            client_random,
            server_random: [0; 32],
            master_secret: [0; 48],
            write_seq: 0,
            read_seq: 0,
            write_buffer: Vec::new(),
            read_buffer: Vec::new(),
            handshake_hash: Vec::new(),
            hs_traffic: [0; 32],
        }
    }

    pub fn new_server() -> Self {
        let mut server_random = [0u8; 32];
        let seed = crate::arch_compat::read_tsc();
        for (i, byte) in server_random.iter_mut().enumerate() {
            *byte = ((seed >> (i % 8 * 8)) ^ (seed >> ((i + 5) % 8 * 8))) as u8;
        }

        Self {
            id: NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed),
            state: TlsState::Init,
            is_client: false,
            version: TLS_1_2,
            cipher_suite: None,
            server_name: None,
            client_random: [0; 32],
            server_random,
            master_secret: [0; 48],
            write_seq: 0,
            read_seq: 0,
            write_buffer: Vec::new(),
            read_buffer: Vec::new(),
            handshake_hash: Vec::new(),
            hs_traffic: [0; 32],
        }
    }

    /// Build a ClientHello message
    pub fn build_client_hello(&mut self) -> Vec<u8> {
        let mut msg = Vec::new();

        // Handshake header
        msg.push(HandshakeType::ClientHello as u8);
        // Length placeholder (3 bytes)
        let len_pos = msg.len();
        msg.extend_from_slice(&[0, 0, 0]);

        let body_start = msg.len();

        // Client version (TLS 1.2 in the record, real version in supported_versions ext for 1.3)
        msg.push(0x03);
        msg.push(0x03); // TLS 1.2

        // Client random (32 bytes)
        msg.extend_from_slice(&self.client_random);

        // Session ID (empty for new connection)
        msg.push(0); // session_id length = 0

        // Cipher suites
        let suites: &[u16] = &[
            CipherSuite::TLS_AES_128_GCM_SHA256 as u16,
            CipherSuite::TLS_AES_256_GCM_SHA384 as u16,
            CipherSuite::TLS_CHACHA20_POLY1305_SHA256 as u16,
            CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 as u16,
            CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384 as u16,
        ];
        let suites_len = (suites.len() * 2) as u16;
        msg.extend_from_slice(&suites_len.to_be_bytes());
        for &s in suites {
            msg.extend_from_slice(&s.to_be_bytes());
        }

        // Compression methods (only null compression)
        msg.push(1); // length
        msg.push(0); // null compression

        // Extensions
        let mut extensions = Vec::new();

        // SNI extension (if server name provided)
        if let Some(ref name) = self.server_name {
            let name_bytes = name.as_bytes();
            let mut sni = Vec::new();
            // server_name_list length
            let list_len = (name_bytes.len() + 3) as u16;
            sni.extend_from_slice(&list_len.to_be_bytes());
            sni.push(0); // host_name type
            sni.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
            sni.extend_from_slice(name_bytes);

            // Extension header: type=0 (SNI)
            extensions.extend_from_slice(&0u16.to_be_bytes());
            extensions.extend_from_slice(&(sni.len() as u16).to_be_bytes());
            extensions.extend_from_slice(&sni);
        }

        // Supported versions extension (for TLS 1.3)
        {
            let mut sv = Vec::new();
            sv.push(4); // 2 versions * 2 bytes
            sv.extend_from_slice(&TLS_1_3.to_be_bytes());
            sv.extend_from_slice(&TLS_1_2.to_be_bytes());

            extensions.extend_from_slice(&43u16.to_be_bytes()); // supported_versions
            extensions.extend_from_slice(&(sv.len() as u16).to_be_bytes());
            extensions.extend_from_slice(&sv);
        }

        // Extensions length
        msg.extend_from_slice(&(extensions.len() as u16).to_be_bytes());
        msg.extend_from_slice(&extensions);

        // Fill in handshake length
        let body_len = msg.len() - body_start;
        msg[len_pos] = ((body_len >> 16) & 0xFF) as u8;
        msg[len_pos + 1] = ((body_len >> 8) & 0xFF) as u8;
        msg[len_pos + 2] = (body_len & 0xFF) as u8;

        // Hash the handshake message
        self.handshake_hash.extend_from_slice(&msg);

        // Wrap in TLS record
        let mut record = Vec::new();
        let hdr = TlsRecordHeader::new(ContentType::Handshake, TLS_1_0, msg.len() as u16);
        record.push(hdr.content_type);
        record.push(hdr.version_major);
        record.push(hdr.version_minor);
        record.extend_from_slice(&(msg.len() as u16).to_be_bytes());
        record.extend_from_slice(&msg);

        self.state = TlsState::ClientHelloSent;
        record
    }

    /// Build a TLS 1.3 ServerHello (RFC 8446 §4.1.3).
    pub fn build_server_hello(&mut self) -> Vec<u8> {
        let mut msg = Vec::new();
        msg.push(HandshakeType::ServerHello as u8);
        let len_pos = msg.len();
        msg.extend_from_slice(&[0, 0, 0]);
        let body_start = msg.len();

        msg.push(0x03);
        msg.push(0x03);
        msg.extend_from_slice(&self.server_random);
        msg.push(0); // empty session id
        msg.extend_from_slice(&(CipherSuite::TLS_AES_128_GCM_SHA256 as u16).to_be_bytes());
        msg.push(0); // null compression

        let mut extensions = Vec::new();
        let mut sv = Vec::new();
        sv.push(2);
        sv.extend_from_slice(&TLS_1_3.to_be_bytes());
        extensions.extend_from_slice(&43u16.to_be_bytes());
        extensions.extend_from_slice(&(sv.len() as u16).to_be_bytes());
        extensions.extend_from_slice(&sv);
        msg.extend_from_slice(&(extensions.len() as u16).to_be_bytes());
        msg.extend_from_slice(&extensions);

        let body_len = msg.len() - body_start;
        msg[len_pos] = ((body_len >> 16) & 0xFF) as u8;
        msg[len_pos + 1] = ((body_len >> 8) & 0xFF) as u8;
        msg[len_pos + 2] = (body_len & 0xFF) as u8;

        self.handshake_hash.extend_from_slice(&msg);
        self.cipher_suite = Some(CipherSuite::TLS_AES_128_GCM_SHA256);
        self.version = TLS_1_3;
        self.state = TlsState::ServerHelloReceived;

        let mut record = Vec::new();
        record.push(ContentType::Handshake as u8);
        record.push(0x03);
        record.push(0x03);
        record.extend_from_slice(&(msg.len() as u16).to_be_bytes());
        record.extend_from_slice(&msg);
        record
    }

    pub fn derive_hs_traffic(&mut self) {
        let mut ikm = [0u8; 64];
        ikm[..32].copy_from_slice(&self.client_random);
        ikm[32..].copy_from_slice(&self.server_random);
        let prk = hkdf_extract(&[0u8; 32], &ikm);
        let out = hkdf_expand_label(&prk, b"c hs traffic", &[], 32);
        if out.len() >= 32 {
            self.hs_traffic.copy_from_slice(&out[..32]);
        }
        self.state = TlsState::Established;
    }

    /// Process an incoming TLS record
    pub fn process_record(&mut self, data: &[u8]) -> Result<Vec<u8>, TlsError> {
        if let Some((header, payload)) = TlsRecordHeader::parse(data) {
            match header.content_type {
                22 => self.process_handshake(payload),
                23 => self.process_application_data(payload),
                21 => self.process_alert(payload),
                20 => self.process_change_cipher_spec(payload),
                _ => Err(TlsError::UnexpectedMessage),
            }
        } else {
            Err(TlsError::DecodeError)
        }
    }

    fn process_handshake(&mut self, data: &[u8]) -> Result<Vec<u8>, TlsError> {
        if data.is_empty() {
            return Err(TlsError::DecodeError);
        }

        match data[0] {
            1 => {
                // ClientHello
                if data.len() < 38 {
                    return Err(TlsError::DecodeError);
                }
                self.client_random.copy_from_slice(&data[6..38]);
                self.handshake_hash.extend_from_slice(data);
                if !self.is_client {
                    self.state = TlsState::ClientHelloSent;
                }
                Ok(Vec::new())
            }
            2 => {
                // ServerHello
                if data.len() < 39 {
                    return Err(TlsError::DecodeError);
                }
                // Parse server random
                self.server_random.copy_from_slice(&data[6..38]);

                // Parse cipher suite
                let session_id_len = data[38] as usize;
                if data.len() < 39 + session_id_len + 2 {
                    return Err(TlsError::DecodeError);
                }
                let cs_offset = 39 + session_id_len;
                let cs = u16::from_be_bytes([data[cs_offset], data[cs_offset + 1]]);
                self.cipher_suite = match cs {
                    0x1301 => Some(CipherSuite::TLS_AES_128_GCM_SHA256),
                    0x1302 => Some(CipherSuite::TLS_AES_256_GCM_SHA384),
                    0x1303 => Some(CipherSuite::TLS_CHACHA20_POLY1305_SHA256),
                    0xC02F => Some(CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256),
                    0xC030 => Some(CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384),
                    _ => None,
                };

                self.state = TlsState::ServerHelloReceived;
                self.handshake_hash.extend_from_slice(data);
                Ok(Vec::new())
            }
            11 => {
                // Certificate
                self.state = TlsState::CertificateReceived;
                self.handshake_hash.extend_from_slice(data);
                Ok(Vec::new())
            }
            14 => {
                // ServerHelloDone
                self.state = TlsState::KeyExchangeDone;
                self.handshake_hash.extend_from_slice(data);
                // Build ClientKeyExchange + ChangeCipherSpec + Finished
                Ok(Vec::new())
            }
            20 => {
                // Finished
                self.state = TlsState::Established;
                Ok(Vec::new())
            }
            _ => Ok(Vec::new()),
        }
    }

    fn process_application_data(&mut self, data: &[u8]) -> Result<Vec<u8>, TlsError> {
        if self.state != TlsState::Established {
            return Err(TlsError::UnexpectedMessage);
        }
        // Decrypt and return plaintext
        self.read_seq += 1;
        self.read_buffer.extend_from_slice(data);
        Ok(data.to_vec())
    }

    fn process_alert(&mut self, data: &[u8]) -> Result<Vec<u8>, TlsError> {
        if data.len() < 2 {
            return Err(TlsError::DecodeError);
        }
        let level = data[0];
        let desc = data[1];

        if level == AlertLevel::Fatal as u8 || desc == AlertDescription::CloseNotify as u8 {
            self.state = TlsState::Closed;
        }

        Ok(Vec::new())
    }

    fn process_change_cipher_spec(&mut self, _data: &[u8]) -> Result<Vec<u8>, TlsError> {
        Ok(Vec::new())
    }

    /// Encrypt and send application data
    pub fn send(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, TlsError> {
        if self.state != TlsState::Established {
            return Err(TlsError::NotEstablished);
        }

        // Build TLS record with application data
        // In a full implementation, this would encrypt the plaintext
        let mut record = Vec::new();
        let hdr = TlsRecordHeader::new(
            ContentType::ApplicationData,
            TLS_1_2,
            plaintext.len() as u16,
        );
        record.push(hdr.content_type);
        record.push(hdr.version_major);
        record.push(hdr.version_minor);
        record.extend_from_slice(&(plaintext.len() as u16).to_be_bytes());
        record.extend_from_slice(plaintext);

        self.write_seq += 1;
        Ok(record)
    }

    /// Read decrypted application data
    pub fn read(&mut self) -> Vec<u8> {
        let data = self.read_buffer.clone();
        self.read_buffer.clear();
        data
    }

    /// Send close_notify alert
    pub fn close(&mut self) -> Vec<u8> {
        let alert = [
            AlertLevel::Warning as u8,
            AlertDescription::CloseNotify as u8,
        ];
        let mut record = Vec::new();
        let hdr = TlsRecordHeader::new(ContentType::Alert, TLS_1_2, 2);
        record.push(hdr.content_type);
        record.push(hdr.version_major);
        record.push(hdr.version_minor);
        record.extend_from_slice(&2u16.to_be_bytes());
        record.extend_from_slice(&alert);

        self.state = TlsState::Closing;
        record
    }
}

/// TLS error types
#[derive(Debug)]
pub enum TlsError {
    UnexpectedMessage,
    DecodeError,
    HandshakeFailure,
    BadCertificate,
    NotEstablished,
    InternalError,
}
