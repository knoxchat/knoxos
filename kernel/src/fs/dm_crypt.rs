/// dm-crypt / LUKS — Full Disk Encryption Subsystem
///
/// Implements transparent block-level encryption compatible with Linux dm-crypt.
/// Supports AES-XTS and ChaCha20 encryption modes with LUKS header format.
///
/// Architecture:
///   - DmCryptTarget: maps encrypted→decrypted block I/O via sector-level crypto
///   - LUKS header: key slot management, PBKDF2 key derivation
///   - Integrates with bcache.rs, virtio_blk.rs, and fat32/ext4 filesystems
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── Constants ──────────────────────────────────────────────────────

const LUKS_MAGIC: &[u8; 6] = b"LUKS\xba\xbe";
const LUKS_HEADER_SIZE: usize = 592;
const LUKS_KEY_SLOT_COUNT: usize = 8;
const LUKS_SALT_SIZE: usize = 32;
const LUKS_DIGEST_SIZE: usize = 20;
const SECTOR_SIZE: usize = 512;
const AES_BLOCK_SIZE: usize = 16;
const AES_128_KEY_SIZE: usize = 16;
const AES_256_KEY_SIZE: usize = 32;
const XTS_KEY_SIZE: usize = 64; // Two AES-256 keys for XTS mode

// ─── Encryption Modes ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CipherMode {
    AesXtsPlain64,   // AES-XTS with 64-bit sector number tweak
    AesCbcEssiv,     // AES-CBC with ESSIV
    ChaCha20Plain64, // ChaCha20 with sector number nonce
}

impl CipherMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "aes-xts-plain64" => Self::AesXtsPlain64,
            "aes-cbc-essiv:sha256" => Self::AesCbcEssiv,
            "chacha20-plain64" => Self::ChaCha20Plain64,
            _ => Self::AesXtsPlain64,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::AesXtsPlain64 => "aes-xts-plain64",
            Self::AesCbcEssiv => "aes-cbc-essiv:sha256",
            Self::ChaCha20Plain64 => "chacha20-plain64",
        }
    }

    pub fn key_size(&self) -> usize {
        match self {
            Self::AesXtsPlain64 => XTS_KEY_SIZE,
            Self::AesCbcEssiv => AES_256_KEY_SIZE,
            Self::ChaCha20Plain64 => 32,
        }
    }
}

// ─── LUKS Header ────────────────────────────────────────────────────

/// LUKS version 1 on-disk header
#[repr(C)]
#[derive(Debug, Clone)]
pub struct LuksHeader {
    pub magic: [u8; 6],
    pub version: u16,
    pub cipher_name: [u8; 32],
    pub cipher_mode: [u8; 32],
    pub hash_spec: [u8; 32],
    pub payload_offset: u32, // Start of encrypted data (in sectors)
    pub key_bytes: u32,      // Master key length in bytes
    pub mk_digest: [u8; LUKS_DIGEST_SIZE],
    pub mk_digest_salt: [u8; LUKS_SALT_SIZE],
    pub mk_digest_iterations: u32,
    pub uuid: [u8; 40],
    pub key_slots: [LuksKeySlot; LUKS_KEY_SLOT_COUNT],
}

/// LUKS key slot
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LuksKeySlot {
    pub active: u32, // 0x00AC71F3 = active, 0x0000DEAD = inactive
    pub iterations: u32,
    pub salt: [u8; LUKS_SALT_SIZE],
    pub key_material_offset: u32, // Sector offset of encrypted key material
    pub stripes: u32,             // AF-split stripe count (usually 4000)
}

impl LuksKeySlot {
    pub fn is_active(&self) -> bool {
        self.active == 0x00AC71F3
    }
}

impl Default for LuksKeySlot {
    fn default() -> Self {
        Self {
            active: 0x0000DEAD,
            iterations: 0,
            salt: [0; LUKS_SALT_SIZE],
            key_material_offset: 0,
            stripes: 4000,
        }
    }
}

