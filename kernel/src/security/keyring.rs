/// Keyring — Secure credential storage
///
/// Provides encrypted storage for passwords, tokens, and secrets:
///   - In-memory encrypted keyring (AES-128-CBC)
///   - PBKDF2-style key derivation with SHA-256 (5000 rounds)
///   - Service/username/password triple storage
///   - Lock/unlock with master password
///   - Auto-lock on idle timeout
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// KEYRING
// ═══════════════════════════════════════════════════════════════════════

/// A stored credential
#[derive(Clone)]
pub struct Credential {
    pub service: String,
    pub username: String,
    /// Obfuscated password bytes (XOR with key)
    encrypted_password: Vec<u8>,
    /// Timestamp when stored
    pub created_at: u64,
    /// Timestamp of last access
    pub last_accessed: u64,
}

impl Credential {
    fn new(service: &str, username: &str, password: &str, key: &[u8], time: u64) -> Self {
        Self {
            service: String::from(service),
            username: String::from(username),
            encrypted_password: aes_cbc_encrypt(password.as_bytes(), key),
            created_at: time,
            last_accessed: time,
        }
    }

    fn decrypt_password(&self, key: &[u8]) -> Option<String> {
        let bytes = aes_cbc_decrypt(&self.encrypted_password, key);
        if bytes.is_empty() {
            return None;
        }
        String::from_utf8(bytes).ok()
    }
}

/// AES-128-CBC encryption for credential storage.
///
/// Uses the kernel's SHA-256 digest for key derivation and a 16-byte
/// AES key.  Encryption is CBC mode with PKCS#7 padding.
fn aes_sbox(b: u8) -> u8 {
    // Rijndael forward S-box (full 256-entry table)
    static SBOX: [u8; 256] = [
        0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab,
        0x76, 0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4,
        0x72, 0xc0, 0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71,
        0xd8, 0x31, 0x15, 0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2,
        0xeb, 0x27, 0xb2, 0x75, 0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6,
        0xb3, 0x29, 0xe3, 0x2f, 0x84, 0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb,
        0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf, 0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45,
        0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8, 0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5,
        0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2, 0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44,
        0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73, 0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a,
        0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb, 0xe0, 0x32, 0x3a, 0x0a, 0x49,
        0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79, 0xe7, 0xc8, 0x37, 0x6d,
        0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08, 0xba, 0x78, 0x25,
        0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a, 0x70, 0x3e,
        0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e, 0xe1,
        0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
        0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb,
        0x16,
    ];
    SBOX[b as usize]
}

/// Inverse AES S-box for decryption
fn aes_inv_sbox(b: u8) -> u8 {
    static INV_SBOX: [u8; 256] = [
        0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7,
        0xfb, 0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde,
        0xe9, 0xcb, 0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42,
        0xfa, 0xc3, 0x4e, 0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49,
        0x6d, 0x8b, 0xd1, 0x25, 0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c,
        0xcc, 0x5d, 0x65, 0xb6, 0x92, 0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15,
        0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84, 0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7,
        0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06, 0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02,
        0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b, 0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc,
        0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73, 0x96, 0xac, 0x74, 0x22, 0xe7, 0xad,
        0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e, 0x47, 0xf1, 0x1a, 0x71, 0x1d,
        0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b, 0xfc, 0x56, 0x3e, 0x4b,
        0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4, 0x1f, 0xdd, 0xa8,
        0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f, 0x60, 0x51,
        0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef, 0xa0,
        0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
        0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c,
        0x7d,
    ];
    INV_SBOX[b as usize]
}

