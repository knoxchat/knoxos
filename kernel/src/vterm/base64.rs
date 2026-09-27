//! Base64 helpers for OSC 52 clipboard and Kitty graphics payloads.
use alloc::string::String;
use alloc::vec::Vec;

// ── Base64 helpers for OSC 52 ────────────────────────────────

const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn base64_encode(data: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() {
            data[i + 1] as u32
        } else {
            0
        };
        let b2 = if i + 2 < data.len() {
            data[i + 2] as u32
        } else {
            0
        };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(B64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(B64_CHARS[((triple >> 12) & 0x3F) as usize] as char);

        if i + 1 < data.len() {
            result.push(B64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if i + 2 < data.len() {
            result.push(B64_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        i += 3;
    }
    result
}

pub(crate) fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let bytes: Vec<u8> = input
        .bytes()
        .filter(|b| *b != b'\n' && *b != b'\r' && *b != b' ')
        .collect();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let a = b64_val(bytes[i])?;
        let b = b64_val(bytes[i + 1])?;
        result.push((a << 2) | (b >> 4));
        if bytes[i + 2] != b'=' {
            let c = b64_val(bytes[i + 2])?;
            result.push(((b & 0xF) << 4) | (c >> 2));
            if bytes[i + 3] != b'=' {
                let d = b64_val(bytes[i + 3])?;
                result.push(((c & 0x3) << 6) | d);
            }
        }
        i += 4;
    }
    Some(result)
}

pub(crate) fn b64_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => Some(0),
    }
}