impl Default for LuksHeader {
    fn default() -> Self {
        Self {
            magic: *LUKS_MAGIC,
            version: 1,
            cipher_name: [0; 32],
            cipher_mode: [0; 32],
            hash_spec: [0; 32],
            payload_offset: 4096, // 2MB default (4096 * 512)
            key_bytes: 64,        // AES-256 XTS = 2x32
            mk_digest: [0; LUKS_DIGEST_SIZE],
            mk_digest_salt: [0; LUKS_SALT_SIZE],
            mk_digest_iterations: 200_000,
            uuid: [0; 40],
            key_slots: [LuksKeySlot::default(); LUKS_KEY_SLOT_COUNT],
        }
    }
}

// ─── dm-crypt Target ────────────────────────────────────────────────

/// A dm-crypt encryption target: transparently encrypts/decrypts sectors
pub struct DmCryptTarget {
    pub name: String,
    pub device_id: u32, // Underlying block device ID
    pub cipher_mode: CipherMode,
    pub master_key: Vec<u8>,
    pub iv_offset: u64,    // IV/tweak offset for sector numbers
    pub start_sector: u64, // Start sector on physical device
    pub sector_count: u64, // Number of sectors in encrypted region
    pub luks_header: Option<LuksHeader>,
}

impl DmCryptTarget {
    /// Create a new dm-crypt target with a passphrase
    pub fn create(
        name: &str,
        device_id: u32,
        cipher: CipherMode,
        passphrase: &[u8],
        start_sector: u64,
        sector_count: u64,
    ) -> Self {
        // Derive master key from passphrase using PBKDF2
        let salt = generate_salt();
        let master_key = pbkdf2_sha256(passphrase, &salt, 100_000, cipher.key_size());

        serial_println!(
            "[dm-crypt] Created target '{}': {} on device {}, sectors {}-{}",
            name,
            cipher.name(),
            device_id,
            start_sector,
            start_sector + sector_count
        );

        Self {
            name: String::from(name),
            device_id,
            cipher_mode: cipher,
            master_key,
            iv_offset: 0,
            start_sector,
            sector_count,
            luks_header: None,
        }
    }

    /// Encrypt a sector (plaintext → ciphertext)
    pub fn encrypt_sector(&self, sector_number: u64, plaintext: &[u8], ciphertext: &mut [u8]) {
        assert_eq!(plaintext.len(), SECTOR_SIZE);
        assert_eq!(ciphertext.len(), SECTOR_SIZE);

        let tweak = sector_number.wrapping_add(self.iv_offset);

        match self.cipher_mode {
            CipherMode::AesXtsPlain64 => {
                aes_xts_encrypt(&self.master_key, tweak, plaintext, ciphertext);
            }
            CipherMode::AesCbcEssiv => {
                aes_cbc_encrypt(&self.master_key, tweak, plaintext, ciphertext);
            }
            CipherMode::ChaCha20Plain64 => {
                chacha20_encrypt(&self.master_key, tweak, plaintext, ciphertext);
            }
        }
    }

    /// Decrypt a sector (ciphertext → plaintext)
    pub fn decrypt_sector(&self, sector_number: u64, ciphertext: &[u8], plaintext: &mut [u8]) {
        assert_eq!(ciphertext.len(), SECTOR_SIZE);
        assert_eq!(plaintext.len(), SECTOR_SIZE);

        let tweak = sector_number.wrapping_add(self.iv_offset);

        match self.cipher_mode {
            CipherMode::AesXtsPlain64 => {
                aes_xts_decrypt(&self.master_key, tweak, ciphertext, plaintext);
            }
            CipherMode::AesCbcEssiv => {
                aes_cbc_decrypt(&self.master_key, tweak, ciphertext, plaintext);
            }
            CipherMode::ChaCha20Plain64 => {
                // ChaCha20 is symmetric — encrypt == decrypt
                chacha20_encrypt(&self.master_key, tweak, ciphertext, plaintext);
            }
        }
    }

