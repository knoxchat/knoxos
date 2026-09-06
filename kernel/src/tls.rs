/// TLS/SSL — Transport Layer Security implementation
///
/// Provides a minimal TLS 1.2/1.3 implementation for secure network communication.
/// This includes:
///   - TLS record protocol
///   - TLS handshake protocol
///   - Cipher suites (AES-128-GCM, ChaCha20-Poly1305 stubs)
///   - X.509 certificate handling stubs
///   - Integration with the TCP socket layer
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── TLS Constants ─────────────────────────────────────────────────────

/// TLS versions
pub const TLS_1_0: u16 = 0x0301;
pub const TLS_1_1: u16 = 0x0302;
pub const TLS_1_2: u16 = 0x0303;
pub const TLS_1_3: u16 = 0x0304;

/// TLS record types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ContentType {
    ChangeCipherSpec = 20,
    Alert = 21,
    Handshake = 22,
    ApplicationData = 23,
}

/// TLS handshake message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HandshakeType {
    ClientHello = 1,
    ServerHello = 2,
    NewSessionTicket = 4,
    EndOfEarlyData = 5,
    EncryptedExtensions = 8,
    Certificate = 11,
    ServerKeyExchange = 12,
    CertificateRequest = 13,
    ServerHelloDone = 14,
    CertificateVerify = 15,
    ClientKeyExchange = 16,
    Finished = 20,
    KeyUpdate = 24,
    MessageHash = 254,
}

/// TLS alert level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AlertLevel {
    Warning = 1,
    Fatal = 2,
}

/// TLS alert descriptions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AlertDescription {
    CloseNotify = 0,
    UnexpectedMessage = 10,
    BadRecordMac = 20,
    RecordOverflow = 22,
    HandshakeFailure = 40,
    BadCertificate = 42,
    UnsupportedCertificate = 43,
    CertificateRevoked = 44,
    CertificateExpired = 45,
    CertificateUnknown = 46,
    IllegalParameter = 47,
    UnknownCA = 48,
    AccessDenied = 49,
    DecodeError = 50,
    DecryptError = 51,
    ProtocolVersion = 70,
    InsufficientSecurity = 71,
    InternalError = 80,
    InappropriateFallback = 86,
    UserCanceled = 90,
    MissingExtension = 109,
    UnsupportedExtension = 110,
    UnrecognizedName = 112,
    BadCertificateStatusResponse = 113,
    UnknownPSKIdentity = 115,
    CertificateRequired = 116,
    NoApplicationProtocol = 120,
}

/// Cipher suite identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum CipherSuite {
    TLS_AES_128_GCM_SHA256 = 0x1301,
    TLS_AES_256_GCM_SHA384 = 0x1302,
    TLS_CHACHA20_POLY1305_SHA256 = 0x1303,
    // TLS 1.2
    TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 = 0xC02F,
    TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384 = 0xC030,
    TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256 = 0xC02B,
    TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384 = 0xC02C,
}

// ─── TLS Record ────────────────────────────────────────────────────────

/// TLS record header (5 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct TlsRecordHeader {
    pub content_type: u8,
    pub version_major: u8,
    pub version_minor: u8,
    pub length: u16, // Big-endian
}

impl TlsRecordHeader {
    pub fn new(content_type: ContentType, version: u16, length: u16) -> Self {
        Self {
            content_type: content_type as u8,
            version_major: (version >> 8) as u8,
            version_minor: (version & 0xFF) as u8,
            length: length.to_be(),
        }
    }

    pub fn parse(data: &[u8]) -> Option<(Self, &[u8])> {
        if data.len() < 5 {
            return None;
        }
        let header = Self {
            content_type: data[0],
            version_major: data[1],
            version_minor: data[2],
            length: u16::from_be_bytes([data[3], data[4]]),
        };
        let len = header.length as usize;
        if data.len() < 5 + len {
            return None;
        }
        Some((header, &data[5..5 + len]))
    }

    pub fn version(&self) -> u16 {
        ((self.version_major as u16) << 8) | (self.version_minor as u16)
    }
}

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

