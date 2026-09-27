use alloc::vec::Vec;

/// Compress data using LZ4 (simplified)
pub fn lz4_compress(data: &[u8]) -> Vec<u8> {
    // Simple RLE-style compression for demonstration
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let byte = data[i];
        let mut count: u8 = 1;
        while i + (count as usize) < data.len() && data[i + count as usize] == byte && count < 255 {
            count += 1;
        }
        if count >= 4 {
            out.push(0xFF); // escape
            out.push(count);
            out.push(byte);
            i += count as usize;
        } else {
            if byte == 0xFF {
                out.push(0xFF);
                out.push(1);
                out.push(0xFF);
            } else {
                out.push(byte);
            }
            i += 1;
        }
    }
    out
}

/// Decompress LZ4 data
pub fn lz4_decompress(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0xFF && i + 2 < data.len() {
            let count = data[i + 1];
            let byte = data[i + 2];
            for _ in 0..count {
                out.push(byte);
            }
            i += 3;
        } else {
            out.push(data[i]);
            i += 1;
        }
    }
    out
}
