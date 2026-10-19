use super::aes::{aes128_encrypt_block, aes128_key_expand};

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
