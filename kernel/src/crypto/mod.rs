/// Crypto — Kernel cryptographic primitives
///
/// Provides fundamental crypto operations for kernel use:
///   - AES-128/256 (software implementation)
///   - ChaCha20-Poly1305
///   - SHA-256, SHA-512
///   - HMAC
///   - PBKDF2
///   - Secure random (RDRAND/RDSEED)
///   - dm-crypt style block encryption stubs
///
/// These are used by TLS, disk encryption, and security modules.
///
/// Module layout:
///   sha256       — SHA-256 hasher and one-shot hash
///   sha512       — SHA-512 one-shot hash
///   hmac         — HMAC-SHA256 and PBKDF2-HMAC-SHA256
///   chacha20     — ChaCha20 stream cipher
///   aes          — AES-128/256 block and CBC
///   random       — RDRAND random_bytes
///   aead         — AES-128-GCM and Poly1305
///   bn           — little-endian limb bignum helpers
///   rsa          — RSA keygen, encrypt, PKCS#1 v1.5 sign/verify
///   ecdsa        — ECDSA P-256
///   curve25519   — X25519 and Ed25519
///   api          — CryptoTransform and algorithm identifiers
///   gates        — GATE_* known-answer self-tests
mod aead;
mod aes;
mod api;
mod bn;
mod chacha20;
mod curve25519;
mod ecdsa;
mod gates;
mod hmac;
mod random;
mod rsa;
mod sha256;
mod sha512;
pub mod tls;

pub use aead::{aes128_gcm_encrypt, poly1305};
pub use aes::{
    aes128_encrypt_block, aes128_key_expand, aes256_cbc_decrypt, aes256_cbc_encrypt,
    aes256_decrypt_block, aes256_encrypt_block, aes256_key_schedule,
};
pub use api::{CipherAlgorithm, CryptoTransform, HashAlgorithm, available_algorithms};
pub use chacha20::{chacha20, chacha20_rfc8439_kat};
pub use curve25519::{ed25519_public_key, ed25519_sign, ed25519_verify, x25519};
pub use ecdsa::{EcdsaPrivateKey, EcdsaPublicKey, ecdsa_generate_keypair, ecdsa_p256_verify};
pub use gates::{
    GATE_AESGCM_MARKER, GATE_ECDSA_MARKER, GATE_ED25519_MARKER, GATE_POLY1305_MARKER,
    GATE_RSA_MARKER, GATE_X25519_MARKER,
};
pub use hmac::{hmac_sha256, pbkdf2_sha256};
pub use random::random_bytes;
pub use rsa::{
    RsaKeyPair, RsaPrivateKey, RsaPublicKey, rsa_decrypt, rsa_encrypt, rsa_generate_keypair,
    rsa_sign, rsa_verify,
};
pub use sha256::{Sha256, sha256};
pub use sha512::sha512;

use crate::serial_println;

use gates::{aead_self_test, curve_self_test};

// ─── Init ──────────────────────────────────────────────────────────────

pub fn init() {
    serial_println!("[CRYPTO] Kernel cryptographic subsystem initialized");
    serial_println!("[CRYPTO]   SHA-256, SHA-512, HMAC-SHA256, PBKDF2");
    serial_println!("[CRYPTO]   AES-128/256-CBC, AES-GCM, ChaCha20, Poly1305");
    serial_println!("[CRYPTO]   RSA-2048/4096, ECDSA-P256, X25519, Ed25519");
    serial_println!("[CRYPTO]   RDRAND secure random");
    let _ = aead_self_test();
    let _ = curve_self_test();
}
