use alloc::vec::Vec;

use super::bn::{
    bn_add_limbs, bn_bitlen, bn_cmp, bn_from_be, bn_mod, bn_modexp, bn_mul, bn_sub, bn_to_be,
};
use super::sha256::sha256;

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

pub(super) fn p256_mul_g(d: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
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