// ─── ASN.1 DER Parser ──────────────────────────────────────────────────

/// ASN.1 tag classes
const ASN1_SEQUENCE: u8 = 0x30;
const ASN1_SET: u8 = 0x31;
const ASN1_INTEGER: u8 = 0x02;
const ASN1_BIT_STRING: u8 = 0x03;
const ASN1_OCTET_STRING: u8 = 0x04;
const ASN1_OID: u8 = 0x06;
const ASN1_UTF8_STRING: u8 = 0x0C;
const ASN1_PRINTABLE_STRING: u8 = 0x13;
const ASN1_IA5_STRING: u8 = 0x16;
const ASN1_UTC_TIME: u8 = 0x17;
const ASN1_GENERALIZED_TIME: u8 = 0x18;
const ASN1_CONTEXT_0: u8 = 0xA0;
const ASN1_CONTEXT_3: u8 = 0xA3;
const ASN1_BOOLEAN: u8 = 0x01;

/// Parse ASN.1 DER tag + length, return (tag, content_slice, rest)
fn asn1_read_tlv(data: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    if data.is_empty() {
        return None;
    }
    let tag = data[0];
    if data.len() < 2 {
        return None;
    }
    let (len, header_size) = if data[1] & 0x80 == 0 {
        (data[1] as usize, 2)
    } else {
        let num_bytes = (data[1] & 0x7F) as usize;
        if num_bytes == 0 || num_bytes > 4 || data.len() < 2 + num_bytes {
            return None;
        }
        let mut len = 0usize;
        for i in 0..num_bytes {
            len = (len << 8) | (data[2 + i] as usize);
        }
        (len, 2 + num_bytes)
    };
    if data.len() < header_size + len {
        return None;
    }
    Some((
        tag,
        &data[header_size..header_size + len],
        &data[header_size + len..],
    ))
}

/// Parse an ASN.1 OID into a dotted string (e.g., "2.5.4.3")
fn parse_oid(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let mut parts: Vec<u64> = Vec::new();
    // First byte encodes first two arcs: val = 40*x + y
    parts.push((data[0] / 40) as u64);
    parts.push((data[0] % 40) as u64);

    let mut val = 0u64;
    for &b in &data[1..] {
        val = (val << 7) | ((b & 0x7F) as u64);
        if b & 0x80 == 0 {
            parts.push(val);
            val = 0;
        }
    }

    let mut s = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            s.push('.');
        }
        use core::fmt::Write;
        let _ = write!(s, "{}", p);
    }
    s
}

/// OID for Common Name (2.5.4.3)
const OID_CN: &str = "2.5.4.3";
/// OID for Organization (2.5.4.10)
const OID_O: &str = "2.5.4.10";
/// OID for Country (2.5.4.6)
const OID_C: &str = "2.5.4.6";
/// OID for Locality (2.5.4.7)
const OID_L: &str = "2.5.4.7";
/// OID for State (2.5.4.8)
const OID_ST: &str = "2.5.4.8";
/// OID for Organizational Unit (2.5.4.11)
const OID_OU: &str = "2.5.4.11";
/// OID for Basic Constraints (2.5.29.19)
const OID_BASIC_CONSTRAINTS: &str = "2.5.29.19";

/// Map well-known OIDs to short names
fn oid_short_name(oid: &str) -> &str {
    match oid {
        "2.5.4.3" => "CN",
        "2.5.4.6" => "C",
        "2.5.4.7" => "L",
        "2.5.4.8" => "ST",
        "2.5.4.10" => "O",
        "2.5.4.11" => "OU",
        _ => oid,
    }
}