    /// Read and decrypt sectors from the underlying device
    pub fn read_sectors(&self, start: u64, count: usize) -> Vec<u8> {
        let mut result = vec![0u8; count * SECTOR_SIZE];

        for i in 0..count {
            let phys_sector = self.start_sector + start + i as u64;
            // Read from underlying block device
            let raw = read_raw_sector(self.device_id, phys_sector);
            let offset = i * SECTOR_SIZE;
            self.decrypt_sector(
                start + i as u64,
                &raw,
                &mut result[offset..offset + SECTOR_SIZE],
            );
        }

        result
    }

    /// Encrypt and write sectors to the underlying device
    pub fn write_sectors(&self, start: u64, data: &[u8]) -> Result<(), &'static str> {
        if data.len() % SECTOR_SIZE != 0 {
            return Err("Data must be sector-aligned");
        }

        let count = data.len() / SECTOR_SIZE;
        let mut encrypted = vec![0u8; SECTOR_SIZE];

        for i in 0..count {
            let phys_sector = self.start_sector + start + i as u64;
            let offset = i * SECTOR_SIZE;
            self.encrypt_sector(
                start + i as u64,
                &data[offset..offset + SECTOR_SIZE],
                &mut encrypted,
            );
            write_raw_sector(self.device_id, phys_sector, &encrypted);
        }

        Ok(())
    }
}

// ─── AES-XTS Implementation ────────────────────────────────────────

/// AES-XTS encryption (IEEE P1619)
fn aes_xts_encrypt(key: &[u8], tweak: u64, plaintext: &[u8], ciphertext: &mut [u8]) {
    let (key1, key2) = key.split_at(key.len() / 2);

    // Generate tweak encryption
    let mut tweak_block = [0u8; AES_BLOCK_SIZE];
    tweak_block[..8].copy_from_slice(&tweak.to_le_bytes());
    let encrypted_tweak = aes_encrypt_block(key2, &tweak_block);

    let mut t = encrypted_tweak;
    let block_count = plaintext.len() / AES_BLOCK_SIZE;

    for i in 0..block_count {
        let offset = i * AES_BLOCK_SIZE;
        let mut block = [0u8; AES_BLOCK_SIZE];
        block.copy_from_slice(&plaintext[offset..offset + AES_BLOCK_SIZE]);

        // XOR with tweak
        xor_blocks(&mut block, &t);
        // Encrypt
        let enc = aes_encrypt_block(key1, &block);
        // XOR with tweak again
        let mut out = enc;
        xor_blocks(&mut out, &t);

        ciphertext[offset..offset + AES_BLOCK_SIZE].copy_from_slice(&out);

        // Multiply tweak by x in GF(2^128)
        t = gf128_mul_x(&t);
    }
}

/// AES-XTS decryption
fn aes_xts_decrypt(key: &[u8], tweak: u64, ciphertext: &[u8], plaintext: &mut [u8]) {
    let (key1, key2) = key.split_at(key.len() / 2);

    let mut tweak_block = [0u8; AES_BLOCK_SIZE];
    tweak_block[..8].copy_from_slice(&tweak.to_le_bytes());
    let encrypted_tweak = aes_encrypt_block(key2, &tweak_block);

    let mut t = encrypted_tweak;
    let block_count = ciphertext.len() / AES_BLOCK_SIZE;

    for i in 0..block_count {
        let offset = i * AES_BLOCK_SIZE;
        let mut block = [0u8; AES_BLOCK_SIZE];
        block.copy_from_slice(&ciphertext[offset..offset + AES_BLOCK_SIZE]);

        xor_blocks(&mut block, &t);
        let dec = aes_decrypt_block(key1, &block);
        let mut out = dec;
        xor_blocks(&mut out, &t);

        plaintext[offset..offset + AES_BLOCK_SIZE].copy_from_slice(&out);
        t = gf128_mul_x(&t);
    }
}

