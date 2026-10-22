use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use super::control::DebError;

// ═══════════════════════════════════════════════════════════════════════
// AR ARCHIVE FORMAT
// ═══════════════════════════════════════════════════════════════════════

/// AR archive magic: "!<arch>\n"
const AR_MAGIC: &[u8; 8] = b"!<arch>\n";

/// AR file header (60 bytes per entry)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ArHeader {
    /// File name (16 bytes, space-padded, terminated by '/')
    pub name: [u8; 16],
    /// Modification timestamp (12 bytes, decimal ASCII)
    pub mtime: [u8; 12],
    /// Owner ID (6 bytes, decimal ASCII)
    pub uid: [u8; 6],
    /// Group ID (6 bytes, decimal ASCII)
    pub gid: [u8; 6],
    /// File mode (8 bytes, octal ASCII)
    pub mode: [u8; 8],
    /// File size (10 bytes, decimal ASCII)
    pub size: [u8; 10],
    /// Magic: "`\n"
    pub fmag: [u8; 2],
}

const AR_HEADER_SIZE: usize = 60;
const AR_FMAG: [u8; 2] = [0x60, 0x0A]; // "`\n"

/// Parsed AR archive member
#[derive(Debug, Clone)]
pub struct ArMember {
    pub name: String,
    pub size: usize,
    pub offset: usize, // offset of data within the archive
}

/// Parse an AR archive and extract member metadata
pub fn parse_ar_archive(data: &[u8]) -> Result<Vec<ArMember>, DebError> {
    if data.len() < 8 {
        return Err(DebError::InvalidArchive("too short for ar magic"));
    }
    if &data[0..8] != AR_MAGIC {
        return Err(DebError::InvalidArchive("bad ar magic"));
    }

    let mut members = Vec::new();
    let mut pos = 8; // skip magic

    while pos + AR_HEADER_SIZE <= data.len() {
        let hdr_bytes = &data[pos..pos + AR_HEADER_SIZE];

        // Validate file magic
        if hdr_bytes[58] != AR_FMAG[0] || hdr_bytes[59] != AR_FMAG[1] {
            return Err(DebError::InvalidArchive("bad ar entry fmag"));
        }

        // Parse name (trim trailing spaces and '/')
        let name_raw = &hdr_bytes[0..16];
        let name = parse_ar_string(name_raw).trim_end_matches('/').to_string();

        // Parse size
        let size_raw = &hdr_bytes[48..58];
        let size = parse_ar_decimal(size_raw)?;

        let data_offset = pos + AR_HEADER_SIZE;

        members.push(ArMember {
            name,
            size,
            offset: data_offset,
        });

        // Advance past header + data, aligned to 2 bytes
        pos = data_offset + size;
        if pos % 2 != 0 {
            pos += 1; // ar entries are 2-byte aligned
        }
    }

    Ok(members)
}

fn parse_ar_string(raw: &[u8]) -> String {
    let s: Vec<u8> = raw.iter().copied().take_while(|&b| b != 0).collect();
    String::from_utf8_lossy(&s).trim().to_string()
}

fn parse_ar_decimal(raw: &[u8]) -> Result<usize, DebError> {
    let s = String::from_utf8_lossy(raw);
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    trimmed
        .parse::<usize>()
        .map_err(|_| DebError::InvalidArchive("bad decimal in ar header"))
}
