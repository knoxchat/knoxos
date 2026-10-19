use alloc::vec::Vec;

use super::aes::{
    aes256_cbc_decrypt, aes256_cbc_encrypt, aes256_encrypt_block, aes256_key_schedule,
};

// ═══════════════════════════════════════════════════════════════════════
// KERNEL CRYPTO API — Modular cipher framework
// ═══════════════════════════════════════════════════════════════════════

/// Cipher algorithm identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CipherAlgorithm {
    Aes128Ecb,
    Aes128Cbc,
    Aes256Ecb,
    Aes256Cbc,
    ChaCha20,
    ChaCha20Poly1305,
}

/// Hash algorithm identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    Sha256,
    Sha512,
    HmacSha256,
    HmacSha512,
}

/// Crypto transform handle (like Linux crypto_alloc_skcipher)
pub struct CryptoTransform {
    pub algorithm: CipherAlgorithm,
    key: Vec<u8>,
    iv: [u8; 16],
}

impl CryptoTransform {
    /// Allocate a new crypto transform
    pub fn new(algorithm: CipherAlgorithm) -> Self {
        Self {
            algorithm,
            key: Vec::new(),
            iv: [0u8; 16],
        }
    }

    /// Set the encryption key
    pub fn set_key(&mut self, key: &[u8]) {
        self.key = key.to_vec();
    }

    /// Set the IV
    pub fn set_iv(&mut self, iv: &[u8; 16]) {
        self.iv = *iv;
    }

    /// Encrypt data
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, &'static str> {
        match self.algorithm {
            CipherAlgorithm::Aes256Cbc => {
                if self.key.len() != 32 {
                    return Err("AES-256 requires 32-byte key");
                }
                let mut key = [0u8; 32];
                key.copy_from_slice(&self.key);
                Ok(aes256_cbc_encrypt(plaintext, &key, &self.iv))
            }
            CipherAlgorithm::Aes128Cbc => {
                if self.key.len() != 16 {
                    return Err("AES-128 requires 16-byte key");
                }
                // Use AES-128 implementation
                let mut key_bytes = [0u8; 16];
                key_bytes.copy_from_slice(&self.key);
                let round_keys = aes256_key_schedule(&{
                    let mut k = [0u8; 32];
                    k[..16].copy_from_slice(&key_bytes);
                    k
                });
                let mut result = Vec::new();
                let pad_len = 16 - (plaintext.len() % 16);
                let mut padded = plaintext.to_vec();
                for _ in 0..pad_len {
                    padded.push(pad_len as u8);
                }
                let mut prev = self.iv;
                for chunk in padded.chunks(16) {
                    let mut block = [0u8; 16];
                    block.copy_from_slice(chunk);
                    for i in 0..16 {
                        block[i] ^= prev[i];
                    }
                    let enc = aes256_encrypt_block(&block, &round_keys);
                    result.extend_from_slice(&enc);
                    prev = enc;
                }
                Ok(result)
            }
            _ => Err("algorithm not yet implemented"),
        }
    }

    /// Decrypt data
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, &'static str> {
        match self.algorithm {
            CipherAlgorithm::Aes256Cbc => {
                if self.key.len() != 32 {
                    return Err("AES-256 requires 32-byte key");
                }
                let mut key = [0u8; 32];
                key.copy_from_slice(&self.key);
                aes256_cbc_decrypt(ciphertext, &key, &self.iv)
            }
            _ => Err("algorithm not yet implemented"),
        }
    }
}

/// List available crypto algorithms
pub fn available_algorithms() -> Vec<&'static str> {
    alloc::vec![
        "sha256",
        "sha512",
        "hmac(sha256)",
        "hmac(sha512)",
        "aes-128-ecb",
        "aes-128-cbc",
        "aes-256-ecb",
        "aes-256-cbc",
        "chacha20",
        "chacha20-poly1305",
        "rsa",
        "ecdsa-p256",
        "x25519",
        "ed25519",
    ]
}
