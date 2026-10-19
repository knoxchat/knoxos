use alloc::vec::Vec;

use super::bn::{bn_bitlen, bn_from_be, bn_modexp, bn_to_be};
use super::sha256::sha256;

// ═══════════════════════════════════════════════════════════════════════
// RSA KEY GENERATION & OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// RSA public key
#[derive(Debug, Clone)]
pub struct RsaPublicKey {
    /// Modulus n = p * q
    pub n: Vec<u8>,
    /// Public exponent e (usually 65537)
    pub e: u32,
    /// Key size in bits
    pub bits: u32,
}

/// RSA private key
#[derive(Debug, Clone)]
pub struct RsaPrivateKey {
    pub public: RsaPublicKey,
    /// Private exponent d
    pub d: Vec<u8>,
    /// Prime p
    pub p: Vec<u8>,
    /// Prime q
    pub q: Vec<u8>,
}

/// RSA key pair
#[derive(Debug, Clone)]
pub struct RsaKeyPair {
    pub public_key: RsaPublicKey,
    pub private_key: RsaPrivateKey,
}

/// Big-number modular exponentiation: base^exp mod modulus (big-endian bytes).
fn mod_pow_bytes(base: &[u8], exp: &[u8], modulus: &[u8]) -> Vec<u8> {
    let m = bn_from_be(modulus);
    if m.len() == 1 && m[0] == 0 {
        return alloc::vec![0];
    }
    let r = bn_modexp(&bn_from_be(base), &bn_from_be(exp), &m);
    let k = modulus.len().max(bn_bitlen(&m).div_ceil(8)).max(1);
    bn_to_be(&r, k)
}

fn u128_to_bytes(val: u128) -> Vec<u8> {
    let bytes = val.to_be_bytes();
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(15);
    bytes[start..].to_vec()
}

/// Modular multiplication avoiding overflow: (a * b) % m
fn mul_mod(a: u128, b: u128, m: u128) -> u128 {
    let mut result: u128 = 0;
    let mut a = a % m;
    let mut b = b;
    while b > 0 {
        if b & 1 == 1 {
            result = (result + a) % m;
        }
        a = (a << 1) % m;
        b >>= 1;
    }
    result
}

/// Miller-Rabin primality test
fn is_probably_prime(n: u64, rounds: u32) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 || n == 3 {
        return true;
    }
    if n % 2 == 0 {
        return false;
    }

    // Write n-1 = 2^r * d
    let mut d = n - 1;
    let mut r = 0u32;
    while d % 2 == 0 {
        d /= 2;
        r += 1;
    }

    // Witness loop
    let witnesses: [u64; 7] = [2, 3, 5, 7, 11, 13, 17];
    for i in 0..core::cmp::min(rounds as usize, witnesses.len()) {
        let a = witnesses[i];
        if a >= n {
            continue;
        }

        let mut x = mod_pow_u64(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }

        let mut found = false;
        for _ in 0..r - 1 {
            x = mul_mod_u64(x, x, n);
            if x == n - 1 {
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }
    true
}

fn mod_pow_u64(mut base: u64, mut exp: u64, modulus: u64) -> u64 {
    let mut result: u128 = 1;
    let mut b = base as u128;
    let m = modulus as u128;
    while exp > 0 {
        if exp & 1 == 1 {
            result = (result * b) % m;
        }
        b = (b * b) % m;
        exp >>= 1;
    }
    result as u64
}

fn mul_mod_u64(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % m as u128) as u64
}

/// Generate a random prime of approximately `bits` bits
fn generate_prime(bits: u32) -> u64 {
    loop {
        let mut val = crate::random::random_u64();
        // Ensure correct bit size
        val |= 1u64 << (bits - 1); // Set high bit
        val |= 1; // Ensure odd
        // Mask to correct size
        if bits < 64 {
            val &= (1u64 << bits) - 1;
            val |= 1u64 << (bits - 1);
        }
        if is_probably_prime(val, 7) {
            return val;
        }
    }
}

/// Extended GCD: returns (gcd, x, y) such that a*x + b*y = gcd
fn extended_gcd(a: i128, b: i128) -> (i128, i128, i128) {
    if a == 0 {
        return (b, 0, 1);
    }
    let (g, x, y) = extended_gcd(b % a, a);
    (g, y - (b / a) * x, x)
}

/// Modular inverse: a^(-1) mod m
fn mod_inverse(a: u64, m: u64) -> Option<u64> {
    let (g, x, _) = extended_gcd(a as i128, m as i128);
    if g != 1 {
        return None;
    }
    Some(((x % m as i128 + m as i128) % m as i128) as u64)
}

