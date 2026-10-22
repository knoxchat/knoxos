//! Enhanced inode metadata, timestamps, and umask.
use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Monotonic tick counter for timestamps (seconds since boot).
static TICK_COUNTER: AtomicU64 = AtomicU64::new(1_700_000_000);

/// Get current timestamp (Unix epoch seconds, approximate).
pub(super) fn now() -> u64 {
    // Use the RTC if available, otherwise use the tick counter
    TICK_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Set the base time (called during init to sync with RTC).
pub fn set_base_time(epoch_secs: u64) {
    TICK_COUNTER.store(epoch_secs, Ordering::Relaxed);
}

/// Enhanced inode metadata stored alongside VFS inodes.
#[derive(Debug, Clone)]
pub struct InodeMeta {
    /// Access time (last read)
    pub atime: u64,
    /// Modification time (last content change)
    pub mtime: u64,
    /// Change time (last metadata change — permissions, owner, etc.)
    pub ctime: u64,
    /// Birth time (creation time, Linux statx)
    pub btime: u64,
    /// Hard link count
    pub nlink: u32,
    /// Symlink target (only for SymLink inodes)
    pub symlink_target: Option<String>,
    /// Device numbers (major, minor) for device files
    pub dev_major: u32,
    pub dev_minor: u32,
    /// Block count (512-byte blocks)
    pub blocks: u64,
    /// Preferred I/O block size
    pub blksize: u64,
    /// Device ID of the filesystem
    pub dev: u64,
}

impl InodeMeta {
    pub fn new() -> Self {
        let t = now();
        Self {
            atime: t,
            mtime: t,
            ctime: t,
            btime: t,
            nlink: 1,
            symlink_target: None,
            dev_major: 0,
            dev_minor: 0,
            blocks: 0,
            blksize: 4096,
            dev: 0,
        }
    }

    pub fn for_directory() -> Self {
        let mut meta = Self::new();
        meta.nlink = 2; // . and parent
        meta
    }

    pub fn for_symlink(target: &str) -> Self {
        let mut meta = Self::new();
        meta.symlink_target = Some(String::from(target));
        meta
    }
}

/// Metadata store keyed by inode number.
lazy_static::lazy_static! {
    pub(super) static ref INODE_META: Mutex<BTreeMap<u64, InodeMeta>> = Mutex::new(BTreeMap::new());
}

/// Process-level umask.
static UMASK: AtomicU64 = AtomicU64::new(0o022);

/// Get the current umask.
pub fn get_umask() -> u16 {
    UMASK.load(Ordering::Relaxed) as u16
}

/// Set the umask, returning the old value.
pub fn set_umask(mask: u16) -> u16 {
    UMASK.swap(mask as u64, Ordering::Relaxed) as u16
}

/// Apply umask to a mode.
pub(super) fn apply_umask(mode: u16) -> u16 {
    mode & !get_umask()
}

/// Get or create metadata for an inode.
pub(super) fn get_or_create_meta(ino: u64) -> InodeMeta {
    let mut store = INODE_META.lock();
    store.entry(ino).or_insert_with(InodeMeta::new).clone()
}

/// Update metadata for an inode.
pub(super) fn update_meta<F: FnOnce(&mut InodeMeta)>(ino: u64, f: F) {
    let mut store = INODE_META.lock();
    let meta = store.entry(ino).or_insert_with(InodeMeta::new);
    f(meta);
}

/// Touch the atime of an inode.
pub(super) fn touch_atime(ino: u64) {
    update_meta(ino, |m| m.atime = now());
}

/// Touch the mtime and ctime of an inode.
pub(super) fn touch_mtime(ino: u64) {
    let t = now();
    update_meta(ino, |m| {
        m.mtime = t;
        m.ctime = t;
    });
}

/// Touch ctime only (metadata change).
pub(super) fn touch_ctime(ino: u64) {
    update_meta(ino, |m| m.ctime = now());
}
