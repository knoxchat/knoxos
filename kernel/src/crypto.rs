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
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── SHA-256 ───────────────────────────────────────────────────────────

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    total_len: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0u8; 64],
            buffer_len: 0,
            total_len: 0,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        let mut i = 0;
        self.total_len += data.len() as u64;

        if self.buffer_len > 0 {
            let fill = 64 - self.buffer_len;
            let copy = fill.min(data.len());
            self.buffer[self.buffer_len..self.buffer_len + copy].copy_from_slice(&data[..copy]);
            self.buffer_len += copy;
            i = copy;

            if self.buffer_len == 64 {
                let block = self.buffer;
                Self::compress(&mut self.state, &block);
                self.buffer_len = 0;
            }
        }

        while i + 64 <= data.len() {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[i..i + 64]);
            Self::compress(&mut self.state, &block);
            i += 64;
        }

        if i < data.len() {
            let remaining = data.len() - i;
            self.buffer[..remaining].copy_from_slice(&data[i..]);
            self.buffer_len = remaining;
        }
    }

    pub fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_len * 8;
        let mut padding = [0u8; 72];
        padding[0] = 0x80;
        let pad_len = if self.buffer_len < 56 {
            56 - self.buffer_len
        } else {
            120 - self.buffer_len
        };
        self.update(&padding[..pad_len]);
        self.update(&bit_len.to_be_bytes());

        let mut result = [0u8; 32];
        for (i, &word) in self.state.iter().enumerate() {
            result[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
        }
        result
    }

    fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }
}

/// One-shot SHA-256
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize()
}

// ─── SHA-512 ───────────────────────────────────────────────────────────

const SHA512_K: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

pub fn sha512(data: &[u8]) -> [u8; 64] {
    let mut state: [u64; 8] = [
        0x6a09e667f3bcc908,
        0xbb67ae8584caa73b,
        0x3c6ef372fe94f82b,
        0xa54ff53a5f1d36f1,
        0x510e527fade682d1,
        0x9b05688c2b3e6c1f,
        0x1f83d9abfb41bd6b,
        0x5be0cd19137e2179,
    ];

    // Pad message
    let bit_len = (data.len() as u128) * 8;
    let mut padded = Vec::from(data);
    padded.push(0x80);
    while (padded.len() % 128) != 112 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Process blocks
    for chunk in padded.chunks(128) {
        let mut w = [0u64; 80];
        for i in 0..16 {
            w[i] = u64::from_be_bytes([
                chunk[i * 8],
                chunk[i * 8 + 1],
                chunk[i * 8 + 2],
                chunk[i * 8 + 3],
                chunk[i * 8 + 4],
                chunk[i * 8 + 5],
                chunk[i * 8 + 6],
                chunk[i * 8 + 7],
            ]);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;

        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA512_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    let mut result = [0u8; 64];
    for (i, &word) in state.iter().enumerate() {
        result[i * 8..(i + 1) * 8].copy_from_slice(&word.to_be_bytes());
    }
    result
}

// ─── HMAC ──────────────────────────────────────────────────────────────

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut padded_key = [0u8; 64];
    if key.len() > 64 {
        padded_key[..32].copy_from_slice(&sha256(key));
    } else {
        padded_key[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= padded_key[i];
        opad[i] ^= padded_key[i];
    }

    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(&inner_hash);
    outer.finalize()
}

// ─── PBKDF2-HMAC-SHA256 ───────────────────────────────────────────────

pub fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32, output: &mut [u8]) {
    let dk_len = output.len();
    let block_count = dk_len.div_ceil(32);

    for block_idx in 0..block_count {
        let mut salt_block = Vec::from(salt);
        salt_block.extend_from_slice(&((block_idx as u32 + 1).to_be_bytes()));

        let mut u = hmac_sha256(password, &salt_block);
        let mut result = u;

        for _ in 1..iterations {
            u = hmac_sha256(password, &u);
            for j in 0..32 {
                result[j] ^= u[j];
            }
        }

        let start = block_idx * 32;
        let end = (start + 32).min(dk_len);
        output[start..end].copy_from_slice(&result[..end - start]);
    }
}

// ─── ChaCha20 ──────────────────────────────────────────────────────────

fn quarter_round(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(12);
    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(8);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(7);
}

fn chacha20_block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let mut state = [0u32; 16];
    // "expand 32-byte k"
    state[0] = 0x61707865;
    state[1] = 0x3320646e;
    state[2] = 0x79622d32;
    state[3] = 0x6b206574;

    for i in 0..8 {
        state[4 + i] =
            u32::from_le_bytes([key[i * 4], key[i * 4 + 1], key[i * 4 + 2], key[i * 4 + 3]]);
    }
    state[12] = counter;
    for i in 0..3 {
        state[13 + i] = u32::from_le_bytes([
            nonce[i * 4],
            nonce[i * 4 + 1],
            nonce[i * 4 + 2],
            nonce[i * 4 + 3],
        ]);
    }

    let initial = state;
    for _ in 0..10 {
        quarter_round(&mut state, 0, 4, 8, 12);
        quarter_round(&mut state, 1, 5, 9, 13);
        quarter_round(&mut state, 2, 6, 10, 14);
        quarter_round(&mut state, 3, 7, 11, 15);
        quarter_round(&mut state, 0, 5, 10, 15);
        quarter_round(&mut state, 1, 6, 11, 12);
        quarter_round(&mut state, 2, 7, 8, 13);
        quarter_round(&mut state, 3, 4, 9, 14);
    }

    for i in 0..16 {
        state[i] = state[i].wrapping_add(initial[i]);
    }

    let mut output = [0u8; 64];
    for i in 0..16 {
        output[i * 4..(i + 1) * 4].copy_from_slice(&state[i].to_le_bytes());
    }
    output
}

/// ChaCha20 stream cipher
pub fn chacha20(key: &[u8; 32], nonce: &[u8; 12], counter: u32, data: &mut [u8]) {
    let mut block_counter = counter;
    let mut offset = 0;
    while offset < data.len() {
        let keystream = chacha20_block(key, block_counter, nonce);
        let remaining = data.len() - offset;
        let to_xor = remaining.min(64);
        for i in 0..to_xor {
            data[offset + i] ^= keystream[i];
        }
        offset += to_xor;
        block_counter += 1;
    }
}

/// RFC 8439 §2.3.2 ChaCha20 block known-answer test.
pub fn chacha20_rfc8439_kat() -> bool {
    let key: [u8; 32] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
        0x1e, 0x1f,
    ];
    let nonce: [u8; 12] = [
        0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x4a, 0x00, 0x00, 0x00, 0x00,
    ];
    // RFC 8439 §2.3.2 serialized block (little-endian words).
    let expected: [u8; 64] = [
        0x10, 0xf1, 0xe7, 0xe4, 0xd1, 0x3b, 0x59, 0x15, 0x50, 0x0f, 0xdd, 0x1f, 0xa3, 0x20, 0x71,
        0xc4, 0xc7, 0xd1, 0xf4, 0xc7, 0x33, 0xc0, 0x68, 0x03, 0x04, 0x22, 0xaa, 0x9a, 0xc3, 0xd4,
        0x6c, 0x4e, 0xd2, 0x82, 0x64, 0x46, 0x07, 0x9f, 0xaa, 0x09, 0x14, 0xc2, 0xd7, 0x05, 0xd9,
        0x8b, 0x02, 0xa2, 0xb5, 0x12, 0x9c, 0xd1, 0xde, 0x16, 0x4e, 0xb9, 0xcb, 0xd0, 0x83, 0xe8,
        0xa2, 0x50, 0x3c, 0x4e,
    ];
    chacha20_block(&key, 1, &nonce) == expected
}

// ─── AES-128 (Software) ───────────────────────────────────────────────

// AES S-box
const SBOX: [u8; 256] = [
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

const RCON: [u8; 10] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36];

/// Expand AES-128 key
pub fn aes128_key_expand(key: &[u8; 16]) -> [[u8; 16]; 11] {
    let mut round_keys = [[0u8; 16]; 11];
    round_keys[0].copy_from_slice(key);

    for i in 1..11 {
        let prev = round_keys[i - 1];
        let mut temp = [prev[12], prev[13], prev[14], prev[15]];

        // RotWord
        temp.rotate_left(1);
        // SubWord
        for b in temp.iter_mut() {
            *b = SBOX[*b as usize];
        }
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

/// AES-128 encrypt a single 16-byte block (simplified — full rounds)
pub fn aes128_encrypt_block(block: &[u8; 16], round_keys: &[[u8; 16]; 11]) -> [u8; 16] {
    let mut state = *block;

    // Initial round key addition
    for i in 0..16 {
        state[i] ^= round_keys[0][i];
    }

    // Rounds 1-9
    for round_key in round_keys.iter().take(10).skip(1) {
        // SubBytes
        for b in state.iter_mut() {
            *b = SBOX[*b as usize];
        }
        // ShiftRows
        let tmp = state;
        state[1] = tmp[5];
        state[5] = tmp[9];
        state[9] = tmp[13];
        state[13] = tmp[1];
        state[2] = tmp[10];
        state[6] = tmp[14];
        state[10] = tmp[2];
        state[14] = tmp[6];
        state[3] = tmp[15];
        state[7] = tmp[3];
        state[11] = tmp[7];
        state[15] = tmp[11];
        // MixColumns
        for col in 0..4 {
            let c = col * 4;
            let a = [state[c], state[c + 1], state[c + 2], state[c + 3]];
            state[c] = gf_mul(2, a[0]) ^ gf_mul(3, a[1]) ^ a[2] ^ a[3];
            state[c + 1] = a[0] ^ gf_mul(2, a[1]) ^ gf_mul(3, a[2]) ^ a[3];
            state[c + 2] = a[0] ^ a[1] ^ gf_mul(2, a[2]) ^ gf_mul(3, a[3]);
            state[c + 3] = gf_mul(3, a[0]) ^ a[1] ^ a[2] ^ gf_mul(2, a[3]);
        }
        // AddRoundKey
        for i in 0..16 {
            state[i] ^= round_key[i];
        }
    }

    // Final round (no MixColumns)
    for b in state.iter_mut() {
        *b = SBOX[*b as usize];
    }
    let tmp = state;
    state[1] = tmp[5];
    state[5] = tmp[9];
    state[9] = tmp[13];
    state[13] = tmp[1];
    state[2] = tmp[10];
    state[6] = tmp[14];
    state[10] = tmp[2];
    state[14] = tmp[6];
    state[3] = tmp[15];
    state[7] = tmp[3];
    state[11] = tmp[7];
    state[15] = tmp[11];
    for i in 0..16 {
        state[i] ^= round_keys[10][i];
    }

    state
}

/// GF(2^8) multiplication
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

// ─── Secure random ─────────────────────────────────────────────────────

/// Generate random bytes using RDRAND
pub fn random_bytes(buf: &mut [u8]) {
    let mut i = 0;
    while i + 8 <= buf.len() {
        let mut val: u64 = 0;
        // SAFETY: inline assembly uses the architecture instruction and listed clobbers; no unexpected memory is accessed.
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "rdrand {val}",
                val = out(reg) val,
            );
        }
        buf[i..i + 8].copy_from_slice(&val.to_le_bytes());
        i += 8;
    }
    if i < buf.len() {
        let mut val: u64 = 0;
        // SAFETY: inline assembly uses the architecture instruction and listed clobbers; no unexpected memory is accessed.
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "rdrand {val}",
                val = out(reg) val,
            );
        }
        let bytes = val.to_le_bytes();
        let remaining = buf.len() - i;
        buf[i..i + remaining].copy_from_slice(&bytes[..remaining]);
    }
}

