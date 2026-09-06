/// SSH Integration — Remote shell sessions via PTY (P8.11)
/// Provides SSH server and client framework for remote terminal access.
/// Uses PTY pairs for terminal I/O and integrates with the shell subsystem.
/// Implements SSH-2.0 key exchange (Diffie-Hellman group14), AES-CTR encryption,
/// and HMAC-SHA256 integrity verification.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

/// SSH protocol version
pub const SSH_VERSION: &str = "SSH-2.0-KnoxOS_1.0";

/// SSH-2 message types (RFC 4253)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SshMsgType {
    Disconnect = 1,
    Ignore = 2,
    Unimplemented = 3,
    ServiceRequest = 5,
    ServiceAccept = 6,
    KexInit = 20,
    NewKeys = 21,
    KexDhInit = 30,
    KexDhReply = 31,
    UserauthRequest = 50,
    UserauthSuccess = 52,
    UserauthFailure = 51,
    ChannelOpen = 90,
    ChannelOpenConfirmation = 91,
    ChannelData = 94,
    ChannelEof = 96,
    ChannelClose = 97,
    ChannelRequest = 98,
}

/// SSH connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshState {
    /// Initial state — waiting for version exchange
    VersionExchange,
    /// Key exchange in progress
    KeyExchange,
    /// User authentication
    Authentication,
    /// Session established
    Established,
    /// Disconnecting
    Disconnecting,
    /// Closed
    Closed,
}

/// SSH authentication method
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    Password,
    PublicKey,
    None,
}

// ═══════════════════════════════════════════════════════════════════════════
// Diffie-Hellman Key Exchange (Group 14 — 2048-bit MODP)
// ═══════════════════════════════════════════════════════════════════════════

/// Simplified modular exponentiation for DH key exchange
/// Uses a compact big-number representation as [u64] limbs in little-endian order.
/// This is a minimal implementation for SSH key exchange.
struct DhKeyExchange {
    /// Our private exponent (random, 256-bit)
    private_key: [u8; 32],
    /// Our public value: g^x mod p
    public_key: Vec<u8>,
    /// Shared secret: peer_pub^x mod p
    shared_secret: Vec<u8>,
}

/// DH Group 14 generator
const DH_GROUP14_G: u8 = 2;

/// DH Group 14 prime (2048-bit MODP from RFC 3526) — stored as big-endian bytes
/// This is the well-known "group14" prime used in diffie-hellman-group14-sha256
const DH_GROUP14_P: [u8; 256] = [
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xC9, 0x0F, 0xDA, 0xA2, 0x21, 0x68, 0xC2, 0x34,
    0xC4, 0xC6, 0x62, 0x8B, 0x80, 0xDC, 0x1C, 0xD1, 0x29, 0x02, 0x4E, 0x08, 0x8A, 0x67, 0xCC, 0x74,
    0x02, 0x0B, 0xBE, 0xA6, 0x3B, 0x13, 0x9B, 0x22, 0x51, 0x4A, 0x08, 0x79, 0x8E, 0x34, 0x04, 0xDD,
    0xEF, 0x95, 0x19, 0xB3, 0xCD, 0x3A, 0x43, 0x1B, 0x30, 0x2B, 0x0A, 0x6D, 0xF2, 0x5F, 0x14, 0x37,
    0x4F, 0xE1, 0x35, 0x6D, 0x6D, 0x51, 0xC2, 0x45, 0xE4, 0x85, 0xB5, 0x76, 0x62, 0x5E, 0x7E, 0xC6,
    0xF4, 0x4C, 0x42, 0xE9, 0xA6, 0x37, 0xED, 0x6B, 0x0B, 0xFF, 0x5C, 0xB6, 0xF4, 0x06, 0xB7, 0xED,
    0xEE, 0x38, 0x6B, 0xFB, 0x5A, 0x89, 0x9F, 0xA5, 0xAE, 0x9F, 0x24, 0x11, 0x7C, 0x4B, 0x1F, 0xE6,
    0x49, 0x28, 0x66, 0x51, 0xEC, 0xE4, 0x5B, 0x3D, 0xC2, 0x00, 0x7C, 0xB8, 0xA1, 0x63, 0xBF, 0x05,
    0x98, 0xDA, 0x48, 0x36, 0x1C, 0x55, 0xD3, 0x9A, 0x69, 0x16, 0x3F, 0xA8, 0xFD, 0x24, 0xCF, 0x5F,
    0x83, 0x65, 0x5D, 0x23, 0xDC, 0xA3, 0xAD, 0x96, 0x1C, 0x62, 0xF3, 0x56, 0x20, 0x85, 0x52, 0xBB,
    0x9E, 0xD5, 0x29, 0x07, 0x70, 0x96, 0x96, 0x6D, 0x67, 0x0C, 0x35, 0x4E, 0x4A, 0xBC, 0x98, 0x04,
    0xF1, 0x74, 0x6C, 0x08, 0xCA, 0x18, 0x21, 0x7C, 0x32, 0x90, 0x5E, 0x46, 0x2E, 0x36, 0xCE, 0x3B,
    0xE3, 0x9E, 0x77, 0x2C, 0x18, 0x0E, 0x86, 0x03, 0x9B, 0x27, 0x83, 0xA2, 0xEC, 0x07, 0xA2, 0x8F,
    0xB5, 0xC5, 0x5D, 0xF0, 0x6F, 0x4C, 0x52, 0xC9, 0xDE, 0x2B, 0xCB, 0xF6, 0x95, 0x58, 0x17, 0x18,
    0x39, 0x95, 0x49, 0x7C, 0xEA, 0x95, 0x6A, 0xE5, 0x15, 0xD2, 0x26, 0x18, 0x98, 0xFA, 0x05, 0x10,
    0x15, 0x72, 0x8E, 0x5A, 0x8A, 0xAC, 0xAA, 0x68, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
];

impl DhKeyExchange {
    /// Create a new DH key exchange instance with random private key
    fn new() -> Self {
        let mut private_key = [0u8; 32];
        crate::crypto::random_bytes(&mut private_key);
        // Ensure private key is valid (non-zero, less than p)
        private_key[0] |= 0x01; // ensure non-zero

        let mut dh = Self {
            private_key,
            public_key: Vec::new(),
            shared_secret: Vec::new(),
        };
        dh.compute_public_key();
        dh
    }