/// AES-128 single-block encryption (SubBytes→ShiftRows→MixColumns→AddRoundKey)
fn aes128_encrypt_block(block: &mut [u8; 16], round_keys: &[[u8; 16]; 11]) {
    // AddRoundKey (round 0)
    for i in 0..16 {
        block[i] ^= round_keys[0][i];
    }
    for round in 1..10 {
        // SubBytes
        for i in 0..16 {
            block[i] = aes_sbox(block[i]);
        }
        // ShiftRows
        let tmp = *block;
        // Row 1: shift left 1
        block[1] = tmp[5];
        block[5] = tmp[9];
        block[9] = tmp[13];
        block[13] = tmp[1];
        // Row 2: shift left 2
        block[2] = tmp[10];
        block[6] = tmp[14];
        block[10] = tmp[2];
        block[14] = tmp[6];
        // Row 3: shift left 3
        block[3] = tmp[15];
        block[7] = tmp[3];
        block[11] = tmp[7];
        block[15] = tmp[11];
        // MixColumns
        fn xtime(a: u8) -> u8 {
            if a & 0x80 != 0 {
                (a << 1) ^ 0x1b
            } else {
                a << 1
            }
        }
        for col in 0..4 {
            let i = col * 4;
            let (a0, a1, a2, a3) = (block[i], block[i + 1], block[i + 2], block[i + 3]);
            let t = a0 ^ a1 ^ a2 ^ a3;
            block[i] = a0 ^ xtime(a0 ^ a1) ^ t;
            block[i + 1] = a1 ^ xtime(a1 ^ a2) ^ t;
            block[i + 2] = a2 ^ xtime(a2 ^ a3) ^ t;
            block[i + 3] = a3 ^ xtime(a3 ^ a0) ^ t;
        }
        // AddRoundKey
        for i in 0..16 {
            block[i] ^= round_keys[round][i];
        }
    }
    // Final round (no MixColumns)
    for i in 0..16 {
        block[i] = aes_sbox(block[i]);
    }
    let tmp = *block;
    block[1] = tmp[5];
    block[5] = tmp[9];
    block[9] = tmp[13];
    block[13] = tmp[1];
    block[2] = tmp[10];
    block[6] = tmp[14];
    block[10] = tmp[2];
    block[14] = tmp[6];
    block[3] = tmp[15];
    block[7] = tmp[3];
    block[11] = tmp[7];
    block[15] = tmp[11];
    for i in 0..16 {
        block[i] ^= round_keys[10][i];
    }
}

/// AES-128 single-block decryption
fn aes128_decrypt_block(block: &mut [u8; 16], round_keys: &[[u8; 16]; 11]) {
    // AddRoundKey (round 10)
    for i in 0..16 {
        block[i] ^= round_keys[10][i];
    }
    // InvShiftRows + InvSubBytes
    let tmp = *block;
    block[1] = tmp[13];
    block[5] = tmp[1];
    block[9] = tmp[5];
    block[13] = tmp[9];
    block[2] = tmp[10];
    block[6] = tmp[14];
    block[10] = tmp[2];
    block[14] = tmp[6];
    block[3] = tmp[7];
    block[7] = tmp[11];
    block[11] = tmp[15];
    block[15] = tmp[3];
    for i in 0..16 {
        block[i] = aes_inv_sbox(block[i]);
    }
    for round in (1..10).rev() {
        // AddRoundKey
        for i in 0..16 {
            block[i] ^= round_keys[round][i];
        }
        // InvMixColumns
        fn xtime(a: u8) -> u8 {
            if a & 0x80 != 0 {
                (a << 1) ^ 0x1b
            } else {
                a << 1
            }
        }
        fn mul(mut a: u8, mut b: u8) -> u8 {
            let mut r: u8 = 0;
            for _ in 0..8 {
                if b & 1 != 0 {
                    r ^= a;
                }
                b >>= 1;
                a = xtime(a);
            }
            r
        }
        for col in 0..4 {
            let i = col * 4;
            let (a0, a1, a2, a3) = (block[i], block[i + 1], block[i + 2], block[i + 3]);
            block[i] = mul(a0, 0x0e) ^ mul(a1, 0x0b) ^ mul(a2, 0x0d) ^ mul(a3, 0x09);
            block[i + 1] = mul(a0, 0x09) ^ mul(a1, 0x0e) ^ mul(a2, 0x0b) ^ mul(a3, 0x0d);
            block[i + 2] = mul(a0, 0x0d) ^ mul(a1, 0x09) ^ mul(a2, 0x0e) ^ mul(a3, 0x0b);
            block[i + 3] = mul(a0, 0x0b) ^ mul(a1, 0x0d) ^ mul(a2, 0x09) ^ mul(a3, 0x0e);
        }
        // InvShiftRows
        let tmp = *block;
        block[1] = tmp[13];
        block[5] = tmp[1];
        block[9] = tmp[5];
        block[13] = tmp[9];
        block[2] = tmp[10];
        block[6] = tmp[14];
        block[10] = tmp[2];
        block[14] = tmp[6];
        block[3] = tmp[7];
        block[7] = tmp[11];
        block[11] = tmp[15];
        block[15] = tmp[3];
        // InvSubBytes
        for i in 0..16 {
            block[i] = aes_inv_sbox(block[i]);
        }
    }
    // AddRoundKey (round 0)
    for i in 0..16 {
        block[i] ^= round_keys[0][i];
    }
}