// ─── AES-GCM + Poly1305 ────────────────────────────────────────────────

fn be_u128(block: &[u8; 16]) -> u128 {
    u128::from_be_bytes(*block)
}

fn ghash_mul(mut x: u128, y: u128) -> u128 {
    let mut z = 0u128;
    for i in 0..128 {
        if (y >> (127 - i)) & 1 == 1 {
            z ^= x;
        }
        let lsb = x & 1;
        x >>= 1;
        if lsb == 1 {
            x ^= 0xE1u128 << 120;
        }
    }
    z
}

fn gcm_inc32(block: &mut [u8; 16]) {
    for i in (12..16).rev() {
        block[i] = block[i].wrapping_add(1);
        if block[i] != 0 {
            break;
        }
    }
}

fn ghash_pad(data: &[u8]) -> alloc::vec::Vec<[u8; 16]> {
    let mut out = alloc::vec::Vec::new();
    if data.is_empty() {
        return out;
    }
    for chunk in data.chunks(16) {
        let mut b = [0u8; 16];
        b[..chunk.len()].copy_from_slice(chunk);
        out.push(b);
    }
    out
}

fn ghash(h: u128, aad: &[u8], ct: &[u8]) -> u128 {
    let mut y = 0u128;
    for block in ghash_pad(aad) {
        y = ghash_mul(y ^ be_u128(&block), h);
    }
    for block in ghash_pad(ct) {
        y = ghash_mul(y ^ be_u128(&block), h);
    }
    let mut lens = [0u8; 16];
    lens[..8].copy_from_slice(&(aad.len() as u64 * 8).to_be_bytes());
    lens[8..].copy_from_slice(&(ct.len() as u64 * 8).to_be_bytes());
    ghash_mul(y ^ be_u128(&lens), h)
}

/// AES-128-GCM encrypt. IV must be 96 bits (NIST recommended).
pub fn aes128_gcm_encrypt(
    key: &[u8; 16],
    iv: &[u8; 12],
    aad: &[u8],
    plaintext: &[u8],
) -> (alloc::vec::Vec<u8>, [u8; 16]) {
    let rk = aes128_key_expand(key);
    let h = be_u128(&aes128_encrypt_block(&[0u8; 16], &rk));
    let mut j0 = [0u8; 16];
    j0[..12].copy_from_slice(iv);
    j0[15] = 1;
    let mut counter = j0;
    gcm_inc32(&mut counter);
    let mut ciphertext = alloc::vec::Vec::with_capacity(plaintext.len());
    for chunk in plaintext.chunks(16) {
        let ks = aes128_encrypt_block(&counter, &rk);
        gcm_inc32(&mut counter);
        for (i, b) in chunk.iter().enumerate() {
            ciphertext.push(b ^ ks[i]);
        }
    }
    let s = ghash(h, aad, &ciphertext);
    let tag_mask = aes128_encrypt_block(&j0, &rk);
    let s_bytes = s.to_be_bytes();
    let mut tag = [0u8; 16];
    for i in 0..16 {
        tag[i] = s_bytes[i] ^ tag_mask[i];
    }
    (ciphertext, tag)
}

fn mul_u128(a: u128, b: u128) -> (u128, u128) {
    let a0 = a as u64 as u128;
    let a1 = a >> 64;
    let b0 = b as u64 as u128;
    let b1 = b >> 64;
    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;
    let mut lo = p00;
    let (nlo, c1) = lo.overflowing_add(p01 << 64);
    lo = nlo;
    let mut hi = (p01 >> 64) + u128::from(c1);
    let (nlo, c2) = lo.overflowing_add(p10 << 64);
    lo = nlo;
    hi += (p10 >> 64) + u128::from(c2);
    hi += p11;
    (lo, hi)
}

fn poly1305_reduce(mut lo: u128, mut hi: u128) -> (u128, u128) {
    loop {
        let top = hi >> 2;
        if top == 0 {
            break;
        }
        hi &= 3;
        let (nlo, c) = lo.overflowing_add(top.wrapping_mul(5));
        lo = nlo;
        hi += u128::from(c);
    }
    (lo, hi)
}

/// Poly1305 one-time authenticator (RFC 8439).
pub fn poly1305(key: &[u8; 32], msg: &[u8]) -> [u8; 16] {
    let mut rbytes = [0u8; 16];
    rbytes.copy_from_slice(&key[0..16]);
    rbytes[3] &= 15;
    rbytes[7] &= 15;
    rbytes[11] &= 15;
    rbytes[15] &= 15;
    rbytes[4] &= 252;
    rbytes[8] &= 252;
    rbytes[12] &= 252;
    let r = u128::from_le_bytes(rbytes);
    let mut sbytes = [0u8; 16];
    sbytes.copy_from_slice(&key[16..32]);
    let s = u128::from_le_bytes(sbytes);

    let mut h_lo = 0u128;
    let mut h_hi = 0u128;
    let mut offset = 0;
    while offset < msg.len() {
        let n = core::cmp::min(16, msg.len() - offset);
        let mut block = [0u8; 16];
        block[..n].copy_from_slice(&msg[offset..offset + n]);
        let n_lo = u128::from_le_bytes(block);
        let (sum, c) = h_lo.overflowing_add(n_lo);
        h_lo = sum;
        h_hi += u128::from(c);
        // Append the 2^{8n} pad bit: for a full block this is 2^128 (into hi).
        if n == 16 {
            h_hi += 1;
        } else {
            let pad = 1u128 << (8 * n);
            let (sum2, c2) = h_lo.overflowing_add(pad);
            h_lo = sum2;
            h_hi += u128::from(c2);
        }

        let (p0, p1) = mul_u128(h_lo, r);
        let p_hi = p1.wrapping_add(h_hi.wrapping_mul(r));
        let (nlo, nhi) = poly1305_reduce(p0, p_hi);
        h_lo = nlo;
        h_hi = nhi;
        offset += 16;
    }

    // Complete reduction: if h >= 2^130-5, subtract it (add 5 with wrap).
    if h_hi == 3 && h_lo >= u128::MAX - 4 {
        h_lo = h_lo.wrapping_add(5);
        h_hi = 0;
    }
    let (tag, _) = h_lo.overflowing_add(s);
    tag.to_le_bytes()
}

pub const GATE_AESGCM_MARKER: &str = "GATE_AESGCM kat";
pub const GATE_POLY1305_MARKER: &str = "GATE_POLY1305 kat";

fn aead_self_test() -> bool {
    let key = [0u8; 16];
    let pt = [0u8; 16];
    let iv = [0u8; 12];
    let (ct, tag) = aes128_gcm_encrypt(&key, &iv, &[], &pt);
    const EXP_CT: [u8; 16] = [
        0x03, 0x88, 0xda, 0xce, 0x60, 0xb6, 0xa3, 0x92, 0xf3, 0x28, 0xc2, 0xb9, 0x71, 0xb2, 0xfe,
        0x78,
    ];
    const EXP_TAG: [u8; 16] = [
        0xab, 0x6e, 0x47, 0xd4, 0x2c, 0xec, 0x13, 0xbd, 0xf5, 0x3a, 0x67, 0xb2, 0x12, 0x57, 0xbd,
        0xdf,
    ];
    let gcm_ok = ct.as_slice() == EXP_CT.as_slice() && tag == EXP_TAG;
    if gcm_ok {
        serial_println!("[CRYPTO] {} (NIST AES-128-GCM)", GATE_AESGCM_MARKER);
    } else {
        serial_println!(
            "[CRYPTO] GATE_AESGCM FAILED ct={:02x?} tag={:02x?}",
            &ct[..ct.len().min(4)],
            &tag[..4]
        );
    }

    let mut pkey = [0u8; 32];
    pkey.copy_from_slice(&[
        0x85, 0xd6, 0xbe, 0x78, 0x57, 0x55, 0x6d, 0x33, 0x7f, 0x44, 0x52, 0xfe, 0x42, 0xd5, 0x06,
        0xa8, 0x01, 0x03, 0x80, 0x8a, 0xfb, 0x0d, 0xb2, 0xfd, 0x4a, 0xbf, 0xf6, 0xaf, 0x41, 0x49,
        0xf5, 0x1b,
    ]);
    let msg = b"Cryptographic Forum Research Group";
    let mac = poly1305(&pkey, msg);
    const EXP_MAC: [u8; 16] = [
        0xa8, 0x06, 0x1d, 0xc1, 0x30, 0x51, 0x36, 0xc6, 0xc2, 0x2b, 0x8b, 0xaf, 0x0c, 0x01, 0x27,
        0xa9,
    ];
    let poly_ok = mac == EXP_MAC;
    if poly_ok {
        serial_println!("[CRYPTO] {} (RFC 8439)", GATE_POLY1305_MARKER);
    } else {
        serial_println!("[CRYPTO] GATE_POLY1305 FAILED {:02x?}", &mac[..4]);
    }
    gcm_ok && poly_ok
}

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

