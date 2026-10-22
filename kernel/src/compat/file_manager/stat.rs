//! Linux-compatible `struct stat` / `statfs` and timestamp formatting.
use alloc::string::String;

use crate::vfs::{FileType, Inode, VFS};

use super::constants::*;
use super::meta::{InodeMeta, get_or_create_meta, touch_atime};
use super::resolve::{resolve_path_follow, resolve_path_nofollow};

/// Linux-compatible stat structure.
#[derive(Debug, Clone)]
pub struct Stat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub st_size: u64,
    pub st_blksize: u64,
    pub st_blocks: u64,
    pub st_atime: u64,
    pub st_atime_nsec: u64,
    pub st_mtime: u64,
    pub st_mtime_nsec: u64,
    pub st_ctime: u64,
    pub st_ctime_nsec: u64,
}

impl Stat {
    /// Build stat from VFS inode + metadata.
    pub fn from_inode(inode: &Inode, meta: &InodeMeta) -> Self {
        let mode_type = match inode.file_type {
            FileType::Regular => S_IFREG,
            FileType::Directory => S_IFDIR,
            FileType::SymLink => S_IFLNK,
            FileType::CharDevice => S_IFCHR,
            FileType::BlockDevice => S_IFBLK,
            FileType::Pipe => S_IFIFO,
            FileType::Socket => S_IFSOCK,
        };

        Self {
            st_dev: meta.dev,
            st_ino: inode.ino,
            st_mode: mode_type | (inode.permissions as u32),
            st_nlink: meta.nlink,
            st_uid: inode.uid,
            st_gid: inode.gid,
            st_rdev: ((meta.dev_major as u64) << 20) | (meta.dev_minor as u64),
            st_size: inode.size,
            st_blksize: meta.blksize,
            st_blocks: meta.blocks.max(inode.size.div_ceil(512)),
            st_atime: meta.atime,
            st_atime_nsec: 0,
            st_mtime: meta.mtime,
            st_mtime_nsec: 0,
            st_ctime: meta.ctime,
            st_ctime_nsec: 0,
        }
    }

    /// Pretty format (like `stat` command output)
    pub fn display(&self, path: &str) -> String {
        let ftype = match self.st_mode & S_IFMT {
            S_IFREG => "regular file",
            S_IFDIR => "directory",
            S_IFLNK => "symbolic link",
            S_IFCHR => "character special file",
            S_IFBLK => "block special file",
            S_IFIFO => "fifo (named pipe)",
            S_IFSOCK => "socket",
            _ => "unknown",
        };

        let perm_bits = (self.st_mode & 0o7777) as u16;
        let perm_str = crate::shell::helpers::format_permissions(perm_bits);

        let type_char = match self.st_mode & S_IFMT {
            S_IFDIR => 'd',
            S_IFLNK => 'l',
            S_IFCHR => 'c',
            S_IFBLK => 'b',
            S_IFIFO => 'p',
            S_IFSOCK => 's',
            _ => '-',
        };

        alloc::format!(
            "  File: {}\n  Size: {:<15} Blocks: {:<10} IO Block: {} {}\n\
             Device: {}  Inode: {:<12} Links: {}\n\
             Access: ({:04o}/{}{})\tUid: ({:>5})\tGid: ({:>5})\n\
             Access: {}\nModify: {}\nChange: {}\n Birth: {}",
            path,
            self.st_size,
            self.st_blocks,
            self.st_blksize,
            ftype,
            self.st_dev,
            self.st_ino,
            self.st_nlink,
            perm_bits,
            type_char,
            perm_str,
            self.st_uid,
            self.st_gid,
            format_timestamp(self.st_atime),
            format_timestamp(self.st_mtime),
            format_timestamp(self.st_ctime),
            format_timestamp(self.st_atime), // btime approximated
        )
    }
}

/// Format a Unix timestamp as a human-readable string.
fn format_timestamp(epoch: u64) -> String {
    if epoch == 0 {
        return String::from("0000-00-00 00:00:00.000000000 +0000");
    }
    // Simple epoch to date conversion
    let secs = epoch;
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Approximate year/month/day from days since epoch (1970-01-01)
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = is_leap_year(y);
    let month_days = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0u32;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining < md {
            m = i as u32 + 1;
            break;
        }
        remaining -= md;
    }
    if m == 0 {
        m = 12;
    }
    let d = remaining as u32 + 1;

    alloc::format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.000000000 +0000",
        y,
        m,
        d,
        hours,
        minutes,
        seconds
    )
}

fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

// ─── stat / lstat ───────────────────────────────────────────────────

/// stat() — get file status, following symlinks.
pub fn stat(path: &str) -> Result<Stat, i32> {
    let (_, ino) = resolve_path_follow(path)?;
    stat_by_ino(ino)
}

/// lstat() — get file status, NOT following the final symlink.
pub fn lstat(path: &str) -> Result<Stat, i32> {
    let (_, ino) = resolve_path_nofollow(path)?;
    stat_by_ino(ino)
}

/// fstat() — stat by inode number (for open file descriptors).
pub fn stat_by_ino(ino: u64) -> Result<Stat, i32> {
    let vfs = VFS.lock();
    let inode = vfs.get_inode(ino).ok_or(ENOENT)?;
    let meta = get_or_create_meta(ino);
    touch_atime(ino);
    Ok(Stat::from_inode(inode, &meta))
}

// ─── Filesystem info ────────────────────────────────────────────────

/// Filesystem statistics (like `statfs(2)`).
#[derive(Debug, Clone)]
pub struct StatFs {
    pub f_type: u64,
    pub f_bsize: u64,
    pub f_blocks: u64,
    pub f_bfree: u64,
    pub f_bavail: u64,
    pub f_files: u64,
    pub f_ffree: u64,
    pub f_namelen: u64,
    pub f_frsize: u64,
}

/// statfs() — get filesystem statistics.
pub fn statfs(path: &str) -> Result<StatFs, i32> {
    let vfs = VFS.lock();
    let _ = vfs.resolve_path(path).ok_or(ENOENT)?;

    let total_inodes = vfs.inode_count() as u64;
    let heap_total = crate::allocator::HEAP_SIZE as u64;
    let block_size = 4096u64;
    let total_blocks = heap_total / block_size;

    Ok(StatFs {
        f_type: 0x01021994, // TMPFS_MAGIC
        f_bsize: block_size,
        f_blocks: total_blocks,
        f_bfree: total_blocks / 2, // Approximate
        f_bavail: total_blocks / 2,
        f_files: total_inodes,
        f_ffree: 1_000_000 - total_inodes,
        f_namelen: NAME_MAX as u64,
        f_frsize: block_size,
    })
}