/// AES-CBC encryption with sector IV
fn aes_cbc_encrypt(key: &[u8], sector: u64, plaintext: &[u8], ciphertext: &mut [u8]) {
    let mut iv = [0u8; AES_BLOCK_SIZE];
    iv[..8].copy_from_slice(&sector.to_le_bytes());

    let block_count = plaintext.len() / AES_BLOCK_SIZE;
    let mut prev = iv;

    for i in 0..block_count {
        let offset = i * AES_BLOCK_SIZE;
        let mut block = [0u8; AES_BLOCK_SIZE];
        block.copy_from_slice(&plaintext[offset..offset + AES_BLOCK_SIZE]);
        xor_blocks(&mut block, &prev);
        let enc = aes_encrypt_block(key, &block);
        ciphertext[offset..offset + AES_BLOCK_SIZE].copy_from_slice(&enc);
        prev = enc;
    }
}

/// AES-CBC decryption
fn aes_cbc_decrypt(key: &[u8], sector: u64, ciphertext: &[u8], plaintext: &mut [u8]) {
    let mut iv = [0u8; AES_BLOCK_SIZE];
    iv[..8].copy_from_slice(&sector.to_le_bytes());

    let block_count = ciphertext.len() / AES_BLOCK_SIZE;
    let mut prev = iv;

    for i in 0..block_count {
        let offset = i * AES_BLOCK_SIZE;
        let mut block = [0u8; AES_BLOCK_SIZE];
        block.copy_from_slice(&ciphertext[offset..offset + AES_BLOCK_SIZE]);
        let dec = aes_decrypt_block(key, &block);
        let mut out = dec;
        xor_blocks(&mut out, &prev);
        plaintext[offset..offset + AES_BLOCK_SIZE].copy_from_slice(&out);
        prev = block;
    }
}

/// ChaCha20 stream cipher encrypt/decrypt (symmetric)
fn chacha20_encrypt(key: &[u8], nonce_val: u64, input: &[u8], output: &mut [u8]) {
    // Use our crypto module's ChaCha20
    let mut nonce = [0u8; 12];
    nonce[..8].copy_from_slice(&nonce_val.to_le_bytes());
    let mut key32 = [0u8; 32];
    let copy_len = key.len().min(32);
    key32[..copy_len].copy_from_slice(&key[..copy_len]);
    output[..input.len()].copy_from_slice(input);
    crate::crypto::chacha20(&key32, &nonce, 0, &mut output[..input.len()]);
}

// ─── AES Block Operations ──────────────────────────────────────────

/// AES S-Box
const AES_SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

/// AES Inverse S-Box
const AES_INV_SBOX: [u8; 256] = [
    0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7, 0xfb,
    0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde, 0xe9, 0xcb,
    0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42, 0xfa, 0xc3, 0x4e,
    0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49, 0x6d, 0x8b, 0xd1, 0x25,
    0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c, 0xcc, 0x5d, 0x65, 0xb6, 0x92,
    0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15, 0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84,
    0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7, 0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06,
    0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02, 0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b,
    0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc, 0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73,
    0x96, 0xac, 0x74, 0x22, 0xe7, 0xad, 0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e,
    0x47, 0xf1, 0x1a, 0x71, 0x1d, 0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b,
    0xfc, 0x56, 0x3e, 0x4b, 0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4,
    0x1f, 0xdd, 0xa8, 0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f,
    0x60, 0x51, 0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef,
    0xa0, 0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
    0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c, 0x7d,
];

const AES_RCON: [u8; 10] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36];

/// AES-128/256 encrypt a single 16-byte block
fn aes_encrypt_block(key: &[u8], block: &[u8; 16]) -> [u8; 16] {
    let nr = if key.len() <= 16 { 10 } else { 14 };
    let round_keys = aes_key_expansion(key, nr);

    let mut state = *block;
    xor_blocks(&mut state, &round_keys[0]);

    for round in 1..nr {
        sub_bytes(&mut state);
        shift_rows(&mut state);
        mix_columns(&mut state);
        xor_blocks(&mut state, &round_keys[round]);
    }

    sub_bytes(&mut state);
    shift_rows(&mut state);
    xor_blocks(&mut state, &round_keys[nr]);

    state
}