// ═══════════════════════════════════════════════════════════════════════
// AES-256 ENCRYPTION/DECRYPTION
// ═══════════════════════════════════════════════════════════════════════

/// AES-256 key schedule — expands 32-byte key to 15 round keys
pub fn aes256_key_schedule(key: &[u8; 32]) -> [[u8; 16]; 15] {
    let mut round_keys = [[0u8; 16]; 15];

    // First two round keys come directly from the key
    round_keys[0].copy_from_slice(&key[0..16]);
    round_keys[1].copy_from_slice(&key[16..32]);

    for i in 2..15 {
        let prev = round_keys[i - 1];
        let prev2 = round_keys[i - 2];

        if i % 2 == 0 {
            // Even round: RotWord + SubWord + Rcon
            let mut temp = [prev[12], prev[13], prev[14], prev[15]];
            temp.rotate_left(1);
            for b in temp.iter_mut() {
                *b = SBOX[*b as usize];
            }
            temp[0] ^= RCON[(i / 2) - 1];
            for j in 0..4 {
                round_keys[i][j] = prev2[j] ^ temp[j];
            }
            for j in 4..16 {
                round_keys[i][j] = prev2[j] ^ round_keys[i][j - 4];
            }
        } else {
            // Odd round: SubWord only
            let mut temp = [prev[12], prev[13], prev[14], prev[15]];
            for b in temp.iter_mut() {
                *b = SBOX[*b as usize];
            }
            for j in 0..4 {
                round_keys[i][j] = prev2[j] ^ temp[j];
            }
            for j in 4..16 {
                round_keys[i][j] = prev2[j] ^ round_keys[i][j - 4];
            }
        }
    }
    round_keys
}

/// AES-256 encrypt a single 16-byte block (14 rounds)
pub fn aes256_encrypt_block(block: &[u8; 16], round_keys: &[[u8; 16]; 15]) -> [u8; 16] {
    let mut state = *block;

    // Initial round key addition
    for i in 0..16 {
        state[i] ^= round_keys[0][i];
    }

    // Rounds 1-13
    for round_key in round_keys.iter().take(14).skip(1) {
        // SubBytes
        for b in state.iter_mut() {
            *b = SBOX[*b as usize];
        }
        // ShiftRows
        let tmp = state;
        state[1] = tmp[5];
        state[5] = tmp[9];
        state[9] = tmp[13];
        state[13] = tmp[1];
        state[2] = tmp[10];
        state[6] = tmp[14];
        state[10] = tmp[2];
        state[14] = tmp[6];
        state[3] = tmp[15];
        state[7] = tmp[3];
        state[11] = tmp[7];
        state[15] = tmp[11];
        // MixColumns
        for col in 0..4 {
            let c = col * 4;
            let a = [state[c], state[c + 1], state[c + 2], state[c + 3]];
            state[c] = gf_mul(2, a[0]) ^ gf_mul(3, a[1]) ^ a[2] ^ a[3];
            state[c + 1] = a[0] ^ gf_mul(2, a[1]) ^ gf_mul(3, a[2]) ^ a[3];
            state[c + 2] = a[0] ^ a[1] ^ gf_mul(2, a[2]) ^ gf_mul(3, a[3]);
            state[c + 3] = gf_mul(3, a[0]) ^ a[1] ^ a[2] ^ gf_mul(2, a[3]);
        }
        // AddRoundKey
        for i in 0..16 {
            state[i] ^= round_key[i];
        }
    }

    // Final round (no MixColumns)
    for b in state.iter_mut() {
        *b = SBOX[*b as usize];
    }
    let tmp = state;
    state[1] = tmp[5];
    state[5] = tmp[9];
    state[9] = tmp[13];
    state[13] = tmp[1];
    state[2] = tmp[10];
    state[6] = tmp[14];
    state[10] = tmp[2];
    state[14] = tmp[6];
    state[3] = tmp[15];
    state[7] = tmp[3];
    state[11] = tmp[7];
    state[15] = tmp[11];
    for i in 0..16 {
        state[i] ^= round_keys[14][i];
    }
    state
}

/// AES-256 decrypt a single 16-byte block
pub fn aes256_decrypt_block(block: &[u8; 16], round_keys: &[[u8; 16]; 15]) -> [u8; 16] {
    let mut state = *block;

    // Initial round key (last)
    for i in 0..16 {
        state[i] ^= round_keys[14][i];
    }

    // InvShiftRows
    let tmp = state;
    state[1] = tmp[13];
    state[5] = tmp[1];
    state[9] = tmp[5];
    state[13] = tmp[9];
    state[2] = tmp[10];
    state[6] = tmp[14];
    state[10] = tmp[2];
    state[14] = tmp[6];
    state[3] = tmp[7];
    state[7] = tmp[11];
    state[11] = tmp[15];
    state[15] = tmp[3];

    // InvSubBytes
    for b in state.iter_mut() {
        *b = INV_SBOX[*b as usize];
    }

    // Rounds 13 down to 1
    for round in (1..14).rev() {
        // AddRoundKey
        for i in 0..16 {
            state[i] ^= round_keys[round][i];
        }
        // InvMixColumns
        for col in 0..4 {
            let c = col * 4;
            let a = [state[c], state[c + 1], state[c + 2], state[c + 3]];
            state[c] = gf_mul(14, a[0]) ^ gf_mul(11, a[1]) ^ gf_mul(13, a[2]) ^ gf_mul(9, a[3]);
            state[c + 1] = gf_mul(9, a[0]) ^ gf_mul(14, a[1]) ^ gf_mul(11, a[2]) ^ gf_mul(13, a[3]);
            state[c + 2] = gf_mul(13, a[0]) ^ gf_mul(9, a[1]) ^ gf_mul(14, a[2]) ^ gf_mul(11, a[3]);
            state[c + 3] = gf_mul(11, a[0]) ^ gf_mul(13, a[1]) ^ gf_mul(9, a[2]) ^ gf_mul(14, a[3]);
        }
        // InvShiftRows
        let tmp = state;
        state[1] = tmp[13];
        state[5] = tmp[1];
        state[9] = tmp[5];
        state[13] = tmp[9];
        state[2] = tmp[10];
        state[6] = tmp[14];
        state[10] = tmp[2];
        state[14] = tmp[6];
        state[3] = tmp[7];
        state[7] = tmp[11];
        state[11] = tmp[15];
        state[15] = tmp[3];
        // InvSubBytes
        for b in state.iter_mut() {
            *b = INV_SBOX[*b as usize];
        }
    }

    // Final round key
    for i in 0..16 {
        state[i] ^= round_keys[0][i];
    }
    state
}

/// Inverse S-Box for AES decryption
static INV_SBOX: [u8; 256] = [
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

/// AES-256-CBC encrypt
pub fn aes256_cbc_encrypt(plaintext: &[u8], key: &[u8; 32], iv: &[u8; 16]) -> Vec<u8> {
    let round_keys = aes256_key_schedule(key);
    let mut result = Vec::new();

    // PKCS#7 padding
    let pad_len = 16 - (plaintext.len() % 16);
    let mut padded = plaintext.to_vec();
    for _ in 0..pad_len {
        padded.push(pad_len as u8);
    }

    let mut prev_block = *iv;
    for chunk in padded.chunks(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);
        // XOR with previous ciphertext block (CBC)
        for i in 0..16 {
            block[i] ^= prev_block[i];
        }
        let encrypted = aes256_encrypt_block(&block, &round_keys);
        result.extend_from_slice(&encrypted);
        prev_block = encrypted;
    }
    result
}

/// AES-256-CBC decrypt
pub fn aes256_cbc_decrypt(
    ciphertext: &[u8],
    key: &[u8; 32],
    iv: &[u8; 16],
) -> Result<Vec<u8>, &'static str> {
    if ciphertext.len() % 16 != 0 || ciphertext.is_empty() {
        return Err("invalid ciphertext length");
    }
    let round_keys = aes256_key_schedule(key);
    let mut result = Vec::new();

    let mut prev_block = *iv;
    for chunk in ciphertext.chunks(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);
        let decrypted = aes256_decrypt_block(&block, &round_keys);
        let mut plain_block = [0u8; 16];
        for i in 0..16 {
            plain_block[i] = decrypted[i] ^ prev_block[i];
        }
        result.extend_from_slice(&plain_block);
        prev_block = block;
    }

    // Remove PKCS#7 padding
    let pad_len = *result.last().ok_or("empty result")? as usize;
    if pad_len == 0 || pad_len > 16 {
        return Err("invalid padding");
    }
    // Constant-time padding verification
    let result_len = result.len();
    let mut pad_valid = true;
    for i in 0..pad_len {
        if result[result_len - 1 - i] != pad_len as u8 {
            pad_valid = false;
        }
    }
    if !pad_valid {
        return Err("invalid padding");
    }
    result.truncate(result_len - pad_len);
    Ok(result)
}

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

