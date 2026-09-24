//! File Manager — Comprehensive Unix/Linux filesystem operations
//!
//! Provides a full-featured file management layer on top of the VFS with
//! proper Unix semantics:
//!   - Symlink creation, resolution, and cycle detection
//!   - Hard links with reference counting
//!   - Timestamps (atime, mtime, ctime) on all operations
//!   - Recursive directory operations (cp -r, rm -rf, chmod -R, chown -R)
//!   - File locking (advisory, POSIX)
//!   - Atomic rename across directories
//!   - umask support
//!   - Inode-level stat with full Linux struct stat compatibility
//!   - Directory entry iteration with readdir/seekdir/telldir
//!   - Truncate, append, sparse file support
//!   - File type detection (magic bytes)
//!   - Disk usage accounting
//!   - Path canonicalization (symlink-following)
//!   - openat/mkdirat/*at family (AT_FDCWD)
//!
//! All operations are Unix-style: no Windows paths, no drive letters, no backslashes.
mod constants;
mod copy;
mod detect;
mod files;
mod find;
mod links;
mod listing;
mod lock;
mod meta;
mod perms;
mod preview;
mod remove;
mod resolve;
mod stat;
mod trash;
mod ui;
mod usage;

pub use constants::*;
pub use copy::*;
pub use detect::*;
pub use files::*;
pub use find::*;
pub use links::*;
pub use listing::*;
pub use lock::*;
pub use meta::{InodeMeta, get_umask, set_base_time, set_umask};
pub use perms::*;
pub use preview::*;
pub use remove::*;
pub use resolve::*;
pub use stat::*;
pub use trash::*;
pub use ui::*;
pub use usage::*;

use crate::serial_println;
use crate::vfs::{FileType, VFS};

use meta::INODE_META;

/// Initialize the file manager subsystem.
pub fn init() {
    // Sync with RTC if available
    let epoch = crate::rtc::unix_time();
    set_base_time(epoch as u64);

    // Create metadata for all existing VFS inodes
    {
        let vfs = VFS.lock();
        let mut meta_store = INODE_META.lock();
        for inode in &vfs.inodes {
            meta_store.entry(inode.ino).or_insert_with(|| {
                if inode.file_type == FileType::Directory {
                    InodeMeta::for_directory()
                } else {
                    InodeMeta::new()
                }
            });
        }
        serial_println!(
            "[KnoxOS] File manager initialized: {} inodes tracked, umask={:04o}",
            meta_store.len(),
            get_umask()
        );
    }

    serial_println!(
        "[KnoxOS] File manager subsystem ready (POSIX paths, symlinks, timestamps, locks)"
    );
}
