//! GATE_TLS13 serial markers and known-answer self-tests.

use crate::serial_println;

use super::api::{TLS_SESSIONS, create_client_session, create_server_session, handshake, process};
use super::hash::{hkdf_expand, hkdf_expand_label, hkdf_extract};
use super::session::TlsState;
use super::x509::{
    CertificateStore, X509Certificate, build_self_signed_der, rsa_public_from_spki_bits,
};

pub const GATE_TLS13_MARKER: &str = "GATE_TLS13 hkdf";

pub(super) fn hkdf_self_test() -> bool {
    // RFC 5869 Test Case 1 (SHA-256).
    let ikm = [0x0bu8; 22];
    let salt = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
    ];
    let info = [0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9];
    let prk = hkdf_extract(&salt, &ikm);
    const EXP_PRK: [u8; 32] = [
        0x07, 0x77, 0x09, 0x36, 0x2c, 0x2e, 0x32, 0xdf, 0x0d, 0xdc, 0x3f, 0x0d, 0xc4, 0x7b, 0xba,
        0x63, 0x90, 0xb6, 0xc7, 0x3b, 0xb5, 0x0f, 0x9c, 0x31, 0x22, 0xec, 0x84, 0x4a, 0xd7, 0xc2,
        0xb3, 0xe5,
    ];
    const EXP_OKM: [u8; 42] = [
        0x3c, 0xb2, 0x5f, 0x25, 0xfa, 0xac, 0xd5, 0x7a, 0x90, 0x43, 0x4f, 0x64, 0xd0, 0x36, 0x2f,
        0x2a, 0x2d, 0x2d, 0x0a, 0x90, 0xcf, 0x1a, 0x5a, 0x4c, 0x5d, 0xb0, 0x2d, 0x56, 0xec, 0xc4,
        0xc5, 0xbf, 0x34, 0x00, 0x72, 0x08, 0xd5, 0xb8, 0x87, 0x18, 0x58, 0x65,
    ];
    let okm = hkdf_expand(&prk, &info, 42);
    let derived = hkdf_expand_label(&prk, b"key", &[], 32);
    let ok = prk == EXP_PRK && okm.as_slice() == EXP_OKM.as_slice() && derived.len() == 32;
    if ok {
        serial_println!("[TLS] {} (RFC 5869 + Expand-Label)", GATE_TLS13_MARKER);
        true
    } else {
        serial_println!(
            "[TLS] GATE_TLS13 FAILED prk={:02x?} okm_len={}",
            &prk[..4],
            okm.len()
        );
        false
    }
}

pub const GATE_TLS13_HS_MARKER: &str = "GATE_TLS13 hs";

pub(super) fn handshake_self_test() -> bool {
    let client = create_client_session(Some("knoxos.test"));
    let server = create_server_session();
    let Some(ch) = handshake(client) else {
        serial_println!("[TLS] GATE_TLS13 FAILED: ClientHello");
        return false;
    };
    if ch.len() < 6 || ch[0] != 22 || ch[5] != 1 {
        serial_println!("[TLS] GATE_TLS13 FAILED: CH type");
        return false;
    }
    if process(server, &ch).is_err() {
        serial_println!("[TLS] GATE_TLS13 FAILED: process CH");
        return false;
    }
    let Some(sh) = handshake(server) else {
        serial_println!("[TLS] GATE_TLS13 FAILED: ServerHello");
        return false;
    };
    if sh.len() < 6 || sh[0] != 22 || sh[5] != 2 {
        serial_println!("[TLS] GATE_TLS13 FAILED: SH type");
        return false;
    }
    if process(client, &sh).is_err() {
        serial_println!("[TLS] GATE_TLS13 FAILED: process SH");
        return false;
    }
    let _ = handshake(client);
    let _ = handshake(server);
    let sessions = TLS_SESSIONS.lock();
    let (Some(c), Some(s)) = (sessions.get(&client), sessions.get(&server)) else {
        serial_println!("[TLS] GATE_TLS13 FAILED: sessions");
        return false;
    };
    if c.hs_traffic != s.hs_traffic || c.hs_traffic.iter().all(|b| *b == 0) {
        serial_println!("[TLS] GATE_TLS13 FAILED: traffic secret");
        return false;
    }
    if c.state != TlsState::Established || s.state != TlsState::Established {
        serial_println!("[TLS] GATE_TLS13 FAILED: state");
        return false;
    }
    serial_println!(
        "[TLS] {} (ClientHello+ServerHello, matching c hs traffic)",
        GATE_TLS13_HS_MARKER
    );
    true
}

pub const GATE_TLS13_X509_MARKER: &str = "GATE_TLS13 x509";

pub(super) fn x509_self_test() -> bool {
    let der = build_self_signed_der();
    let Some(cert) = X509Certificate::from_der(&der) else {
        serial_println!("[TLS] GATE_TLS13 FAILED: x509 parse");
        return false;
    };
    if !cert.is_self_signed() || !cert.is_ca {
        serial_println!(
            "[TLS] GATE_TLS13 FAILED: x509 name/ca subject={}",
            cert.subject
        );
        return false;
    }
    let Some(key) = rsa_public_from_spki_bits(&cert.public_key) else {
        serial_println!("[TLS] GATE_TLS13 FAILED: x509 spki");
        return false;
    };
    if !crate::crypto::rsa_verify(&cert.tbs_raw, &cert.signature, &key) {
        serial_println!("[TLS] GATE_TLS13 FAILED: x509 rsa verify");
        return false;
    }
    let mut bad = cert.tbs_raw.clone();
    if let Some(b) = bad.last_mut() {
        *b ^= 0x01;
    }
    if crate::crypto::rsa_verify(&bad, &cert.signature, &key) {
        serial_println!("[TLS] GATE_TLS13 FAILED: x509 tamper accepted");
        return false;
    }
    let mut store = CertificateStore::new();
    store.add_trusted_root(cert.clone());
    if !store.verify_chain(&[cert]) {
        serial_println!("[TLS] GATE_TLS13 FAILED: x509 chain");
        return false;
    }
    serial_println!(
        "[TLS] {} (DER parse + RSA PKCS#1 over TBS)",
        GATE_TLS13_X509_MARKER
    );
    true
}
