use alloc::vec::Vec;

use super::sha256::{Sha256, sha256};

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