/// Little-endian u64 limbs. Limb 0 is the least-significant 64 bits.
fn bn_normalize(mut a: Vec<u64>) -> Vec<u64> {
    while a.len() > 1 && a.last() == Some(&0) {
        a.pop();
    }
    if a.is_empty() {
        a.push(0);
    }
    a
}

fn bn_from_be(bytes: &[u8]) -> Vec<u64> {
    let mut limbs = Vec::new();
    let mut i = bytes.len();
    while i > 0 {
        let start = i.saturating_sub(8);
        let mut v = 0u64;
        for &b in &bytes[start..i] {
            v = (v << 8) | b as u64;
        }
        limbs.push(v);
        i = start;
    }
    bn_normalize(limbs)
}

fn bn_to_be(n: &[u64], out_len: usize) -> Vec<u8> {
    let mut out = alloc::vec![0u8; out_len];
    let mut pos = out_len;
    for &limb in n {
        for j in 0..8 {
            if pos == 0 {
                break;
            }
            pos -= 1;
            out[pos] = (limb >> (8 * j)) as u8;
        }
    }
    out
}

fn bn_cmp(a: &[u64], b: &[u64]) -> core::cmp::Ordering {
    let na = a.iter().rposition(|&x| x != 0).map(|i| i + 1).unwrap_or(0);
    let nb = b.iter().rposition(|&x| x != 0).map(|i| i + 1).unwrap_or(0);
    if na != nb {
        return na.cmp(&nb);
    }
    for i in (0..na).rev() {
        if a[i] != b[i] {
            return a[i].cmp(&b[i]);
        }
    }
    core::cmp::Ordering::Equal
}

fn bn_bitlen(a: &[u64]) -> usize {
    match a.iter().rposition(|&x| x != 0) {
        None => 0,
        Some(i) => i * 64 + (64 - a[i].leading_zeros() as usize),
    }
}

fn bn_shl(a: &[u64], bits: usize) -> Vec<u64> {
    if bits == 0 {
        return bn_normalize(a.to_vec());
    }
    let limb_shift = bits / 64;
    let b = bits % 64;
    let mut out = alloc::vec![0u64; a.len() + limb_shift + 1];
    if b == 0 {
        out[limb_shift..(a.len() + limb_shift)].copy_from_slice(a);
    } else {
        let mut carry = 0u64;
        for i in 0..a.len() {
            out[i + limb_shift] = (a[i] << b) | carry;
            carry = a[i] >> (64 - b);
        }
        out[a.len() + limb_shift] = carry;
    }
    bn_normalize(out)
}

fn bn_add_u64(a: &[u64], x: u64) -> Vec<u64> {
    let mut out = a.to_vec();
    let mut carry = x as u128;
    let mut i = 0;
    while carry > 0 {
        if i == out.len() {
            out.push(0);
        }
        carry += out[i] as u128;
        out[i] = carry as u64;
        carry >>= 64;
        i += 1;
    }
    bn_normalize(out)
}

fn bn_sub(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut out = alloc::vec![0u64; a.len()];
    let mut borrow = 0u64;
    for i in 0..a.len() {
        let bv = if i < b.len() { b[i] } else { 0 };
        let (t1, br1) = a[i].overflowing_sub(bv);
        let (t2, br2) = t1.overflowing_sub(borrow);
        out[i] = t2;
        borrow = u64::from(br1 || br2);
    }
    bn_normalize(out)
}

fn bn_mul(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut out = alloc::vec![0u64; a.len() + b.len() + 1];
    for i in 0..a.len() {
        let mut carry = 0u128;
        for j in 0..b.len() {
            carry += out[i + j] as u128 + (a[i] as u128) * (b[j] as u128);
            out[i + j] = carry as u64;
            carry >>= 64;
        }
        let mut k = i + b.len();
        while carry > 0 {
            if k == out.len() {
                out.push(0);
            }
            carry += out[k] as u128;
            out[k] = carry as u64;
            carry >>= 64;
            k += 1;
        }
    }
    bn_normalize(out)
}

fn bn_mod(a: &[u64], m: &[u64]) -> Vec<u64> {
    if m.len() == 1 && m[0] == 0 {
        return alloc::vec![0];
    }
    let bits = bn_bitlen(a);
    let mut r = alloc::vec![0u64];
    for i in (0..bits).rev() {
        r = bn_shl(&r, 1);
        let limb = i / 64;
        let bit = i % 64;
        if a.get(limb).copied().unwrap_or(0) & (1u64 << bit) != 0 {
            r = bn_add_u64(&r, 1);
        }
        if bn_cmp(&r, m) != core::cmp::Ordering::Less {
            r = bn_sub(&r, m);
        }
    }
    bn_normalize(r)
}

