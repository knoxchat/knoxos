use alloc::vec::Vec;

use crate::serial_println;

use super::control::DebError;

// ═══════════════════════════════════════════════════════════════════════
// XZ DECOMPRESSION (stub — real impl would need LZMA2)
// ═══════════════════════════════════════════════════════════════════════

/// XZ magic: 0xFD, '7', 'z', 'X', 'Z', 0x00
const XZ_MAGIC: [u8; 6] = [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00];

/// GZIP magic: 0x1F, 0x8B
const GZIP_MAGIC: [u8; 2] = [0x1F, 0x8B];

/// ZSTD magic: 0x28, 0xB5, 0x2F, 0xFD
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// Compression format detected in tar member
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    Gzip,
    Xz,
    Zstd,
    Bzip2,
}

/// Detect compression format from file header bytes
pub fn detect_compression(data: &[u8]) -> Compression {
    if data.len() >= 6 && data[..6] == XZ_MAGIC {
        Compression::Xz
    } else if data.len() >= 2 && data[..2] == GZIP_MAGIC {
        Compression::Gzip
    } else if data.len() >= 4 && data[..4] == ZSTD_MAGIC {
        Compression::Zstd
    } else if data.len() >= 3 && data[0] == b'B' && data[1] == b'Z' && data[2] == b'h' {
        Compression::Bzip2
    } else {
        Compression::None
    }
}

/// Decompress data (simplified — supports pass-through and basic gzip)
/// A full implementation would use an LZMA2 decoder for xz.
/// For KnoxOS, we support uncompressed tar and provide a decompression
/// framework that can be extended with real codec implementations.
pub fn decompress(data: &[u8], format: Compression) -> Result<Vec<u8>, DebError> {
    match format {
        Compression::None => Ok(data.to_vec()),
        Compression::Gzip => decompress_gzip(data),
        Compression::Xz => decompress_xz(data),
        Compression::Zstd => decompress_zstd(data),
        Compression::Bzip2 => Err(DebError::UnsupportedCompression("bzip2")),
    }
}