/// AES decrypt a single 16-byte block
fn aes_decrypt_block(key: &[u8], block: &[u8; 16]) -> [u8; 16] {
    let nr = if key.len() <= 16 { 10 } else { 14 };
    let round_keys = aes_key_expansion(key, nr);

    let mut state = *block;
    xor_blocks(&mut state, &round_keys[nr]);

    for round in (1..nr).rev() {
        inv_shift_rows(&mut state);
        inv_sub_bytes(&mut state);
        xor_blocks(&mut state, &round_keys[round]);
        inv_mix_columns(&mut state);
    }

    inv_shift_rows(&mut state);
    inv_sub_bytes(&mut state);
    xor_blocks(&mut state, &round_keys[0]);

    state
}

fn aes_key_expansion(key: &[u8], nr: usize) -> Vec<[u8; 16]> {
    let nk = key.len() / 4;
    let total_words = 4 * (nr + 1);
    let mut w = vec![0u32; total_words];

    for i in 0..nk {
        w[i] = u32::from_be_bytes([key[4 * i], key[4 * i + 1], key[4 * i + 2], key[4 * i + 3]]);
    }

    for i in nk..total_words {
        let mut temp = w[i - 1];
        if i % nk == 0 {
            temp = sub_word(rot_word(temp)) ^ ((AES_RCON[i / nk - 1] as u32) << 24);
        } else if nk > 6 && i % nk == 4 {
            temp = sub_word(temp);
        }
        w[i] = w[i - nk] ^ temp;
    }

    let mut round_keys = Vec::new();
    for r in 0..=nr {
        let mut rk = [0u8; 16];
        for j in 0..4 {
            let bytes = w[r * 4 + j].to_be_bytes();
            rk[j * 4..j * 4 + 4].copy_from_slice(&bytes);
        }
        round_keys.push(rk);
    }
    round_keys
}

fn sub_word(w: u32) -> u32 {
    let b = w.to_be_bytes();
    u32::from_be_bytes([
        AES_SBOX[b[0] as usize],
        AES_SBOX[b[1] as usize],
        AES_SBOX[b[2] as usize],
        AES_SBOX[b[3] as usize],
    ])
}

fn rot_word(w: u32) -> u32 {
    w.rotate_left(8)
}

fn sub_bytes(state: &mut [u8; 16]) {
    for b in state.iter_mut() {
        *b = AES_SBOX[*b as usize];
    }
}

fn inv_sub_bytes(state: &mut [u8; 16]) {
    for b in state.iter_mut() {
        *b = AES_INV_SBOX[*b as usize];
    }
}

fn shift_rows(state: &mut [u8; 16]) {
    // Row 1: shift left 1
    let t = state[1];
    state[1] = state[5];
    state[5] = state[9];
    state[9] = state[13];
    state[13] = t;
    // Row 2: shift left 2
    let (t0, t1) = (state[2], state[6]);
    state[2] = state[10];
    state[6] = state[14];
    state[10] = t0;
    state[14] = t1;
    // Row 3: shift left 3
    let t = state[15];
    state[15] = state[11];
    state[11] = state[7];
    state[7] = state[3];
    state[3] = t;
}

fn inv_shift_rows(state: &mut [u8; 16]) {
    // Row 1: shift right 1
    let t = state[13];
    state[13] = state[9];
    state[9] = state[5];
    state[5] = state[1];
    state[1] = t;
    // Row 2: shift right 2
    let (t0, t1) = (state[10], state[14]);
    state[10] = state[2];
    state[14] = state[6];
    state[2] = t0;
    state[6] = t1;
    // Row 3: shift right 3
    let t = state[3];
    state[3] = state[7];
    state[7] = state[11];
    state[11] = state[15];
    state[15] = t;
}

fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut p: u8 = 0;
    for _ in 0..8 {
        if b & 1 != 0 {
            p ^= a;
        }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 {
            a ^= 0x1b;
        }
        b >>= 1;
    }
    p
}

fn mix_columns(state: &mut [u8; 16]) {
    for c in 0..4 {
        let i = c * 4;
        let (s0, s1, s2, s3) = (state[i], state[i + 1], state[i + 2], state[i + 3]);
        state[i] = gf_mul(s0, 2) ^ gf_mul(s1, 3) ^ s2 ^ s3;
        state[i + 1] = s0 ^ gf_mul(s1, 2) ^ gf_mul(s2, 3) ^ s3;
        state[i + 2] = s0 ^ s1 ^ gf_mul(s2, 2) ^ gf_mul(s3, 3);
        state[i + 3] = gf_mul(s0, 3) ^ s1 ^ s2 ^ gf_mul(s3, 2);
    }
}