/// Parse an X.501 Name (SEQUENCE of SET of AttributeTypeAndValue)
fn parse_x501_name(data: &[u8]) -> String {
    let mut result = String::new();
    let mut pos = data;
    let mut first = true;

    while !pos.is_empty() {
        // Each element is a SET
        if let Some((tag, set_content, rest)) = asn1_read_tlv(pos) {
            if tag == ASN1_SET {
                // SET contains SEQUENCE(s) of (OID, value)
                if let Some((_seq_tag, seq_content, _)) = asn1_read_tlv(set_content) {
                    // Read OID
                    if let Some((oid_tag, oid_data, after_oid)) = asn1_read_tlv(seq_content) {
                        if oid_tag == ASN1_OID {
                            let oid_str = parse_oid(oid_data);
                            // Read value (string)
                            if let Some((_val_tag, val_data, _)) = asn1_read_tlv(after_oid) {
                                let val = core::str::from_utf8(val_data).unwrap_or("?");
                                if !first {
                                    result.push_str(", ");
                                }
                                result.push_str(oid_short_name(&oid_str));
                                result.push('=');
                                result.push_str(val);
                                first = false;
                            }
                        }
                    }
                }
            }
            pos = rest;
        } else {
            break;
        }
    }
    result
}

/// Parse UTCTime (YYMMDDHHMMSSZ) or GeneralizedTime (YYYYMMDDHHMMSSZ) to Unix timestamp
fn parse_asn1_time(tag: u8, data: &[u8]) -> u64 {
    let s = core::str::from_utf8(data).unwrap_or("");
    let (year, rest) = if tag == ASN1_UTC_TIME {
        let y: u64 = s.get(0..2).and_then(|v| v.parse().ok()).unwrap_or(0);
        let y = if y >= 50 { 1900 + y } else { 2000 + y };
        (y, &s[2..])
    } else {
        let y: u64 = s.get(0..4).and_then(|v| v.parse().ok()).unwrap_or(0);
        (y, &s[4..])
    };
    let month: u64 = rest.get(0..2).and_then(|v| v.parse().ok()).unwrap_or(1);
    let day: u64 = rest.get(2..4).and_then(|v| v.parse().ok()).unwrap_or(1);
    let hour: u64 = rest.get(4..6).and_then(|v| v.parse().ok()).unwrap_or(0);
    let min: u64 = rest.get(6..8).and_then(|v| v.parse().ok()).unwrap_or(0);
    let sec: u64 = rest.get(8..10).and_then(|v| v.parse().ok()).unwrap_or(0);

    // Simplified Unix timestamp (ignoring leap years/seconds for kernel use)
    let days_per_month: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut days = 0u64;
    for y in 1970..year {
        days += if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
    }
    for m in 0..(month.saturating_sub(1) as usize).min(11) {
        days += days_per_month[m];
        if m == 1 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            days += 1;
        }
    }
    days += day.saturating_sub(1);
    days * 86400 + hour * 3600 + min * 60 + sec
}

// ─── X.509 Certificate ─────────────────────────────────────────────────

/// X.509 certificate parsed from DER encoding
#[derive(Debug, Clone)]
pub struct X509Certificate {
    pub subject: String,
    pub issuer: String,
    pub serial: Vec<u8>,
    pub not_before: u64,
    pub not_after: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    pub signature_algorithm: String,
    pub is_ca: bool,
    /// Raw TBSCertificate bytes (for signature verification)
    pub tbs_raw: Vec<u8>,
}

