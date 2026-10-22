use alloc::vec::Vec;

/// Encode a QUIC variable-length integer
pub fn encode_varint(buf: &mut Vec<u8>, value: u64) {
    if value < 64 {
        buf.push(value as u8);
    } else if value < 16384 {
        buf.push(0x40 | (value >> 8) as u8);
        buf.push(value as u8);
    } else if value < 1_073_741_824 {
        buf.push(0x80 | (value >> 24) as u8);
        buf.push((value >> 16) as u8);
        buf.push((value >> 8) as u8);
        buf.push(value as u8);
    } else {
        buf.push(0xC0 | (value >> 56) as u8);
        buf.push((value >> 48) as u8);
        buf.push((value >> 40) as u8);
        buf.push((value >> 32) as u8);
        buf.push((value >> 24) as u8);
        buf.push((value >> 16) as u8);
        buf.push((value >> 8) as u8);
        buf.push(value as u8);
    }
}

/// Decode a QUIC variable-length integer
pub fn decode_varint(data: &[u8]) -> Option<(u64, usize)> {
    if data.is_empty() {
        return None;
    }
    let first = data[0];
    let prefix = first >> 6;
    match prefix {
        0 => Some((first as u64, 1)),
        1 => {
            if data.len() < 2 {
                return None;
            }
            let value = ((first as u64 & 0x3F) << 8) | data[1] as u64;
            Some((value, 2))
        }
        2 => {
            if data.len() < 4 {
                return None;
            }
            let value = ((first as u64 & 0x3F) << 24)
                | ((data[1] as u64) << 16)
                | ((data[2] as u64) << 8)
                | data[3] as u64;
            Some((value, 4))
        }
        3 => {
            if data.len() < 8 {
                return None;
            }
            let value = ((first as u64 & 0x3F) << 56)
                | ((data[1] as u64) << 48)
                | ((data[2] as u64) << 40)
                | ((data[3] as u64) << 32)
                | ((data[4] as u64) << 24)
                | ((data[5] as u64) << 16)
                | ((data[6] as u64) << 8)
                | data[7] as u64;
            Some((value, 8))
        }
        _ => None,
    }
}
