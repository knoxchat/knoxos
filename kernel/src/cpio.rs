/// CPIO — CPIO archive (initramfs) parser
///
/// Provides support for loading an initial RAM filesystem:
///   - newc (SVR4) CPIO format parsing
///   - File extraction into VFS
///   - Used for early boot userspace (/init, busybox, etc.)
///   - initrd/initramfs loading from bootloader
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── CPIO Constants ────────────────────────────────────────────────────

/// CPIO newc magic number
pub const CPIO_MAGIC: &[u8; 6] = b"070701";

/// CPIO file types (mode field, upper bits)
pub const S_IFMT: u32 = 0o170000;
pub const S_IFSOCK: u32 = 0o140000;
pub const S_IFLNK: u32 = 0o120000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFBLK: u32 = 0o060000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFCHR: u32 = 0o020000;
pub const S_IFIFO: u32 = 0o010000;

// ─── CPIO Entry ────────────────────────────────────────────────────────

/// A single entry in the CPIO archive
#[derive(Debug, Clone)]
pub struct CpioEntry {
    pub name: String,
    pub ino: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u32,
    pub mtime: u32,
    pub filesize: u32,
    pub devmajor: u32,
    pub devminor: u32,
    pub rdevmajor: u32,
    pub rdevminor: u32,
    pub data_offset: usize, // offset into archive
}

impl CpioEntry {
    pub fn is_dir(&self) -> bool {
        (self.mode & S_IFMT) == S_IFDIR
    }

    pub fn is_file(&self) -> bool {
        (self.mode & S_IFMT) == S_IFREG
    }

    pub fn is_symlink(&self) -> bool {
        (self.mode & S_IFMT) == S_IFLNK
    }

    pub fn is_chardev(&self) -> bool {
        (self.mode & S_IFMT) == S_IFCHR
    }

    pub fn is_blockdev(&self) -> bool {
        (self.mode & S_IFMT) == S_IFBLK
    }

    pub fn permissions(&self) -> u32 {
        self.mode & 0o7777
    }
}

// ─── CPIO Parser ───────────────────────────────────────────────────────

/// Parse a hex string from the CPIO header
fn parse_hex(data: &[u8], offset: usize, len: usize) -> u32 {
    let s = &data[offset..offset + len];
    let mut val: u32 = 0;
    for &b in s {
        val <<= 4;
        val |= match b {
            b'0'..=b'9' => (b - b'0') as u32,
            b'a'..=b'f' => (b - b'a' + 10) as u32,
            b'A'..=b'F' => (b - b'A' + 10) as u32,
            _ => 0,
        };
    }
    val
}

/// Align offset to 4-byte boundary
fn align4(offset: usize) -> usize {
    (offset + 3) & !3
}

/// Parse a CPIO newc archive from a byte buffer
pub fn parse(data: &[u8]) -> Vec<CpioEntry> {
    let mut entries = Vec::new();
    let mut offset = 0;

    while offset + 110 <= data.len() {
        // Check magic
        if &data[offset..offset + 6] != CPIO_MAGIC {
            serial_println!("[CPIO] Bad magic at offset {:#x}", offset);
            break;
        }

        let ino = parse_hex(data, offset + 6, 8);
        let mode = parse_hex(data, offset + 14, 8);
        let uid = parse_hex(data, offset + 22, 8);
        let gid = parse_hex(data, offset + 30, 8);
        let nlink = parse_hex(data, offset + 38, 8);
        let mtime = parse_hex(data, offset + 46, 8);
        let filesize = parse_hex(data, offset + 54, 8);
        let devmajor = parse_hex(data, offset + 62, 8);
        let devminor = parse_hex(data, offset + 70, 8);
        let rdevmajor = parse_hex(data, offset + 78, 8);
        let rdevminor = parse_hex(data, offset + 86, 8);
        let namesize = parse_hex(data, offset + 94, 8) as usize;
        // let _check = parse_hex(data, offset + 102, 8);

        let name_start = offset + 110;
        let name_end = name_start + namesize - 1; // subtract null terminator
        if name_end > data.len() {
            break;
        }

        let name = core::str::from_utf8(&data[name_start..name_end])
            .unwrap_or("<invalid>")
            .to_string();

        // End of archive marker
        if name == "TRAILER!!!" {
            break;
        }

        let data_start = align4(name_start + namesize);
        let data_end = data_start + filesize as usize;

        entries.push(CpioEntry {
            name,
            ino,
            mode,
            uid,
            gid,
            nlink,
            mtime,
            filesize,
            devmajor,
            devminor,
            rdevmajor,
            rdevminor,
            data_offset: data_start,
        });

        offset = align4(data_end);
    }

    entries
}

