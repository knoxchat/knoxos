/// Ext4 Filesystem Driver — Linux-compatible ext4 with journal and extents
///
/// Extends ext2 with:
///   - Extent-based block mapping (instead of indirect blocks)
///   - Journal (JBD2) for crash recovery
///   - Large file support (>2GB)
///   - Dir hashing (HTree)
///   - Delayed allocation
///   - 64-bit block numbers
///
/// Compatible with Linux ext4 on-disk format.
///
/// Module layout:
///   types   — on-disk structs, constants, in-memory state, errors
///   io      — block read/write
///   inode   — inode read/write and bitmap allocation
///   extent  — extent tree walk and inode data mapping
///   dir     — directory entries, path lookup, stat
///   journal — JBD2 init, commit, and replay
///   ops     — mount and public file operations
use crate::serial_println;

mod dir;
mod extent;
mod inode;
mod io;
mod journal;
mod ops;
mod types;

pub use ops::{
    create_file, list_dir, list_mounts, mkdir, mount, read_file, stat_file, sync_all, write_file,
};
pub use types::{DirEntry, Ext4Error, FileInfo};

/// Initialize ext4 driver
pub fn init() {
    serial_println!("[KnoxOS] ext4 filesystem driver loaded (journal + extents)");
}