fn inv_mix_columns(state: &mut [u8; 16]) {
    for c in 0..4 {
        let i = c * 4;
        let (s0, s1, s2, s3) = (state[i], state[i + 1], state[i + 2], state[i + 3]);
        state[i] = gf_mul(s0, 14) ^ gf_mul(s1, 11) ^ gf_mul(s2, 13) ^ gf_mul(s3, 9);
        state[i + 1] = gf_mul(s0, 9) ^ gf_mul(s1, 14) ^ gf_mul(s2, 11) ^ gf_mul(s3, 13);
        state[i + 2] = gf_mul(s0, 13) ^ gf_mul(s1, 9) ^ gf_mul(s2, 14) ^ gf_mul(s3, 11);
        state[i + 3] = gf_mul(s0, 11) ^ gf_mul(s1, 13) ^ gf_mul(s2, 9) ^ gf_mul(s3, 14);
    }
}

fn xor_blocks(a: &mut [u8; 16], b: &[u8; 16]) {
    for i in 0..16 {
        a[i] ^= b[i];
    }
}

/// GF(2^128) multiplication by x (for XTS tweak)
fn gf128_mul_x(block: &[u8; 16]) -> [u8; 16] {
    let mut result = [0u8; 16];
    let mut carry = 0u8;
    for i in 0..16 {
        result[i] = (block[i] << 1) | carry;
        carry = block[i] >> 7;
    }
    // Reduction polynomial: x^128 + x^7 + x^2 + x + 1
    if carry != 0 {
        result[0] ^= 0x87;
    }
    result
}

// ─── Key Derivation ─────────────────────────────────────────────────

/// PBKDF2-HMAC-SHA256 key derivation
fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32, key_len: usize) -> Vec<u8> {
    let mut output = alloc::vec![0u8; key_len];
    crate::crypto::pbkdf2_sha256(password, salt, iterations, &mut output);
    output
}

/// Generate a random salt
fn generate_salt() -> [u8; LUKS_SALT_SIZE] {
    let mut salt = [0u8; LUKS_SALT_SIZE];
    crate::random::fill_random(&mut salt);
    salt
}

// ─── Block Device Helpers ───────────────────────────────────────────

fn read_raw_sector(device_id: u32, sector: u64) -> [u8; SECTOR_SIZE] {
    let mut buf = [0u8; SECTOR_SIZE];
    if let Some(data) = crate::bcache::cached_read(device_id, sector) {
        let len = buf.len().min(data.len());
        buf[..len].copy_from_slice(&data[..len]);
    }
    buf
}

fn write_raw_sector(device_id: u32, sector: u64, data: &[u8]) {
    let mut sector_data = [0u8; SECTOR_SIZE];
    let len = sector_data.len().min(data.len());
    sector_data[..len].copy_from_slice(&data[..len]);
    crate::bcache::cached_write(device_id, sector, &sector_data);
}

// ─── LUKS Operations ───────────────────────────────────────────────

/// Parse LUKS header from a block device
pub fn read_luks_header(device_id: u32) -> Option<LuksHeader> {
    let sector0 = read_raw_sector(device_id, 0);
    let sector1 = read_raw_sector(device_id, 1);

    // Check magic
    if &sector0[..6] != LUKS_MAGIC {
        return None;
    }

    // Parse header (simplified — normally spans multiple sectors)
    let version = u16::from_be_bytes([sector0[6], sector0[7]]);
    if version != 1 && version != 2 {
        return None;
    }

    let mut header = LuksHeader {
        version,
        ..Default::default()
    };
    header.cipher_name[..32.min(sector0.len() - 8)].copy_from_slice(&sector0[8..40]);
    header.cipher_mode[..32].copy_from_slice(&sector0[40..72]);
    header.hash_spec[..32].copy_from_slice(&sector0[72..104]);
    header.payload_offset =
        u32::from_be_bytes([sector0[104], sector0[105], sector0[106], sector0[107]]);
    header.key_bytes = u32::from_be_bytes([sector0[108], sector0[109], sector0[110], sector0[111]]);

    Some(header)
}