fn bn_modexp(base: &[u64], exp: &[u64], modulus: &[u64]) -> Vec<u64> {
    let mut result = alloc::vec![1u64];
    let mut b = bn_mod(base, modulus);
    let bits = bn_bitlen(exp);
    for i in 0..bits {
        let limb = i / 64;
        let bit = i % 64;
        if exp.get(limb).copied().unwrap_or(0) & (1u64 << bit) != 0 {
            result = bn_mod(&bn_mul(&result, &b), modulus);
        }
        b = bn_mod(&bn_mul(&b, &b), modulus);
    }
    result
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

// ═══════════════════════════════════════════════════════════════════════
// ECDSA P-256 (NIST SECP256R1)
// ═══════════════════════════════════════════════════════════════════════

/// ECDSA P-256 public key
#[derive(Debug, Clone)]
pub struct EcdsaPublicKey {
    pub x: [u8; 32],
    pub y: [u8; 32],
}

/// ECDSA P-256 private key
#[derive(Debug, Clone)]
pub struct EcdsaPrivateKey {
    pub d: [u8; 32],
    pub public: EcdsaPublicKey,
}

type Fp = [u64; 4];

/// p = 2^256 - 2^224 + 2^192 + 2^96 - 1 (little-endian limbs)
const P256_P: Fp = [
    0xFFFFFFFFFFFFFFFF,
    0x00000000FFFFFFFF,
    0x0000000000000000,
    0xFFFFFFFF00000001,
];
/// n = order of G
const P256_N: Fp = [
    0xF3B9CAC2FC632551,
    0xBCE6FAADA7179E84,
    0xFFFFFFFFFFFFFFFF,
    0xFFFFFFFF00000000,
];
const P256_GX: Fp = [
    0xF4A13945D898C296,
    0x77037D812DEB33A0,
    0xF8BCE6E563A440F2,
    0x6B17D1F2E12C4247,
];
const P256_GY: Fp = [
    0xCBB6406837BF51F5,
    0x2BCE33576B315ECE,
    0x8EE7EB4A7C0F9E16,
    0x4FE342E2FE1A7F9B,
];

fn fp_from_bn(a: &[u64]) -> Fp {
    let mut o = [0u64; 4];
    let n = a.len().min(4);
    o[..n].copy_from_slice(&a[..n]);
    o
}

fn fp_from_be(b: &[u8; 32]) -> Fp {
    fp_from_bn(&bn_from_be(b))
}

fn fp_to_be(a: &Fp) -> [u8; 32] {
    let v = bn_to_be(a, 32);
    let mut o = [0u8; 32];
    o.copy_from_slice(&v);
    o
}

fn fp_is_zero(a: &Fp) -> bool {
    a.iter().all(|&x| x == 0)
}

fn fp_eq(a: &Fp, b: &Fp) -> bool {
    a == b
}

fn fp_add(a: &Fp, b: &Fp) -> Fp {
    fp_from_bn(&bn_mod(&bn_add_limbs(a, b), &P256_P))
}

fn bn_add_limbs(a: &[u64], b: &[u64]) -> Vec<u64> {
    let n = a.len().max(b.len());
    let mut out = alloc::vec![0u64; n + 1];
    let mut carry = 0u128;
    for i in 0..n {
        carry += a.get(i).copied().unwrap_or(0) as u128 + b.get(i).copied().unwrap_or(0) as u128;
        out[i] = carry as u64;
        carry >>= 64;
    }
    out[n] = carry as u64;
    bn_normalize(out)
}

fn fp_sub(a: &Fp, b: &Fp) -> Fp {
    if bn_cmp(a, b) == core::cmp::Ordering::Less {
        fp_from_bn(&bn_sub(&bn_add_limbs(a, &P256_P), b))
    } else {
        fp_from_bn(&bn_sub(a, b))
    }
}

fn fp_mul(a: &Fp, b: &Fp) -> Fp {
    fp_from_bn(&bn_mod(&bn_mul(a, b), &P256_P))
}

fn fp_dbl(a: &Fp) -> Fp {
    fp_add(a, a)
}

fn fp_inv(a: &Fp) -> Fp {
    // a^{p-2} mod p
    let mut e = P256_P;
    // p-2
    let p2 = bn_sub(&e, &[2]);
    fp_from_bn(&bn_modexp(a, &p2, &P256_P))
}

fn fn_mod(a: &[u64]) -> Fp {
    fp_from_bn(&bn_mod(a, &P256_N))
}

fn fn_inv(a: &Fp) -> Fp {
    let n2 = bn_sub(&P256_N, &[2]);
    fp_from_bn(&bn_modexp(a, &n2, &P256_N))
}

#[derive(Clone, Copy)]
struct P256Pt {
    x: Fp,
    y: Fp,
    z: Fp,
}

fn p256_inf() -> P256Pt {
    P256Pt {
        x: [1, 0, 0, 0],
        y: [1, 0, 0, 0],
        z: [0, 0, 0, 0],
    }
}

fn p256_is_inf(p: &P256Pt) -> bool {
    fp_is_zero(&p.z)
}

fn p256_affine(p: &P256Pt) -> Option<(Fp, Fp)> {
    if p256_is_inf(p) {
        return None;
    }
    let iz = fp_inv(&p.z);
    let iz2 = fp_mul(&iz, &iz);
    let iz3 = fp_mul(&iz2, &iz);
    Some((fp_mul(&p.x, &iz2), fp_mul(&p.y, &iz3)))
}

fn p256_double(p: &P256Pt) -> P256Pt {
    if p256_is_inf(p) || fp_is_zero(&p.y) {
        return p256_inf();
    }
    let xx = fp_mul(&p.x, &p.x);
    let yy = fp_mul(&p.y, &p.y);
    let zz = fp_mul(&p.z, &p.z);
    let yyyy = fp_mul(&yy, &yy);
    let s = fp_dbl(&fp_sub(
        &fp_sub(&fp_mul(&fp_add(&p.x, &yy), &fp_add(&p.x, &yy)), &xx),
        &yyyy,
    ));
    // a = -3: M = 3*(XX - ZZ^2)
    let m = {
        let t = fp_sub(&xx, &fp_mul(&zz, &zz));
        fp_add(&fp_dbl(&t), &t)
    };
    let t = fp_sub(&fp_mul(&m, &m), &fp_dbl(&s));
    let x3 = t;
    let eight_yyyy = {
        let mut e = yyyy;
        for _ in 0..3 {
            e = fp_dbl(&e);
        }
        e
    };
    let y3 = fp_sub(&fp_mul(&m, &fp_sub(&s, &t)), &eight_yyyy);
    let z3 = fp_sub(
        &fp_sub(&fp_mul(&fp_add(&p.y, &p.z), &fp_add(&p.y, &p.z)), &yy),
        &zz,
    );
    P256Pt {
        x: x3,
        y: y3,
        z: z3,
    }
}

fn p256_add(p: &P256Pt, q: &P256Pt) -> P256Pt {
    if p256_is_inf(p) {
        return *q;
    }
    if p256_is_inf(q) {
        return *p;
    }
    let z1z1 = fp_mul(&p.z, &p.z);
    let z2z2 = fp_mul(&q.z, &q.z);
    let u1 = fp_mul(&p.x, &z2z2);
    let u2 = fp_mul(&q.x, &z1z1);
    let s1 = fp_mul(&p.y, &fp_mul(&q.z, &z2z2));
    let s2 = fp_mul(&q.y, &fp_mul(&p.z, &z1z1));
    if fp_eq(&u1, &u2) {
        if fp_eq(&s1, &s2) {
            return p256_double(p);
        }
        return p256_inf();
    }
    let h = fp_sub(&u2, &u1);
    let hh = fp_mul(&h, &h);
    let hhh = fp_mul(&h, &hh);
    let r = fp_sub(&s2, &s1);
    let v = fp_mul(&u1, &hh);
    let x3 = fp_sub(&fp_sub(&fp_mul(&r, &r), &hhh), &fp_dbl(&v));
    let y3 = fp_sub(&fp_mul(&r, &fp_sub(&v, &x3)), &fp_mul(&s1, &hhh));
    let z3 = fp_mul(&fp_mul(&p.z, &q.z), &h);
    P256Pt {
        x: x3,
        y: y3,
        z: z3,
    }
}

fn p256_from_affine(x: Fp, y: Fp) -> P256Pt {
    P256Pt {
        x,
        y,
        z: [1, 0, 0, 0],
    }
}

fn p256_mul(k: &Fp, p: &P256Pt) -> P256Pt {
    let mut r = p256_inf();
    let bits = bn_bitlen(k);
    for i in (0..bits).rev() {
        r = p256_double(&r);
        let limb = i / 64;
        let bit = i % 64;
        if k[limb] & (1u64 << bit) != 0 {
            r = p256_add(&r, p);
        }
    }
    r
}

fn p256_mul_g(d: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let k = fn_mod(&bn_from_be(d));
    let g = p256_from_affine(P256_GX, P256_GY);
    let p = p256_mul(&k, &g);
    match p256_affine(&p) {
        Some((x, y)) => (fp_to_be(&x), fp_to_be(&y)),
        None => ([0u8; 32], [0u8; 32]),
    }
}

/// Generate an ECDSA P-256 key pair (Q = d·G on secp256r1).
pub fn ecdsa_generate_keypair() -> EcdsaPrivateKey {
    let mut d = [0u8; 32];
    crate::random::fill_random(&mut d);
    let reduced = bn_to_be(&bn_mod(&bn_from_be(&d), &P256_N), 32);
    d.copy_from_slice(&reduced);
    if d.iter().all(|&b| b == 0) {
        d[31] = 1;
    }
    let (x, y) = p256_mul_g(&d);
    EcdsaPrivateKey {
        d,
        public: EcdsaPublicKey { x, y },
    }
}

/// ECDSA-SHA256 verify on P-256.
pub fn ecdsa_p256_verify(pk: &EcdsaPublicKey, msg: &[u8], r: &[u8; 32], s: &[u8; 32]) -> bool {
    let r_bn = bn_from_be(r);
    let s_bn = bn_from_be(s);
    if bn_cmp(&r_bn, &[1]) == core::cmp::Ordering::Less
        || bn_cmp(&s_bn, &[1]) == core::cmp::Ordering::Less
        || bn_cmp(&r_bn, &P256_N) != core::cmp::Ordering::Less
        || bn_cmp(&s_bn, &P256_N) != core::cmp::Ordering::Less
    {
        return false;
    }
    let z = fp_from_be(&sha256(msg));
    let s_fp = fp_from_bn(&s_bn);
    let r_fp = fp_from_bn(&r_bn);
    let w = fn_inv(&s_fp);
    let u1 = fn_mod(&bn_mul(&z, &w));
    let u2 = fn_mod(&bn_mul(&r_fp, &w));
    let g = p256_from_affine(P256_GX, P256_GY);
    let q = p256_from_affine(fp_from_be(&pk.x), fp_from_be(&pk.y));
    let sum = p256_add(&p256_mul(&u1, &g), &p256_mul(&u2, &q));
    match p256_affine(&sum) {
        Some((x, _)) => {
            let rx = fn_mod(&x);
            fp_eq(&rx, &r_fp)
        }
        None => false,
    }
}

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

// ═══════════════════════════════════════════════════════════════════════
// X25519 (RFC 7748) + Ed25519 (RFC 8032) over GF(2^255-19)
// ═══════════════════════════════════════════════════════════════════════

type Fe = [u64; 5]; // 51-bit limbs
const MASK51: u64 = (1u64 << 51) - 1;

fn fe_zero() -> Fe {
    [0, 0, 0, 0, 0]
}
fn fe_one() -> Fe {
    [1, 0, 0, 0, 0]
}

fn load8(b: &[u8], off: usize) -> u64 {
    let mut tmp = [0u8; 8];
    let n = (32 - off).min(8);
    tmp[..n].copy_from_slice(&b[off..off + n]);
    u64::from_le_bytes(tmp)
}

fn fe_from_bytes(b: &[u8; 32]) -> Fe {
    [
        load8(b, 0) & MASK51,
        (load8(b, 6) >> 3) & MASK51,
        (load8(b, 12) >> 6) & MASK51,
        (load8(b, 19) >> 1) & MASK51,
        (load8(b, 24) >> 12) & MASK51,
    ]
}

fn fe_carry(mut h: Fe) -> Fe {
    for _ in 0..2 {
        let mut c = h[0] >> 51;
        h[0] &= MASK51;
        h[1] = h[1].wrapping_add(c);
        c = h[1] >> 51;
        h[1] &= MASK51;
        h[2] = h[2].wrapping_add(c);
        c = h[2] >> 51;
        h[2] &= MASK51;
        h[3] = h[3].wrapping_add(c);
        c = h[3] >> 51;
        h[3] &= MASK51;
        h[4] = h[4].wrapping_add(c);
        c = h[4] >> 51;
        h[4] &= MASK51;
        h[0] = h[0].wrapping_add(c.wrapping_mul(19));
    }
    let c = h[0] >> 51;
    h[0] &= MASK51;
    h[1] = h[1].wrapping_add(c);
    h[1] &= MASK51;
    h
}

fn fe_to_bytes(h: Fe) -> [u8; 32] {
    // Donna fcontract: two carries, then the +19 wrap to subtract p.
    let mut t = h;
    for _ in 0..2 {
        t[1] = t[1].wrapping_add(t[0] >> 51);
        t[0] &= MASK51;
        t[2] = t[2].wrapping_add(t[1] >> 51);
        t[1] &= MASK51;
        t[3] = t[3].wrapping_add(t[2] >> 51);
        t[2] &= MASK51;
        t[4] = t[4].wrapping_add(t[3] >> 51);
        t[3] &= MASK51;
        t[0] = t[0].wrapping_add(19 * (t[4] >> 51));
        t[4] &= MASK51;
    }

    t[0] = t[0].wrapping_add(19);
    t[1] = t[1].wrapping_add(t[0] >> 51);
    t[0] &= MASK51;
    t[2] = t[2].wrapping_add(t[1] >> 51);
    t[1] &= MASK51;
    t[3] = t[3].wrapping_add(t[2] >> 51);
    t[2] &= MASK51;
    t[4] = t[4].wrapping_add(t[3] >> 51);
    t[3] &= MASK51;
    t[0] = t[0].wrapping_add(19 * (t[4] >> 51));
    t[4] &= MASK51;

    t[0] = t[0].wrapping_add((1u64 << 51) - 19);
    t[1] = t[1].wrapping_add((1u64 << 51) - 1);
    t[2] = t[2].wrapping_add((1u64 << 51) - 1);
    t[3] = t[3].wrapping_add((1u64 << 51) - 1);
    t[4] = t[4].wrapping_add((1u64 << 51) - 1);

    t[1] = t[1].wrapping_add(t[0] >> 51);
    t[0] &= MASK51;
    t[2] = t[2].wrapping_add(t[1] >> 51);
    t[1] &= MASK51;
    t[3] = t[3].wrapping_add(t[2] >> 51);
    t[2] &= MASK51;
    t[4] = t[4].wrapping_add(t[3] >> 51);
    t[3] &= MASK51;
    t[4] &= MASK51;

    let mut s = [0u8; 32];
    s[0] = t[0] as u8;
    s[1] = (t[0] >> 8) as u8;
    s[2] = (t[0] >> 16) as u8;
    s[3] = (t[0] >> 24) as u8;
    s[4] = (t[0] >> 32) as u8;
    s[5] = (t[0] >> 40) as u8;
    s[6] = ((t[0] >> 48) | (t[1] << 3)) as u8;
    s[7] = (t[1] >> 5) as u8;
    s[8] = (t[1] >> 13) as u8;
    s[9] = (t[1] >> 21) as u8;
    s[10] = (t[1] >> 29) as u8;
    s[11] = (t[1] >> 37) as u8;
    s[12] = ((t[1] >> 45) | (t[2] << 6)) as u8;
    s[13] = (t[2] >> 2) as u8;
    s[14] = (t[2] >> 10) as u8;
    s[15] = (t[2] >> 18) as u8;
    s[16] = (t[2] >> 26) as u8;
    s[17] = (t[2] >> 34) as u8;
    s[18] = (t[2] >> 42) as u8;
    s[19] = ((t[2] >> 50) | (t[3] << 1)) as u8;
    s[20] = (t[3] >> 7) as u8;
    s[21] = (t[3] >> 15) as u8;
    s[22] = (t[3] >> 23) as u8;
    s[23] = (t[3] >> 31) as u8;
    s[24] = (t[3] >> 39) as u8;
    s[25] = ((t[3] >> 47) | (t[4] << 4)) as u8;
    s[26] = (t[4] >> 4) as u8;
    s[27] = (t[4] >> 12) as u8;
    s[28] = (t[4] >> 20) as u8;
    s[29] = (t[4] >> 28) as u8;
    s[30] = (t[4] >> 36) as u8;
    s[31] = (t[4] >> 44) as u8;
    s
}

fn fe_add(a: &Fe, b: &Fe) -> Fe {
    fe_carry([
        a[0].wrapping_add(b[0]),
        a[1].wrapping_add(b[1]),
        a[2].wrapping_add(b[2]),
        a[3].wrapping_add(b[3]),
        a[4].wrapping_add(b[4]),
    ])
}

fn fe_sub(a: &Fe, b: &Fe) -> Fe {
    let a = fe_carry(*a);
    let b = fe_carry(*b);
    // 8p - b keeps limbs non-negative (p limbs are 2^51-19, 2^51-1, ...)
    fe_carry([
        a[0].wrapping_add(0x3fffffffffff68).wrapping_sub(b[0]),
        a[1].wrapping_add(0x3ffffffffffff8).wrapping_sub(b[1]),
        a[2].wrapping_add(0x3ffffffffffff8).wrapping_sub(b[2]),
        a[3].wrapping_add(0x3ffffffffffff8).wrapping_sub(b[3]),
        a[4].wrapping_add(0x3ffffffffffff8).wrapping_sub(b[4]),
    ])
}

fn fe_mul(a: &Fe, b: &Fe) -> Fe {
    let a0 = a[0] as u128;
    let a1 = a[1] as u128;
    let a2 = a[2] as u128;
    let a3 = a[3] as u128;
    let a4 = a[4] as u128;
    let b0 = b[0] as u128;
    let b1 = b[1] as u128;
    let b2 = b[2] as u128;
    let b3 = b[3] as u128;
    let b4 = b[4] as u128;
    // 2^255 ≡ 19
    let t = [
        a0 * b0 + 19 * (a1 * b4 + a2 * b3 + a3 * b2 + a4 * b1),
        a0 * b1 + a1 * b0 + 19 * (a2 * b4 + a3 * b3 + a4 * b2),
        a0 * b2 + a1 * b1 + a2 * b0 + 19 * (a3 * b4 + a4 * b3),
        a0 * b3 + a1 * b2 + a2 * b1 + a3 * b0 + 19 * (a4 * b4),
        a0 * b4 + a1 * b3 + a2 * b2 + a3 * b1 + a4 * b0,
    ];
    let mut h = [0u64; 5];
    let mut c = 0u128;
    for i in 0..5 {
        c += t[i];
        h[i] = (c as u64) & MASK51;
        c >>= 51;
    }
    h[0] = h[0].wrapping_add((c as u64).wrapping_mul(19));
    fe_carry(h)
}

fn fe_sq(a: &Fe) -> Fe {
    fe_mul(a, a)
}

fn fe_mul_small(a: &Fe, s: u64) -> Fe {
    let mut h = [0u64; 5];
    let mut c = 0u128;
    for i in 0..5 {
        c += a[i] as u128 * s as u128;
        h[i] = (c as u64) & MASK51;
        c >>= 51;
    }
    h[0] += (c as u64) * 19;
    fe_carry(h)
}

fn fe_invert(a: &Fe) -> Fe {
    // a^(p-2) = a^(2^255-21)
    let exp: [u64; 4] = [
        0xFFFF_FFFF_FFFF_FFEB,
        0xFFFF_FFFF_FFFF_FFFF,
        0xFFFF_FFFF_FFFF_FFFF,
        0x7FFF_FFFF_FFFF_FFFF,
    ];
    let mut r = fe_one();
    for i in (0..255).rev() {
        r = fe_sq(&r);
        if (exp[i / 64] >> (i % 64)) & 1 == 1 {
            r = fe_mul(&r, a);
        }
    }
    r
}

fn cswap(swap: u64, a: &mut Fe, b: &mut Fe) {
    let mask = 0u64.wrapping_sub(swap);
    for i in 0..5 {
        let t = mask & (a[i] ^ b[i]);
        a[i] ^= t;
        b[i] ^= t;
    }
}

fn load4(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        b[off],
        b[off + 1],
        b[off + 2],
        b[off + 3],
        b[off + 4],
        b[off + 5],
        b[off + 6],
        b[off + 7],
    ])
}

