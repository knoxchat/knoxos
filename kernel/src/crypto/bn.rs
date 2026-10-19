use alloc::vec::Vec;

/// Little-endian u64 limbs. Limb 0 is the least-significant 64 bits.
pub(super) fn bn_normalize(mut a: Vec<u64>) -> Vec<u64> {
    while a.len() > 1 && a.last() == Some(&0) {
        a.pop();
    }
    if a.is_empty() {
        a.push(0);
    }
    a
}

pub(super) fn bn_from_be(bytes: &[u8]) -> Vec<u64> {
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

pub(super) fn bn_to_be(n: &[u64], out_len: usize) -> Vec<u8> {
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

pub(super) fn bn_cmp(a: &[u64], b: &[u64]) -> core::cmp::Ordering {
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

pub(super) fn bn_bitlen(a: &[u64]) -> usize {
    match a.iter().rposition(|&x| x != 0) {
        None => 0,
        Some(i) => i * 64 + (64 - a[i].leading_zeros() as usize),
    }
}

pub(super) fn bn_shl(a: &[u64], bits: usize) -> Vec<u64> {
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

pub(super) fn bn_add_u64(a: &[u64], x: u64) -> Vec<u64> {
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

pub(super) fn bn_sub(a: &[u64], b: &[u64]) -> Vec<u64> {
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

pub(super) fn bn_mul(a: &[u64], b: &[u64]) -> Vec<u64> {
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

pub(super) fn bn_mod(a: &[u64], m: &[u64]) -> Vec<u64> {
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

pub(super) fn bn_modexp(base: &[u64], exp: &[u64], modulus: &[u64]) -> Vec<u64> {
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

pub(super) fn bn_add_limbs(a: &[u64], b: &[u64]) -> Vec<u64> {
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