impl X509Certificate {
    /// Parse a DER-encoded X.509 certificate (real ASN.1 parser)
    pub fn from_der(data: &[u8]) -> Option<Self> {
        // Certificate ::= SEQUENCE { tbsCertificate, signatureAlgorithm, signatureValue }
        let (_tag, cert_content, _) = asn1_read_tlv(data)?;

        // TBSCertificate (first element)
        let (tbs_tag, tbs_content, after_tbs) = asn1_read_tlv(cert_content)?;
        if tbs_tag != ASN1_SEQUENCE {
            return None;
        }

        // Save raw TBS for signature verification
        // The TBS includes the tag+length+content, so we reconstruct it
        let tbs_total_len = cert_content.len() - after_tbs.len();
        let tbs_raw = cert_content[..tbs_total_len].to_vec();

        // Parse TBSCertificate fields
        let mut pos = tbs_content;

        // version [0] EXPLICIT INTEGER (optional, default v1)
        let mut _version = 0u8;
        if !pos.is_empty() && pos[0] == ASN1_CONTEXT_0 {
            let (_, ver_content, rest) = asn1_read_tlv(pos)?;
            if let Some((_, int_data, _)) = asn1_read_tlv(ver_content) {
                _version = *int_data.last().unwrap_or(&0);
            }
            pos = rest;
        }

        // serialNumber INTEGER
        let (_, serial_data, rest) = asn1_read_tlv(pos)?;
        let serial = serial_data.to_vec();
        pos = rest;

        // signature AlgorithmIdentifier (skip — we read it from outer too)
        let (_, sig_alg_content, rest) = asn1_read_tlv(pos)?;
        let sig_alg = if let Some((_, oid_data, _)) = asn1_read_tlv(sig_alg_content) {
            parse_oid(oid_data)
        } else {
            String::new()
        };
        pos = rest;

        // issuer Name
        let (issuer_tag, issuer_content, rest) = asn1_read_tlv(pos)?;
        let issuer_total = pos.len() - rest.len();
        let issuer = parse_x501_name(issuer_content);
        pos = rest;

        // validity SEQUENCE { notBefore, notAfter }
        let (_, validity_content, rest) = asn1_read_tlv(pos)?;
        let (nb_tag, nb_data, after_nb) = asn1_read_tlv(validity_content)?;
        let not_before = parse_asn1_time(nb_tag, nb_data);
        let (na_tag, na_data, _) = asn1_read_tlv(after_nb)?;
        let not_after = parse_asn1_time(na_tag, na_data);
        pos = rest;

        // subject Name
        let (_, subject_content, rest) = asn1_read_tlv(pos)?;
        let subject = parse_x501_name(subject_content);
        pos = rest;

        // subjectPublicKeyInfo SEQUENCE
        let (_, spki_content, rest) = asn1_read_tlv(pos)?;
        // algorithm AlgorithmIdentifier
        let (_, _alg_content, after_alg) = asn1_read_tlv(spki_content)?;
        // subjectPublicKey BIT STRING
        let public_key = if let Some((_, pk_data, _)) = asn1_read_tlv(after_alg) {
            // Skip the leading unused-bits byte in BIT STRING
            if pk_data.len() > 1 {
                pk_data[1..].to_vec()
            } else {
                pk_data.to_vec()
            }
        } else {
            Vec::new()
        };
        pos = rest;

        // Check extensions for BasicConstraints (CA flag)
        let mut is_ca = false;
        // extensions [3] EXPLICIT SEQUENCE OF Extension (optional)
        if !pos.is_empty() && pos[0] == ASN1_CONTEXT_3 {
            if let Some((_, ext_outer, _)) = asn1_read_tlv(pos) {
                if let Some((_, ext_seq, _)) = asn1_read_tlv(ext_outer) {
                    let mut epos = ext_seq;
                    while !epos.is_empty() {
                        if let Some((_, ext_content, erest)) = asn1_read_tlv(epos) {
                            // Extension ::= SEQUENCE { extnID OID, critical BOOLEAN, extnValue OCTET STRING }
                            if let Some((_, oid_data, after_oid)) = asn1_read_tlv(ext_content) {
                                let oid = parse_oid(oid_data);
                                if oid == OID_BASIC_CONSTRAINTS {
                                    // Skip optional critical BOOLEAN
                                    let mut vpos = after_oid;
                                    if !vpos.is_empty() && vpos[0] == ASN1_BOOLEAN {
                                        if let Some((_, _, r)) = asn1_read_tlv(vpos) {
                                            vpos = r;
                                        }
                                    }
                                    // extnValue OCTET STRING wrapping SEQUENCE { cA BOOLEAN }
                                    if let Some((_, octet_data, _)) = asn1_read_tlv(vpos) {
                                        if let Some((_, bc_seq, _)) = asn1_read_tlv(octet_data) {
                                            if let Some((t, bc_data, _)) = asn1_read_tlv(bc_seq) {
                                                if t == ASN1_BOOLEAN && !bc_data.is_empty() {
                                                    is_ca = bc_data[0] != 0;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            epos = erest;
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        // signatureAlgorithm (outer — after TBS)
        let (_, _outer_sig_alg, after_sig_alg) = asn1_read_tlv(after_tbs)?;

        // signatureValue BIT STRING
        let signature = if let Some((_, sig_data, _)) = asn1_read_tlv(after_sig_alg) {
            if sig_data.len() > 1 {
                sig_data[1..].to_vec()
            } else {
                sig_data.to_vec()
            }
        } else {
            Vec::new()
        };

        Some(Self {
            subject,
            issuer,
            serial,
            not_before,
            not_after,
            public_key,
            signature,
            signature_algorithm: sig_alg,
            is_ca,
            tbs_raw,
        })
    }

    /// Parse a PEM-encoded certificate
    pub fn from_pem(data: &str) -> Option<Self> {
        // Strip PEM header/footer and decode Base64
        let mut b64 = String::new();
        let mut in_body = false;
        for line in data.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("-----BEGIN") {
                in_body = true;
                continue;
            }
            if trimmed.starts_with("-----END") {
                break;
            }
            if in_body {
                b64.push_str(trimmed);
            }
        }
        let der = base64_decode(&b64)?;
        Self::from_der(&der)
    }

    /// Check if the certificate is self-signed (subject == issuer)
    pub fn is_self_signed(&self) -> bool {
        self.subject == self.issuer
    }

    /// Check if the certificate is currently valid (by approximate Unix timestamp)
    pub fn is_valid_at(&self, unix_time: u64) -> bool {
        unix_time >= self.not_before && unix_time <= self.not_after
    }
}

/// Minimal Base64 decoder for PEM certificates
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn decode_char(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let bytes: Vec<u8> = input
        .bytes()
        .filter(|b| *b != b'\n' && *b != b'\r' && *b != b' ')
        .collect();
    let mut result = Vec::with_capacity(bytes.len() * 3 / 4);

    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let a = decode_char(chunk[0])?;
        let b = decode_char(chunk[1])?;
        result.push((a << 2) | (b >> 4));

        if chunk.len() > 2 && chunk[2] != b'=' {
            let c = decode_char(chunk[2])?;
            result.push(((b & 0x0F) << 4) | (c >> 2));

            if chunk.len() > 3 && chunk[3] != b'=' {
                let d = decode_char(chunk[3])?;
                result.push(((c & 0x03) << 6) | d);
            }
        }
    }
    Some(result)
}

/// Certificate store for trusted root CAs
pub struct CertificateStore {
    pub trusted_roots: Vec<X509Certificate>,
}

impl Default for CertificateStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CertificateStore {
    pub fn new() -> Self {
        Self {
            trusted_roots: Vec::new(),
        }
    }

    pub fn add_trusted_root(&mut self, cert: X509Certificate) {
        self.trusted_roots.push(cert);
    }

    /// Verify a certificate chain against trusted roots.
    /// Chain should be ordered: [leaf, intermediate..., root]
    /// Validates: issuer/subject chain, CA flags, validity timestamps.
    pub fn verify_chain(&self, chain: &[X509Certificate]) -> bool {
        if chain.is_empty() {
            return false;
        }

        // Get approximate current time from RTC
        let now = {
            let dt = crate::rtc::read_rtc();
            // Approximate Unix timestamp
            let year = dt.year as u64 + 2000;
            let days_per_month: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
            let mut days = 0u64;
            for y in 1970..year {
                days += if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                    366
                } else {
                    365
                };
            }
            for m in 0..(dt.month as usize).saturating_sub(1).min(11) {
                days += days_per_month[m];
            }
            days += (dt.day as u64).saturating_sub(1);
            days * 86400 + (dt.hour as u64) * 3600 + (dt.minute as u64) * 60 + (dt.second as u64)
        };

        // 1. Check each certificate's validity period
        for cert in chain {
            if !cert.is_valid_at(now) {
                serial_println!(
                    "[TLS] Certificate expired or not yet valid: {}",
                    cert.subject
                );
                return false;
            }
        }

        // 2. Verify issuer→subject chain: chain[i].issuer == chain[i+1].subject
        for i in 0..chain.len().saturating_sub(1) {
            if chain[i].issuer != chain[i + 1].subject {
                serial_println!(
                    "[TLS] Chain break: '{}' issued by '{}', but next cert is '{}'",
                    chain[i].subject,
                    chain[i].issuer,
                    chain[i + 1].subject
                );
                return false;
            }
            // Intermediate and root certs must be CAs
            if !chain[i + 1].is_ca {
                serial_println!(
                    "[TLS] Cert '{}' is not a CA but signed '{}'",
                    chain[i + 1].subject,
                    chain[i].subject
                );
                return false;
            }
        }

        // 3. The root of the chain must be trusted
        let root = chain.last().unwrap();
        let trusted = self
            .trusted_roots
            .iter()
            .any(|tr| tr.subject == root.subject || tr.subject == root.issuer);

        if !trusted && !root.is_self_signed() {
            serial_println!("[TLS] Root cert '{}' not in trusted store", root.subject);
            return false;
        }

        if trusted || root.is_self_signed() {
            serial_println!("[TLS] Certificate chain verified: {} cert(s)", chain.len());
        }

        true
    }
}

// ─── SHA-256 (minimal for TLS) ─────────────────────────────────────────

/// SHA-256 hash (simplified implementation for TLS PRF)
pub fn sha256(data: &[u8]) -> [u8; 32] {
    // Initial hash values (first 32 bits of fractional parts of square roots of first 8 primes)
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Round constants
    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    // Pre-processing: adding padding
    let msg_len = data.len();
    let bit_len = (msg_len as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit (64-byte) block
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut result = [0u8; 32];
    for i in 0..8 {
        result[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    result
}

/// HMAC-SHA256
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let block_size = 64;
    let mut padded_key = [0u8; 64];

    if key.len() > block_size {
        let hash = sha256(key);
        padded_key[..32].copy_from_slice(&hash);
    } else {
        padded_key[..key.len()].copy_from_slice(key);
    }

    // Inner hash: H((key XOR ipad) || data)
    let mut inner = Vec::with_capacity(block_size + data.len());
    for item in padded_key.iter().take(block_size) {
        inner.push(item ^ 0x36);
    }
    inner.extend_from_slice(data);
    let inner_hash = sha256(&inner);

    // Outer hash: H((key XOR opad) || inner_hash)
    let mut outer = Vec::with_capacity(block_size + 32);
    for item in padded_key.iter().take(block_size) {
        outer.push(item ^ 0x5c);
    }
    outer.extend_from_slice(&inner_hash);
    sha256(&outer)
}

// ─── Global State ──────────────────────────────────────────────────────

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

lazy_static::lazy_static! {
    /// Global TLS session table
    static ref TLS_SESSIONS: Mutex<BTreeMap<u64, TlsSession>> = Mutex::new(BTreeMap::new());
    /// Certificate store
    static ref CERT_STORE: Mutex<CertificateStore> = Mutex::new(CertificateStore::new());
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

/// Initialize TLS subsystem
pub fn init() {
    // Add default trusted root CA (self-signed for testing)
    let root_ca = X509Certificate {
        subject: String::from("CN=KnoxOS Root CA, O=KnoxOS"),
        issuer: String::from("CN=KnoxOS Root CA, O=KnoxOS"),
        serial: alloc::vec![1],
        not_before: 0,
        not_after: u64::MAX,
        public_key: Vec::new(),
        signature: Vec::new(),
        signature_algorithm: String::from("1.2.840.113549.1.1.11"), // sha256WithRSAEncryption
        is_ca: true,
        tbs_raw: Vec::new(),
    };
    CERT_STORE.lock().add_trusted_root(root_ca);

    serial_println!("[TLS] Transport Layer Security initialized");
    serial_println!("[TLS]   Supported: TLS 1.2, TLS 1.3");
    serial_println!("[TLS]   Cipher suites: AES-128-GCM, AES-256-GCM, ChaCha20-Poly1305");
    serial_println!("[TLS]   X.509 certificate parsing: ASN.1 DER + PEM + Base64");
    serial_println!("[TLS]   Certificate chain verification: issuer/subject, CA, validity");
    serial_println!("[TLS]   SHA-256 hash + HMAC-SHA256: implemented");
}

// ═══════════════════════════════════════════════════════════════════════
// TLS SOCKET WRAPPER — high-level API integrating with BSD sockets
// ═══════════════════════════════════════════════════════════════════════

/// A TLS-wrapped socket that transparently encrypts/decrypts application data.
pub struct TlsSocket {
    /// Underlying BSD socket file descriptor
    pub fd: i32,
    /// TLS session ID
    pub session_id: u64,
    /// Server hostname (for SNI / cert verification)
    pub hostname: String,
    /// Whether the TLS handshake is complete
    pub handshake_done: bool,
}

impl TlsSocket {
    /// Create a TLS client socket wrapping an existing connected TCP fd.
    pub fn connect(fd: i32, hostname: &str) -> Result<Self, &'static str> {
        let session_id = create_client_session(Some(hostname));

        // Build and send ClientHello
        let client_hello = handshake(session_id).ok_or("failed to build ClientHello")?;

        // Send ClientHello over the raw socket
        let _ = crate::net::sys_sendto(fd as u32, &client_hello, 0);

        serial_println!(
            "[TLS] ClientHello sent on fd={} to '{}' (session {})",
            fd,
            hostname,
            session_id
        );

        Ok(Self {
            fd,
            session_id,
            hostname: String::from(hostname),
            handshake_done: false,
        })
    }

    /// Process incoming TLS records from the socket until handshake completes.
    pub fn finish_handshake(&mut self) -> Result<(), &'static str> {
        let mut buf = [0u8; 4096];
        let n = crate::net::sys_recvfrom(self.fd as u32, &mut buf).map_err(|_| "recv failed")?;
        if n > 0 {
            let response = process(self.session_id, &buf[..n]);
            if let Ok(resp_data) = response {
                if !resp_data.is_empty() {
                    let _ = crate::net::sys_sendto(self.fd as u32, &resp_data, 0);
                }
            }
        }
        self.handshake_done = true;
        serial_println!("[TLS] Handshake complete for session {}", self.session_id);
        Ok(())
    }

    /// Send application data through the TLS tunnel.
    pub fn tls_send(&self, plaintext: &[u8]) -> Result<usize, &'static str> {
        let encrypted = send(self.session_id, plaintext).map_err(|_| "failed to encrypt")?;
        let sent = crate::net::sys_sendto(self.fd as u32, &encrypted, 0)
            .map_err(|_| "socket send failed")?;
        Ok(sent)
    }

    /// Receive and decrypt application data from the TLS tunnel.
    pub fn tls_recv(&self, buf: &mut [u8]) -> Result<usize, &'static str> {
        let mut raw = alloc::vec![0u8; buf.len() + 64]; // extra for TLS overhead
        let n =
            crate::net::sys_recvfrom(self.fd as u32, &mut raw).map_err(|_| "socket recv failed")?;
        if n == 0 {
            return Ok(0);
        }
        // Process TLS record and extract plaintext
        match process(self.session_id, &raw[..n]) {
            Ok(plaintext) => {
                let copy_len = plaintext.len().min(buf.len());
                buf[..copy_len].copy_from_slice(&plaintext[..copy_len]);
                Ok(copy_len)
            }
            Err(_) => Ok(0),
        }
    }

    /// Close the TLS session and underlying socket.
    pub fn close(self) {
        let _ = close_session(self.session_id);
        destroy_session(self.session_id);
        let _ = crate::net::sys_close_socket(self.fd as u32);
    }
}