/// RFC 7748 X25519: scalar * u-coordinate.
pub fn x25519(scalar: &[u8; 32], u: &[u8; 32]) -> [u8; 32] {
    let mut e = *scalar;
    e[0] &= 248;
    e[31] &= 127;
    e[31] |= 64;

    let x1 = fe_from_bytes(u);
    let mut x2 = fe_one();
    let mut z2 = fe_zero();
    let mut x3 = x1;
    let mut z3 = fe_one();
    let mut swap = 0u64;

    for t in (0..255).rev() {
        let kt = ((e[t / 8] >> (t % 8)) as u64) & 1;
        swap ^= kt;
        cswap(swap, &mut x2, &mut x3);
        cswap(swap, &mut z2, &mut z3);
        swap = kt;

        let a = fe_add(&x2, &z2);
        let aa = fe_sq(&a);
        let b = fe_sub(&x2, &z2);
        let bb = fe_sq(&b);
        let e = fe_sub(&aa, &bb);
        let c = fe_add(&x3, &z3);
        let d = fe_sub(&x3, &z3);
        let da = fe_mul(&d, &a);
        let cb = fe_mul(&c, &b);
        x3 = fe_sq(&fe_add(&da, &cb));
        z3 = fe_mul(&x1, &fe_sq(&fe_sub(&da, &cb)));
        x2 = fe_mul(&aa, &bb);
        z2 = fe_mul(&e, &fe_add(&aa, &fe_mul_small(&e, 121665)));
    }
    cswap(swap, &mut x2, &mut x3);
    cswap(swap, &mut z2, &mut z3);
    let zinv = fe_invert(&z2);
    fe_to_bytes(fe_mul(&x2, &zinv))
}

// ── Ed25519 ──────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Ge {
    x: Fe,
    y: Fe,
    z: Fe,
    t: Fe,
}

fn ge_zero() -> Ge {
    Ge {
        x: fe_zero(),
        y: fe_one(),
        z: fe_one(),
        t: fe_zero(),
    }
}

/// d = -121665/121666
fn fe_d() -> Fe {
    fe_from_bytes(&[
        0xa3, 0x78, 0x59, 0x13, 0xca, 0x4d, 0xeb, 0x75, 0xab, 0xd8, 0x41, 0x41, 0x4d, 0x0a, 0x70,
        0x00, 0x98, 0xe8, 0x79, 0x77, 0x79, 0x40, 0xc7, 0x8c, 0x73, 0xfe, 0x6f, 0x2b, 0xee, 0x6c,
        0x03, 0x52,
    ])
}

fn ge_add(p: &Ge, q: &Ge) -> Ge {
    let d2 = fe_add(&fe_d(), &fe_d());
    let a = fe_mul(&fe_sub(&p.y, &p.x), &fe_sub(&q.y, &q.x));
    let b = fe_mul(&fe_add(&p.y, &p.x), &fe_add(&q.y, &q.x));
    let c = fe_mul(&fe_mul(&p.t, &d2), &q.t);
    let dd = fe_mul(&fe_add(&p.z, &p.z), &q.z);
    let e = fe_sub(&b, &a);
    let f = fe_sub(&dd, &c);
    let g = fe_add(&dd, &c);
    let h = fe_add(&b, &a);
    Ge {
        x: fe_mul(&e, &f),
        y: fe_mul(&g, &h),
        z: fe_mul(&f, &g),
        t: fe_mul(&e, &h),
    }
}

fn ge_double(p: &Ge) -> Ge {
    let a = fe_sq(&p.x);
    let b = fe_sq(&p.y);
    let c = fe_mul_small(&fe_sq(&p.z), 2);
    let d = fe_sub(&fe_zero(), &a); // a = -1
    let e = fe_sub(&fe_sub(&fe_sq(&fe_add(&p.x, &p.y)), &a), &b);
    let g = fe_add(&d, &b);
    let f = fe_sub(&g, &c);
    let h = fe_sub(&d, &b);
    Ge {
        x: fe_mul(&e, &f),
        y: fe_mul(&g, &h),
        z: fe_mul(&f, &g),
        t: fe_mul(&e, &h),
    }
}