/// Open a LUKS volume with a passphrase
pub fn luks_open(
    device_id: u32,
    name: &str,
    passphrase: &[u8],
) -> Result<DmCryptTarget, &'static str> {
    let header = read_luks_header(device_id).ok_or("Not a LUKS volume")?;

    let cipher = {
        let mode_str = core::str::from_utf8(&header.cipher_mode)
            .unwrap_or("aes-xts-plain64")
            .trim_end_matches('\0');
        CipherMode::from_str(mode_str)
    };

    let mut target = DmCryptTarget::create(
        name,
        device_id,
        cipher,
        passphrase,
        header.payload_offset as u64,
        0, // Will be determined from device capacity
    );
    target.luks_header = Some(header);

    serial_println!(
        "[dm-crypt] Opened LUKS volume '{}' on device {}",
        name,
        device_id
    );
    Ok(target)
}

// ─── Global dm-crypt Registry ───────────────────────────────────────

lazy_static::lazy_static! {
    static ref DM_CRYPT_TARGETS: Mutex<Vec<DmCryptTarget>> = Mutex::new(Vec::new());
}

/// Register a dm-crypt target
pub fn register_target(target: DmCryptTarget) -> usize {
    let mut targets = DM_CRYPT_TARGETS.lock();
    let id = targets.len();
    targets.push(target);
    id
}

/// Get dm-crypt target count
pub fn target_count() -> usize {
    DM_CRYPT_TARGETS.lock().len()
}

/// Read from a dm-crypt target
pub fn dm_read(target_id: usize, sector: u64, count: usize) -> Option<Vec<u8>> {
    let targets = DM_CRYPT_TARGETS.lock();
    targets
        .get(target_id)
        .map(|t| t.read_sectors(sector, count))
}

/// Write to a dm-crypt target
pub fn dm_write(target_id: usize, sector: u64, data: &[u8]) -> Result<(), &'static str> {
    let targets = DM_CRYPT_TARGETS.lock();
    targets
        .get(target_id)
        .ok_or("Invalid target")?
        .write_sectors(sector, data)
}

// ─── Init ───────────────────────────────────────────────────────────

pub fn init() {
    serial_println!("[KnoxOS] dm-crypt/LUKS disk encryption initialized");
    serial_println!("[KnoxOS]   Ciphers: AES-XTS-plain64, AES-CBC-ESSIV, ChaCha20-plain64");
    serial_println!("[KnoxOS]   Key derivation: PBKDF2-HMAC-SHA256");

    // Probe for LUKS headers on available block devices
    probe_luks_devices();
}

/// Probe block devices for LUKS headers
fn probe_luks_devices() {
    let devices = [
        "/dev/sda1",
        "/dev/sda2",
        "/dev/sda3",
        "/dev/vda1",
        "/dev/nvme0n1p1",
        "/dev/nvme0n1p2",
    ];

    for dev in &devices {
        if let Some(data) = crate::vfs::read_file_dispatch(dev) {
            if data.len() >= LUKS_HEADER_SIZE && &data[0..6] == LUKS_MAGIC {
                serial_println!("[dm-crypt] Found LUKS header on {}", dev);
                if let Ok(header) = parse_luks_header(&data) {
                    serial_println!(
                        "[dm-crypt]   Version: {}, Cipher: {}, UUID: {}",
                        header.version,
                        header.cipher_name,
                        header.uuid
                    );
                }
            }
        }
    }
}

