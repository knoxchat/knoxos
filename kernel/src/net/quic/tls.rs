use alloc::vec::Vec;

pub(super) fn build_tls_client_hello() -> Vec<u8> {
    // TLS 1.3 ClientHello for QUIC (RFC 8446 §4.1.2)
    let mut hello = Vec::with_capacity(256);
    hello.push(0x01); // ClientHello handshake type

    // Length placeholder (3 bytes) — will be patched below
    let len_pos = hello.len();
    hello.extend_from_slice(&[0x00, 0x00, 0x00]);

    // Legacy version: TLS 1.2 (0x0303)
    hello.extend_from_slice(&[0x03, 0x03]);

    // Random (32 bytes) — use kernel random
    let r1 = crate::random::random_u64();
    let r2 = crate::random::random_u64();
    let r3 = crate::random::random_u64();
    let r4 = crate::random::random_u64();
    hello.extend_from_slice(&r1.to_le_bytes());
    hello.extend_from_slice(&r2.to_le_bytes());
    hello.extend_from_slice(&r3.to_le_bytes());
    hello.extend_from_slice(&r4.to_le_bytes());

    // Legacy session ID length = 0 (QUIC does not use session IDs)
    hello.push(0x00);

    // Cipher suites: TLS_AES_128_GCM_SHA256 (0x1301), TLS_AES_256_GCM_SHA384 (0x1302)
    hello.extend_from_slice(&[0x00, 0x04, 0x13, 0x01, 0x13, 0x02]);

    // Legacy compression methods: null only
    hello.extend_from_slice(&[0x01, 0x00]);

    // Extensions
    let ext_start = hello.len();
    hello.extend_from_slice(&[0x00, 0x00]); // Extensions length placeholder

    // Extension: supported_versions (0x002B) — advertise TLS 1.3
    hello.extend_from_slice(&[0x00, 0x2B]); // extension type
    hello.extend_from_slice(&[0x00, 0x03]); // extension data length
    hello.push(0x02); // supported versions list length
    hello.extend_from_slice(&[0x03, 0x04]); // TLS 1.3

    // Extension: supported_groups (0x000A) — x25519
    hello.extend_from_slice(&[0x00, 0x0A]); // extension type
    hello.extend_from_slice(&[0x00, 0x04]); // extension data length
    hello.extend_from_slice(&[0x00, 0x02]); // named group list length
    hello.extend_from_slice(&[0x00, 0x1D]); // x25519

    // Extension: signature_algorithms (0x000D) — ecdsa_secp256r1_sha256
    hello.extend_from_slice(&[0x00, 0x0D]); // extension type
    hello.extend_from_slice(&[0x00, 0x04]); // extension data length
    hello.extend_from_slice(&[0x00, 0x02]); // algorithms list length
    hello.extend_from_slice(&[0x04, 0x03]); // ecdsa_secp256r1_sha256

    // Extension: QUIC transport parameters (0x0039) — empty for now
    hello.extend_from_slice(&[0x00, 0x39]); // extension type
    hello.extend_from_slice(&[0x00, 0x00]); // empty

    // Patch extensions length
    let ext_len = (hello.len() - ext_start - 2) as u16;
    hello[ext_start] = (ext_len >> 8) as u8;
    hello[ext_start + 1] = ext_len as u8;

    // Patch handshake length
    let body_len = (hello.len() - len_pos - 3) as u32;
    hello[len_pos] = ((body_len >> 16) & 0xFF) as u8;
    hello[len_pos + 1] = ((body_len >> 8) & 0xFF) as u8;
    hello[len_pos + 2] = (body_len & 0xFF) as u8;

    hello
}

pub(super) fn build_tls_server_hello() -> Vec<u8> {
    // TLS 1.3 ServerHello for QUIC (RFC 8446 §4.1.3)
    let mut hello = Vec::with_capacity(256);
    hello.push(0x02); // ServerHello handshake type

    // Length placeholder (3 bytes)
    let len_pos = hello.len();
    hello.extend_from_slice(&[0x00, 0x00, 0x00]);

    // Legacy version: TLS 1.2 (0x0303)
    hello.extend_from_slice(&[0x03, 0x03]);

    // Random (32 bytes)
    let r1 = crate::random::random_u64();
    let r2 = crate::random::random_u64();
    let r3 = crate::random::random_u64();
    let r4 = crate::random::random_u64();
    hello.extend_from_slice(&r1.to_le_bytes());
    hello.extend_from_slice(&r2.to_le_bytes());
    hello.extend_from_slice(&r3.to_le_bytes());
    hello.extend_from_slice(&r4.to_le_bytes());

    // Legacy session ID echo (length = 0 for QUIC)
    hello.push(0x00);

    // Cipher suite: TLS_AES_128_GCM_SHA256
    hello.extend_from_slice(&[0x13, 0x01]);

    // Legacy compression: null
    hello.push(0x00);

    // Extensions
    let ext_start = hello.len();
    hello.extend_from_slice(&[0x00, 0x00]); // Extensions length placeholder

    // Extension: supported_versions (0x002B) — TLS 1.3 selected
    hello.extend_from_slice(&[0x00, 0x2B]); // extension type
    hello.extend_from_slice(&[0x00, 0x02]); // extension data length
    hello.extend_from_slice(&[0x03, 0x04]); // TLS 1.3

    // Patch extensions length
    let ext_len = (hello.len() - ext_start - 2) as u16;
    hello[ext_start] = (ext_len >> 8) as u8;
    hello[ext_start + 1] = ext_len as u8;

    // Patch handshake length
    let body_len = (hello.len() - len_pos - 3) as u32;
    hello[len_pos] = ((body_len >> 16) & 0xFF) as u8;
    hello[len_pos + 1] = ((body_len >> 8) & 0xFF) as u8;
    hello[len_pos + 2] = (body_len & 0xFF) as u8;

    hello
}