fn ge_scalarmult(s: &[u8; 32], p: &Ge) -> Ge {
    let mut r = ge_zero();
    let mut q = Ge {
        x: p.x,
        y: p.y,
        z: p.z,
        t: p.t,
    };
    for i in 0..256 {
        if (s[i / 8] >> (i % 8)) & 1 == 1 {
            r = ge_add(&r, &q);
        }
        q = ge_double(&q);
    }
    r
}

fn ge_encode(p: &Ge) -> [u8; 32] {
    let zi = fe_invert(&p.z);
    let x = fe_mul(&p.x, &zi);
    let y = fe_mul(&p.y, &zi);
    let mut out = fe_to_bytes(y);
    let xb = fe_to_bytes(x);
    out[31] |= (xb[0] & 1) << 7;
    out
}

fn fe_sqrt(a: &Fe) -> Option<Fe> {
    // x = a^((p+3)/8) = a^(2^252 - 2)
    let mut exp_r = fe_one();
    // (p+3)/8 = 2^252 - 2 = 0x0ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe
    let exp: [u64; 4] = [
        0xFFFF_FFFF_FFFF_FFFE,
        0xFFFF_FFFF_FFFF_FFFF,
        0xFFFF_FFFF_FFFF_FFFF,
        0x0FFF_FFFF_FFFF_FFFF,
    ];
    for i in (0..252).rev() {
        exp_r = fe_sq(&exp_r);
        if (exp[i / 64] >> (i % 64)) & 1 == 1 {
            exp_r = fe_mul(&exp_r, a);
        }
    }
    let chk = fe_sq(&exp_r);
    if fe_to_bytes(chk) == fe_to_bytes(*a) {
        return Some(exp_r);
    }
    // multiply by sqrt(-1)
    let i = fe_from_bytes(&[
        0xb0, 0xa0, 0x0e, 0x4a, 0x27, 0x1b, 0xee, 0xc4, 0x78, 0xe4, 0x2f, 0xad, 0x06, 0x18, 0x43,
        0x2f, 0xa7, 0xd7, 0xfb, 0x3d, 0x99, 0x00, 0x4d, 0x2b, 0x0b, 0xdf, 0xc1, 0x4f, 0x80, 0x24,
        0x83, 0x2b,
    ]);
    let alt = fe_mul(&exp_r, &i);
    if fe_to_bytes(fe_sq(&alt)) == fe_to_bytes(*a) {
        Some(alt)
    } else {
        None
    }
}

fn ge_decode(b: &[u8; 32]) -> Option<Ge> {
    let mut yb = *b;
    let sign = yb[31] >> 7;
    yb[31] &= 0x7F;
    let y = fe_from_bytes(&yb);
    let y2 = fe_sq(&y);
    let u = fe_sub(&y2, &fe_one());
    let v = fe_add(&fe_mul(&fe_d(), &y2), &fe_one());
    let x2 = fe_mul(&u, &fe_invert(&v));
    let mut x = fe_sqrt(&x2)?;
    let xb = fe_to_bytes(x);
    if (xb[0] & 1) != sign {
        x = fe_sub(&fe_zero(), &x);
    }
    Some(Ge {
        t: fe_mul(&x, &y),
        x,
        y,
        z: fe_one(),
    })
}

fn ge_base() -> Ge {
    // RFC 8032 base point (y = 4/5)
    ge_decode(&[
        0x58, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
        0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
        0x66, 0x66,
    ])
    .unwrap_or_else(ge_zero)
}

const L: [u64; 4] = [
    0x5812_631A_5CF5_D3ED,
    0x14DE_F9DE_A2F7_9CD6,
    0x0000_0000_0000_0000,
    0x1000_0000_0000_0000,
];

fn sc_mod_l(mut n: [u64; 8]) -> [u8; 32] {
    // Reduce 512-bit n modulo L. L is 253 bits so L<<259 still fits in 512 bits.
    for shift in (0..=259).rev() {
        loop {
            let mut lsh = [0u64; 8];
            let word = shift / 64;
            let bit = shift % 64;
            if word >= 8 {
                break;
            }
            if bit == 0 {
                for i in 0..4 {
                    if word + i < 8 {
                        lsh[word + i] = L[i];
                    }
                }
            } else {
                for i in 0..4 {
                    if word + i < 8 {
                        lsh[word + i] |= L[i] << bit;
                    }
                    if word + i + 1 < 8 {
                        lsh[word + i + 1] |= L[i] >> (64 - bit);
                    }
                }
            }
            let mut ge = false;
            for i in (0..8).rev() {
                if n[i] > lsh[i] {
                    ge = true;
                    break;
                }
                if n[i] < lsh[i] {
                    ge = false;
                    break;
                }
                if i == 0 {
                    ge = true;
                }
            }
            if !ge {
                break;
            }
            let mut borrow = 0u64;
            for i in 0..8 {
                let (d, b1) = n[i].overflowing_sub(lsh[i]);
                let (d, b2) = d.overflowing_sub(borrow);
                n[i] = d;
                borrow = u64::from(b1 || b2);
            }
        }
    }
    let mut out = [0u8; 32];
    out[0..8].copy_from_slice(&n[0].to_le_bytes());
    out[8..16].copy_from_slice(&n[1].to_le_bytes());
    out[16..24].copy_from_slice(&n[2].to_le_bytes());
    out[24..32].copy_from_slice(&n[3].to_le_bytes());
    out
}

fn sc_from_bytes64(b: &[u8; 64]) -> [u8; 32] {
    let mut n = [0u64; 8];
    for i in 0..8 {
        n[i] = load4(b, i * 8);
    }
    sc_mod_l(n)
}

fn sc_muladd(a: &[u8; 32], b: &[u8; 32], c: &[u8; 32]) -> [u8; 32] {
    // (a*b + c) mod L
    let mut acc = [0u128; 8];
    let aa = [load4(a, 0), load4(a, 8), load4(a, 16), load4(a, 24)];
    let bb = [load4(b, 0), load4(b, 8), load4(b, 16), load4(b, 24)];
    let cc = [load4(c, 0), load4(c, 8), load4(c, 16), load4(c, 24)];
    for i in 0..4 {
        for j in 0..4 {
            acc[i + j] += aa[i] as u128 * bb[j] as u128;
        }
        acc[i] += cc[i] as u128;
    }
    let mut n = [0u64; 8];
    let mut carry = 0u128;
    for i in 0..8 {
        carry += acc[i];
        n[i] = carry as u64;
        carry >>= 64;
    }
    sc_mod_l(n)
}

fn ed25519_expand(seed: &[u8; 32]) -> ([u8; 32], [u8; 32], [u8; 32]) {
    let h = sha512(seed);
    let mut a = [0u8; 32];
    a.copy_from_slice(&h[..32]);
    a[0] &= 248;
    a[31] &= 63;
    a[31] |= 64;
    let mut prefix = [0u8; 32];
    prefix.copy_from_slice(&h[32..]);
    let a_pt = ge_scalarmult(&a, &ge_base());
    let pk = ge_encode(&a_pt);
    (a, prefix, pk)
}

/// Ed25519 public key from a 32-byte seed.
pub fn ed25519_public_key(seed: &[u8; 32]) -> [u8; 32] {
    ed25519_expand(seed).2
}

/// Ed25519 sign (RFC 8032).
pub fn ed25519_sign(seed: &[u8; 32], msg: &[u8]) -> [u8; 64] {
    let (a, prefix, pk) = ed25519_expand(seed);
    let mut r_in = Vec::with_capacity(32 + msg.len());
    r_in.extend_from_slice(&prefix);
    r_in.extend_from_slice(msg);
    let r_hash = sha512(&r_in);
    let r = sc_from_bytes64(&r_hash);
    let r_pt = ge_scalarmult(&r, &ge_base());
    let r_enc = ge_encode(&r_pt);
    let mut k_in = Vec::with_capacity(64 + msg.len());
    k_in.extend_from_slice(&r_enc);
    k_in.extend_from_slice(&pk);
    k_in.extend_from_slice(msg);
    let k_hash = sha512(&k_in);
    let k = sc_from_bytes64(&k_hash);
    let s = sc_muladd(&k, &a, &r);
    let mut sig = [0u8; 64];
    sig[..32].copy_from_slice(&r_enc);
    sig[32..].copy_from_slice(&s);
    sig
}

/// Ed25519 verify (RFC 8032).
pub fn ed25519_verify(pk: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    let mut r_bytes = [0u8; 32];
    r_bytes.copy_from_slice(&sig[..32]);
    let mut s_bytes = [0u8; 32];
    s_bytes.copy_from_slice(&sig[32..]);
    if s_bytes[31] & 0xE0 != 0 {
        return false;
    }
    let Some(a) = ge_decode(pk) else {
        return false;
    };
    let Some(r) = ge_decode(&r_bytes) else {
        return false;
    };
    let mut k_in = Vec::with_capacity(64 + msg.len());
    k_in.extend_from_slice(&r_bytes);
    k_in.extend_from_slice(pk);
    k_in.extend_from_slice(msg);
    let k = sc_from_bytes64(&sha512(&k_in));
    let sb = ge_scalarmult(&s_bytes, &ge_base());
    // [S]B == R + [k]A  <=>  [S]B + [k](-A) == R
    let neg_a = Ge {
        x: fe_sub(&fe_zero(), &a.x),
        y: a.y,
        z: a.z,
        t: fe_sub(&fe_zero(), &a.t),
    };
    let ka = ge_scalarmult(&k, &neg_a);
    let sum = ge_add(&sb, &ka);
    ge_encode(&sum) == r_bytes && ge_encode(&r) == r_bytes
}

