use alloc::vec::Vec;

use super::sha512::sha512;

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