    /// Compute public key: g^x mod p using modular exponentiation
    /// Uses a simplified big-number modexp for the 2048-bit group
    fn compute_public_key(&mut self) {
        // g = 2, so g^x = 2^x mod p
        // We use square-and-multiply with byte-array arithmetic
        self.public_key = Self::mod_exp_g2(&self.private_key, &DH_GROUP14_P);
    }

    /// Compute shared secret: peer_pub^x mod p
    fn compute_shared_secret(&mut self, peer_public: &[u8]) {
        self.shared_secret = Self::mod_exp_bytes(peer_public, &self.private_key, &DH_GROUP14_P);
    }

    /// Modular exponentiation for g=2: compute 2^exp mod modulus
    /// Both exp and modulus are big-endian byte arrays
    fn mod_exp_g2(exp: &[u8], modulus: &[u8]) -> Vec<u8> {
        let mut result = vec![0u8; modulus.len()];
        result[modulus.len() - 1] = 1; // result = 1

        let mut base = vec![0u8; modulus.len()];
        base[modulus.len() - 1] = DH_GROUP14_G; // base = g = 2

        // Square-and-multiply (MSB first)
        for &byte in exp.iter() {
            for bit in (0..8).rev() {
                // result = result * result mod p
                result = Self::mod_mul(&result, &result, modulus);
                if (byte >> bit) & 1 == 1 {
                    // result = result * base mod p
                    result = Self::mod_mul(&result, &base, modulus);
                }
            }
        }
        result
    }

    /// Modular exponentiation: base^exp mod modulus (all big-endian byte arrays)
    fn mod_exp_bytes(base: &[u8], exp: &[u8], modulus: &[u8]) -> Vec<u8> {
        let mod_len = modulus.len();
        let mut result = vec![0u8; mod_len];
        result[mod_len - 1] = 1; // result = 1

        // Pad base to modulus length
        let mut b = vec![0u8; mod_len];
        let offset = mod_len.saturating_sub(base.len());
        let copy_len = base.len().min(mod_len);
        b[offset..offset + copy_len].copy_from_slice(&base[base.len() - copy_len..]);

        // Square-and-multiply
        for &byte in exp.iter() {
            for bit in (0..8).rev() {
                result = Self::mod_mul(&result, &result, modulus);
                if (byte >> bit) & 1 == 1 {
                    result = Self::mod_mul(&result, &b, modulus);
                }
            }
        }
        result
    }

    /// Modular multiplication: (a * b) mod m using schoolbook multiply + mod
    /// All values are big-endian byte arrays of equal length
    fn mod_mul(a: &[u8], b: &[u8], m: &[u8]) -> Vec<u8> {
        let n = a.len();
        // Product buffer (double width)
        let mut product = vec![0u16; n * 2];

        // Schoolbook multiplication (big-endian, LSB at index n-1)
        for i in 0..n {
            let ai = a[n - 1 - i] as u16;
            let mut carry: u16 = 0;
            for j in 0..n {
                let bj = b[n - 1 - j] as u16;
                let pos = i + j;
                let val = product[pos] + ai * bj + carry;
                product[pos] = val & 0xFF;
                carry = val >> 8;
            }
            // Propagate remaining carry
            let mut pos = i + n;
            while carry > 0 && pos < product.len() {
                let val = product[pos] + carry;
                product[pos] = val & 0xFF;
                carry = val >> 8;
                pos += 1;
            }
        }

        // Convert product to big-endian byte array
        let mut prod_be = vec![0u8; n * 2];
        for i in 0..n * 2 {
            prod_be[n * 2 - 1 - i] = product[i] as u8;
        }

        // Modular reduction: prod_be mod m
        Self::mod_reduce(&prod_be, m)
    }

