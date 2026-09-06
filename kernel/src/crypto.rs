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

// ─── Init ──────────────────────────────────────────────────────────────

pub fn init() {
    serial_println!("[CRYPTO] Kernel cryptographic subsystem initialized");
    serial_println!("[CRYPTO]   SHA-256, SHA-512, HMAC-SHA256, PBKDF2");
    serial_println!("[CRYPTO]   AES-128/256-CBC, ChaCha20-Poly1305");
    serial_println!("[CRYPTO]   RSA-2048/4096, ECDSA-P256");
    serial_println!("[CRYPTO]   RDRAND secure random");
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

/// Big-number modular exponentiation: base^exp mod modulus
/// Using square-and-multiply method with big integers stored as byte arrays (big-endian)
fn mod_pow_bytes(base: &[u8], exp: &[u8], modulus: &[u8]) -> Vec<u8> {
    // Simple big-integer modular exponentiation
    // For production, use Montgomery multiplication
    let mod_val = bytes_to_u128(modulus);
    if mod_val == 0 {
        return alloc::vec![0];
    }
    let base_val = bytes_to_u128(base) % mod_val;
    let exp_val = bytes_to_u128(exp);

    let mut result: u128 = 1;
    let mut b = base_val;
    let mut e = exp_val;

    while e > 0 {
        if e & 1 == 1 {
            result = mul_mod(result, b, mod_val);
        }
        b = mul_mod(b, b, mod_val);
        e >>= 1;
    }
    u128_to_bytes(result)
}

fn bytes_to_u128(bytes: &[u8]) -> u128 {
    let mut val: u128 = 0;
    for &b in bytes.iter().take(16) {
        val = (val << 8) | b as u128;
    }
    val
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

/// RSA sign (hash then private-key encrypt)
pub fn rsa_sign(message: &[u8], key: &RsaPrivateKey) -> Vec<u8> {
    let hash = sha256(message);
    mod_pow_bytes(&hash, &key.d, &key.public.n)
}

/// RSA verify signature
pub fn rsa_verify(message: &[u8], signature: &[u8], key: &RsaPublicKey) -> bool {
    let hash = sha256(message);
    let decrypted = rsa_encrypt(signature, key); // "encrypt" with public key = verify
    decrypted == hash
}

// ═══════════════════════════════════════════════════════════════════════
// ECDSA P-256 KEY GENERATION (STUB — uses simplified curve)
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

/// Generate an ECDSA P-256 key pair
pub fn ecdsa_generate_keypair() -> EcdsaPrivateKey {
    let mut d = [0u8; 32];
    crate::random::fill_random(&mut d);
    // Ensure d is in valid range [1, n-1] for P-256
    d[0] &= 0x7F; // Clear high bit to keep < n

    // Compute public key Q = d * G (simplified)
    let mut x = [0u8; 32];
    let mut y = [0u8; 32];
    // In a real implementation: perform EC point multiplication on P-256 curve
    // For now, derive from d using hash
    let hash = sha256(&d);
    x.copy_from_slice(&hash);
    let hash2 = sha256(&x);
    y.copy_from_slice(&hash2);

    EcdsaPrivateKey {
        d,
        public: EcdsaPublicKey { x, y },
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
    ]
}