/// Generate an RSA key pair (simplified — uses 64-bit primes for demo)
/// In production, use 1024+ bit primes via big-integer library
pub fn rsa_generate_keypair(bits: u32) -> RsaKeyPair {
    let prime_bits = core::cmp::min(bits / 2, 31); // Limited by u64 arithmetic

    let p = generate_prime(prime_bits);
    let q = generate_prime(prime_bits);
    let n = p as u128 * q as u128;
    let phi = (p - 1) as u128 * (q - 1) as u128;
    let e: u32 = 65537;

    // Compute d = e^(-1) mod phi
    let d = {
        let (g, x, _) = extended_gcd(e as i128, phi as i128);
        if g != 1 {
            1u128
        } else {
            ((x % phi as i128 + phi as i128) % phi as i128) as u128
        }
    };

    let n_bytes = u128_to_bytes(n);
    let d_bytes = u128_to_bytes(d);
    let p_bytes = p.to_be_bytes().to_vec();
    let q_bytes = q.to_be_bytes().to_vec();

    let public_key = RsaPublicKey {
        n: n_bytes.clone(),
        e,
        bits,
    };
    let private_key = RsaPrivateKey {
        public: RsaPublicKey {
            n: n_bytes,
            e,
            bits,
        },
        d: d_bytes,
        p: p_bytes,
        q: q_bytes,
    };

    RsaKeyPair {
        public_key,
        private_key,
    }
}

/// RSA encrypt (raw, PKCS#1 v1.5 padding should be used in practice)
pub fn rsa_encrypt(plaintext: &[u8], key: &RsaPublicKey) -> Vec<u8> {
    let e_bytes = key.e.to_be_bytes().to_vec();
    mod_pow_bytes(plaintext, &e_bytes, &key.n)
}

/// RSA decrypt
pub fn rsa_decrypt(ciphertext: &[u8], key: &RsaPrivateKey) -> Vec<u8> {
    mod_pow_bytes(ciphertext, &key.d, &key.public.n)
}

/// SHA-256 DigestInfo prefix (RFC 8017, EMSA-PKCS1-v1_5).
const PKCS1_SHA256_DIGESTINFO: [u8; 19] = [
    0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05,
    0x00, 0x04, 0x20,
];

/// RSA PKCS#1 v1.5 sign (SHA-256 DigestInfo).
pub fn rsa_sign(message: &[u8], key: &RsaPrivateKey) -> Vec<u8> {
    let hash = sha256(message);
    let k = key.public.n.len().max(1);
    if k < PKCS1_SHA256_DIGESTINFO.len() + 32 + 11 {
        return Vec::new();
    }
    let mut em = alloc::vec![0xFFu8; k];
    em[0] = 0x00;
    em[1] = 0x01;
    let t_off = k - PKCS1_SHA256_DIGESTINFO.len() - 32;
    em[t_off - 1] = 0x00;
    em[t_off..t_off + PKCS1_SHA256_DIGESTINFO.len()].copy_from_slice(&PKCS1_SHA256_DIGESTINFO);
    em[t_off + PKCS1_SHA256_DIGESTINFO.len()..].copy_from_slice(&hash);
    mod_pow_bytes(&em, &key.d, &key.public.n)
}

/// RSA PKCS#1 v1.5 verify (SHA-256 DigestInfo).
pub fn rsa_verify(message: &[u8], signature: &[u8], key: &RsaPublicKey) -> bool {
    let hash = sha256(message);
    let k = key.n.len().max(1);
    if signature.is_empty() || k < PKCS1_SHA256_DIGESTINFO.len() + 32 + 11 {
        return false;
    }
    let em = {
        let mut raw = rsa_encrypt(signature, key);
        if raw.len() < k {
            let mut pad = alloc::vec![0u8; k - raw.len()];
            pad.extend_from_slice(&raw);
            raw = pad;
        } else if raw.len() > k {
            raw = raw[raw.len() - k..].to_vec();
        }
        raw
    };
    if em.len() != k || em[0] != 0x00 || em[1] != 0x01 {
        return false;
    }
    let mut i = 2;
    while i < k && em[i] == 0xFF {
        i += 1;
    }
    if i < 10 || i >= k || em[i] != 0x00 {
        return false;
    }
    i += 1;
    let t_len = PKCS1_SHA256_DIGESTINFO.len() + 32;
    if k - i != t_len {
        return false;
    }
    em[i..i + PKCS1_SHA256_DIGESTINFO.len()] == PKCS1_SHA256_DIGESTINFO
        && em[i + PKCS1_SHA256_DIGESTINFO.len()..] == hash
}