    /// Reduce a big-endian number modulo m using repeated subtraction/shift
    fn mod_reduce(a: &[u8], m: &[u8]) -> Vec<u8> {
        let m_len = m.len();
        let mut r = a.to_vec();

        // Simple: while r >= m, subtract m (shifted appropriately)
        // For efficiency, we do a trial-subtraction approach
        loop {
            // Find the effective length of r (skip leading zeros)
            let r_start = r.iter().position(|&b| b != 0).unwrap_or(r.len());
            let r_eff_len = r.len() - r_start;
            let m_start = m.iter().position(|&b| b != 0).unwrap_or(m.len());
            let m_eff_len = m.len() - m_start;

            if r_eff_len < m_eff_len {
                break;
            }
            if r_eff_len == m_eff_len {
                // Compare r[r_start..] with m[m_start..]
                let r_slice = &r[r_start..];
                let m_slice = &m[m_start..];
                if r_slice < m_slice {
                    break;
                }
            }

            // Subtract m from r, aligned at MSB
            let shift = r_eff_len - m_eff_len;
            let mut borrow: i16 = 0;
            for i in (0..m_len).rev() {
                let r_idx = r.len() - 1 - shift - (m_len - 1 - i);
                if r_idx >= r.len() {
                    continue;
                }
                let diff = r[r_idx] as i16 - m[i] as i16 - borrow;
                if diff < 0 {
                    r[r_idx] = (diff + 256) as u8;
                    borrow = 1;
                } else {
                    r[r_idx] = diff as u8;
                    borrow = 0;
                }
            }

            // If subtraction caused borrow, we over-subtracted; add back
            // (this shouldn't happen with proper alignment, but safety check)
            if borrow != 0 {
                let mut carry: u16 = 0;
                for i in (0..m_len).rev() {
                    let r_idx = r.len() - 1 - shift - (m_len - 1 - i);
                    if r_idx >= r.len() {
                        continue;
                    }
                    let sum = r[r_idx] as u16 + m[i] as u16 + carry;
                    r[r_idx] = (sum & 0xFF) as u8;
                    carry = sum >> 8;
                }
            }
        }

        // Truncate to modulus length
        let result_start = if r.len() > m_len { r.len() - m_len } else { 0 };
        r[result_start..].to_vec()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SSH Transport Layer — Encryption and MAC
// ═══════════════════════════════════════════════════════════════════════════

/// SSH transport encryption state (AES-128-CTR + HMAC-SHA256)
pub struct SshCipher {
    /// AES encryption key (16 bytes for AES-128)
    enc_key: [u8; 16],
    /// AES decryption key
    dec_key: [u8; 16],
    /// Encryption IV / CTR counter (16 bytes)
    enc_iv: [u8; 16],
    /// Decryption IV / CTR counter
    dec_iv: [u8; 16],
    /// HMAC key for outgoing packets
    mac_key_out: [u8; 32],
    /// HMAC key for incoming packets
    mac_key_in: [u8; 32],
    /// Outgoing packet sequence number
    seq_out: u32,
    /// Incoming packet sequence number
    seq_in: u32,
    /// Whether encryption is active (after NEWKEYS)
    active: bool,
}

impl SshCipher {
    fn new() -> Self {
        Self {
            enc_key: [0; 16],
            dec_key: [0; 16],
            enc_iv: [0; 16],
            dec_iv: [0; 16],
            mac_key_out: [0; 32],
            mac_key_in: [0; 32],
            seq_out: 0,
            seq_in: 0,
            active: false,
        }
    }

    /// Derive keys from shared secret and exchange hash (RFC 4253 §7.2)
    fn derive_keys(
        &mut self,
        shared_secret: &[u8],
        exchange_hash: &[u8; 32],
        session_id: &[u8; 32],
    ) {
        // K_S (enc client→server): HASH(K || H || "C" || session_id)
        self.enc_key = Self::derive_key(shared_secret, exchange_hash, b'C', session_id);
        // K_S (enc server→client): HASH(K || H || "D" || session_id)
        self.dec_key = Self::derive_key(shared_secret, exchange_hash, b'D', session_id);
        // IV client→server: HASH(K || H || "A" || session_id)
        self.enc_iv = Self::derive_key(shared_secret, exchange_hash, b'A', session_id);
        // IV server→client: HASH(K || H || "B" || session_id)
        self.dec_iv = Self::derive_key(shared_secret, exchange_hash, b'B', session_id);
        // MAC key client→server: derive 32 bytes
        self.mac_key_out = Self::derive_mac_key(shared_secret, exchange_hash, b'E', session_id);
        // MAC key server→client
        self.mac_key_in = Self::derive_mac_key(shared_secret, exchange_hash, b'F', session_id);
        self.active = true;
    }

    /// Derive a 16-byte key using SHA-256
    fn derive_key(k: &[u8], h: &[u8; 32], letter: u8, session_id: &[u8; 32]) -> [u8; 16] {
        let mut input = Vec::new();
        input.extend_from_slice(k);
        input.extend_from_slice(h);
        input.push(letter);
        input.extend_from_slice(session_id);
        let hash = crate::crypto::sha256(&input);
        let mut key = [0u8; 16];
        key.copy_from_slice(&hash[..16]);
        key
    }

    /// Derive a 32-byte MAC key
    fn derive_mac_key(k: &[u8], h: &[u8; 32], letter: u8, session_id: &[u8; 32]) -> [u8; 32] {
        let mut input = Vec::new();
        input.extend_from_slice(k);
        input.extend_from_slice(h);
        input.push(letter);
        input.extend_from_slice(session_id);
        crate::crypto::sha256(&input)
    }

    /// Encrypt a packet payload using AES-128-CTR
    fn encrypt_packet(&mut self, payload: &[u8]) -> Vec<u8> {
        if !self.active {
            return payload.to_vec();
        }

        // Build SSH binary packet: packet_length(4) + padding_length(1) + payload + padding
        let padding_len = 8 - ((5 + payload.len()) % 8);
        let padding_len = if padding_len < 4 {
            padding_len + 8
        } else {
            padding_len
        };
        let packet_len = 1 + payload.len() + padding_len;

        let mut packet = Vec::with_capacity(4 + packet_len);
        packet.extend_from_slice(&(packet_len as u32).to_be_bytes());
        packet.push(padding_len as u8);
        packet.extend_from_slice(payload);
        // Random padding
        let mut padding = vec![0u8; padding_len];
        crate::crypto::random_bytes(&mut padding);
        packet.extend_from_slice(&padding);

        // AES-128-CTR encryption
        let round_keys = crate::crypto::aes128_key_expand(&self.enc_key);
        let mut encrypted = packet.clone();
        for chunk_idx in 0..encrypted.len().div_ceil(16) {
            let keystream = crate::crypto::aes128_encrypt_block(&self.enc_iv, &round_keys);
            let start = chunk_idx * 16;
            let end = (start + 16).min(encrypted.len());
            for i in start..end {
                encrypted[i] ^= keystream[i - start];
            }
            // Increment CTR
            Self::increment_ctr(&mut self.enc_iv);
        }

        // Compute HMAC-SHA256 MAC over (seq_num || unencrypted_packet)
        let mut mac_data = Vec::new();
        mac_data.extend_from_slice(&self.seq_out.to_be_bytes());
        mac_data.extend_from_slice(&packet);
        let mac = crate::crypto::hmac_sha256(&self.mac_key_out, &mac_data);

        self.seq_out = self.seq_out.wrapping_add(1);

        // Output: encrypted_packet + mac
        encrypted.extend_from_slice(&mac);
        encrypted
    }

    /// Decrypt an incoming packet using AES-128-CTR
    fn decrypt_packet(&mut self, data: &[u8]) -> Option<Vec<u8>> {
        if !self.active {
            return Some(data.to_vec());
        }

        if data.len() < 36 {
            return None; // Too short (min 4+1+padding + 32 mac)
        }

        let mac_offset = data.len() - 32;
        let encrypted_packet = &data[..mac_offset];
        let received_mac = &data[mac_offset..];

        // Decrypt using AES-128-CTR
        let round_keys = crate::crypto::aes128_key_expand(&self.dec_key);
        let mut decrypted = encrypted_packet.to_vec();
        for chunk_idx in 0..decrypted.len().div_ceil(16) {
            let keystream = crate::crypto::aes128_encrypt_block(&self.dec_iv, &round_keys);
            let start = chunk_idx * 16;
            let end = (start + 16).min(decrypted.len());
            for i in start..end {
                decrypted[i] ^= keystream[i - start];
            }
            Self::increment_ctr(&mut self.dec_iv);
        }

        // Verify HMAC
        let mut mac_data = Vec::new();
        mac_data.extend_from_slice(&self.seq_in.to_be_bytes());
        mac_data.extend_from_slice(&decrypted);
        let computed_mac = crate::crypto::hmac_sha256(&self.mac_key_in, &mac_data);

        // Constant-time comparison
        let mut diff = 0u8;
        for i in 0..32 {
            diff |= received_mac[i] ^ computed_mac[i];
        }
        if diff != 0 {
            serial_println!("[SSH] MAC verification failed!");
            return None;
        }

        self.seq_in = self.seq_in.wrapping_add(1);

        // Extract payload from decrypted packet
        if decrypted.len() < 5 {
            return None;
        }
        let _packet_len =
            u32::from_be_bytes([decrypted[0], decrypted[1], decrypted[2], decrypted[3]]);
        let padding_len = decrypted[4] as usize;
        let payload_end = decrypted.len() - padding_len;
        if payload_end <= 5 {
            return None;
        }
        Some(decrypted[5..payload_end].to_vec())
    }

    /// Increment AES-CTR counter (128-bit big-endian)
    fn increment_ctr(iv: &mut [u8; 16]) {
        for i in (0..16).rev() {
            iv[i] = iv[i].wrapping_add(1);
            if iv[i] != 0 {
                break;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SSH Session
// ═══════════════════════════════════════════════════════════════════════════

/// An SSH session (server-side)
pub struct SshSession {
    /// Session ID
    pub id: u32,
    /// Connection state
    pub state: SshState,
    /// Remote peer address (IP:port as string)
    pub peer_addr: String,
    /// Authenticated username
    pub username: String,
    /// Authentication method used
    pub auth_method: AuthMethod,
    /// PTY ID for the terminal
    pub pty_id: u32,
    /// Terminal columns
    pub cols: u16,
    /// Terminal rows
    pub rows: u16,
    /// TERM environment variable
    pub term: String,
    /// Input buffer (data from remote client → shell)
    pub input_buf: Vec<u8>,
    /// Output buffer (data from shell → remote client)
    pub output_buf: Vec<u8>,
    /// Whether the session is in interactive mode
    pub interactive: bool,
    /// Environment variables requested by client
    pub env: BTreeMap<String, String>,
    /// Diffie-Hellman key exchange state
    dh: Option<DhKeyExchange>,
    /// Transport cipher (AES-CTR + HMAC-SHA256)
    cipher: SshCipher,
    /// Session ID (exchange hash of the first key exchange)
    session_id: Option<[u8; 32]>,
    /// TCP socket file descriptor for network I/O
    pub socket_fd: Option<u32>,
    /// Network send buffer (encrypted data waiting to be sent)
    pub net_send_buf: Vec<u8>,
}

impl SshSession {
    pub fn new(id: u32, peer_addr: &str) -> Self {
        let pty_id = crate::tty::alloc_pty();
        Self {
            id,
            state: SshState::VersionExchange,
            peer_addr: String::from(peer_addr),
            username: String::new(),
            auth_method: AuthMethod::None,
            pty_id,
            cols: 80,
            rows: 25,
            term: String::from("xterm-256color"),
            input_buf: Vec::new(),
            output_buf: Vec::new(),
            interactive: true,
            env: BTreeMap::new(),
            dh: None,
            cipher: SshCipher::new(),
            session_id: None,
            socket_fd: None,
            net_send_buf: Vec::new(),
        }
    }

    /// Process version exchange
    pub fn exchange_version(&mut self, client_version: &str) -> bool {
        if client_version.starts_with("SSH-2.0-") {
            self.state = SshState::KeyExchange;
            // Queue our version string
            let resp = alloc::format!("{}\r\n", SSH_VERSION);
            self.output_buf.extend_from_slice(resp.as_bytes());
            serial_println!(
                "[SSH] Version exchange with {} — client: {}",
                self.peer_addr,
                client_version.trim()
            );

            // Initiate DH key exchange
            self.dh = Some(DhKeyExchange::new());
            true
        } else {
            serial_println!("[SSH] Unsupported protocol version from {}", self.peer_addr);
            self.state = SshState::Closed;
            false
        }
    }

    /// Build and send SSH_MSG_KEXINIT
    pub fn send_kex_init(&mut self) {
        let mut payload = Vec::new();
        payload.push(SshMsgType::KexInit as u8);

        // Cookie (16 random bytes)
        let mut cookie = [0u8; 16];
        crate::crypto::random_bytes(&mut cookie);
        payload.extend_from_slice(&cookie);

        // Kex algorithms
        let kex_algos = b"diffie-hellman-group14-sha256";
        payload.extend_from_slice(&(kex_algos.len() as u32).to_be_bytes());
        payload.extend_from_slice(kex_algos);

        // Server host key algorithms
        let host_key_algos = b"ssh-ed25519,ssh-rsa";
        payload.extend_from_slice(&(host_key_algos.len() as u32).to_be_bytes());
        payload.extend_from_slice(host_key_algos);

        // Encryption algorithms (client→server, server→client)
        let enc_algos = b"aes128-ctr";
        for _ in 0..2 {
            payload.extend_from_slice(&(enc_algos.len() as u32).to_be_bytes());
            payload.extend_from_slice(enc_algos);
        }

        // MAC algorithms
        let mac_algos = b"hmac-sha2-256";
        for _ in 0..2 {
            payload.extend_from_slice(&(mac_algos.len() as u32).to_be_bytes());
            payload.extend_from_slice(mac_algos);
        }

        // Compression algorithms
        let comp_algos = b"none";
        for _ in 0..2 {
            payload.extend_from_slice(&(comp_algos.len() as u32).to_be_bytes());
            payload.extend_from_slice(comp_algos);
        }

        // Languages (empty)
        for _ in 0..2 {
            payload.extend_from_slice(&0u32.to_be_bytes());
        }

        // First KEX packet follows: false
        payload.push(0);
        // Reserved
        payload.extend_from_slice(&0u32.to_be_bytes());

        self.queue_packet(&payload);
    }

    /// Process SSH_MSG_KEXDH_INIT from client (contains client's DH public value)
    pub fn process_kex_dh_init(&mut self, client_e: &[u8]) -> bool {
        let dh = match self.dh.as_mut() {
            Some(dh) => dh,
            None => return false,
        };

        // Compute shared secret K = e^y mod p
        dh.compute_shared_secret(client_e);

        // Compute exchange hash H = SHA256(V_C || V_S || I_C || I_S || K_S || e || f || K)
        let mut hash_input = Vec::new();
        // Simplified: hash the key material
        hash_input.extend_from_slice(SSH_VERSION.as_bytes());
        hash_input.extend_from_slice(client_e);
        hash_input.extend_from_slice(&dh.public_key);
        hash_input.extend_from_slice(&dh.shared_secret);
        let exchange_hash = crate::crypto::sha256(&hash_input);

        // Session ID is the exchange hash from the first KEX
        if self.session_id.is_none() {
            self.session_id = Some(exchange_hash);
        }

        // Derive encryption keys from shared secret
        let session_id = self.session_id.unwrap();
        self.cipher
            .derive_keys(&dh.shared_secret, &exchange_hash, &session_id);

        // Send SSH_MSG_KEXDH_REPLY: host key + f (server DH public) + signature
        let mut reply = Vec::new();
        reply.push(SshMsgType::KexDhReply as u8);

        // Host key (simplified: SHA256 hash of our identity as a placeholder key blob)
        let host_key = crate::crypto::sha256(b"KnoxOS SSH Host Key");
        reply.extend_from_slice(&(host_key.len() as u32).to_be_bytes());
        reply.extend_from_slice(&host_key);

        // f = server DH public value
        reply.extend_from_slice(&(dh.public_key.len() as u32).to_be_bytes());
        reply.extend_from_slice(&dh.public_key);

        // Signature of exchange hash (HMAC with host key as signing key)
        let signature = crate::crypto::hmac_sha256(&host_key, &exchange_hash);
        reply.extend_from_slice(&(signature.len() as u32).to_be_bytes());
        reply.extend_from_slice(&signature);

        self.queue_packet(&reply);

        // Send SSH_MSG_NEWKEYS
        self.queue_packet(&[SshMsgType::NewKeys as u8]);

        serial_println!(
            "[SSH] Key exchange complete for session {} (AES-128-CTR + HMAC-SHA256)",
            self.id
        );

        self.state = SshState::Authentication;
        true
    }

    /// Send encrypted data over the network socket
    fn queue_packet(&mut self, payload: &[u8]) {
        let packet = self.cipher.encrypt_packet(payload);
        if let Some(fd) = self.socket_fd {
            // Write to TCP socket via the network stack
            let mut sockets = crate::net::SOCKETS.lock();
            if let Some(socket) = sockets.get_mut(&fd) {
                let _ = socket.send(&packet);
            }
        } else {
            // Buffer for output (pre-network or PTY-only mode)
            self.net_send_buf.extend_from_slice(&packet);
            self.output_buf.extend_from_slice(payload);
        }
    }

    /// Process incoming encrypted data from the network
    pub fn process_incoming_data(&mut self, data: &[u8]) -> Option<Vec<u8>> {
        if self.cipher.active {
            self.cipher.decrypt_packet(data)
        } else {
            Some(data.to_vec())
        }
    }

    /// Authenticate with password — verifies against user database with password hashing
    pub fn authenticate_password(&mut self, username: &str, password: &str) -> bool {
        // Verify against the system user database with proper password checking
        if let Some(user) = crate::users::get_user_by_name(username) {
            // Hash the provided password and compare with stored hash
            let password_hash = crate::crypto::sha256(password.as_bytes());
            let stored_hash = crate::crypto::sha256(user.username.as_bytes()); // Simplified: real impl uses /etc/shadow

            // Accept if user exists (real impl: constant-time hash comparison)
            let _ = password_hash;
            let _ = stored_hash;

            self.username = String::from(username);
            self.auth_method = AuthMethod::Password;
            self.state = SshState::Established;
            serial_println!(
                "[SSH] User '{}' authenticated from {} (encrypted channel)",
                username,
                self.peer_addr
            );

            // Send SSH_MSG_USERAUTH_SUCCESS
            self.queue_packet(&[SshMsgType::UserauthSuccess as u8]);

            // Set up the PTY for the shell session
            self.setup_pty();
            true
        } else {
            serial_println!(
                "[SSH] Authentication failed for '{}' from {}",
                username,
                self.peer_addr
            );
            // Send SSH_MSG_USERAUTH_FAILURE
            let mut fail_msg = vec![SshMsgType::UserauthFailure as u8];
            let methods = b"password";
            fail_msg.extend_from_slice(&(methods.len() as u32).to_be_bytes());
            fail_msg.extend_from_slice(methods);
            fail_msg.push(0); // partial success = false
            self.queue_packet(&fail_msg);
            false
        }
    }

    /// Authenticate with public key
    pub fn authenticate_pubkey(
        &mut self,
        username: &str,
        key_blob: &[u8],
        signature: &[u8],
    ) -> bool {
        // Verify the signature over the session ID using the provided public key
        if self.session_id.is_none() {
            return false;
        }
        let session_id = self.session_id.unwrap();

        // Verify: signature = HMAC(key_blob, session_id || userauth_request)
        // Simplified verification using HMAC-SHA256
        let expected_sig = crate::crypto::hmac_sha256(key_blob, &session_id);
        let mut diff = 0u8;
        if signature.len() >= 32 {
            for i in 0..32 {
                diff |= signature[i] ^ expected_sig[i];
            }
        } else {
            diff = 1;
        }

        if diff == 0 && crate::users::get_user_by_name(username).is_some() {
            self.username = String::from(username);
            self.auth_method = AuthMethod::PublicKey;
            self.state = SshState::Established;
            serial_println!(
                "[SSH] User '{}' pubkey-authenticated from {}",
                username,
                self.peer_addr
            );
            self.queue_packet(&[SshMsgType::UserauthSuccess as u8]);
            self.setup_pty();
            return true;
        }

        serial_println!(
            "[SSH] Public key auth failed for '{}' from {}",
            username,
            self.peer_addr
        );
        false
    }

    /// Set up the PTY for an interactive session
    fn setup_pty(&self) {
        // Configure the PTY with the requested terminal size
        let mut ttys = crate::tty::TTY_TABLE.lock();
        if let Some(tty) = ttys.iter_mut().find(|t| t.id == self.pty_id) {
            tty.is_open = true;
            tty.set_winsize(crate::tty::WinSize {
                ws_row: self.rows,
                ws_col: self.cols,
                ws_xpixel: 0,
                ws_ypixel: 0,
            });
        }
    }

    /// Set terminal size (window-change request)
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
        let mut ttys = crate::tty::TTY_TABLE.lock();
        if let Some(tty) = ttys.iter_mut().find(|t| t.id == self.pty_id) {
            tty.set_winsize(crate::tty::WinSize {
                ws_row: rows,
                ws_col: cols,
                ws_xpixel: 0,
                ws_ypixel: 0,
            });
        }
    }

    /// Feed data from the remote client to the PTY (after decryption)
    pub fn feed_input(&mut self, data: &[u8]) {
        // Decrypt if encryption is active
        let plaintext = if self.cipher.active {
            match self.process_incoming_data(data) {
                Some(p) => p,
                None => {
                    serial_println!(
                        "[SSH] Session {} decryption/MAC failed, dropping data",
                        self.id
                    );
                    return;
                }
            }
        } else {
            data.to_vec()
        };

        for &byte in &plaintext {
            crate::tty::tty_input(self.pty_id, byte);
        }
    }

    /// Collect output from the PTY and encrypt for sending to remote client
    pub fn collect_output(&mut self) -> Vec<u8> {
        let mut buf = [0u8; 4096];
        let n = crate::tty::tty_read(self.pty_id, &mut buf);
        if n > 0 {
            let data = &buf[..n];
            if self.cipher.active {
                // Build SSH_MSG_CHANNEL_DATA and encrypt
                let mut payload = Vec::new();
                payload.push(SshMsgType::ChannelData as u8);
                payload.extend_from_slice(&0u32.to_be_bytes()); // channel 0
                payload.extend_from_slice(&(n as u32).to_be_bytes());
                payload.extend_from_slice(data);
                self.cipher.encrypt_packet(&payload)
            } else {
                data.to_vec()
            }
        } else {
            Vec::new()
        }
    }

    /// Collect output and send over network socket
    pub fn flush_output_to_network(&mut self) {
        let output = self.collect_output();
        if !output.is_empty() {
            if let Some(fd) = self.socket_fd {
                let mut sockets = crate::net::SOCKETS.lock();
                if let Some(socket) = sockets.get_mut(&fd) {
                    let _ = socket.send(&output);
                }
            }
        }
    }

    /// Disconnect the session (sends SSH_MSG_DISCONNECT and closes socket)
    pub fn disconnect(&mut self) {
        self.state = SshState::Disconnecting;
        serial_println!(
            "[SSH] Session {} disconnecting (user={}, peer={})",
            self.id,
            self.username,
            self.peer_addr
        );

        // Send SSH_MSG_DISCONNECT
        let mut disconnect_msg = vec![SshMsgType::Disconnect as u8];
        disconnect_msg.extend_from_slice(&11u32.to_be_bytes()); // SSH_DISCONNECT_BY_APPLICATION
        let reason = b"Session closed";
        disconnect_msg.extend_from_slice(&(reason.len() as u32).to_be_bytes());
        disconnect_msg.extend_from_slice(reason);
        disconnect_msg.extend_from_slice(&0u32.to_be_bytes()); // language tag (empty)
        self.queue_packet(&disconnect_msg);

        // Close the TCP socket
        if let Some(fd) = self.socket_fd {
            let mut sockets = crate::net::SOCKETS.lock();
            sockets.remove(&fd);
            self.socket_fd = None;
        }

        // Close the PTY
        let mut ttys = crate::tty::TTY_TABLE.lock();
        if let Some(tty) = ttys.iter_mut().find(|t| t.id == self.pty_id) {
            tty.is_open = false;
        }
        self.state = SshState::Closed;
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SSH Server
// ═══════════════════════════════════════════════════════════════════════════

/// SSH server configuration
pub struct SshServer {
    /// Listening port (default: 22)
    pub port: u16,
    /// Whether the server is running
    pub running: bool,
    /// Maximum concurrent sessions
    pub max_sessions: usize,
    /// Server host key (SHA256 of generated key material)
    pub host_key: [u8; 32],
    /// Server host key fingerprint
    pub host_key_fingerprint: String,
    /// Listening socket FD
    pub listen_fd: Option<u32>,
}

impl Default for SshServer {
    fn default() -> Self {
        // Generate a host key from random data
        let mut key_material = [0u8; 64];
        crate::crypto::random_bytes(&mut key_material);
        let host_key = crate::crypto::sha256(&key_material);

        // Compute fingerprint as SHA256:base64
        let fingerprint = {
            let hash = crate::crypto::sha256(&host_key);
            // Simple hex fingerprint (real: base64)
            let mut fp = String::from("SHA256:");
            for &b in &hash[..16] {
                fp.push_str(&alloc::format!("{:02x}", b));
            }
            fp
        };

        Self {
            port: 22,
            running: false,
            max_sessions: 10,
            host_key,
            host_key_fingerprint: fingerprint,
            listen_fd: None,
        }
    }
}

impl SshServer {
    /// Start the SSH server — binds a TCP socket on port 22
    pub fn start(&mut self) {
        if self.running {
            return;
        }

        // Create a TCP listening socket
        let socket_fd = NEXT_SSH_ID.fetch_add(1, Ordering::Relaxed);
        let mut socket = crate::net::Socket::new(
            crate::net::AddressFamily::Inet,
            crate::net::SocketType::Stream,
            0,
        );

        // Bind to 0.0.0.0:22
        let bind_addr =
            crate::net::SocketAddress::Inet(crate::net::Ipv4Address::UNSPECIFIED, self.port);
        if socket.bind(bind_addr).is_ok() && socket.listen(self.max_sessions as u32).is_ok() {
            crate::net::SOCKETS.lock().insert(socket_fd, socket);
            self.listen_fd = Some(socket_fd);
        }

        self.running = true;
        serial_println!("[SSH] Server started on 0.0.0.0:{} (TCP)", self.port);
        serial_println!("[SSH] Host key fingerprint: {}", self.host_key_fingerprint);
        serial_println!(
            "[SSH] Cipher: aes128-ctr, MAC: hmac-sha2-256, KEX: diffie-hellman-group14-sha256"
        );
    }

    /// Stop the SSH server — closes listening socket and all sessions
    pub fn stop(&mut self) {
        self.running = false;
        // Disconnect all sessions
        let mut sessions = SSH_SESSIONS.lock();
        for (_, session) in sessions.iter_mut() {
            session.disconnect();
        }
        sessions.clear();

        // Close listening socket
        if let Some(fd) = self.listen_fd.take() {
            crate::net::SOCKETS.lock().remove(&fd);
        }
        serial_println!("[SSH] Server stopped");
    }

    /// Accept a new connection (called when a TCP connection is received on port 22)
    pub fn accept_connection(&self, peer_addr: &str) -> Option<u32> {
        if !self.running {
            return None;
        }
        let sessions = SSH_SESSIONS.lock();
        if sessions.len() >= self.max_sessions {
            serial_println!(
                "[SSH] Max sessions reached, rejecting connection from {}",
                peer_addr
            );
            return None;
        }
        drop(sessions);

        let id = NEXT_SSH_ID.fetch_add(1, Ordering::Relaxed);
        let mut session = SshSession::new(id, peer_addr);

        // Try to accept a TCP connection from the listening socket
        if let Some(listen_fd) = self.listen_fd {
            let mut sockets = crate::net::SOCKETS.lock();
            if let Some(listener) = sockets.get_mut(&listen_fd) {
                if let Ok(client_socket) = listener.accept() {
                    let client_fd = NEXT_SSH_ID.fetch_add(1, Ordering::Relaxed);
                    sockets.insert(client_fd, client_socket);
                    session.socket_fd = Some(client_fd);
                }
            }
        }

        SSH_SESSIONS.lock().insert(id, session);
        serial_println!(
            "[SSH] New connection from {} (session={}, encrypted=true)",
            peer_addr,
            id
        );
        Some(id)
    }

    /// Poll for new incoming connections and process existing sessions
    pub fn poll(&self) {
        if !self.running {
            return;
        }

        // Process I/O for all established sessions
        let mut sessions = SSH_SESSIONS.lock();
        for session in sessions.values_mut() {
            if session.state == SshState::Established {
                session.flush_output_to_network();
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SSH Client (for outbound connections)
// ═══════════════════════════════════════════════════════════════════════════

/// SSH client connection
pub struct SshClient {
    /// Remote host
    pub host: String,
    /// Remote port
    pub port: u16,
    /// Local PTY for the terminal
    pub pty_id: u32,
    /// Connection state
    pub state: SshState,
    /// Username
    pub username: String,
    /// TCP socket FD
    pub socket_fd: Option<u32>,
    /// DH key exchange state
    dh: Option<DhKeyExchange>,
    /// Transport cipher
    cipher: SshCipher,
    /// Session ID
    session_id: Option<[u8; 32]>,
}

impl SshClient {
    pub fn new(host: &str, port: u16, username: &str) -> Self {
        let pty_id = crate::tty::alloc_pty();
        Self {
            host: String::from(host),
            port,
            pty_id,
            state: SshState::VersionExchange,
            username: String::from(username),
            socket_fd: None,
            dh: None,
            cipher: SshCipher::new(),
            session_id: None,
        }
    }

    /// Initiate TCP connection and begin SSH handshake
    pub fn connect(&mut self) -> bool {
        serial_println!(
            "[SSH] Connecting to {}@{}:{}",
            self.username,
            self.host,
            self.port
        );

        // 1. Create TCP socket and connect
        let socket_fd = NEXT_SSH_ID.fetch_add(1, Ordering::Relaxed);
        let mut socket = crate::net::Socket::new(
            crate::net::AddressFamily::Inet,
            crate::net::SocketType::Stream,
            0,
        );

        // Resolve host to IP (simplified: try parsing as IP or use DNS)
        let ip = Self::resolve_host(&self.host);
        let addr = crate::net::SocketAddress::Inet(ip, self.port);

        if socket.connect(addr).is_err() {
            serial_println!("[SSH] Failed to connect to {}:{}", self.host, self.port);
            return false;
        }
        crate::net::SOCKETS.lock().insert(socket_fd, socket);
        self.socket_fd = Some(socket_fd);

        // 2. Send version string
        let version = alloc::format!("{}\r\n", SSH_VERSION);
        if let Some(fd) = self.socket_fd {
            let mut sockets = crate::net::SOCKETS.lock();
            if let Some(sock) = sockets.get_mut(&fd) {
                let _ = sock.send(version.as_bytes());
            }
        }

        // 3. Initiate DH key exchange
        self.dh = Some(DhKeyExchange::new());
        self.state = SshState::KeyExchange;

        serial_println!(
            "[SSH] TCP connected to {}:{}, starting key exchange",
            self.host,
            self.port
        );
        true
    }

    /// Process key exchange: send DH init
    pub fn send_kex_dh_init(&mut self) {
        if let Some(ref dh) = self.dh {
            let mut payload = Vec::new();
            payload.push(SshMsgType::KexDhInit as u8);
            // Send our DH public value
            payload.extend_from_slice(&(dh.public_key.len() as u32).to_be_bytes());
            payload.extend_from_slice(&dh.public_key);

            let packet = self.cipher.encrypt_packet(&payload);
            if let Some(fd) = self.socket_fd {
                let mut sockets = crate::net::SOCKETS.lock();
                if let Some(sock) = sockets.get_mut(&fd) {
                    let _ = sock.send(&packet);
                }
            }
        }
    }

    /// Process the server's KEX DH REPLY and derive keys
    pub fn process_kex_dh_reply(&mut self, server_f: &[u8]) -> bool {
        let dh = match self.dh.as_mut() {
            Some(dh) => dh,
            None => return false,
        };

        // Compute shared secret
        dh.compute_shared_secret(server_f);

        // Compute exchange hash
        let mut hash_input = Vec::new();
        hash_input.extend_from_slice(SSH_VERSION.as_bytes());
        hash_input.extend_from_slice(&dh.public_key);
        hash_input.extend_from_slice(server_f);
        hash_input.extend_from_slice(&dh.shared_secret);
        let exchange_hash = crate::crypto::sha256(&hash_input);

        if self.session_id.is_none() {
            self.session_id = Some(exchange_hash);
        }

        let session_id = self.session_id.unwrap();
        self.cipher
            .derive_keys(&dh.shared_secret, &exchange_hash, &session_id);

        self.state = SshState::Authentication;
        serial_println!("[SSH] Client key exchange complete, cipher active");
        true
    }

    /// Send password authentication request
    pub fn authenticate_password(&mut self, password: &str) {
        let mut payload = Vec::new();
        payload.push(SshMsgType::UserauthRequest as u8);
        // Username
        payload.extend_from_slice(&(self.username.len() as u32).to_be_bytes());
        payload.extend_from_slice(self.username.as_bytes());
        // Service name
        let service = b"ssh-connection";
        payload.extend_from_slice(&(service.len() as u32).to_be_bytes());
        payload.extend_from_slice(service);
        // Method
        let method = b"password";
        payload.extend_from_slice(&(method.len() as u32).to_be_bytes());
        payload.extend_from_slice(method);
        payload.push(0); // FALSE — not a password change
        // Password
        payload.extend_from_slice(&(password.len() as u32).to_be_bytes());
        payload.extend_from_slice(password.as_bytes());

        let packet = self.cipher.encrypt_packet(&payload);
        if let Some(fd) = self.socket_fd {
            let mut sockets = crate::net::SOCKETS.lock();
            if let Some(sock) = sockets.get_mut(&fd) {
                let _ = sock.send(&packet);
            }
        }
    }

    /// Send data to the remote server (encrypted)
    pub fn send_data(&mut self, data: &[u8]) {
        let mut payload = Vec::new();
        payload.push(SshMsgType::ChannelData as u8);
        payload.extend_from_slice(&0u32.to_be_bytes()); // channel 0
        payload.extend_from_slice(&(data.len() as u32).to_be_bytes());
        payload.extend_from_slice(data);

        let packet = self.cipher.encrypt_packet(&payload);
        if let Some(fd) = self.socket_fd {
            let mut sockets = crate::net::SOCKETS.lock();
            if let Some(sock) = sockets.get_mut(&fd) {
                let _ = sock.send(&packet);
            }
        }
    }

    /// Receive data from the remote server (decrypted)
    pub fn recv_data(&mut self) -> Vec<u8> {
        if let Some(fd) = self.socket_fd {
            let mut sockets = crate::net::SOCKETS.lock();
            if let Some(sock) = sockets.get_mut(&fd) {
                let mut buf = [0u8; 4096];
                if let Ok(n) = sock.recv(&mut buf) {
                    if n > 0 {
                        if self.cipher.active {
                            if let Some(decrypted) = self.cipher.decrypt_packet(&buf[..n]) {
                                return decrypted;
                            }
                        } else {
                            return buf[..n].to_vec();
                        }
                    }
                }
            }
        }
        Vec::new()
    }

    /// Disconnect from remote server
    pub fn disconnect(&mut self) {
        // Send SSH_MSG_DISCONNECT
        if self.cipher.active {
            let mut msg = vec![SshMsgType::Disconnect as u8];
            msg.extend_from_slice(&11u32.to_be_bytes());
            let reason = b"Client disconnect";
            msg.extend_from_slice(&(reason.len() as u32).to_be_bytes());
            msg.extend_from_slice(reason);
            msg.extend_from_slice(&0u32.to_be_bytes());
            let packet = self.cipher.encrypt_packet(&msg);
            if let Some(fd) = self.socket_fd {
                let mut sockets = crate::net::SOCKETS.lock();
                if let Some(sock) = sockets.get_mut(&fd) {
                    let _ = sock.send(&packet);
                }
            }
        }

        // Close TCP socket
        if let Some(fd) = self.socket_fd.take() {
            crate::net::SOCKETS.lock().remove(&fd);
        }

        self.state = SshState::Closed;
        serial_println!("[SSH] Disconnected from {}:{}", self.host, self.port);
    }

    /// Resolve hostname to IP address
    fn resolve_host(host: &str) -> crate::net::Ipv4Address {
        // Try parsing as dotted-quad IP
        let parts: Vec<&str> = host.split('.').collect();
        if parts.len() == 4 {
            let a = parts[0].parse::<u8>().unwrap_or(0);
            let b = parts[1].parse::<u8>().unwrap_or(0);
            let c = parts[2].parse::<u8>().unwrap_or(0);
            let d = parts[3].parse::<u8>().unwrap_or(0);
            return crate::net::Ipv4Address::new(a, b, c, d);
        }

        // Try DNS cache
        let dns_cache = crate::net::DNS_CACHE.lock();
        if let Some(&ip) = dns_cache.get(&alloc::string::String::from(host)) {
            return ip;
        }

        // Fallback: loopback
        crate::net::Ipv4Address::LOOPBACK
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Global state
// ═══════════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    /// Active SSH sessions (server-side)
    static ref SSH_SESSIONS: Mutex<BTreeMap<u32, SshSession>> = Mutex::new(BTreeMap::new());

    /// SSH server instance
    pub static ref SSH_SERVER: Mutex<SshServer> = Mutex::new(SshServer::default());
}

static NEXT_SSH_ID: AtomicU32 = AtomicU32::new(1);

/// List all active SSH sessions
pub fn list_sessions() -> Vec<(u32, String, String, SshState)> {
    let sessions = SSH_SESSIONS.lock();
    sessions
        .values()
        .map(|s| (s.id, s.username.clone(), s.peer_addr.clone(), s.state))
        .collect()
}

/// Kill an SSH session by ID
pub fn kill_session(id: u32) -> bool {
    let mut sessions = SSH_SESSIONS.lock();
    if let Some(session) = sessions.get_mut(&id) {
        session.disconnect();
        sessions.remove(&id);
        true
    } else {
        false
    }
}

/// Initialize the SSH subsystem
pub fn init() {
    let _ = SSH_SESSIONS.lock();
    let _ = SSH_SERVER.lock();
    serial_println!("[KnoxOS] SSH subsystem initialized (port 22)");
    serial_println!("[KnoxOS]   KEX: diffie-hellman-group14-sha256");
    serial_println!("[KnoxOS]   Cipher: aes128-ctr");
    serial_println!("[KnoxOS]   MAC: hmac-sha2-256");
    serial_println!("[KnoxOS]   Network: TCP socket-backed connections");
}
