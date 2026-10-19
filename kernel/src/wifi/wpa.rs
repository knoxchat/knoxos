use alloc::vec::Vec;

/// HMAC-SHA1 for WPA2 key derivation
fn hmac_sha1(key: &[u8], data: &[u8]) -> [u8; 20] {
    let block_size = 64;
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5Cu8; 64];

    // If key > block_size, hash it first
    let k = if key.len() > block_size {
        let h = sha1_digest(key);
        h.to_vec()
    } else {
        key.to_vec()
    };

    for (i, &byte) in k.iter().enumerate() {
        ipad[i] ^= byte;
        opad[i] ^= byte;
    }

    // Inner hash: SHA1(ipad || data)
    let mut inner = Vec::with_capacity(block_size + data.len());
    inner.extend_from_slice(&ipad);
    inner.extend_from_slice(data);
    let inner_hash = sha1_digest(&inner);

    // Outer hash: SHA1(opad || inner_hash)
    let mut outer = Vec::with_capacity(block_size + 20);
    outer.extend_from_slice(&opad);
    outer.extend_from_slice(&inner_hash);
    sha1_digest(&outer)
}

/// SHA-1 hash (used only for WPA2 PBKDF2 key derivation)
fn sha1_digest(data: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    // Pre-processing: pad message
    let ml = data.len() as u64 * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&ml.to_be_bytes());

    // Process each 512-bit block
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut result = [0u8; 20];
    result[0..4].copy_from_slice(&h0.to_be_bytes());
    result[4..8].copy_from_slice(&h1.to_be_bytes());
    result[8..12].copy_from_slice(&h2.to_be_bytes());
    result[12..16].copy_from_slice(&h3.to_be_bytes());
    result[16..20].copy_from_slice(&h4.to_be_bytes());
    result
}

/// PBKDF2-SHA1 as specified by WPA2 (4096 iterations, 256-bit output)
pub(super) fn wpa2_derive_pmk(passphrase: &[u8], ssid: &[u8], output: &mut [u8; 32]) {
    let iterations = 4096u32;

    // Block 1 (bytes 0-19)
    let mut u_prev = {
        let mut salt_block = Vec::with_capacity(ssid.len() + 4);
        salt_block.extend_from_slice(ssid);
        salt_block.extend_from_slice(&1u32.to_be_bytes());
        hmac_sha1(passphrase, &salt_block)
    };
    let mut result1 = u_prev;

    for _ in 1..iterations {
        u_prev = hmac_sha1(passphrase, &u_prev);
        for j in 0..20 {
            result1[j] ^= u_prev[j];
        }
    }

    // Block 2 (bytes 20-31, only need 12 bytes)
    let mut u_prev2 = {
        let mut salt_block = Vec::with_capacity(ssid.len() + 4);
        salt_block.extend_from_slice(ssid);
        salt_block.extend_from_slice(&2u32.to_be_bytes());
        hmac_sha1(passphrase, &salt_block)
    };
    let mut result2 = u_prev2;

    for _ in 1..iterations {
        u_prev2 = hmac_sha1(passphrase, &u_prev2);
        for j in 0..20 {
            result2[j] ^= u_prev2[j];
        }
    }

    output[..20].copy_from_slice(&result1);
    output[20..32].copy_from_slice(&result2[..12]);
}

/// PRF-384 for PTK derivation (IEEE 802.11i)
/// PTK = PRF-384(PMK, label, min(AA,SA)||max(AA,SA)||min(ANonce,SNonce)||max(ANonce,SNonce))
pub(super) fn wpa2_derive_ptk(
    pmk: &[u8; 32],
    anonce: &[u8; 32],
    snonce: &[u8; 32],
    aa: &[u8; 6], // Authenticator address (AP MAC)
    sa: &[u8; 6], // Supplicant address (STA MAC)
) -> [u8; 48] {
    let label = b"Pairwise key expansion";

    // Build the data: min(AA,SA) || max(AA,SA) || min(ANonce,SNonce) || max(ANonce,SNonce)
    let mut data = Vec::with_capacity(76);
    if aa < sa {
        data.extend_from_slice(aa);
        data.extend_from_slice(sa);
    } else {
        data.extend_from_slice(sa);
        data.extend_from_slice(aa);
    }
    if anonce < snonce {
        data.extend_from_slice(anonce);
        data.extend_from_slice(snonce);
    } else {
        data.extend_from_slice(snonce);
        data.extend_from_slice(anonce);
    }

    // PRF-384: generate 48 bytes (3 × 16: KCK + KEK + TK)
    let mut ptk = [0u8; 48];
    let num_blocks = 48_usize.div_ceil(20); // ceil(48/20) = 3

    for i in 0..num_blocks {
        let mut prf_input = Vec::with_capacity(label.len() + 1 + data.len() + 1);
        prf_input.extend_from_slice(label);
        prf_input.push(0); // Zero byte separator
        prf_input.extend_from_slice(&data);
        prf_input.push(i as u8);

        let block = hmac_sha1(pmk, &prf_input);
        let start = i * 20;
        let end = (start + 20).min(48);
        ptk[start..end].copy_from_slice(&block[..end - start]);
    }

    ptk
}

/// Build an 802.11 EAPOL-Key frame for the 4-way handshake
pub fn build_eapol_key(
    key_type: u8,     // 1 = pairwise, 2 = group
    key_info: u16,    // Key descriptor info bits
    nonce: &[u8; 32], // SNonce or ANonce
    mic: &[u8; 16],   // MIC (computed with KCK)
    key_data: &[u8],  // Optional encrypted key data
) -> Vec<u8> {
    let body_len = 95 + key_data.len();
    let mut frame = Vec::with_capacity(4 + body_len);

    // IEEE 802.1X header
    frame.push(0x02); // Version: 802.1X-2004
    frame.push(0x03); // Type: EAPOL-Key
    frame.extend_from_slice(&(body_len as u16).to_be_bytes());

    // Key descriptor
    frame.push(0x02); // Descriptor type: RSN
    frame.extend_from_slice(&key_info.to_be_bytes());
    frame.extend_from_slice(&16u16.to_be_bytes()); // Key length (128-bit TK)
    frame.extend_from_slice(&[0u8; 8]); // Replay counter
    frame.extend_from_slice(nonce); // Key nonce
    frame.extend_from_slice(&[0u8; 16]); // Key IV
    frame.extend_from_slice(&[0u8; 8]); // Key RSC
    frame.extend_from_slice(&[0u8; 8]); // Reserved
    frame.extend_from_slice(mic); // Key MIC
    frame.extend_from_slice(&(key_data.len() as u16).to_be_bytes()); // Key data length
    frame.extend_from_slice(key_data);

    frame
}
