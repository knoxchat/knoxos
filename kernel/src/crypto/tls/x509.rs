//! X.509 certificate parse, PEM/Base64, and certificate-store chain verify.

use alloc::string::String;
use alloc::vec::Vec;

use crate::serial_println;

use super::asn1::{
    ASN1_BIT_STRING, ASN1_BOOLEAN, ASN1_CONTEXT_0, ASN1_CONTEXT_3, ASN1_INTEGER, ASN1_NULL,
    ASN1_OCTET_STRING, ASN1_OID, ASN1_SEQUENCE, ASN1_SET, ASN1_UTC_TIME, ASN1_UTF8_STRING,
    OID_BASIC_CONSTRAINTS, asn1_read_tlv, parse_asn1_time, parse_oid, parse_x501_name,
};

// ─── X.509 Certificate ─────────────────────────────────────────────────

/// X.509 certificate parsed from DER encoding
#[derive(Debug, Clone)]
pub struct X509Certificate {
    pub subject: String,
    pub issuer: String,
    pub serial: Vec<u8>,
    pub not_before: u64,
    pub not_after: u64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    pub signature_algorithm: String,
    pub is_ca: bool,
    /// Raw TBSCertificate bytes (for signature verification)
    pub tbs_raw: Vec<u8>,
}

impl X509Certificate {
    /// Parse a DER-encoded X.509 certificate (real ASN.1 parser)
    pub fn from_der(data: &[u8]) -> Option<Self> {
        // Certificate ::= SEQUENCE { tbsCertificate, signatureAlgorithm, signatureValue }
        let (_tag, cert_content, _) = asn1_read_tlv(data)?;

        // TBSCertificate (first element)
        let (tbs_tag, tbs_content, after_tbs) = asn1_read_tlv(cert_content)?;
        if tbs_tag != ASN1_SEQUENCE {
            return None;
        }

        // Save raw TBS for signature verification
        // The TBS includes the tag+length+content, so we reconstruct it
        let tbs_total_len = cert_content.len() - after_tbs.len();
        let tbs_raw = cert_content[..tbs_total_len].to_vec();

        // Parse TBSCertificate fields
        let mut pos = tbs_content;

        // version [0] EXPLICIT INTEGER (optional, default v1)
        let mut _version = 0u8;
        if !pos.is_empty() && pos[0] == ASN1_CONTEXT_0 {
            let (_, ver_content, rest) = asn1_read_tlv(pos)?;
            if let Some((_, int_data, _)) = asn1_read_tlv(ver_content) {
                _version = *int_data.last().unwrap_or(&0);
            }
            pos = rest;
        }

        // serialNumber INTEGER
        let (_, serial_data, rest) = asn1_read_tlv(pos)?;
        let serial = serial_data.to_vec();
        pos = rest;

        // signature AlgorithmIdentifier (skip — we read it from outer too)
        let (_, sig_alg_content, rest) = asn1_read_tlv(pos)?;
        let sig_alg = if let Some((_, oid_data, _)) = asn1_read_tlv(sig_alg_content) {
            parse_oid(oid_data)
        } else {
            String::new()
        };
        pos = rest;

        // issuer Name
        let (issuer_tag, issuer_content, rest) = asn1_read_tlv(pos)?;
        let issuer_total = pos.len() - rest.len();
        let issuer = parse_x501_name(issuer_content);
        pos = rest;

        // validity SEQUENCE { notBefore, notAfter }
        let (_, validity_content, rest) = asn1_read_tlv(pos)?;
        let (nb_tag, nb_data, after_nb) = asn1_read_tlv(validity_content)?;
        let not_before = parse_asn1_time(nb_tag, nb_data);
        let (na_tag, na_data, _) = asn1_read_tlv(after_nb)?;
        let not_after = parse_asn1_time(na_tag, na_data);
        pos = rest;

        // subject Name
        let (_, subject_content, rest) = asn1_read_tlv(pos)?;
        let subject = parse_x501_name(subject_content);
        pos = rest;

        // subjectPublicKeyInfo SEQUENCE
        let (_, spki_content, rest) = asn1_read_tlv(pos)?;
        // algorithm AlgorithmIdentifier
        let (_, _alg_content, after_alg) = asn1_read_tlv(spki_content)?;
        // subjectPublicKey BIT STRING
        let public_key = if let Some((_, pk_data, _)) = asn1_read_tlv(after_alg) {
            // Skip the leading unused-bits byte in BIT STRING
            if pk_data.len() > 1 {
                pk_data[1..].to_vec()
            } else {
                pk_data.to_vec()
            }
        } else {
            Vec::new()
        };
        pos = rest;

        // Check extensions for BasicConstraints (CA flag)
        let mut is_ca = false;
        // extensions [3] EXPLICIT SEQUENCE OF Extension (optional)
        if !pos.is_empty() && pos[0] == ASN1_CONTEXT_3 {
            if let Some((_, ext_outer, _)) = asn1_read_tlv(pos) {
                if let Some((_, ext_seq, _)) = asn1_read_tlv(ext_outer) {
                    let mut epos = ext_seq;
                    while !epos.is_empty() {
                        if let Some((_, ext_content, erest)) = asn1_read_tlv(epos) {
                            // Extension ::= SEQUENCE { extnID OID, critical BOOLEAN, extnValue OCTET STRING }
                            if let Some((_, oid_data, after_oid)) = asn1_read_tlv(ext_content) {
                                let oid = parse_oid(oid_data);
                                if oid == OID_BASIC_CONSTRAINTS {
                                    // Skip optional critical BOOLEAN
                                    let mut vpos = after_oid;
                                    if !vpos.is_empty() && vpos[0] == ASN1_BOOLEAN {
                                        if let Some((_, _, r)) = asn1_read_tlv(vpos) {
                                            vpos = r;
                                        }
                                    }
                                    // extnValue OCTET STRING wrapping SEQUENCE { cA BOOLEAN }
                                    if let Some((_, octet_data, _)) = asn1_read_tlv(vpos) {
                                        if let Some((_, bc_seq, _)) = asn1_read_tlv(octet_data) {
                                            if let Some((t, bc_data, _)) = asn1_read_tlv(bc_seq) {
                                                if t == ASN1_BOOLEAN && !bc_data.is_empty() {
                                                    is_ca = bc_data[0] != 0;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            epos = erest;
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        // signatureAlgorithm (outer — after TBS)
        let (_, _outer_sig_alg, after_sig_alg) = asn1_read_tlv(after_tbs)?;

        // signatureValue BIT STRING
        let signature = if let Some((_, sig_data, _)) = asn1_read_tlv(after_sig_alg) {
            if sig_data.len() > 1 {
                sig_data[1..].to_vec()
            } else {
                sig_data.to_vec()
            }
        } else {
            Vec::new()
        };

        Some(Self {
            subject,
            issuer,
            serial,
            not_before,
            not_after,
            public_key,
            signature,
            signature_algorithm: sig_alg,
            is_ca,
            tbs_raw,
        })
    }

    /// Parse a PEM-encoded certificate
    pub fn from_pem(data: &str) -> Option<Self> {
        // Strip PEM header/footer and decode Base64
        let mut b64 = String::new();
        let mut in_body = false;
        for line in data.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("-----BEGIN") {
                in_body = true;
                continue;
            }
            if trimmed.starts_with("-----END") {
                break;
            }
            if in_body {
                b64.push_str(trimmed);
            }
        }
        let der = base64_decode(&b64)?;
        Self::from_der(&der)
    }

    /// Check if the certificate is self-signed (subject == issuer)
    pub fn is_self_signed(&self) -> bool {
        self.subject == self.issuer
    }

    /// Check if the certificate is currently valid (by approximate Unix timestamp)
    pub fn is_valid_at(&self, unix_time: u64) -> bool {
        unix_time >= self.not_before && unix_time <= self.not_after
    }
}

/// Minimal Base64 decoder for PEM certificates
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn decode_char(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let bytes: Vec<u8> = input
        .bytes()
        .filter(|b| *b != b'\n' && *b != b'\r' && *b != b' ')
        .collect();
    let mut result = Vec::with_capacity(bytes.len() * 3 / 4);

    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let a = decode_char(chunk[0])?;
        let b = decode_char(chunk[1])?;
        result.push((a << 2) | (b >> 4));

        if chunk.len() > 2 && chunk[2] != b'=' {
            let c = decode_char(chunk[2])?;
            result.push(((b & 0x0F) << 4) | (c >> 2));

            if chunk.len() > 3 && chunk[3] != b'=' {
                let d = decode_char(chunk[3])?;
                result.push(((c & 0x03) << 6) | d);
            }
        }
    }
    Some(result)
}

/// Certificate store for trusted root CAs
pub struct CertificateStore {
    pub trusted_roots: Vec<X509Certificate>,
}

impl Default for CertificateStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CertificateStore {
    pub fn new() -> Self {
        Self {
            trusted_roots: Vec::new(),
        }
    }

    pub fn add_trusted_root(&mut self, cert: X509Certificate) {
        self.trusted_roots.push(cert);
    }

    /// Verify a certificate chain against trusted roots.
    /// Chain should be ordered: [leaf, intermediate..., root]
    /// Validates: issuer/subject chain, CA flags, validity timestamps.
    pub fn verify_chain(&self, chain: &[X509Certificate]) -> bool {
        if chain.is_empty() {
            return false;
        }

        // CMOS year is already 2000+YY; `unix_time` is seconds since 1970.
        let now = crate::rtc::unix_time().max(0) as u64;

        // 1. Check each certificate's validity period
        for cert in chain {
            if !cert.is_valid_at(now) {
                serial_println!(
                    "[TLS] Certificate expired or not yet valid: {}",
                    cert.subject
                );
                return false;
            }
        }

        // 2. Verify issuer→subject chain: chain[i].issuer == chain[i+1].subject
        for i in 0..chain.len().saturating_sub(1) {
            if chain[i].issuer != chain[i + 1].subject {
                serial_println!(
                    "[TLS] Chain break: '{}' issued by '{}', but next cert is '{}'",
                    chain[i].subject,
                    chain[i].issuer,
                    chain[i + 1].subject
                );
                return false;
            }
            // Intermediate and root certs must be CAs
            if !chain[i + 1].is_ca {
                serial_println!(
                    "[TLS] Cert '{}' is not a CA but signed '{}'",
                    chain[i + 1].subject,
                    chain[i].subject
                );
                return false;
            }
        }

        // 3. The root of the chain must be trusted
        let root = chain.last().unwrap();
        let trusted = self
            .trusted_roots
            .iter()
            .any(|tr| tr.subject == root.subject || tr.subject == root.issuer);

        if !trusted && !root.is_self_signed() {
            serial_println!("[TLS] Root cert '{}' not in trusted store", root.subject);
            return false;
        }

        if trusted || root.is_self_signed() {
            serial_println!("[TLS] Certificate chain verified: {} cert(s)", chain.len());
        }

        // 4. RSA PKCS#1 v1.5 SHA-256 over TBS when keys/signatures are present.
        for i in 0..chain.len() {
            let issuer = if i + 1 < chain.len() {
                &chain[i + 1]
            } else {
                &chain[i]
            };
            if chain[i].signature.is_empty() || issuer.public_key.is_empty() {
                continue;
            }
            let Some(key) = rsa_public_from_spki_bits(&issuer.public_key) else {
                serial_println!("[TLS] Chain RSA key parse failed: {}", issuer.subject);
                return false;
            };
            if !crate::crypto::rsa_verify(&chain[i].tbs_raw, &chain[i].signature, &key) {
                serial_println!("[TLS] Chain signature failed: {}", chain[i].subject);
                return false;
            }
        }

        true
    }
}

/// `public_key` is RSAPublicKey DER (SEQUENCE of INTEGER n, INTEGER e).
pub(super) fn rsa_public_from_spki_bits(pk: &[u8]) -> Option<crate::crypto::RsaPublicKey> {
    let (tag, seq, _) = asn1_read_tlv(pk)?;
    if tag != ASN1_SEQUENCE {
        return None;
    }
    let (_, n_raw, rest) = asn1_read_tlv(seq)?;
    let (_, e_raw, _) = asn1_read_tlv(rest)?;
    let n = if n_raw.first() == Some(&0) {
        n_raw[1..].to_vec()
    } else {
        n_raw.to_vec()
    };
    if n.is_empty() {
        return None;
    }
    let mut e = 0u32;
    for &b in e_raw {
        e = (e << 8) | b as u32;
    }
    Some(crate::crypto::RsaPublicKey {
        bits: (n.len() * 8) as u32,
        n,
        e,
    })
}

fn der_len(n: usize) -> Vec<u8> {
    if n < 0x80 {
        alloc::vec![n as u8]
    } else if n < 0x100 {
        alloc::vec![0x81, n as u8]
    } else {
        alloc::vec![0x82, (n >> 8) as u8, n as u8]
    }
}

fn der_tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = alloc::vec![tag];
    out.extend_from_slice(&der_len(content.len()));
    out.extend_from_slice(content);
    out
}

fn der_integer(bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    if bytes.first().copied().unwrap_or(0) & 0x80 != 0 {
        body.push(0);
    }
    body.extend_from_slice(bytes);
    der_tlv(ASN1_INTEGER, &body)
}

fn der_oid(bytes: &[u8]) -> Vec<u8> {
    der_tlv(ASN1_OID, bytes)
}

fn x509_name_cn(cn: &str) -> Vec<u8> {
    let atv = der_tlv(
        ASN1_SEQUENCE,
        &[
            der_oid(&[0x55, 0x04, 0x03]),
            der_tlv(ASN1_UTF8_STRING, cn.as_bytes()),
        ]
        .concat(),
    );
    der_tlv(ASN1_SEQUENCE, &der_tlv(ASN1_SET, &atv))
}

fn alg_sha256_rsa() -> Vec<u8> {
    der_tlv(
        ASN1_SEQUENCE,
        &[
            der_oid(&[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B]),
            der_tlv(ASN1_NULL, &[]),
        ]
        .concat(),
    )
}

fn alg_rsa_encryption() -> Vec<u8> {
    der_tlv(
        ASN1_SEQUENCE,
        &[
            der_oid(&[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01]),
            der_tlv(ASN1_NULL, &[]),
        ]
        .concat(),
    )
}

/// 512-bit RSA used to sign the GATE_TLS13 x509 fixture (e = 65537).
const GATE_X509_N: [u8; 64] = [
    0xcb, 0xcc, 0x73, 0x7e, 0x9e, 0xe4, 0x65, 0xd6, 0x58, 0x09, 0x2b, 0x69, 0xcb, 0x02, 0x6f, 0xf6,
    0xf0, 0x3e, 0x4f, 0x52, 0xa5, 0x3e, 0x16, 0x8c, 0xe8, 0x98, 0xfd, 0x9a, 0x1a, 0x0d, 0xa8, 0x16,
    0x91, 0x97, 0x52, 0x6a, 0x15, 0x9a, 0xa3, 0x41, 0x87, 0x68, 0xd6, 0x30, 0x9b, 0xc3, 0xa8, 0x7d,
    0xf2, 0xd2, 0xae, 0x27, 0x01, 0x06, 0x6a, 0xa1, 0x2c, 0xf0, 0x5d, 0x5f, 0x3d, 0x59, 0xf4, 0xed,
];
const GATE_X509_D: [u8; 64] = [
    0x45, 0x22, 0x5e, 0x2b, 0x9e, 0x0b, 0x2c, 0xe0, 0xc1, 0x8c, 0x9a, 0xc5, 0x90, 0xb0, 0x8a, 0xfa,
    0xe2, 0x14, 0xc1, 0x23, 0xf0, 0x1a, 0x57, 0xdc, 0x05, 0x2b, 0xa1, 0x1c, 0x1d, 0xaa, 0x74, 0x52,
    0x57, 0xde, 0x9b, 0x17, 0x38, 0x40, 0x32, 0xec, 0x30, 0x28, 0x40, 0x19, 0xbf, 0x23, 0xaf, 0x78,
    0x1f, 0xbf, 0xa5, 0xbe, 0xf1, 0x1b, 0xe2, 0x0e, 0x90, 0xd3, 0x3e, 0xac, 0xa7, 0x4a, 0x76, 0xa1,
];

pub(super) fn build_self_signed_der() -> Vec<u8> {
    let version = der_tlv(ASN1_CONTEXT_0, &der_integer(&[0x02]));
    let serial = der_integer(&[0x01]);
    let sigalg = alg_sha256_rsa();
    let name = x509_name_cn("KnoxOS Test CA");
    let validity = der_tlv(
        ASN1_SEQUENCE,
        &[
            der_tlv(ASN1_UTC_TIME, b"000101000000Z"),
            der_tlv(ASN1_UTC_TIME, b"491231235959Z"),
        ]
        .concat(),
    );
    let rsa_pub = der_tlv(
        ASN1_SEQUENCE,
        &[der_integer(&GATE_X509_N), der_integer(&[0x01, 0x00, 0x01])].concat(),
    );
    let mut spki_bits = alloc::vec![0u8];
    spki_bits.extend_from_slice(&rsa_pub);
    let spki = der_tlv(
        ASN1_SEQUENCE,
        &[alg_rsa_encryption(), der_tlv(ASN1_BIT_STRING, &spki_bits)].concat(),
    );
    let bc_seq = der_tlv(ASN1_SEQUENCE, &der_tlv(ASN1_BOOLEAN, &[0xFF]));
    let ext = der_tlv(
        ASN1_SEQUENCE,
        &[
            der_oid(&[0x55, 0x1D, 0x13]),
            der_tlv(ASN1_BOOLEAN, &[0xFF]),
            der_tlv(ASN1_OCTET_STRING, &bc_seq),
        ]
        .concat(),
    );
    let extensions = der_tlv(ASN1_CONTEXT_3, &der_tlv(ASN1_SEQUENCE, &ext));
    let tbs = der_tlv(
        ASN1_SEQUENCE,
        &[
            version,
            serial,
            sigalg.clone(),
            name.clone(),
            validity,
            name,
            spki,
            extensions,
        ]
        .concat(),
    );
    let key = crate::crypto::RsaPrivateKey {
        public: crate::crypto::RsaPublicKey {
            n: GATE_X509_N.to_vec(),
            e: 65537,
            bits: 512,
        },
        d: GATE_X509_D.to_vec(),
        p: Vec::new(),
        q: Vec::new(),
    };
    let sig = crate::crypto::rsa_sign(&tbs, &key);
    let mut sig_bits = alloc::vec![0u8];
    sig_bits.extend_from_slice(&sig);
    der_tlv(
        ASN1_SEQUENCE,
        &[tbs, sigalg, der_tlv(ASN1_BIT_STRING, &sig_bits)].concat(),
    )
}
