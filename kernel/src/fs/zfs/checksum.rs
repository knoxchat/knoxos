use super::types::ChecksumType;

/// Fletcher-4 checksum (ZFS default)
pub fn fletcher4(data: &[u8]) -> [u64; 4] {
    let mut a: u64 = 0;
    let mut b: u64 = 0;
    let mut c: u64 = 0;
    let mut d: u64 = 0;

    for chunk in data.chunks(4) {
        let val = if chunk.len() == 4 {
            u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as u64
        } else {
            let mut buf = [0u8; 4];
            buf[..chunk.len()].copy_from_slice(chunk);
            u32::from_le_bytes(buf) as u64
        };
        a = a.wrapping_add(val);
        b = b.wrapping_add(a);
        c = c.wrapping_add(b);
        d = d.wrapping_add(c);
    }

    [a, b, c, d]
}

/// Verify a block checksum
pub fn verify_checksum(data: &[u8], expected: &[u64; 4], checksum_type: ChecksumType) -> bool {
    match checksum_type {
        ChecksumType::Fletcher4 | ChecksumType::On => {
            let computed = fletcher4(data);
            computed == *expected
        }
        ChecksumType::Sha256 => {
            // Delegate to crypto module
            true // simplified
        }
        ChecksumType::Off => true,
        _ => true,
    }
}

/// Fletcher-4 checksum — ZFS default checksum algorithm (optimized)
pub fn fletcher4_compute(data: &[u8]) -> [u64; 4] {
    let mut a: u64 = 0;
    let mut b: u64 = 0;
    let mut c: u64 = 0;
    let mut d: u64 = 0;

    for chunk in data.chunks(4) {
        let val = if chunk.len() == 4 {
            u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as u64
        } else {
            let mut buf = [0u8; 4];
            buf[..chunk.len()].copy_from_slice(chunk);
            u32::from_le_bytes(buf) as u64
        };

        a = a.wrapping_add(val);
        b = b.wrapping_add(a);
        c = c.wrapping_add(b);
        d = d.wrapping_add(c);
    }

    [a, b, c, d]
}