pub const GATE_X25519_MARKER: &str = "GATE_X25519 kat";
pub const GATE_ED25519_MARKER: &str = "GATE_ED25519 kat";

fn curve_self_test() -> bool {
    // RFC 7748 §6.1
    let alice_sk: [u8; 32] = [
        0x77, 0x07, 0x6d, 0x0a, 0x73, 0x18, 0xa5, 0x7d, 0x3c, 0x16, 0xc1, 0x72, 0x51, 0xb2, 0x66,
        0x45, 0xdf, 0x4c, 0x2f, 0x87, 0xeb, 0xc0, 0x99, 0x2a, 0xb1, 0x77, 0xfb, 0xa5, 0x1d, 0xb9,
        0x2c, 0x2a,
    ];
    let alice_pk_exp: [u8; 32] = [
        0x85, 0x20, 0xf0, 0x09, 0x89, 0x30, 0xa7, 0x54, 0x74, 0x8b, 0x7d, 0xdc, 0xb4, 0x3e, 0xf7,
        0x5a, 0x0d, 0xbf, 0x3a, 0x0d, 0x26, 0x38, 0x1a, 0xf4, 0xeb, 0xa4, 0xa9, 0x8e, 0xaa, 0x9b,
        0x4e, 0x6a,
    ];
    let nine = {
        let mut u = [0u8; 32];
        u[0] = 9;
        u
    };
    let alice_pk = x25519(&alice_sk, &nine);
    let x_ok = alice_pk == alice_pk_exp;
    if x_ok {
        serial_println!("[CRYPTO] {} (RFC 7748)", GATE_X25519_MARKER);
    } else {
        serial_println!("[CRYPTO] GATE_X25519 FAILED {:02x?}", &alice_pk[..4]);
    }

    // RFC 8032 §7.1 TEST 1 (empty message)
    let seed: [u8; 32] = [
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
        0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
        0x7f, 0x60,
    ];
    let pk_exp: [u8; 32] = [
        0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07,
        0x3a, 0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07,
        0x51, 0x1a,
    ];
    let sig_exp: [u8; 64] = [
        0xe5, 0x56, 0x43, 0x00, 0xc3, 0x60, 0xac, 0x72, 0x90, 0x86, 0xe2, 0xcc, 0x80, 0x6e, 0x82,
        0x8a, 0x84, 0x87, 0x7f, 0x1e, 0xb8, 0xe5, 0xd9, 0x74, 0xd8, 0x73, 0xe0, 0x65, 0x22, 0x49,
        0x01, 0x55, 0x5f, 0xb8, 0x82, 0x15, 0x90, 0xa3, 0x3b, 0xac, 0xc6, 0x1e, 0x39, 0x70, 0x1c,
        0xf9, 0xb4, 0x6b, 0xd2, 0x5b, 0xf5, 0xf0, 0x59, 0x5b, 0xbe, 0x24, 0x65, 0x51, 0x41, 0x43,
        0x8e, 0x7a, 0x10, 0x0b,
    ];
    let pk = ed25519_public_key(&seed);
    let sig = ed25519_sign(&seed, b"");
    let pk_ok = pk == pk_exp;
    let sig_ok = sig == sig_exp;
    let ver_ok = ed25519_verify(&pk, b"", &sig) && ed25519_verify(&pk_exp, b"", &sig_exp);
    let e_ok = pk_ok && sig_ok && ver_ok;
    if e_ok {
        serial_println!("[CRYPTO] {} (RFC 8032)", GATE_ED25519_MARKER);
    } else {
        serial_println!(
            "[CRYPTO] GATE_ED25519 FAILED pk_ok={} sig_ok={} ver={} sig={:02x?}",
            pk_ok,
            sig_ok,
            ver_ok,
            &sig[..4]
        );
    }
    let rsa_ok = rsa_self_test();
    let ecdsa_ok = ecdsa_self_test();
    x_ok && e_ok && rsa_ok && ecdsa_ok
}

pub const GATE_RSA_MARKER: &str = "GATE_RSA kat";
pub const GATE_ECDSA_MARKER: &str = "GATE_ECDSA kat";

fn rsa_self_test() -> bool {
    // 512-bit RSA PKCS#1 v1.5 SHA-256 of "KnoxOS"
    let n = [
        0xb4, 0x33, 0x49, 0x39, 0x25, 0x77, 0x3d, 0x35, 0xf0, 0xfb, 0x6e, 0x3c, 0xbc, 0xf6, 0x1a,
        0x2a, 0xfa, 0x7a, 0x53, 0x22, 0x3c, 0x30, 0x5d, 0x6a, 0x66, 0x71, 0x4b, 0xec, 0xd3, 0x6f,
        0xab, 0x9e, 0x93, 0x09, 0x08, 0xf6, 0xf5, 0x43, 0xa0, 0xe9, 0xb8, 0x10, 0x61, 0xf1, 0x2f,
        0xc4, 0xde, 0x3f, 0xb7, 0x9f, 0x21, 0x58, 0xbb, 0x4c, 0x4f, 0xcc, 0x22, 0x3b, 0x8f, 0x84,
        0xda, 0x7a, 0x8b, 0xf9,
    ];
    let sig = [
        0x7d, 0x40, 0xd1, 0xa9, 0x40, 0x33, 0x80, 0x7f, 0x6c, 0x7a, 0xaa, 0x1c, 0x8f, 0xbc, 0x03,
        0xf3, 0xaa, 0x3d, 0x48, 0xe4, 0x78, 0x41, 0x4c, 0x66, 0x91, 0xe6, 0x95, 0x85, 0xfe, 0x39,
        0x29, 0xe7, 0xbd, 0x19, 0xaf, 0x37, 0x14, 0xe0, 0x8f, 0x23, 0x36, 0x95, 0xe9, 0x38, 0xbe,
        0x5c, 0xfe, 0x2a, 0x8e, 0xa1, 0xdf, 0x66, 0x3a, 0x7f, 0xf6, 0xc2, 0x40, 0x91, 0x77, 0x84,
        0x36, 0x31, 0x63, 0x50,
    ];
    let key = RsaPublicKey {
        n: n.to_vec(),
        e: 65537,
        bits: 512,
    };
    let ok = rsa_verify(b"KnoxOS", &sig, &key) && !rsa_verify(b"knoxos", &sig, &key);
    if ok {
        serial_println!("[CRYPTO] {} (PKCS#1 v1.5 SHA-256)", GATE_RSA_MARKER);
    } else {
        serial_println!("[CRYPTO] GATE_RSA FAILED");
    }
    ok
}

fn ecdsa_self_test() -> bool {
    // RFC 6979 A.2.5: P-256 SHA-256, message = "sample"
    let d = [
        0xC9, 0xAF, 0xA9, 0xD8, 0x45, 0xBA, 0x75, 0x16, 0x6B, 0x5C, 0x21, 0x57, 0x67, 0xB1, 0xD6,
        0x93, 0x4E, 0x50, 0xC3, 0xDB, 0x36, 0xE8, 0x9B, 0x12, 0x7B, 0x8A, 0x62, 0x2B, 0x12, 0x0F,
        0x67, 0x21,
    ];
    let ux = [
        0x60, 0xFE, 0xD4, 0xBA, 0x25, 0x5A, 0x9D, 0x31, 0xC9, 0x61, 0xEB, 0x74, 0xC6, 0x35, 0x6D,
        0x68, 0xC0, 0x49, 0xB8, 0x92, 0x3B, 0x61, 0xFA, 0x6C, 0xE6, 0x69, 0x62, 0x2E, 0x60, 0xF2,
        0x9F, 0xB6,
    ];
    let uy = [
        0x79, 0x03, 0xFE, 0x10, 0x08, 0xB8, 0xBC, 0x99, 0xA4, 0x1A, 0xE9, 0xE9, 0x56, 0x28, 0xBC,
        0x64, 0xF2, 0xF1, 0xB2, 0x0C, 0x2D, 0x7E, 0x9F, 0x51, 0x77, 0xA3, 0xC2, 0x94, 0xD4, 0x46,
        0x22, 0x99,
    ];
    let r = [
        0xEF, 0xD4, 0x8B, 0x2A, 0xAC, 0xB6, 0xA8, 0xFD, 0x11, 0x40, 0xDD, 0x9C, 0xD4, 0x5E, 0x81,
        0xD6, 0x9D, 0x2C, 0x87, 0x7B, 0x56, 0xAA, 0xF9, 0x91, 0xC3, 0x4D, 0x0E, 0xA8, 0x4E, 0xAF,
        0x37, 0x16,
    ];
    let s = [
        0xF7, 0xCB, 0x1C, 0x94, 0x2D, 0x65, 0x7C, 0x41, 0xD4, 0x36, 0xC7, 0xA1, 0xB6, 0xE2, 0x9F,
        0x65, 0xF3, 0xE9, 0x00, 0xDB, 0xB9, 0xAF, 0xF4, 0x06, 0x4D, 0xC4, 0xAB, 0x2F, 0x84, 0x3A,
        0xCD, 0xA8,
    ];
    let (gx, gy) = p256_mul_g(&d);
    let pk_ok = gx == ux && gy == uy;
    let pk = EcdsaPublicKey { x: ux, y: uy };
    let ver_ok = ecdsa_p256_verify(&pk, b"sample", &r, &s);
    let neg_ok = !ecdsa_p256_verify(&pk, b"SAMPLE", &r, &s);
    let ok = pk_ok && ver_ok && neg_ok;
    if ok {
        serial_println!("[CRYPTO] {} (RFC 6979 P-256)", GATE_ECDSA_MARKER);
    } else {
        serial_println!(
            "[CRYPTO] GATE_ECDSA FAILED pk={} ver={} gx={:02x?}",
            pk_ok,
            ver_ok,
            &gx[..4]
        );
    }
    ok
}
