// Directory operations (dirent.h)
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// struct dirent
#[repr(C)]
pub struct Dirent {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 256],
}

/// DT_* constants for d_type
pub const DT_UNKNOWN: u8 = 0;
pub const DT_FIFO: u8 = 1;
pub const DT_CHR: u8 = 2;
pub const DT_DIR: u8 = 4;
pub const DT_BLK: u8 = 6;
pub const DT_REG: u8 = 8;
pub const DT_LNK: u8 = 10;
pub const DT_SOCK: u8 = 12;

/// DIR stream
pub struct DirStream {
    pub path: String,
    pub entries: Vec<(String, u8)>, // (name, type)
    pub pos: usize,
}

lazy_static::lazy_static! {
    static ref OPEN_DIRS: Mutex<BTreeMap<u64, DirStream>> = Mutex::new(BTreeMap::new());
}

static NEXT_DIR_HANDLE: AtomicU64 = AtomicU64::new(1);

/// opendir — open a directory stream
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendir(name: *const u8) -> u64 {
    if name.is_null() {
        return 0;
    }
    let mut len = 0;
    while *name.add(len) != 0 {
        len += 1;
    }
    let path = core::str::from_utf8_unchecked(core::slice::from_raw_parts(name, len));

    // List directory via VFS
    let listing = crate::vfs::list_directory(path).unwrap_or_default();
    let entries: Vec<(String, u8)> = listing
        .iter()
        .map(|e| {
            let dtype = if e.ends_with('/') { DT_DIR } else { DT_REG };
            let name = e.trim_end_matches('/').to_string();
            (name, dtype)
        })
        .collect();

    let handle = NEXT_DIR_HANDLE.fetch_add(1, Ordering::Relaxed);
    OPEN_DIRS.lock().insert(
        handle,
        DirStream {
            path: String::from(path),
            entries,
            pos: 0,
        },
    );

    handle
}

/// closedir — close a directory stream
#[unsafe(no_mangle)]
pub unsafe extern "C" fn closedir(dirp: u64) -> i32 {
    OPEN_DIRS.lock().remove(&dirp);
    0
}
