use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

// ═══════════════════════════════════════════════════════════════════════
// TARBALL EXTRACTION (simplified)
// ═══════════════════════════════════════════════════════════════════════

/// TAR header (POSIX/UStar, 512 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TarHeader {
    pub name: [u8; 100],
    pub mode: [u8; 8],
    pub uid: [u8; 8],
    pub gid: [u8; 8],
    pub size: [u8; 12],  // octal
    pub mtime: [u8; 12], // octal
    pub chksum: [u8; 8], // octal
    pub typeflag: u8,    // '0' = file, '5' = directory, '2' = symlink
    pub linkname: [u8; 100],
    pub magic: [u8; 6], // "ustar\0" or "ustar "
    pub version: [u8; 2],
    pub uname: [u8; 32],
    pub gname: [u8; 32],
    pub devmajor: [u8; 8],
    pub devminor: [u8; 8],
    pub prefix: [u8; 155],
    pub _pad: [u8; 12],
}

const TAR_BLOCK_SIZE: usize = 512;

/// Tar entry types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TarEntryType {
    RegularFile,
    HardLink,
    SymLink,
    Directory,
    Other,
}

/// Parsed tar entry
#[derive(Debug, Clone)]
pub struct TarEntry {
    pub path: String,
    pub entry_type: TarEntryType,
    pub size: usize,
    pub mode: u32,
    pub data_offset: usize, // offset within tar data
    pub link_target: Option<String>,
}

/// Parse a tar archive (uncompressed) and list entries
pub fn parse_tar(data: &[u8]) -> Vec<TarEntry> {
    let mut entries = Vec::new();
    let mut pos = 0;

    while pos + TAR_BLOCK_SIZE <= data.len() {
        let header = &data[pos..pos + TAR_BLOCK_SIZE];

        // Check for null block (end of archive)
        if header.iter().all(|&b| b == 0) {
            break;
        }

        // Parse name
        let name = parse_tar_string(&header[0..100]);
        let prefix = parse_tar_string(&header[345..500]);
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{}/{}", prefix, name)
        };

        // Parse size (octal)
        let size = parse_tar_octal(&header[124..136]);

        // Parse mode (octal)
        let mode = parse_tar_octal(&header[100..108]) as u32;

        // Parse type
        let entry_type = match header[156] {
            b'0' | 0 => TarEntryType::RegularFile,
            b'1' => TarEntryType::HardLink,
            b'2' => TarEntryType::SymLink,
            b'5' => TarEntryType::Directory,
            _ => TarEntryType::Other,
        };

        // Parse link target for symlinks
        let link_target = if entry_type == TarEntryType::SymLink {
            Some(parse_tar_string(&header[157..257]))
        } else {
            None
        };

        let data_offset = pos + TAR_BLOCK_SIZE;

        entries.push(TarEntry {
            path,
            entry_type,
            size,
            mode,
            data_offset,
            link_target,
        });

        // Advance past data blocks
        let data_blocks = size.div_ceil(TAR_BLOCK_SIZE);
        pos = data_offset + data_blocks * TAR_BLOCK_SIZE;
    }

    entries
}

fn parse_tar_string(raw: &[u8]) -> String {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).trim().to_string()
}

fn parse_tar_octal(raw: &[u8]) -> usize {
    let s = String::from_utf8_lossy(raw);
    let trimmed = s.trim().trim_end_matches('\0');
    if trimmed.is_empty() {
        return 0;
    }
    usize::from_str_radix(trimmed, 8).unwrap_or(0)
}