/// Get file data from a CPIO entry
pub fn get_file_data<'a>(archive: &'a [u8], entry: &CpioEntry) -> &'a [u8] {
    let end = entry.data_offset + entry.filesize as usize;
    if end > archive.len() {
        return &[];
    }
    &archive[entry.data_offset..end]
}

// ─── Initramfs ─────────────────────────────────────────────────────────

pub struct Initramfs {
    pub entries: Vec<CpioEntry>,
    pub data: Vec<u8>,
    pub total_files: usize,
    pub total_dirs: usize,
    pub total_size: usize,
}

impl Default for Initramfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Initramfs {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            data: Vec::new(),
            total_files: 0,
            total_dirs: 0,
            total_size: 0,
        }
    }

    /// Load initramfs from raw bytes (e.g., from bootloader module)
    pub fn load(&mut self, data: &[u8]) -> usize {
        self.data = Vec::from(data);
        self.entries = parse(&self.data);

        self.total_files = self.entries.iter().filter(|e| e.is_file()).count();
        self.total_dirs = self.entries.iter().filter(|e| e.is_dir()).count();
        self.total_size = self.entries.iter().map(|e| e.filesize as usize).sum();

        serial_println!(
            "[CPIO] Loaded initramfs: {} entries ({} files, {} dirs, {} bytes)",
            self.entries.len(),
            self.total_files,
            self.total_dirs,
            self.total_size
        );

        self.entries.len()
    }

    /// Find an entry by path
    pub fn find(&self, path: &str) -> Option<&CpioEntry> {
        // Normalize: strip leading "." or "./"
        let normalized = path.trim_start_matches("./").trim_start_matches('/');
        self.entries.iter().find(|e| {
            let entry_name = e.name.trim_start_matches("./").trim_start_matches('/');
            entry_name == normalized
        })
    }

    /// Get file data for a path
    pub fn read_file(&self, path: &str) -> Option<&[u8]> {
        let entry = self.find(path)?;
        if !entry.is_file() {
            return None;
        }
        Some(get_file_data(&self.data, entry))
    }

    /// List directory contents
    pub fn list_dir(&self, path: &str) -> Vec<&CpioEntry> {
        let normalized = if path.is_empty() || path == "/" {
            String::new()
        } else {
            let p = path
                .trim_start_matches("./")
                .trim_start_matches('/')
                .trim_end_matches('/');
            format!("{}/", p)
        };

        self.entries
            .iter()
            .filter(|e| {
                let name = e.name.trim_start_matches("./").trim_start_matches('/');
                if normalized.is_empty() {
                    // Root: entries without /
                    !name.contains('/')
                } else {
                    name.starts_with(&normalized) && {
                        let rest = &name[normalized.len()..];
                        !rest.is_empty() && !rest.contains('/')
                    }
                }
            })
            .collect()
    }
}

// ─── Global state ──────────────────────────────────────────────────────

lazy_static::lazy_static! {
    pub static ref INITRAMFS: Mutex<Initramfs> = Mutex::new(Initramfs::new());
}

static INITRAMFS_LOADED: AtomicBool = AtomicBool::new(false);

pub fn is_loaded() -> bool {
    INITRAMFS_LOADED.load(Ordering::Relaxed)
}

/// Load initramfs from boot module data
pub fn load_initramfs(data: &[u8]) -> usize {
    let count = INITRAMFS.lock().load(data);
    if count > 0 {
        INITRAMFS_LOADED.store(true, Ordering::Relaxed);
    }
    count
}

/// Read a file from the initramfs
pub fn read_file(path: &str) -> Option<Vec<u8>> {
    let initrd = INITRAMFS.lock();
    initrd.read_file(path).map(Vec::from)
}

/// Initialize CPIO/initramfs subsystem
pub fn init() {
    serial_println!("[CPIO] Initramfs subsystem initialized");
}
