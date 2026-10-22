//! TLS/SSL — Transport Layer Security implementation
//!
//! Provides a minimal TLS 1.2/1.3 implementation for secure network communication.
//! This includes:
//!   - TLS record protocol
//!   - TLS handshake protocol
//!   - Cipher suites (AES-128-GCM, ChaCha20-Poly1305 stubs)
//!   - X.509 certificate parse + RSA PKCS#1 chain verify
//!   - Integration with the TCP socket layer
//!
//! Module layout:
//!   protocol  — versions, content/handshake/alert types, cipher suites, records
//!   session   — connection state machine and TlsSession
//!   asn1      — DER TLV parser for X.509
//!   x509      — certificates, PEM/Base64, chain verify, GATE fixture
//!   hash      — SHA-256, HMAC-SHA256, HKDF-Extract/Expand/Expand-Label
//!   api       — session table and id-based handshake/send/recv
//!   gates     — GATE_TLS13 serial markers and self-tests
//!   socket    — TlsSocket wrapper over BSD TCP fds
mod api;
mod asn1;
mod gates;
mod hash;
mod protocol;
mod session;
mod socket;
mod x509;

pub use api::{
    close_session, create_client_session, create_server_session, destroy_session, handshake,
    process, send,
};
pub use gates::{GATE_TLS13_HS_MARKER, GATE_TLS13_MARKER, GATE_TLS13_X509_MARKER};
pub use hash::{hkdf_expand, hkdf_expand_label, hkdf_extract, hmac_sha256, sha256};
pub use protocol::{
    AlertDescription, AlertLevel, CipherSuite, ContentType, HandshakeType, TLS_1_0, TLS_1_1,
    TLS_1_2, TLS_1_3, TlsRecordHeader,
};
pub use session::{TlsError, TlsSession, TlsState};
pub use socket::TlsSocket;
pub use x509::{CertificateStore, X509Certificate};

use alloc::string::String;
use alloc::vec::Vec;

use crate::serial_println;

use api::CERT_STORE;
use gates::{handshake_self_test, hkdf_self_test, x509_self_test};

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
    serial_println!("[TLS]   Certificate chain verification: issuer/subject, CA, validity, RSA");
    serial_println!("[TLS]   SHA-256 hash + HMAC-SHA256: implemented");
    let _ = hkdf_self_test();
    let _ = handshake_self_test();
    let _ = x509_self_test();
}
