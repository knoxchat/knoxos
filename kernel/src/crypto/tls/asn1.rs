//! ASN.1 DER parser used by X.509 certificates.

use alloc::string::String;
use alloc::vec::Vec;

// ─── ASN.1 DER Parser ──────────────────────────────────────────────────

/// ASN.1 tag classes
pub(super) const ASN1_SEQUENCE: u8 = 0x30;
pub(super) const ASN1_SET: u8 = 0x31;
pub(super) const ASN1_INTEGER: u8 = 0x02;
pub(super) const ASN1_BIT_STRING: u8 = 0x03;
pub(super) const ASN1_OCTET_STRING: u8 = 0x04;
pub(super) const ASN1_OID: u8 = 0x06;
pub(super) const ASN1_UTF8_STRING: u8 = 0x0C;
pub(super) const ASN1_PRINTABLE_STRING: u8 = 0x13;
pub(super) const ASN1_IA5_STRING: u8 = 0x16;
pub(super) const ASN1_UTC_TIME: u8 = 0x17;
pub(super) const ASN1_GENERALIZED_TIME: u8 = 0x18;
pub(super) const ASN1_CONTEXT_0: u8 = 0xA0;
pub(super) const ASN1_CONTEXT_3: u8 = 0xA3;
pub(super) const ASN1_BOOLEAN: u8 = 0x01;
pub(super) const ASN1_NULL: u8 = 0x05;

/// Parse ASN.1 DER tag + length, return (tag, content_slice, rest)
pub(super) fn asn1_read_tlv(data: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    if data.is_empty() {
        return None;
    }
    let tag = data[0];
    if data.len() < 2 {
        return None;
    }
    let (len, header_size) = if data[1] & 0x80 == 0 {
        (data[1] as usize, 2)
    } else {
        let num_bytes = (data[1] & 0x7F) as usize;
        if num_bytes == 0 || num_bytes > 4 || data.len() < 2 + num_bytes {
            return None;
        }
        let mut len = 0usize;
        for i in 0..num_bytes {
            len = (len << 8) | (data[2 + i] as usize);
        }
        (len, 2 + num_bytes)
    };
    if data.len() < header_size + len {
        return None;
    }
    Some((
        tag,
        &data[header_size..header_size + len],
        &data[header_size + len..],
    ))
}

/// Parse an ASN.1 OID into a dotted string (e.g., "2.5.4.3")
pub(super) fn parse_oid(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let mut parts: Vec<u64> = Vec::new();
    // First byte encodes first two arcs: val = 40*x + y
    parts.push((data[0] / 40) as u64);
    parts.push((data[0] % 40) as u64);

    let mut val = 0u64;
    for &b in &data[1..] {
        val = (val << 7) | ((b & 0x7F) as u64);
        if b & 0x80 == 0 {
            parts.push(val);
            val = 0;
        }
    }

    let mut s = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            s.push('.');
        }
        use core::fmt::Write;
        let _ = write!(s, "{}", p);
    }
    s
}

/// OID for Common Name (2.5.4.3)
pub(super) const OID_CN: &str = "2.5.4.3";
/// OID for Organization (2.5.4.10)
pub(super) const OID_O: &str = "2.5.4.10";
/// OID for Country (2.5.4.6)
pub(super) const OID_C: &str = "2.5.4.6";
/// OID for Locality (2.5.4.7)
pub(super) const OID_L: &str = "2.5.4.7";
/// OID for State (2.5.4.8)
pub(super) const OID_ST: &str = "2.5.4.8";
/// OID for Organizational Unit (2.5.4.11)
pub(super) const OID_OU: &str = "2.5.4.11";
/// OID for Basic Constraints (2.5.29.19)
pub(super) const OID_BASIC_CONSTRAINTS: &str = "2.5.29.19";

/// Map well-known OIDs to short names
fn oid_short_name(oid: &str) -> &str {
    match oid {
        "2.5.4.3" => "CN",
        "2.5.4.6" => "C",
        "2.5.4.7" => "L",
        "2.5.4.8" => "ST",
        "2.5.4.10" => "O",
        "2.5.4.11" => "OU",
        _ => oid,
    }
}

/// Parse an X.501 Name (SEQUENCE of SET of AttributeTypeAndValue)
pub(super) fn parse_x501_name(data: &[u8]) -> String {
    let mut result = String::new();
    let mut pos = data;
    let mut first = true;

    while !pos.is_empty() {
        // Each element is a SET
        if let Some((tag, set_content, rest)) = asn1_read_tlv(pos) {
            if tag == ASN1_SET {
                // SET contains SEQUENCE(s) of (OID, value)
                if let Some((_seq_tag, seq_content, _)) = asn1_read_tlv(set_content) {
                    // Read OID
                    if let Some((oid_tag, oid_data, after_oid)) = asn1_read_tlv(seq_content) {
                        if oid_tag == ASN1_OID {
                            let oid_str = parse_oid(oid_data);
                            // Read value (string)
                            if let Some((_val_tag, val_data, _)) = asn1_read_tlv(after_oid) {
                                let val = core::str::from_utf8(val_data).unwrap_or("?");
                                if !first {
                                    result.push_str(", ");
                                }
                                result.push_str(oid_short_name(&oid_str));
                                result.push('=');
                                result.push_str(val);
                                first = false;
                            }
                        }
                    }
                }
            }
            pos = rest;
        } else {
            break;
        }
    }
    result
}

/// Parse UTCTime (YYMMDDHHMMSSZ) or GeneralizedTime (YYYYMMDDHHMMSSZ) to Unix timestamp
pub(super) fn parse_asn1_time(tag: u8, data: &[u8]) -> u64 {
    let s = core::str::from_utf8(data).unwrap_or("");
    let (year, rest) = if tag == ASN1_UTC_TIME {
        let y: u64 = s.get(0..2).and_then(|v| v.parse().ok()).unwrap_or(0);
        let y = if y >= 50 { 1900 + y } else { 2000 + y };
        (y, &s[2..])
    } else {
        let y: u64 = s.get(0..4).and_then(|v| v.parse().ok()).unwrap_or(0);
        (y, &s[4..])
    };
    let month: u64 = rest.get(0..2).and_then(|v| v.parse().ok()).unwrap_or(1);
    let day: u64 = rest.get(2..4).and_then(|v| v.parse().ok()).unwrap_or(1);
    let hour: u64 = rest.get(4..6).and_then(|v| v.parse().ok()).unwrap_or(0);
    let min: u64 = rest.get(6..8).and_then(|v| v.parse().ok()).unwrap_or(0);
    let sec: u64 = rest.get(8..10).and_then(|v| v.parse().ok()).unwrap_or(0);

    // Simplified Unix timestamp (ignoring leap years/seconds for kernel use)
    let days_per_month: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut days = 0u64;
    for y in 1970..year {
        days += if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
    }
    for m in 0..(month.saturating_sub(1) as usize).min(11) {
        days += days_per_month[m];
        if m == 1 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            days += 1;
        }
    }
    days += day.saturating_sub(1);
    days * 86400 + hour * 3600 + min * 60 + sec
}