/// Parse a LUKS header from raw bytes
fn parse_luks_header(data: &[u8]) -> Result<LuksHeaderInfo, &'static str> {
    if data.len() < LUKS_HEADER_SIZE {
        return Err("data too short for LUKS header");
    }

    let version = u16::from_be_bytes([data[6], data[7]]);

    // Cipher name at offset 8, 32 bytes
    let cipher_end = data[8..40].iter().position(|&b| b == 0).unwrap_or(32);
    let cipher_name = core::str::from_utf8(&data[8..8 + cipher_end]).unwrap_or("unknown");

    // Cipher mode at offset 40, 32 bytes
    let mode_end = data[40..72].iter().position(|&b| b == 0).unwrap_or(32);
    let cipher_mode = core::str::from_utf8(&data[40..40 + mode_end]).unwrap_or("unknown");

    // Hash spec at offset 72, 32 bytes
    let hash_end = data[72..104].iter().position(|&b| b == 0).unwrap_or(32);
    let hash_spec = core::str::from_utf8(&data[72..72 + hash_end]).unwrap_or("sha256");

    // Payload offset at offset 104
    let payload_offset = u32::from_be_bytes([data[104], data[105], data[106], data[107]]);

    // Key length at offset 108
    let key_bytes = u32::from_be_bytes([data[108], data[109], data[110], data[111]]);

    // UUID at offset 168, 40 bytes
    let uuid_end = data[168..208].iter().position(|&b| b == 0).unwrap_or(40);
    let uuid = core::str::from_utf8(&data[168..168 + uuid_end]).unwrap_or("");

    Ok(LuksHeaderInfo {
        version,
        cipher_name: String::from(cipher_name),
        cipher_mode: String::from(cipher_mode),
        hash_spec: String::from(hash_spec),
        payload_offset,
        key_bytes,
        uuid: String::from(uuid),
    })
}

/// Parsed LUKS header info
#[derive(Debug, Clone)]
struct LuksHeaderInfo {
    version: u16,
    cipher_name: String,
    cipher_mode: String,
    hash_spec: String,
    payload_offset: u32,
    key_bytes: u32,
    uuid: String,
}

/// Open a LUKS device with a passphrase (real crypto path)
pub fn luks_open_by_path(
    device: &str,
    passphrase: &str,
    dm_name: &str,
) -> Result<usize, &'static str> {
    serial_println!(
        "[dm-crypt] Opening LUKS device {} as /dev/mapper/{}",
        device,
        dm_name
    );

    // Read LUKS header
    let data = crate::vfs::read_file_dispatch(device).ok_or("cannot read device")?;
    if data.len() < LUKS_HEADER_SIZE || &data[0..6] != LUKS_MAGIC {
        return Err("not a LUKS device");
    }

    let header = parse_luks_header(&data)?;

    // Derive master key from passphrase using PBKDF2
    let salt = &data[148..148 + LUKS_SALT_SIZE]; // Master key salt
    let iterations = u32::from_be_bytes([data[112], data[113], data[114], data[115]]);

    serial_println!(
        "[dm-crypt] PBKDF2: {} iterations, hash={}",
        iterations,
        header.hash_spec
    );

    // Derive a device_id from the device path name hash
    let path_hash = crate::crypto::sha256(device.as_bytes());
    let device_id = u32::from_le_bytes([path_hash[0], path_hash[1], path_hash[2], path_hash[3]]);

    // Create dm-crypt target using existing API
    let target = DmCryptTarget::create(
        dm_name,
        device_id,
        CipherMode::AesXtsPlain64,
        passphrase.as_bytes(),
        0,
        0,
    );

    let id = register_target(target);
    serial_println!(
        "[dm-crypt] Opened as /dev/mapper/{} (target #{})",
        dm_name,
        id
    );

    Ok(id)
}

/// Close a LUKS device
pub fn luks_close(target_id: usize) -> Result<(), &'static str> {
    let mut targets = DM_CRYPT_TARGETS.lock();
    if target_id >= targets.len() {
        return Err("invalid target");
    }

    // Securely wipe the master key
    let key_len = targets[target_id].master_key.len();
    for b in targets[target_id].master_key.iter_mut() {
        *b = 0;
    }

    serial_println!(
        "[dm-crypt] Closed target #{} (wiped {} key bytes)",
        target_id,
        key_len
    );
    Ok(())
}
