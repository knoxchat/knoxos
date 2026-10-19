//! TLS versions, record types, alerts, cipher suites, and record headers.

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