/// AES-128 key expansion
fn aes128_expand_key(key: &[u8; 16]) -> [[u8; 16]; 11] {
    static RCON: [u8; 10] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36];
    let mut round_keys = [[0u8; 16]; 11];
    round_keys[0] = *key;
    for i in 1..11 {
        let prev = round_keys[i - 1];
        // RotWord + SubWord + Rcon
        let mut temp = [
            aes_sbox(prev[13]),
            aes_sbox(prev[14]),
            aes_sbox(prev[15]),
            aes_sbox(prev[12]),
        ];
        temp[0] ^= RCON[i - 1];
        for j in 0..4 {
            round_keys[i][j] = prev[j] ^ temp[j];
        }
        for j in 4..16 {
            round_keys[i][j] = prev[j] ^ round_keys[i][j - 4];
        }
    }
    round_keys
}

/// Encrypt data using AES-128-CBC with PKCS#7 padding.
/// Returns IV (16 bytes) prepended to ciphertext.
fn aes_cbc_encrypt(data: &[u8], key: &[u8]) -> Vec<u8> {
    let mut aes_key = [0u8; 16];
    for (i, &b) in key.iter().take(16).enumerate() {
        aes_key[i] = b;
    }
    let round_keys = aes128_expand_key(&aes_key);

    // Generate IV from RTC + TSC
    let mut iv = [0u8; 16];
    let ts = crate::rtc::unix_time() as u64;
    let tsc = crate::arch_compat::read_tsc();
    iv[..8].copy_from_slice(&ts.to_le_bytes());
    iv[8..16].copy_from_slice(&tsc.to_le_bytes());

    // PKCS#7 padding
    let pad_len = 16 - (data.len() % 16);
    let mut padded = data.to_vec();
    for _ in 0..pad_len {
        padded.push(pad_len as u8);
    }

    let mut result = Vec::with_capacity(16 + padded.len());
    result.extend_from_slice(&iv);

    let mut prev = iv;
    for chunk in padded.chunks(16) {
        let mut block = [0u8; 16];
        for i in 0..16 {
            block[i] = chunk[i] ^ prev[i];
        }
        aes128_encrypt_block(&mut block, &round_keys);
        result.extend_from_slice(&block);
        prev = block;
    }

    result
}

/// Decrypt AES-128-CBC data (IV is first 16 bytes)
fn aes_cbc_decrypt(data: &[u8], key: &[u8]) -> Vec<u8> {
    if data.len() < 32 || data.len() % 16 != 0 {
        return Vec::new();
    }
    let mut aes_key = [0u8; 16];
    for (i, &b) in key.iter().take(16).enumerate() {
        aes_key[i] = b;
    }
    let round_keys = aes128_expand_key(&aes_key);

    let iv = &data[..16];
    let ciphertext = &data[16..];
    let mut result = Vec::with_capacity(ciphertext.len());
    let mut prev = [0u8; 16];
    prev.copy_from_slice(iv);

    for chunk in ciphertext.chunks(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);
        aes128_decrypt_block(&mut block, &round_keys);
        for i in 0..16 {
            block[i] ^= prev[i];
        }
        result.extend_from_slice(&block);
        prev.copy_from_slice(chunk);
    }

    // Remove PKCS#7 padding
    if let Some(&pad) = result.last() {
        let pad = pad as usize;
        if (1..=16).contains(&pad) && result.len() >= pad {
            let valid = result[result.len() - pad..]
                .iter()
                .all(|&b| b as usize == pad);
            if valid {
                result.truncate(result.len() - pad);
            }
        }
    }

    result
}

/// SHA-256 hash for master password verification (uses kernel crypto module)
fn sha256_hash(data: &[u8]) -> [u8; 32] {
    crate::crypto::sha256(data)
}

/// Derive a 32-byte encryption key from master password via PBKDF2-like stretching
fn derive_key(master: &str) -> Vec<u8> {
    let salt = b"KnoxOS-Keyring-Salt-v1";
    let mut key_material = Vec::with_capacity(master.len() + salt.len());
    key_material.extend_from_slice(master.as_bytes());
    key_material.extend_from_slice(salt);

    // 5000 rounds of SHA-256 stretching (matches password hashing policy)
    let mut hash = sha256_hash(&key_material);
    for _ in 0..5000 {
        let mut round_input = hash.to_vec();
        round_input.extend_from_slice(master.as_bytes());
        hash = sha256_hash(&round_input);
    }

    hash.to_vec()
}

/// Keyring state
pub struct Keyring {
    credentials: Vec<Credential>,
    /// Derived encryption key (only set when unlocked)
    encryption_key: Option<Vec<u8>>,
    /// SHA-256 hash of master password for verification
    master_hash: Option<[u8; 32]>,
    /// Whether the keyring is locked
    locked: bool,
    /// Auto-lock timeout (seconds, 0 = disabled)
    auto_lock_timeout: u64,
    /// Last activity timestamp
    last_activity: u64,
}

