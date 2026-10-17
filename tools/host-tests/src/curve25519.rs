//! Host copy of kernel X25519 field math (51-bit limbs) for RFC 7748 KATs.

type Fe = [u64; 5];
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
    h[0] = h[0].wrapping_add((c as u64).wrapping_mul(19));
    fe_carry(h)
}

fn fe_invert(a: &Fe) -> Fe {
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
    let d = fe_sub(&fe_zero(), &a);
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
    let mut q = *p;
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
    let mut exp_r = fe_one();
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
    ge_decode(&[
        0x58, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
        0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
        0x66, 0x66,
    ])
    .unwrap_or_else(ge_zero)
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

const L: [u64; 4] = [
    0x5812_631A_5CF5_D3ED,
    0x14DE_F9DE_A2F7_9CD6,
    0x0000_0000_0000_0000,
    0x1000_0000_0000_0000,
];

fn sc_mod_l(mut n: [u64; 8]) -> [u8; 32] {
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
    let h = crate::sha512::sha512(seed);
    let mut a = [0u8; 32];
    a.copy_from_slice(&h[..32]);
    a[0] &= 248;
    a[31] &= 63;
    a[31] |= 64;
    let mut prefix = [0u8; 32];
    prefix.copy_from_slice(&h[32..]);
    let pk = ge_encode(&ge_scalarmult(&a, &ge_base()));
    (a, prefix, pk)
}

fn ed25519_sign(seed: &[u8; 32], msg: &[u8]) -> [u8; 64] {
    let (a, prefix, pk) = ed25519_expand(seed);
    let mut r_in = Vec::with_capacity(32 + msg.len());
    r_in.extend_from_slice(&prefix);
    r_in.extend_from_slice(msg);
    let r = sc_from_bytes64(&crate::sha512::sha512(&r_in));
    let r_enc = ge_encode(&ge_scalarmult(&r, &ge_base()));
    let mut k_in = Vec::with_capacity(64 + msg.len());
    k_in.extend_from_slice(&r_enc);
    k_in.extend_from_slice(&pk);
    k_in.extend_from_slice(msg);
    let k = sc_from_bytes64(&crate::sha512::sha512(&k_in));
    let s = sc_muladd(&k, &a, &r);
    let mut sig = [0u8; 64];
    sig[..32].copy_from_slice(&r_enc);
    sig[32..].copy_from_slice(&s);
    sig
}

fn ed25519_verify(pk: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
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
    let mut k_in = Vec::with_capacity(64 + msg.len());
    k_in.extend_from_slice(&r_bytes);
    k_in.extend_from_slice(pk);
    k_in.extend_from_slice(msg);
    let k = sc_from_bytes64(&crate::sha512::sha512(&k_in));
    let sb = ge_scalarmult(&s_bytes, &ge_base());
    let neg_a = Ge {
        x: fe_sub(&fe_zero(), &a.x),
        y: a.y,
        z: a.z,
        t: fe_sub(&fe_zero(), &a.t),
    };
    let ka = ge_scalarmult(&k, &neg_a);
    let sum = ge_add(&sb, &ka);
    ge_encode(&sum) == r_bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_nine() {
        let mut nine = [0u8; 32];
        nine[0] = 9;
        assert_eq!(fe_to_bytes(fe_from_bytes(&nine)), nine);
        let two = fe_from_bytes(&[
            2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ]);
        let three = fe_from_bytes(&[
            3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ]);
        let six = fe_mul(&two, &three);
        assert_eq!(fe_to_bytes(six)[0], 6);
        let inv = fe_invert(&two);
        assert_eq!(fe_to_bytes(fe_mul(&two, &inv))[0], 1);
    }

    #[test]
    fn rfc7748_alice() {
        let alice_sk: [u8; 32] = [
            0x77, 0x07, 0x6d, 0x0a, 0x73, 0x18, 0xa5, 0x7d, 0x3c, 0x16, 0xc1, 0x72, 0x51, 0xb2,
            0x66, 0x45, 0xdf, 0x4c, 0x2f, 0x87, 0xeb, 0xc0, 0x99, 0x2a, 0xb1, 0x77, 0xfb, 0xa5,
            0x1d, 0xb9, 0x2c, 0x2a,
        ];
        let alice_pk_exp: [u8; 32] = [
            0x85, 0x20, 0xf0, 0x09, 0x89, 0x30, 0xa7, 0x54, 0x74, 0x8b, 0x7d, 0xdc, 0xb4, 0x3e,
            0xf7, 0x5a, 0x0d, 0xbf, 0x3a, 0x0d, 0x26, 0x38, 0x1a, 0xf4, 0xeb, 0xa4, 0xa9, 0x8e,
            0xaa, 0x9b, 0x4e, 0x6a,
        ];
        let mut nine = [0u8; 32];
        nine[0] = 9;
        let alice_pk = x25519(&alice_sk, &nine);
        assert_eq!(alice_pk, alice_pk_exp, "got {:02x?}", alice_pk);
    }

    #[test]
    fn ed25519_basepoint_roundtrip() {
        let enc = [
            0x58, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
            0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
            0x66, 0x66, 0x66, 0x66,
        ];
        let p = super::ge_decode(&enc).expect("decode base");
        assert_eq!(super::ge_encode(&p), enc);
        let mut one = [0u8; 32];
        one[0] = 1;
        assert_eq!(super::ge_encode(&super::ge_scalarmult(&one, &p)), enc, "1B");
        let mut two = [0u8; 32];
        two[0] = 2;
        let b2 = super::ge_encode(&super::ge_scalarmult(&two, &p));
        assert_eq!(
            b2,
            [
                0xc9, 0xa3, 0xf8, 0x6a, 0xae, 0x46, 0x5f, 0x0e, 0x56, 0x51, 0x38, 0x64, 0x51, 0x0f,
                0x39, 0x97, 0x56, 0x1f, 0xa2, 0xc9, 0xe8, 0x5e, 0xa2, 0x1d, 0xc2, 0x29, 0x23, 0x09,
                0xf3, 0xcd, 0x60, 0x22,
            ],
            "2B got {:02x?}",
            b2
        );
        let mut three = [0u8; 32];
        three[0] = 3;
        let b3 = super::ge_encode(&super::ge_scalarmult(&three, &p));
        assert_eq!(
            b3,
            [
                0xd4, 0xb4, 0xf5, 0x78, 0x48, 0x68, 0xc3, 0x02, 0x04, 0x03, 0x24, 0x67, 0x17, 0xec,
                0x16, 0x9f, 0xf7, 0x9e, 0x26, 0x60, 0x8e, 0xa1, 0x26, 0xa1, 0xab, 0x69, 0xee, 0x77,
                0xd1, 0xb1, 0x67, 0x12,
            ],
            "3B got {:02x?}",
            b3
        );
    }

    #[test]
    fn ed25519_rfc8032_test1_public_key() {
        // SHA-512(seed) clamped (RFC 8032 TEST 1), then [a]B.
        let a = [
            0x30, 0x7c, 0x83, 0x86, 0x4f, 0x28, 0x33, 0xcb, 0x42, 0x7a, 0x2e, 0xf1, 0xc0, 0x0a,
            0x01, 0x3c, 0xfd, 0xff, 0x27, 0x68, 0xd9, 0x80, 0xc0, 0xa3, 0xa5, 0x20, 0xf0, 0x06,
            0x90, 0x4d, 0xe9, 0x4f,
        ];
        let pk_exp = [
            0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64,
            0x07, 0x3a, 0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68,
            0xf7, 0x07, 0x51, 0x1a,
        ];
        let enc = [
            0x58, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
            0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
            0x66, 0x66, 0x66, 0x66,
        ];
        let b = super::ge_decode(&enc).expect("base");
        let pk = super::ge_encode(&super::ge_scalarmult(&a, &b));
        assert_eq!(pk, pk_exp, "got {:02x?}", pk);
    }

    #[test]
    fn sha512_nist() {
        let empty = crate::sha512::sha512(b"");
        assert_eq!(
            empty,
            [
                0xcf, 0x83, 0xe1, 0x35, 0x7e, 0xef, 0xb8, 0xbd, 0xf1, 0x54, 0x28, 0x50, 0xd6, 0x6d,
                0x80, 0x07, 0xd6, 0x20, 0xe4, 0x05, 0x0b, 0x57, 0x15, 0xdc, 0x83, 0xf4, 0xa9, 0x21,
                0xd3, 0x6c, 0xe9, 0xce, 0x47, 0xd0, 0xd1, 0x3c, 0x5d, 0x85, 0xf2, 0xb0, 0xff, 0x83,
                0x18, 0xd2, 0x87, 0x7e, 0xec, 0x2f, 0x63, 0xb9, 0x31, 0xbd, 0x47, 0x41, 0x7a, 0x81,
                0xa5, 0x38, 0x32, 0x7a, 0xf9, 0x27, 0xda, 0x3e,
            ]
        );
        let abc = crate::sha512::sha512(b"abc");
        assert_eq!(&abc[..8], &[0xdd, 0xaf, 0x35, 0xa1, 0x93, 0x61, 0x7a, 0xba]);
    }

    #[test]
    fn ed25519_rfc8032_test1_sign() {
        let seed = [
            0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec,
            0x2c, 0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03,
            0x1c, 0xae, 0x7f, 0x60,
        ];
        let pk_exp = [
            0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64,
            0x07, 0x3a, 0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68,
            0xf7, 0x07, 0x51, 0x1a,
        ];
        let sig_exp = [
            0xe5, 0x56, 0x43, 0x00, 0xc3, 0x60, 0xac, 0x72, 0x90, 0x86, 0xe2, 0xcc, 0x80, 0x6e,
            0x82, 0x8a, 0x84, 0x87, 0x7f, 0x1e, 0xb8, 0xe5, 0xd9, 0x74, 0xd8, 0x73, 0xe0, 0x65,
            0x22, 0x49, 0x01, 0x55, 0x5f, 0xb8, 0x82, 0x15, 0x90, 0xa3, 0x3b, 0xac, 0xc6, 0x1e,
            0x39, 0x70, 0x1c, 0xf9, 0xb4, 0x6b, 0xd2, 0x5b, 0xf5, 0xf0, 0x59, 0x5b, 0xbe, 0x24,
            0x65, 0x51, 0x41, 0x43, 0x8e, 0x7a, 0x10, 0x0b,
        ];
        let pk = super::ed25519_expand(&seed).2;
        assert_eq!(pk, pk_exp, "pk {:02x?}", pk);
        let sig = super::ed25519_sign(&seed, b"");
        assert_eq!(sig, sig_exp, "sig {:02x?}", sig);
        assert!(super::ed25519_verify(&pk, b"", &sig));
        assert!(super::ed25519_verify(&pk_exp, b"", &sig_exp));
    }
}