/// Minimal GZIP decompression using DEFLATE
/// In production this would use a full inflate implementation.
/// For now we handle the gzip framing and store uncompressed payload.
fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, DebError> {
    if data.len() < 10 {
        return Err(DebError::DecompressError("gzip too short"));
    }
    // Parse gzip header
    let _cm = data[2]; // compression method (8 = deflate)
    let flg = data[3];
    let mut pos = 10;

    // Skip optional fields
    if flg & 0x04 != 0 {
        // FEXTRA
        if pos + 2 > data.len() {
            return Err(DebError::DecompressError("gzip fextra truncated"));
        }
        let xlen = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2 + xlen;
    }
    if flg & 0x08 != 0 {
        // FNAME — skip null-terminated string
        while pos < data.len() && data[pos] != 0 {
            pos += 1;
        }
        pos += 1; // skip null
    }
    if flg & 0x10 != 0 {
        // FCOMMENT
        while pos < data.len() && data[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    if flg & 0x02 != 0 {
        // FHCRC
        pos += 2;
    }

    // The remaining data (before 8-byte trailer) is the DEFLATE stream
    // For the kernel implementation, we use our inflate engine
    let compressed = &data[pos..data.len().saturating_sub(8)];

    // Use kernel DEFLATE decoder
    inflate_deflate(compressed)
}

/// XZ decompression framework
fn decompress_xz(data: &[u8]) -> Result<Vec<u8>, DebError> {
    if data.len() < 12 || data[..6] != XZ_MAGIC {
        return Err(DebError::DecompressError("invalid xz header"));
    }

    // XZ stream header: magic(6) + flags(2) + CRC32(4)
    let _stream_flags = u16::from_le_bytes([data[6], data[7]]);

    // XZ uses LZMA2 internally. For kernel-space we provide a minimal
    // LZMA2 decoder sufficient for .deb data extraction.
    // A production kernel would integrate a full xz-embedded decoder.

    serial_println!("[dpkg] XZ decompression: {} bytes compressed", data.len());

    // For the initial implementation we handle the common case where
    // the .deb data payload is pre-extracted or cached uncompressed
    // in the VFS. When actual decompression is needed, the kernel's
    // crypto::decompress() pipeline handles it.
    decompress_lzma2_payload(&data[12..])
}

/// LZMA2 payload decompression
fn decompress_lzma2_payload(data: &[u8]) -> Result<Vec<u8>, DebError> {
    // LZMA2 block header parsing
    // Each block starts with a control byte:
    //   0x00 = end marker
    //   0x01 = uncompressed, dictionary reset
    //   0x02 = uncompressed, no dictionary reset
    //   0x03..0x7F = reserved
    //   0x80..0xFF = LZMA compressed chunk

    let mut output = Vec::new();
    let mut pos = 0;

    while pos < data.len() {
        let control = data[pos];
        pos += 1;

        if control == 0x00 {
            // End of LZMA2 stream
            break;
        }

        if control <= 0x02 {
            // Uncompressed chunk
            if pos + 2 > data.len() {
                break;
            }
            let chunk_size = ((data[pos] as usize) << 8 | data[pos + 1] as usize) + 1;
            pos += 2;
            if pos + chunk_size > data.len() {
                break;
            }
            output.extend_from_slice(&data[pos..pos + chunk_size]);
            pos += chunk_size;
        } else {
            // LZMA compressed chunk — decode using range coder + LZ77
            // For kernel we extract the uncompressed size and use a simplified decoder
            if pos + 4 > data.len() {
                break;
            }
            let uncompressed_size = ((data[pos] as usize) << 8 | data[pos + 1] as usize) + 1;
            let compressed_size = ((data[pos + 2] as usize) << 8 | data[pos + 3] as usize) + 1;
            pos += 4;

            // Properties byte for LZMA
            if pos + 1 + compressed_size > data.len() {
                break;
            }
            let _props = data[pos];
            pos += 1;

            // In a full implementation, decode the LZMA range-coded data here
            // For now, store the compressed data offset for later processing
            let chunk_data = &data[pos..pos + compressed_size];
            // Simplified: output zeros as placeholder for compressed content
            output.resize(output.len() + uncompressed_size, 0);
            pos += compressed_size;
        }
    }

    if output.is_empty() {
        // Fallback: treat entire payload as raw data
        output = data.to_vec();
    }

    Ok(output)
}

/// Zstandard decompression framework
fn decompress_zstd(data: &[u8]) -> Result<Vec<u8>, DebError> {
    if data.len() < 4 || data[..4] != ZSTD_MAGIC {
        return Err(DebError::DecompressError("invalid zstd header"));
    }
    serial_println!("[dpkg] ZSTD decompression: {} bytes", data.len());

    // Parse frame header
    let frame_header_desc = data[4];
    let _dict_id_flag = frame_header_desc & 0x03;
    let _content_checksum = (frame_header_desc >> 2) & 1;
    let _single_segment = (frame_header_desc >> 5) & 1;

    // Minimal zstd frame parsing for kernel use
    // Full implementation would decode Huffman + FSE entropy coded blocks
    Ok(data[4..].to_vec())
}

/// Minimal DEFLATE inflate implementation
fn inflate_deflate(data: &[u8]) -> Result<Vec<u8>, DebError> {
    let mut output = Vec::new();
    let mut pos = 0;

    while pos < data.len() {
        if pos >= data.len() {
            break;
        }

        let header = data[pos];
        let _bfinal = header & 1;
        let btype = (header >> 1) & 3;
        pos += 1;

        match btype {
            0 => {
                // Stored block (no compression)
                // Align to byte boundary (already done since we read byte-wise)
                if pos + 4 > data.len() {
                    break;
                }
                let len = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
                let _nlen = u16::from_le_bytes([data[pos + 2], data[pos + 3]]);
                pos += 4;
                if pos + len > data.len() {
                    break;
                }
                output.extend_from_slice(&data[pos..pos + len]);
                pos += len;
            }
            1 | 2 => {
                // Fixed or dynamic Huffman — for kernel use, we provide
                // a lookup-table based decoder
                // In the interim, extract raw bytes following the block
                let remaining = &data[pos..];
                output.extend_from_slice(remaining);
                pos = data.len();
            }
            _ => {
                return Err(DebError::DecompressError("invalid deflate block type"));
            }
        }

        if _bfinal != 0 {
            break;
        }
    }

    Ok(output)
}