impl Keyring {
    pub fn new() -> Self {
        Self {
            credentials: Vec::new(),
            encryption_key: None,
            master_hash: None,
            locked: true,
            auto_lock_timeout: 300, // 5 minutes
            last_activity: 0,
        }
    }

    /// Set up the keyring with a master password
    pub fn setup(&mut self, master_password: &str) -> bool {
        if master_password.len() < 4 {
            serial_println!("[Keyring] Master password too short (min 4)");
            return false;
        }
        self.master_hash = Some(sha256_hash(master_password.as_bytes()));
        self.encryption_key = Some(derive_key(master_password));
        self.locked = false;
        serial_println!("[Keyring] Master password set (AES-128-CBC), keyring unlocked");
        true
    }

    /// Unlock the keyring with master password
    pub fn unlock(&mut self, master_password: &str) -> bool {
        if let Some(ref stored_hash) = self.master_hash {
            let input_hash = sha256_hash(master_password.as_bytes());
            // Constant-time comparison
            let mut diff = 0u8;
            for (a, b) in stored_hash.iter().zip(input_hash.iter()) {
                diff |= a ^ b;
            }
            if diff == 0 {
                self.encryption_key = Some(derive_key(master_password));
                self.locked = false;
                serial_println!("[Keyring] Unlocked");
                return true;
            }
            serial_println!("[Keyring] Unlock failed: wrong password");
            false
        } else {
            serial_println!("[Keyring] No master password set, call setup() first");
            false
        }
    }

    /// Lock the keyring (clears encryption key from memory)
    pub fn lock(&mut self) {
        // Zero out the key before dropping
        if let Some(ref mut key) = self.encryption_key {
            for b in key.iter_mut() {
                *b = 0;
            }
        }
        self.encryption_key = None;
        self.locked = true;
        serial_println!("[Keyring] Locked");
    }

    /// Check if locked
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Store a credential
    pub fn store(&mut self, service: &str, username: &str, password: &str) -> bool {
        if self.locked {
            serial_println!("[Keyring] Cannot store: keyring is locked");
            return false;
        }
        let key = match &self.encryption_key {
            Some(k) => k.clone(),
            None => return false,
        };

        let now = crate::rtc::unix_time() as u64;

        // Remove existing entry for same service+username
        self.credentials
            .retain(|c| !(c.service == service && c.username == username));

        self.credentials
            .push(Credential::new(service, username, password, &key, now));

        serial_println!("[Keyring] Stored credential for {}@{}", username, service);
        true
    }

    /// Retrieve a password
    pub fn get_password(&mut self, service: &str, username: &str) -> Option<String> {
        if self.locked {
            serial_println!("[Keyring] Cannot read: keyring is locked");
            return None;
        }
        let key = self.encryption_key.as_ref()?.clone();
        let now = crate::rtc::unix_time() as u64;

        for cred in &mut self.credentials {
            if cred.service == service && cred.username == username {
                cred.last_accessed = now;
                self.last_activity = now;
                return cred.decrypt_password(&key);
            }
        }
        None
    }

    /// Delete a credential
    pub fn delete(&mut self, service: &str, username: &str) -> bool {
        if self.locked {
            return false;
        }
        let before = self.credentials.len();
        self.credentials
            .retain(|c| !(c.service == service && c.username == username));
        self.credentials.len() < before
    }

    /// List all stored services (doesn't require unlock for service names)
    pub fn list_services(&self) -> Vec<(String, String)> {
        self.credentials
            .iter()
            .map(|c| (c.service.clone(), c.username.clone()))
            .collect()
    }

    /// Check auto-lock timeout
    pub fn check_auto_lock(&mut self, current_time: u64) {
        if !self.locked
            && self.auto_lock_timeout > 0
            && self.last_activity > 0
            && current_time - self.last_activity > self.auto_lock_timeout
        {
            self.lock();
            serial_println!("[Keyring] Auto-locked due to inactivity");
        }
    }

    /// Set auto-lock timeout (0 to disable)
    pub fn set_auto_lock_timeout(&mut self, seconds: u64) {
        self.auto_lock_timeout = seconds;
    }
}

lazy_static::lazy_static! {
    pub static ref KEYRING: Mutex<Keyring> = Mutex::new(Keyring::new());
}

/// Initialize keyring subsystem
pub fn init() {
    serial_println!("[KnoxOS] Keyring subsystem initialized");
}
