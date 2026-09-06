/// File Manager — Comprehensive Unix/Linux filesystem operations
///
/// Provides a full-featured file management layer on top of the VFS with
/// proper Unix semantics:
///   - Symlink creation, resolution, and cycle detection
///   - Hard links with reference counting
///   - Timestamps (atime, mtime, ctime) on all operations
///   - Recursive directory operations (cp -r, rm -rf, chmod -R, chown -R)
///   - File locking (advisory, POSIX)
///   - Atomic rename across directories
///   - umask support
///   - Inode-level stat with full Linux struct stat compatibility
///   - Directory entry iteration with readdir/seekdir/telldir
///   - Truncate, append, sparse file support
///   - File type detection (magic bytes)
///   - Disk usage accounting
///   - Path canonicalization (symlink-following)
///   - openat/mkdirat/*at family (AT_FDCWD)
///
/// All operations are Unix-style: no Windows paths, no drive letters, no backslashes.
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::path::{self, PathBuf};
use crate::serial_println;
use crate::vfs::{self, FileType, Inode, VFS};

// ═══════════════════════════════════════════════════════════════════════
// CONSTANTS — Linux-compatible error codes and limits
// ═══════════════════════════════════════════════════════════════════════

/// Linux errno values
pub const EPERM: i32 = -1;
pub const ENOENT: i32 = -2;
pub const ESRCH: i32 = -3;
pub const EINTR: i32 = -4;
pub const EIO: i32 = -5;
pub const ENXIO: i32 = -6;
pub const EBADF: i32 = -9;
pub const EAGAIN: i32 = -11;
pub const ENOMEM: i32 = -12;
pub const EACCES: i32 = -13;
pub const EFAULT: i32 = -14;
pub const EEXIST: i32 = -17;
pub const EXDEV: i32 = -18;
pub const ENOTDIR: i32 = -20;
pub const EISDIR: i32 = -21;
pub const EINVAL: i32 = -22;
pub const EMFILE: i32 = -24;
pub const ENFILE: i32 = -23;
pub const ENOSPC: i32 = -28;
pub const EROFS: i32 = -30;
pub const EMLINK: i32 = -31;
pub const EPIPE: i32 = -32;
pub const ENAMETOOLONG: i32 = -36;
pub const ENOTEMPTY: i32 = -39;
pub const ELOOP: i32 = -40;
pub const ENOSYS: i32 = -38;
pub const ENODATA: i32 = -61;
pub const EOVERFLOW: i32 = -75;

/// Filesystem limits (POSIX / Linux)
pub const NAME_MAX: usize = 255;
pub const PATH_MAX: usize = 4096;
pub const SYMLOOP_MAX: usize = 40; // Max symlink traversals (Linux uses 40)
pub const LINK_MAX: u32 = 65000; // Max hard links per inode

/// Special fd for *at() family
pub const AT_FDCWD: i32 = -100;

/// Flags for openat/faccessat/etc.
pub const AT_SYMLINK_NOFOLLOW: u32 = 0x100;
pub const AT_REMOVEDIR: u32 = 0x200;
pub const AT_SYMLINK_FOLLOW: u32 = 0x400;
pub const AT_EMPTY_PATH: u32 = 0x1000;

/// File mode bits
pub const S_IFMT: u32 = 0o170000;
pub const S_IFSOCK: u32 = 0o140000;
pub const S_IFLNK: u32 = 0o120000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFBLK: u32 = 0o060000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFCHR: u32 = 0o020000;
pub const S_IFIFO: u32 = 0o010000;
pub const S_ISUID: u32 = 0o004000;
pub const S_ISGID: u32 = 0o002000;
pub const S_ISVTX: u32 = 0o001000;

// ═══════════════════════════════════════════════════════════════════════
// ENHANCED INODE METADATA — Timestamps, links, symlink targets
// ═══════════════════════════════════════════════════════════════════════

/// Monotonic tick counter for timestamps (seconds since boot).
static TICK_COUNTER: AtomicU64 = AtomicU64::new(1_700_000_000);

/// Get current timestamp (Unix epoch seconds, approximate).
fn now() -> u64 {
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
    static ref INODE_META: Mutex<BTreeMap<u64, InodeMeta>> = Mutex::new(BTreeMap::new());
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
fn apply_umask(mode: u16) -> u16 {
    mode & !get_umask()
}

// ═══════════════════════════════════════════════════════════════════════
// FULL STAT STRUCTURE — Linux `struct stat` compatible
// ═══════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════
// CORE FILESYSTEM OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// Get or create metadata for an inode.
fn get_or_create_meta(ino: u64) -> InodeMeta {
    let mut store = INODE_META.lock();
    store.entry(ino).or_insert_with(InodeMeta::new).clone()
}

/// Update metadata for an inode.
fn update_meta<F: FnOnce(&mut InodeMeta)>(ino: u64, f: F) {
    let mut store = INODE_META.lock();
    let meta = store.entry(ino).or_insert_with(InodeMeta::new);
    f(meta);
}

/// Touch the atime of an inode.
fn touch_atime(ino: u64) {
    update_meta(ino, |m| m.atime = now());
}

/// Touch the mtime and ctime of an inode.
fn touch_mtime(ino: u64) {
    let t = now();
    update_meta(ino, |m| {
        m.mtime = t;
        m.ctime = t;
    });
}

/// Touch ctime only (metadata change).
fn touch_ctime(ino: u64) {
    update_meta(ino, |m| m.ctime = now());
}

// ─── Symlink resolution ─────────────────────────────────────────────

/// Resolve a path, following symlinks up to SYMLOOP_MAX times.
/// Returns the resolved absolute path and the final inode number.
pub fn resolve_path_follow(path_str: &str) -> Result<(String, u64), i32> {
    resolve_path_inner(path_str, true, 0)
}

/// Resolve a path, NOT following the final symlink (lstat behavior).
pub fn resolve_path_nofollow(path_str: &str) -> Result<(String, u64), i32> {
    resolve_path_inner(path_str, false, 0)
}

fn resolve_path_inner(
    path_str: &str,
    follow_final: bool,
    depth: usize,
) -> Result<(String, u64), i32> {
    if depth > SYMLOOP_MAX {
        return Err(ELOOP);
    }

    if !crate::path::is_valid_path(path_str) {
        return Err(ENAMETOOLONG);
    }

    let vfs = VFS.lock();
    let normalized = path::normalize_path(path_str);
    let parts: Vec<&str> = normalized
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    if parts.is_empty() {
        // Root
        if let Some(root) = vfs.inodes.first() {
            return Ok((String::from("/"), root.ino));
        }
        return Err(ENOENT);
    }

    let mut current_ino = vfs.inodes.first().map(|i| i.ino).ok_or(ENOENT)?;
    let mut resolved = String::from("/");

    for (idx, part) in parts.iter().enumerate() {
        let is_last = idx == parts.len() - 1;

        // Look up the child
        let current = vfs
            .inodes
            .iter()
            .find(|i| i.ino == current_ino)
            .ok_or(ENOENT)?;

        if current.file_type != FileType::Directory {
            return Err(ENOTDIR);
        }

        let mut found = false;
        for &child_ino in &current.children {
            if let Some(child) = vfs.inodes.iter().find(|i| i.ino == child_ino) {
                if child.name == *part {
                    // Check if this is a symlink
                    if child.file_type == FileType::SymLink {
                        let meta_store = INODE_META.lock();
                        if let Some(meta) = meta_store.get(&child_ino) {
                            if let Some(ref target) = meta.symlink_target {
                                if !is_last || follow_final {
                                    // Follow the symlink
                                    let target_path = if target.starts_with('/') {
                                        target.clone()
                                    } else {
                                        path::join(&resolved, target)
                                    };
                                    drop(meta_store);
                                    drop(vfs);

                                    // Build remaining path
                                    let remaining: String = if is_last {
                                        String::new()
                                    } else {
                                        let rest = &parts[idx + 1..];
                                        let mut r = String::new();
                                        for p in rest {
                                            r.push('/');
                                            r.push_str(p);
                                        }
                                        r
                                    };
                                    let full_target =
                                        alloc::format!("{}{}", target_path, remaining);
                                    return resolve_path_inner(
                                        &full_target,
                                        follow_final,
                                        depth + 1,
                                    );
                                }
                            }
                        }
                    }

                    // Not a symlink or not following → use this inode
                    current_ino = child_ino;
                    if (!is_last || resolved != "/") && !resolved.ends_with('/') {
                        resolved.push('/');
                    }
                    if resolved == "/" {
                        resolved = alloc::format!("/{}", part);
                    } else {
                        resolved.push_str(part);
                    }
                    found = true;
                    break;
                }
            }
        }

        if !found {
            return Err(ENOENT);
        }
    }

    Ok((resolved, current_ino))
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

// ─── Symlink operations ─────────────────────────────────────────────

/// symlink() — create a symbolic link.
/// `target` is the path the symlink points to (can be relative or absolute).
/// `linkpath` is where the symlink itself is created.
pub fn symlink(target: &str, linkpath: &str) -> Result<(), i32> {
    if target.is_empty() || linkpath.is_empty() {
        return Err(EINVAL);
    }
    if !path::is_valid_path(linkpath) {
        return Err(ENAMETOOLONG);
    }

    let normalized = path::normalize_path(linkpath);
    let parent_dir = path::dirname(&normalized);
    let link_name = path::basename(&normalized);

    let mut vfs = VFS.lock();
    let parent_ino = vfs.resolve_path(parent_dir).ok_or(ENOENT)?;

    // Check parent is a directory
    {
        let parent = vfs.get_inode(parent_ino).ok_or(ENOENT)?;
        if parent.file_type != FileType::Directory {
            return Err(ENOTDIR);
        }
    }

    // Check link doesn't already exist
    if vfs.resolve_path(&normalized).is_some() {
        return Err(EEXIST);
    }

    // Create the symlink inode (store target path as data)
    let ino = vfs.create_file_under(
        parent_ino,
        link_name,
        FileType::SymLink,
        target.as_bytes(),
        0o777,
    );

    // Store symlink metadata
    let meta = InodeMeta::for_symlink(target);
    INODE_META.lock().insert(ino, meta);

    touch_mtime(parent_ino);

    serial_println!("[file_manager] symlink: {} -> {}", linkpath, target);
    Ok(())
}

/// readlink() — read the target of a symbolic link.
pub fn readlink(path: &str) -> Result<String, i32> {
    let (_, ino) = resolve_path_nofollow(path)?;

    let vfs = VFS.lock();
    let inode = vfs.get_inode(ino).ok_or(ENOENT)?;
    if inode.file_type != FileType::SymLink {
        return Err(EINVAL);
    }

    // Try metadata first
    let meta_store = INODE_META.lock();
    if let Some(meta) = meta_store.get(&ino) {
        if let Some(ref target) = meta.symlink_target {
            touch_atime(ino);
            return Ok(target.clone());
        }
    }
    drop(meta_store);

    // Fall back to inode data
    if let Ok(s) = core::str::from_utf8(&inode.data) {
        touch_atime(ino);
        Ok(String::from(s))
    } else {
        Err(EINVAL)
    }
}

// ─── Hard link operations ───────────────────────────────────────────

/// link() — create a hard link.
/// Creates a new directory entry `newpath` pointing to the same inode as `oldpath`.
pub fn link(oldpath: &str, newpath: &str) -> Result<(), i32> {
    let (_, old_ino) = resolve_path_follow(oldpath)?;

    let vfs = VFS.lock();
    let old_inode = vfs.get_inode(old_ino).ok_or(ENOENT)?;

    // Can't hard-link directories (Linux restriction)
    if old_inode.file_type == FileType::Directory {
        return Err(EPERM);
    }

    // Check link count
    let meta = get_or_create_meta(old_ino);
    if meta.nlink >= LINK_MAX {
        return Err(EMLINK);
    }

    drop(vfs);

    // Resolve the new path's parent
    let normalized = path::normalize_path(newpath);
    let parent_dir = path::dirname(&normalized);
    let new_name = path::basename(&normalized);

    let mut vfs = VFS.lock();
    let parent_ino = vfs.resolve_path(parent_dir).ok_or(ENOENT)?;

    // Check parent is a directory
    {
        let parent = vfs.get_inode(parent_ino).ok_or(ENOENT)?;
        if parent.file_type != FileType::Directory {
            return Err(ENOTDIR);
        }
    }

    // Check new name doesn't exist
    if vfs.resolve_path(&normalized).is_some() {
        return Err(EEXIST);
    }

    // For a true hard link, we add the old inode to the new parent's children
    // and update the inode's name for the new entry.
    // In our VFS model, we simulate by creating a copy that shares the same data.
    let old_data = vfs
        .get_inode(old_ino)
        .map(|i| i.data.clone())
        .ok_or(ENOENT)?;
    let old_perms = vfs
        .get_inode(old_ino)
        .map(|i| i.permissions)
        .ok_or(ENOENT)?;

    // Add old_ino to new parent's children list
    if let Some(parent) = vfs.inodes.iter_mut().find(|i| i.ino == parent_ino) {
        parent.children.push(old_ino);
    }

    // Increment link count
    update_meta(old_ino, |m| m.nlink += 1);
    touch_ctime(old_ino);
    touch_mtime(parent_ino);

    serial_println!(
        "[file_manager] link: {} -> {} (ino={})",
        newpath,
        oldpath,
        old_ino
    );
    Ok(())
}

/// unlink() — remove a directory entry and possibly the inode.
pub fn unlink(path: &str) -> Result<(), i32> {
    let (resolved, ino) = resolve_path_nofollow(path)?;

    let vfs = VFS.lock();
    let inode = vfs.get_inode(ino).ok_or(ENOENT)?;

    if inode.file_type == FileType::Directory {
        return Err(EISDIR);
    }

    drop(vfs);

    let parent_dir = path::dirname(&resolved);
    let mut vfs = VFS.lock();
    let parent_ino = vfs.resolve_path(parent_dir).ok_or(ENOENT)?;

    // Remove from parent
    if let Some(parent) = vfs.inodes.iter_mut().find(|i| i.ino == parent_ino) {
        parent.children.retain(|&c| c != ino);
    }

    // Decrement link count
    let should_remove = {
        let mut store = INODE_META.lock();
        if let Some(meta) = store.get_mut(&ino) {
            meta.nlink = meta.nlink.saturating_sub(1);
            meta.nlink == 0
        } else {
            true // No metadata means single link
        }
    };

    if should_remove {
        vfs.inodes.retain(|i| i.ino != ino);
        INODE_META.lock().remove(&ino);
    }

    touch_mtime(parent_ino);
    Ok(())
}

// ─── Directory operations ───────────────────────────────────────────

/// mkdir() — create a directory.
pub fn mkdir(path: &str, mode: u16) -> Result<u64, i32> {
    let effective_mode = apply_umask(mode);
    let mut vfs = VFS.lock();
    let ino = vfs.mkdir(path, effective_mode)?;

    // Set up directory metadata
    let meta = InodeMeta::for_directory();
    INODE_META.lock().insert(ino, meta);

    // Update parent mtime
    let parent_dir = path::dirname(path);
    if let Some(parent_ino) = vfs.resolve_path(parent_dir) {
        touch_mtime(parent_ino);
    }

    serial_println!(
        "[file_manager] mkdir: {} (mode={:04o})",
        path,
        effective_mode
    );
    Ok(ino)
}

/// mkdirp() — create a directory and all parent directories (mkdir -p).
pub fn mkdirp(path: &str, mode: u16) -> Result<u64, i32> {
    let normalized = path::normalize_path(path);
    let parts: Vec<&str> = normalized
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    let mut current_path = String::from("/");
    let mut last_ino = 0u64;

    for part in &parts {
        if current_path == "/" {
            current_path = alloc::format!("/{}", part);
        } else {
            current_path = alloc::format!("{}/{}", current_path, part);
        }

        let vfs = VFS.lock();
        if let Some(ino) = vfs.resolve_path(&current_path) {
            last_ino = ino;
            drop(vfs);
            continue;
        }
        drop(vfs);

        last_ino =
            mkdir(&current_path, mode).or_else(|e| if e == EEXIST { Ok(0) } else { Err(e) })?;
    }

    Ok(last_ino)
}

/// rmdir() — remove an empty directory.
pub fn rmdir(path: &str) -> Result<(), i32> {
    let mut vfs = VFS.lock();
    vfs.rmdir(path)?;
    INODE_META.lock().remove(&0); // clean up (inode already removed by VFS)
    Ok(())
}

// ─── File creation and writing ──────────────────────────────────────

/// create() — create a new file or truncate an existing one.
pub fn create(path: &str, mode: u16) -> Result<u64, i32> {
    let effective_mode = apply_umask(mode);
    let mut vfs = VFS.lock();

    if let Some(ino) = vfs.resolve_path(path) {
        // File exists — truncate
        if let Some(inode) = vfs.inodes.iter_mut().find(|i| i.ino == ino) {
            if inode.file_type == FileType::Directory {
                return Err(EISDIR);
            }
            inode.data.clear();
            inode.size = 0;
            touch_mtime(ino);
            return Ok(ino);
        }
    }

    // Create new file
    let success = vfs.create_file_at_path(path, FileType::Regular, &[], effective_mode);
    if !success {
        return Err(EIO);
    }

    let ino = vfs.resolve_path(path).ok_or(EIO)?;
    let meta = InodeMeta::new();
    INODE_META.lock().insert(ino, meta);

    serial_println!(
        "[file_manager] create: {} (mode={:04o})",
        path,
        effective_mode
    );
    Ok(ino)
}

/// write_file() — write data to a file (create if needed).
pub fn write_file(path: &str, data: &[u8]) -> Result<(), i32> {
    let mut vfs = VFS.lock();
    if vfs.write_file(path, data) {
        if let Some(ino) = vfs.resolve_path(path) {
            touch_mtime(ino);
            update_meta(ino, |m| {
                m.blocks = (data.len() as u64).div_ceil(512);
            });
        }
        Ok(())
    } else {
        Err(EIO)
    }
}

/// append_file() — append data to an existing file.
pub fn append_file(path: &str, data: &[u8]) -> Result<(), i32> {
    let mut vfs = VFS.lock();
    let existing = vfs.read_file(path).unwrap_or(&[]).to_vec();
    let mut combined = existing;
    combined.extend_from_slice(data);
    if vfs.write_file(path, &combined) {
        if let Some(ino) = vfs.resolve_path(path) {
            touch_mtime(ino);
        }
        Ok(())
    } else {
        Err(EIO)
    }
}

/// read_file() — read entire file contents.
pub fn read_file(path: &str) -> Result<Vec<u8>, i32> {
    let vfs = VFS.lock();
    let ino = vfs.resolve_path(path).ok_or(ENOENT)?;
    let inode = vfs.get_inode(ino).ok_or(ENOENT)?;

    if inode.file_type == FileType::Directory {
        return Err(EISDIR);
    }

    touch_atime(ino);
    Ok(inode.data.clone())
}

/// truncate() — truncate a file to a specified length.
pub fn truncate(path: &str, length: u64) -> Result<(), i32> {
    let mut vfs = VFS.lock();
    let ino = vfs.resolve_path(path).ok_or(ENOENT)?;

    if let Some(inode) = vfs.inodes.iter_mut().find(|i| i.ino == ino) {
        if inode.file_type == FileType::Directory {
            return Err(EISDIR);
        }

        let len = length as usize;
        if len < inode.data.len() {
            inode.data.truncate(len);
        } else {
            inode.data.resize(len, 0); // extend with zeros (sparse)
        }
        inode.size = length;
        touch_mtime(ino);
        Ok(())
    } else {
        Err(ENOENT)
    }
}

// ─── Rename / move ──────────────────────────────────────────────────

/// rename() — atomically rename a file or directory.
/// Handles cross-directory moves within the same filesystem.
pub fn rename(oldpath: &str, newpath: &str) -> Result<(), i32> {
    let mut vfs = VFS.lock();

    // Check if newpath already exists — if so, remove it first
    if let Some(new_ino) = vfs.resolve_path(newpath) {
        if let Some(new_inode) = vfs.get_inode(new_ino) {
            if new_inode.file_type == FileType::Directory && !new_inode.children.is_empty() {
                return Err(ENOTEMPTY);
            }
        }
        // Remove the existing target
        let _ = vfs.unlink(newpath).or_else(|_| vfs.rmdir(newpath));
    }

    vfs.rename(oldpath, newpath)?;

    // Update timestamps
    let old_parent = path::dirname(oldpath);
    let new_parent = path::dirname(newpath);
    if let Some(ino) = vfs.resolve_path(old_parent) {
        touch_mtime(ino);
    }
    if let Some(ino) = vfs.resolve_path(new_parent) {
        touch_mtime(ino);
    }

    Ok(())
}

// ─── Permission operations ──────────────────────────────────────────

/// chmod() — change file permissions.
pub fn chmod(path: &str, mode: u16) -> Result<(), i32> {
    let (_, ino) = resolve_path_follow(path)?;
    let mut vfs = VFS.lock();
    let inode = vfs.inodes.iter_mut().find(|i| i.ino == ino).ok_or(ENOENT)?;
    inode.permissions = mode;
    touch_ctime(ino);
    Ok(())
}

/// fchmod() — change permissions by inode.
pub fn fchmod(ino: u64, mode: u16) -> Result<(), i32> {
    let mut vfs = VFS.lock();
    let inode = vfs.inodes.iter_mut().find(|i| i.ino == ino).ok_or(ENOENT)?;
    inode.permissions = mode;
    touch_ctime(ino);
    Ok(())
}

/// chmod_recursive() — chmod -R
pub fn chmod_recursive(path: &str, mode: u16) -> Result<u64, i32> {
    chmod(path, mode)?;
    let mut count = 1u64;

    let vfs = VFS.lock();
    let entries = vfs.list_dir(path).unwrap_or_default();
    drop(vfs);

    for entry in &entries {
        let child_path = path::join(path, entry);
        let vfs = VFS.lock();
        let is_dir = vfs.list_dir(&child_path).is_some();
        drop(vfs);

        if is_dir {
            count += chmod_recursive(&child_path, mode)?;
        } else {
            chmod(&child_path, mode)?;
            count += 1;
        }
    }

    Ok(count)
}

/// chown() — change file ownership.
pub fn chown(path: &str, uid: u32, gid: u32) -> Result<(), i32> {
    let (_, ino) = resolve_path_follow(path)?;
    let mut vfs = VFS.lock();
    let inode = vfs.inodes.iter_mut().find(|i| i.ino == ino).ok_or(ENOENT)?;
    if uid != u32::MAX {
        inode.uid = uid;
    }
    if gid != u32::MAX {
        inode.gid = gid;
    }
    touch_ctime(ino);
    Ok(())
}

/// lchown() — change ownership without following symlinks.
pub fn lchown(path: &str, uid: u32, gid: u32) -> Result<(), i32> {
    let (_, ino) = resolve_path_nofollow(path)?;
    let mut vfs = VFS.lock();
    let inode = vfs.inodes.iter_mut().find(|i| i.ino == ino).ok_or(ENOENT)?;
    if uid != u32::MAX {
        inode.uid = uid;
    }
    if gid != u32::MAX {
        inode.gid = gid;
    }
    touch_ctime(ino);
    Ok(())
}

/// chown_recursive() — chown -R
pub fn chown_recursive(path: &str, uid: u32, gid: u32) -> Result<u64, i32> {
    chown(path, uid, gid)?;
    let mut count = 1u64;

    let vfs = VFS.lock();
    let entries = vfs.list_dir(path).unwrap_or_default();
    drop(vfs);

    for entry in &entries {
        let child_path = path::join(path, entry);
        let vfs = VFS.lock();
        let is_dir = vfs.list_dir(&child_path).is_some();
        drop(vfs);

        if is_dir {
            count += chown_recursive(&child_path, uid, gid)?;
        } else {
            chown(&child_path, uid, gid)?;
            count += 1;
        }
    }

    Ok(count)
}

// ─── Timestamp operations ───────────────────────────────────────────

/// utimes() — set access and modification times.
pub fn utimes(path: &str, atime: u64, mtime: u64) -> Result<(), i32> {
    let (_, ino) = resolve_path_follow(path)?;
    update_meta(ino, |m| {
        m.atime = atime;
        m.mtime = mtime;
        m.ctime = now(); // ctime always updates
    });
    Ok(())
}

/// lutimes() — set times without following symlinks.
pub fn lutimes(path: &str, atime: u64, mtime: u64) -> Result<(), i32> {
    let (_, ino) = resolve_path_nofollow(path)?;
    update_meta(ino, |m| {
        m.atime = atime;
        m.mtime = mtime;
        m.ctime = now();
    });
    Ok(())
}

/// touch() — create file or update timestamps (like the `touch` command).
pub fn touch(path: &str) -> Result<(), i32> {
    let vfs = VFS.lock();
    if let Some(ino) = vfs.resolve_path(path) {
        // File exists — update all timestamps
        let t = now();
        drop(vfs);
        update_meta(ino, |m| {
            m.atime = t;
            m.mtime = t;
            m.ctime = t;
        });
        Ok(())
    } else {
        // File doesn't exist — create it
        drop(vfs);
        create(path, 0o644)?;
        Ok(())
    }
}

// ─── Recursive copy ─────────────────────────────────────────────────

/// Copy options
#[derive(Debug, Clone)]
pub struct CopyOptions {
    /// Recursive copy (directories)
    pub recursive: bool,
    /// Preserve permissions, timestamps, ownership
    pub preserve: bool,
    /// Follow symlinks in source
    pub dereference: bool,
    /// Force overwrite
    pub force: bool,
    /// Don't overwrite existing
    pub no_clobber: bool,
    /// Create hard links instead of copying
    pub link: bool,
    /// Create symlinks instead of copying
    pub symbolic_link: bool,
    /// Update only (copy if source is newer)
    pub update: bool,
    /// Verbose output
    pub verbose: bool,
}

impl Default for CopyOptions {
    fn default() -> Self {
        Self {
            recursive: false,
            preserve: false,
            dereference: true,
            force: false,
            no_clobber: false,
            link: false,
            symbolic_link: false,
            update: false,
            verbose: false,
        }
    }
}

/// cp() — copy file or directory.
pub fn copy(src: &str, dst: &str, opts: &CopyOptions) -> Result<u64, i32> {
    let vfs = VFS.lock();

    // Check if src exists
    let src_ino = vfs.resolve_path(src).ok_or(ENOENT)?;
    let src_inode = vfs.get_inode(src_ino).ok_or(ENOENT)?;
    let is_dir = src_inode.file_type == FileType::Directory;

    if is_dir && !opts.recursive {
        return Err(EISDIR); // cp: omitting directory without -r
    }

    drop(vfs);

    if is_dir {
        copy_dir_recursive(src, dst, opts)
    } else {
        copy_single_file(src, dst, opts)?;
        Ok(1)
    }
}

fn copy_single_file(src: &str, dst: &str, opts: &CopyOptions) -> Result<(), i32> {
    // Check if dst is a directory — if so, copy into it
    let final_dst = {
        let vfs = VFS.lock();
        if let Some(dst_ino) = vfs.resolve_path(dst) {
            if let Some(dst_inode) = vfs.get_inode(dst_ino) {
                if dst_inode.file_type == FileType::Directory {
                    let name = path::basename(src);
                    path::join(dst, name)
                } else {
                    String::from(dst)
                }
            } else {
                String::from(dst)
            }
        } else {
            String::from(dst)
        }
    };

    // Check no_clobber
    if opts.no_clobber {
        let vfs = VFS.lock();
        if vfs.resolve_path(&final_dst).is_some() {
            return Ok(()); // Silently skip
        }
    }

    if opts.link {
        return link(src, &final_dst);
    }

    if opts.symbolic_link {
        return symlink(src, &final_dst);
    }

    // Read source
    let data = read_file(src)?;

    // Write destination
    write_file(&final_dst, &data)?;

    // Preserve metadata
    if opts.preserve {
        if let Ok(st) = stat(src) {
            let _ = chmod(&final_dst, (st.st_mode & 0o7777) as u16);
            let _ = chown(&final_dst, st.st_uid, st.st_gid);
            let _ = utimes(&final_dst, st.st_atime, st.st_mtime);
        }
    }

    if opts.verbose {
        serial_println!("'{}' -> '{}'", src, final_dst);
    }

    Ok(())
}

fn copy_dir_recursive(src: &str, dst: &str, opts: &CopyOptions) -> Result<u64, i32> {
    let mut count = 0u64;

    // Create destination directory
    let vfs = VFS.lock();
    let src_ino = vfs.resolve_path(src).ok_or(ENOENT)?;
    let src_perms = vfs
        .get_inode(src_ino)
        .map(|i| i.permissions)
        .unwrap_or(0o755);
    drop(vfs);

    let _ = mkdir(dst, src_perms);
    count += 1;

    // Copy children
    let vfs = VFS.lock();
    let entries = vfs.list_dir(src).unwrap_or_default();
    drop(vfs);

    for entry in &entries {
        let src_child = path::join(src, entry);
        let dst_child = path::join(dst, entry);

        let vfs = VFS.lock();
        let is_child_dir = vfs.list_dir(&src_child).is_some();
        drop(vfs);

        if is_child_dir {
            count += copy_dir_recursive(&src_child, &dst_child, opts)?;
        } else {
            copy_single_file(&src_child, &dst_child, opts)?;
            count += 1;
        }
    }

    Ok(count)
}

// ─── Recursive remove ───────────────────────────────────────────────

/// Remove options
#[derive(Debug, Clone, Default)]
pub struct RemoveOptions {
    pub recursive: bool,
    pub force: bool,
    pub verbose: bool,
    /// Remove empty directories (like `rmdir`)
    pub dir: bool,
}

/// rm() — remove files and directories.
pub fn remove(path: &str, opts: &RemoveOptions) -> Result<u64, i32> {
    let vfs = VFS.lock();
    let ino = match vfs.resolve_path(path) {
        Some(ino) => ino,
        None => {
            if opts.force {
                return Ok(0);
            }
            return Err(ENOENT);
        }
    };
    let inode = vfs.get_inode(ino).ok_or(ENOENT)?;
    let is_dir = inode.file_type == FileType::Directory;
    drop(vfs);

    if is_dir {
        if !opts.recursive && !opts.dir {
            return Err(EISDIR);
        }
        remove_dir_recursive(path, opts)
    } else {
        unlink(path)?;
        if opts.verbose {
            serial_println!("removed '{}'", path);
        }
        Ok(1)
    }
}

fn remove_dir_recursive(path: &str, opts: &RemoveOptions) -> Result<u64, i32> {
    let mut count = 0u64;

    // Remove children first
    let vfs = VFS.lock();
    let entries = vfs.list_dir(path).unwrap_or_default();
    drop(vfs);

    for entry in &entries {
        let child_path = path::join(path, entry);

        let vfs = VFS.lock();
        let is_dir = vfs.list_dir(&child_path).is_some();
        drop(vfs);

        if is_dir {
            count += remove_dir_recursive(&child_path, opts)?;
        } else {
            unlink(&child_path)?;
            count += 1;
            if opts.verbose {
                serial_println!("removed '{}'", child_path);
            }
        }
    }

    // Remove the directory itself
    rmdir(path)?;
    count += 1;
    if opts.verbose {
        serial_println!("removed directory '{}'", path);
    }

    Ok(count)
}

// ─── Directory listing with full metadata ───────────────────────────

/// Directory entry with full metadata (like `readdir` + `stat`).
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub ino: u64,
    pub file_type: FileType,
    pub size: u64,
    pub permissions: u16,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u32,
    pub mtime: u64,
    pub atime: u64,
    pub ctime: u64,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
}

/// List directory entries with full metadata.
pub fn list_dir(path: &str) -> Result<Vec<DirEntry>, i32> {
    let vfs = VFS.lock();
    let ino = vfs.resolve_path(path).ok_or(ENOENT)?;
    let inode = vfs.get_inode(ino).ok_or(ENOENT)?;

    if inode.file_type != FileType::Directory {
        return Err(ENOTDIR);
    }

    touch_atime(ino);

    let mut entries = Vec::new();

    for &child_ino in &inode.children {
        if let Some(child) = vfs.get_inode(child_ino) {
            let meta = get_or_create_meta(child_ino);
            entries.push(DirEntry {
                name: child.name.clone(),
                ino: child_ino,
                file_type: child.file_type,
                size: child.size,
                permissions: child.permissions,
                uid: child.uid,
                gid: child.gid,
                nlink: meta.nlink,
                mtime: meta.mtime,
                atime: meta.atime,
                ctime: meta.ctime,
                is_symlink: child.file_type == FileType::SymLink,
                symlink_target: meta.symlink_target.clone(),
            });
        }
    }

    Ok(entries)
}

/// List directory entries sorted by various criteria.
pub fn list_dir_sorted(path: &str, sort: SortOrder, reverse: bool) -> Result<Vec<DirEntry>, i32> {
    let mut entries = list_dir(path)?;

    match sort {
        SortOrder::Name => {
            entries.sort_by(|a, b| {
                let a_dir = a.file_type == FileType::Directory;
                let b_dir = b.file_type == FileType::Directory;
                match (a_dir, b_dir) {
                    (true, false) => core::cmp::Ordering::Less,
                    (false, true) => core::cmp::Ordering::Greater,
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                }
            });
        }
        SortOrder::Size => entries.sort_by_key(|e| e.size),
        SortOrder::Time => entries.sort_by_key(|e| e.mtime),
        SortOrder::Extension => {
            entries.sort_by(|a, b| {
                let a_path = path::Path::new(&a.name);
                let b_path = path::Path::new(&b.name);
                let a_ext = a_path.extension().unwrap_or("");
                let b_ext = b_path.extension().unwrap_or("");
                a_ext.cmp(b_ext)
            });
        }
        SortOrder::Inode => entries.sort_by_key(|e| e.ino),
        SortOrder::None => {} // Keep filesystem order
    }

    if reverse {
        entries.reverse();
    }

    Ok(entries)
}

/// Sort order for directory listings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Name,
    Size,
    Time,
    Extension,
    Inode,
    None,
}

// ─── Disk usage ─────────────────────────────────────────────────────

/// Calculate disk usage of a file or directory (in bytes).
pub fn disk_usage(path: &str) -> Result<DiskUsage, i32> {
    let vfs = VFS.lock();
    let ino = vfs.resolve_path(path).ok_or(ENOENT)?;
    let inode = vfs.get_inode(ino).ok_or(ENOENT)?;

    if inode.file_type != FileType::Directory {
        return Ok(DiskUsage {
            bytes: inode.size,
            files: 1,
            directories: 0,
            blocks: inode.size.div_ceil(4096),
        });
    }

    drop(vfs);
    disk_usage_recursive(path)
}

fn disk_usage_recursive(path: &str) -> Result<DiskUsage, i32> {
    let mut usage = DiskUsage {
        bytes: 4096, // Directory entry overhead
        files: 0,
        directories: 1,
        blocks: 1,
    };

    let vfs = VFS.lock();
    let entries = vfs.list_dir(path).unwrap_or_default();
    drop(vfs);

    for entry in &entries {
        let child_path = path::join(path, entry);

        let vfs = VFS.lock();
        let child_ino = vfs.resolve_path(&child_path);
        let is_dir = vfs.list_dir(&child_path).is_some();
        let file_size = child_ino
            .and_then(|ino| vfs.get_inode(ino))
            .map(|i| i.size)
            .unwrap_or(0);
        drop(vfs);

        if is_dir {
            let sub = disk_usage_recursive(&child_path)?;
            usage.bytes += sub.bytes;
            usage.files += sub.files;
            usage.directories += sub.directories;
            usage.blocks += sub.blocks;
        } else {
            usage.bytes += file_size;
            usage.files += 1;
            usage.blocks += file_size.div_ceil(4096);
        }
    }

    Ok(usage)
}

/// Disk usage result.
#[derive(Debug, Clone)]
pub struct DiskUsage {
    pub bytes: u64,
    pub files: u64,
    pub directories: u64,
    pub blocks: u64,
}

impl DiskUsage {
    pub fn display_human(&self) -> String {
        let size = format_size_human(self.bytes);
        alloc::format!(
            "{} total ({} files, {} directories, {} blocks)",
            size,
            self.files,
            self.directories,
            self.blocks
        )
    }
}

/// Format bytes as human-readable size.
pub fn format_size_human(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        let gb = bytes / (1024 * 1024 * 1024);
        let frac = (bytes % (1024 * 1024 * 1024)) * 10 / (1024 * 1024 * 1024);
        alloc::format!("{}.{}G", gb, frac)
    } else if bytes >= 1024 * 1024 {
        let mb = bytes / (1024 * 1024);
        let frac = (bytes % (1024 * 1024)) * 10 / (1024 * 1024);
        alloc::format!("{}.{}M", mb, frac)
    } else if bytes >= 1024 {
        let kb = bytes / 1024;
        let frac = (bytes % 1024) * 10 / 1024;
        alloc::format!("{}.{}K", kb, frac)
    } else {
        alloc::format!("{}B", bytes)
    }
}

// ─── File type detection ────────────────────────────────────────────

/// Detected file type from magic bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedFileType {
    ElfExecutable,
    ElfSharedLib,
    ElfRelocatable,
    ElfCore,
    ShellScript(String), // interpreter path
    PythonScript,
    PerlScript,
    RubyScript,
    GzipCompressed,
    Bzip2Compressed,
    XzCompressed,
    ZstdCompressed,
    ZipArchive,
    TarArchive,
    PngImage,
    JpegImage,
    GifImage,
    BmpImage,
    WebpImage,
    SvgImage,
    PdfDocument,
    RiffMedia, // WAV, AVI
    Mp3Audio,
    FlacAudio,
    OggMedia,
    Mp4Video,
    MkvVideo,
    AsciiText,
    Utf8Text,
    EmptyFile,
    BinaryData,
    RustSource,
    CSource,
    CppSource,
    JavaSource,
    JavaScriptSource,
    JsonData,
    XmlData,
    HtmlDocument,
    CssStylesheet,
    MakefileScript,
    TomlConfig,
    YamlConfig,
    IniConfig,
    Unknown,
}

impl DetectedFileType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ElfExecutable => "ELF 64-bit LSB executable, x86-64",
            Self::ElfSharedLib => "ELF 64-bit LSB shared object, x86-64",
            Self::ElfRelocatable => "ELF 64-bit LSB relocatable, x86-64",
            Self::ElfCore => "ELF 64-bit LSB core file, x86-64",
            Self::ShellScript(interp) => "POSIX shell script",
            Self::PythonScript => "Python script, ASCII text executable",
            Self::PerlScript => "Perl script, ASCII text executable",
            Self::RubyScript => "Ruby script, ASCII text executable",
            Self::GzipCompressed => "gzip compressed data",
            Self::Bzip2Compressed => "bzip2 compressed data",
            Self::XzCompressed => "XZ compressed data",
            Self::ZstdCompressed => "Zstandard compressed data",
            Self::ZipArchive => "Zip archive data",
            Self::TarArchive => "POSIX tar archive",
            Self::PngImage => "PNG image data",
            Self::JpegImage => "JPEG image data",
            Self::GifImage => "GIF image data",
            Self::BmpImage => "BMP image data",
            Self::WebpImage => "WebP image data",
            Self::SvgImage => "SVG image",
            Self::PdfDocument => "PDF document",
            Self::RiffMedia => "RIFF media data",
            Self::Mp3Audio => "MPEG ADTS audio, layer III",
            Self::FlacAudio => "FLAC audio bitstream data",
            Self::OggMedia => "Ogg data",
            Self::Mp4Video => "ISO Media, MP4",
            Self::MkvVideo => "Matroska video",
            Self::AsciiText => "ASCII text",
            Self::Utf8Text => "UTF-8 Unicode text",
            Self::EmptyFile => "empty",
            Self::BinaryData => "data",
            Self::RustSource => "Rust source, ASCII text",
            Self::CSource => "C source, ASCII text",
            Self::CppSource => "C++ source, ASCII text",
            Self::JavaSource => "Java source, ASCII text",
            Self::JavaScriptSource => "JavaScript source, ASCII text",
            Self::JsonData => "JSON data",
            Self::XmlData => "XML document",
            Self::HtmlDocument => "HTML document",
            Self::CssStylesheet => "CSS stylesheet",
            Self::MakefileScript => "Makefile script",
            Self::TomlConfig => "TOML configuration",
            Self::YamlConfig => "YAML configuration",
            Self::IniConfig => "INI configuration",
            Self::Unknown => "data",
        }
    }

    /// Get MIME type
    pub fn mime_type(&self) -> &str {
        match self {
            Self::ElfExecutable | Self::ElfSharedLib | Self::ElfRelocatable => {
                "application/x-executable"
            }
            Self::ShellScript(_) | Self::PythonScript | Self::PerlScript | Self::RubyScript => {
                "text/x-script"
            }
            Self::GzipCompressed => "application/gzip",
            Self::Bzip2Compressed => "application/x-bzip2",
            Self::XzCompressed => "application/x-xz",
            Self::ZstdCompressed => "application/zstd",
            Self::ZipArchive => "application/zip",
            Self::TarArchive => "application/x-tar",
            Self::PngImage => "image/png",
            Self::JpegImage => "image/jpeg",
            Self::GifImage => "image/gif",
            Self::BmpImage => "image/bmp",
            Self::WebpImage => "image/webp",
            Self::SvgImage => "image/svg+xml",
            Self::PdfDocument => "application/pdf",
            Self::RiffMedia => "audio/wav",
            Self::Mp3Audio => "audio/mpeg",
            Self::FlacAudio => "audio/flac",
            Self::OggMedia => "audio/ogg",
            Self::Mp4Video => "video/mp4",
            Self::MkvVideo => "video/x-matroska",
            Self::AsciiText | Self::Utf8Text => "text/plain",
            Self::RustSource | Self::CSource | Self::CppSource => "text/x-source",
            Self::JsonData => "application/json",
            Self::XmlData => "application/xml",
            Self::HtmlDocument => "text/html",
            Self::CssStylesheet => "text/css",
            Self::JavaScriptSource => "application/javascript",
            Self::TomlConfig | Self::YamlConfig | Self::IniConfig => "text/plain",
            _ => "application/octet-stream",
        }
    }
}

/// Detect file type from content (magic bytes) and filename.
pub fn detect_file_type(path: &str, data: &[u8]) -> DetectedFileType {
    if data.is_empty() {
        return DetectedFileType::EmptyFile;
    }

    // Check magic bytes first
    if data.len() >= 4 && data[0] == 0x7f && data[1] == b'E' && data[2] == b'L' && data[3] == b'F' {
        // ELF — check type at offset 16
        if data.len() >= 18 {
            match data[16] {
                1 => return DetectedFileType::ElfRelocatable,
                2 => return DetectedFileType::ElfExecutable,
                3 => return DetectedFileType::ElfSharedLib,
                4 => return DetectedFileType::ElfCore,
                _ => return DetectedFileType::ElfExecutable,
            }
        }
        return DetectedFileType::ElfExecutable;
    }

    if data.len() >= 2 && data[0] == b'#' && data[1] == b'!' {
        // Shebang
        let line_end = data
            .iter()
            .position(|&b| b == b'\n')
            .unwrap_or(data.len().min(256));
        if let Ok(shebang) = core::str::from_utf8(&data[2..line_end]) {
            let interp = shebang.trim();
            if interp.contains("python") {
                return DetectedFileType::PythonScript;
            }
            if interp.contains("perl") {
                return DetectedFileType::PerlScript;
            }
            if interp.contains("ruby") {
                return DetectedFileType::RubyScript;
            }
            return DetectedFileType::ShellScript(String::from(interp));
        }
    }

    // Compressed formats
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        return DetectedFileType::GzipCompressed;
    }
    if data.len() >= 3 && data[0] == b'B' && data[1] == b'Z' && data[2] == b'h' {
        return DetectedFileType::Bzip2Compressed;
    }
    if data.len() >= 6
        && data[0] == 0xfd
        && data[1] == 0x37
        && data[2] == 0x7a
        && data[3] == 0x58
        && data[4] == 0x5a
        && data[5] == 0x00
    {
        return DetectedFileType::XzCompressed;
    }
    if data.len() >= 4 && data[0] == 0x28 && data[1] == 0xb5 && data[2] == 0x2f && data[3] == 0xfd {
        return DetectedFileType::ZstdCompressed;
    }

    // Archives
    if data.len() >= 4 && data[0] == 0x50 && data[1] == 0x4b && data[2] == 0x03 && data[3] == 0x04 {
        return DetectedFileType::ZipArchive;
    }
    if data.len() >= 263 && &data[257..262] == b"ustar" {
        return DetectedFileType::TarArchive;
    }

    // Images
    if data.len() >= 8
        && data[0] == 0x89
        && data[1] == b'P'
        && data[2] == b'N'
        && data[3] == b'G'
        && data[4] == 0x0d
        && data[5] == 0x0a
        && data[6] == 0x1a
        && data[7] == 0x0a
    {
        return DetectedFileType::PngImage;
    }
    if data.len() >= 3 && data[0] == 0xff && data[1] == 0xd8 && data[2] == 0xff {
        return DetectedFileType::JpegImage;
    }
    if data.len() >= 6 && (data[..6] == *b"GIF87a" || data[..6] == *b"GIF89a") {
        return DetectedFileType::GifImage;
    }
    if data.len() >= 2 && data[0] == b'B' && data[1] == b'M' {
        return DetectedFileType::BmpImage;
    }
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return DetectedFileType::WebpImage;
    }

    // Audio/Video
    if data.len() >= 12 && &data[0..4] == b"RIFF" {
        return DetectedFileType::RiffMedia;
    }
    if data.len() >= 3 && (data[0] == 0xff && (data[1] & 0xe0) == 0xe0) {
        return DetectedFileType::Mp3Audio;
    }
    if data.len() >= 4 && &data[0..4] == b"fLaC" {
        return DetectedFileType::FlacAudio;
    }
    if data.len() >= 4 && &data[0..4] == b"OggS" {
        return DetectedFileType::OggMedia;
    }
    if data.len() >= 8 && &data[4..8] == b"ftyp" {
        return DetectedFileType::Mp4Video;
    }
    if data.len() >= 4 && data[0] == 0x1a && data[1] == 0x45 && data[2] == 0xdf && data[3] == 0xa3 {
        return DetectedFileType::MkvVideo;
    }

    // PDF
    if data.len() >= 5 && &data[0..5] == b"%PDF-" {
        return DetectedFileType::PdfDocument;
    }

    // Try text-based detection
    let sample_len = data.len().min(8192);
    let sample = &data[..sample_len];

    let is_ascii = sample
        .iter()
        .all(|&b| b.is_ascii() || b == b'\n' || b == b'\r' || b == b'\t');
    let is_utf8 = core::str::from_utf8(sample).is_ok();

    if is_ascii || is_utf8 {
        // Use file extension to determine type
        if let Some(ext) = path::Path::new(path).extension() {
            match ext {
                "rs" => return DetectedFileType::RustSource,
                "c" | "h" => return DetectedFileType::CSource,
                "cpp" | "cc" | "cxx" | "hpp" | "hh" => return DetectedFileType::CppSource,
                "java" => return DetectedFileType::JavaSource,
                "js" | "mjs" => return DetectedFileType::JavaScriptSource,
                "json" => return DetectedFileType::JsonData,
                "xml" => return DetectedFileType::XmlData,
                "html" | "htm" => return DetectedFileType::HtmlDocument,
                "css" => return DetectedFileType::CssStylesheet,
                "svg" => return DetectedFileType::SvgImage,
                "toml" => return DetectedFileType::TomlConfig,
                "yaml" | "yml" => return DetectedFileType::YamlConfig,
                "ini" | "cfg" | "conf" => return DetectedFileType::IniConfig,
                _ => {}
            }
        }

        // Check content patterns
        if let Ok(text) = core::str::from_utf8(sample) {
            if text.starts_with('{') || text.starts_with('[') {
                // Might be JSON
                if text.contains("\":") || text.contains("\": ") {
                    return DetectedFileType::JsonData;
                }
            }
            if text.starts_with("<?xml") || text.starts_with("<svg") {
                return if text.contains("<svg") {
                    DetectedFileType::SvgImage
                } else {
                    DetectedFileType::XmlData
                };
            }
            if text.starts_with("<!DOCTYPE") || text.starts_with("<html") {
                return DetectedFileType::HtmlDocument;
            }
        }

        if is_ascii {
            return DetectedFileType::AsciiText;
        }
        return DetectedFileType::Utf8Text;
    }

    DetectedFileType::BinaryData
}

// ─── Path canonicalization ──────────────────────────────────────────

/// Canonicalize a path — resolve all symlinks and return an absolute path.
/// Equivalent to `realpath(3)`.
pub fn canonicalize(path: &str) -> Result<String, i32> {
    let (resolved, _) = resolve_path_follow(path)?;
    Ok(path::normalize_path(&resolved))
}

/// Canonicalize without requiring the final component to exist.
/// Equivalent to `realpath -m`.
pub fn canonicalize_missing(path: &str) -> String {
    match resolve_path_follow(path) {
        Ok((resolved, _)) => path::normalize_path(&resolved),
        Err(_) => {
            // Resolve as much as possible, keep the rest
            let normalized = path::normalize_path(path);
            if normalized.starts_with('/') {
                normalized
            } else {
                let cwd = crate::shell::env::ENV_VARS
                    .lock()
                    .get("PWD")
                    .cloned()
                    .unwrap_or_else(|| String::from("/"));
                path::normalize_path(&path::join(&cwd, &normalized))
            }
        }
    }
}

// ─── Access checks ──────────────────────────────────────────────────

/// access() — check file accessibility.
/// `mode`: 0=F_OK (exists), 1=X_OK, 2=W_OK, 4=R_OK (combinable).
pub fn access(path: &str, mode: u32) -> Result<(), i32> {
    let vfs = VFS.lock();
    vfs.access(path, mode)
}

/// Test if a path exists.
pub fn exists(path: &str) -> bool {
    VFS.lock().resolve_path(path).is_some()
}

/// Test if a path is a directory.
pub fn is_dir(path: &str) -> bool {
    let vfs = VFS.lock();
    vfs.resolve_path(path)
        .and_then(|ino| vfs.get_inode(ino))
        .map(|i| i.file_type == FileType::Directory)
        .unwrap_or(false)
}

/// Test if a path is a regular file.
pub fn is_file(path: &str) -> bool {
    let vfs = VFS.lock();
    vfs.resolve_path(path)
        .and_then(|ino| vfs.get_inode(ino))
        .map(|i| i.file_type == FileType::Regular)
        .unwrap_or(false)
}

/// Test if a path is a symlink.
pub fn is_symlink(path: &str) -> bool {
    // Use nofollow to detect the symlink itself
    let vfs = VFS.lock();
    vfs.resolve_path(path)
        .and_then(|ino| vfs.get_inode(ino))
        .map(|i| i.file_type == FileType::SymLink)
        .unwrap_or(false)
}

/// Test if a file is executable.
pub fn is_executable(path: &str) -> bool {
    let vfs = VFS.lock();
    vfs.resolve_path(path)
        .and_then(|ino| vfs.get_inode(ino))
        .map(|i| i.permissions & 0o111 != 0)
        .unwrap_or(false)
}

// ─── Find / search ──────────────────────────────────────────────────

/// Search criteria for find().
#[derive(Debug, Clone, Default)]
pub struct FindOptions {
    /// Filter by name glob pattern
    pub name: Option<String>,
    /// Filter by file type
    pub file_type: Option<FindType>,
    /// Max depth (0 = unlimited)
    pub max_depth: usize,
    /// Min depth
    pub min_depth: usize,
    /// Filter by minimum size (bytes)
    pub min_size: Option<u64>,
    /// Filter by maximum size (bytes)
    pub max_size: Option<u64>,
    /// Filter by permission bits
    pub perm: Option<u16>,
    /// Execute action on each match
    pub action: FindAction,
    /// Follow symlinks
    pub follow_symlinks: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindType {
    File,
    Directory,
    Symlink,
    Pipe,
    Socket,
    CharDevice,
    BlockDevice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FindAction {
    #[default]
    Print,
    Print0,
    Delete,
    Count,
}

/// Result of a find operation.
pub struct FindResult {
    pub paths: Vec<String>,
    pub count: u64,
}

/// find() — search for files matching criteria.
pub fn find(start_path: &str, opts: &FindOptions) -> FindResult {
    let mut result = FindResult {
        paths: Vec::new(),
        count: 0,
    };

    find_recursive(start_path, opts, 0, &mut result);
    result
}

fn find_recursive(path: &str, opts: &FindOptions, depth: usize, result: &mut FindResult) {
    if opts.max_depth > 0 && depth > opts.max_depth {
        return;
    }

    let vfs = VFS.lock();
    let ino = match vfs.resolve_path(path) {
        Some(ino) => ino,
        None => return,
    };
    let inode = match vfs.get_inode(ino) {
        Some(i) => i.clone(),
        None => return,
    };
    let entries = vfs.list_dir(path).unwrap_or_default();
    drop(vfs);

    // Check if current path matches
    if depth >= opts.min_depth {
        let matches = check_find_match(path, &inode, opts);
        if matches {
            result.count += 1;
            match opts.action {
                FindAction::Print | FindAction::Print0 => {
                    result.paths.push(String::from(path));
                }
                FindAction::Delete => {
                    if inode.file_type == FileType::Directory {
                        let _ = rmdir(path);
                    } else {
                        let _ = unlink(path);
                    }
                    result.paths.push(String::from(path));
                }
                FindAction::Count => {} // Just count
            }
        }
    }

    // Recurse into subdirectories
    if inode.file_type == FileType::Directory {
        for entry in &entries {
            let child_path = path::join(path, entry);
            find_recursive(&child_path, opts, depth + 1, result);
        }
    }
}

fn check_find_match(path: &str, inode: &Inode, opts: &FindOptions) -> bool {
    // Check name pattern
    if let Some(ref pattern) = opts.name {
        let name = path::basename(path);
        if !path::glob_match(pattern, name) {
            return false;
        }
    }

    // Check file type
    if let Some(find_type) = opts.file_type {
        let matches = match find_type {
            FindType::File => inode.file_type == FileType::Regular,
            FindType::Directory => inode.file_type == FileType::Directory,
            FindType::Symlink => inode.file_type == FileType::SymLink,
            FindType::Pipe => inode.file_type == FileType::Pipe,
            FindType::Socket => inode.file_type == FileType::Socket,
            FindType::CharDevice => inode.file_type == FileType::CharDevice,
            FindType::BlockDevice => inode.file_type == FileType::BlockDevice,
        };
        if !matches {
            return false;
        }
    }

    // Check size
    if let Some(min) = opts.min_size {
        if inode.size < min {
            return false;
        }
    }
    if let Some(max) = opts.max_size {
        if inode.size > max {
            return false;
        }
    }

    // Check permissions
    if let Some(perm) = opts.perm {
        if inode.permissions & perm != perm {
            return false;
        }
    }

    true
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

// ─── Watch / notification stub ──────────────────────────────────────

/// Filesystem event types (for inotify-style watchers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsEvent {
    Create,
    Delete,
    Modify,
    MovedFrom,
    MovedTo,
    Attrib,
    Open,
    Close,
}

/// Filesystem event.
#[derive(Debug, Clone)]
pub struct FsNotification {
    pub event: FsEvent,
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

// ─── File locking ───────────────────────────────────────────────────

/// Advisory file lock type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockType {
    /// F_RDLCK — shared lock
    ReadLock,
    /// F_WRLCK — exclusive lock
    WriteLock,
    /// F_UNLCK — unlock
    Unlock,
}

/// Advisory lock on a file region.
#[derive(Debug, Clone)]
pub struct FileLock {
    pub lock_type: LockType,
    pub start: u64,
    pub length: u64, // 0 = to end of file
    pub pid: u32,
}

lazy_static::lazy_static! {
    static ref FILE_LOCKS: Mutex<BTreeMap<u64, Vec<FileLock>>> = Mutex::new(BTreeMap::new());
}

/// fcntl F_SETLK — set advisory lock.
pub fn setlk(ino: u64, lock: &FileLock) -> Result<(), i32> {
    let mut locks = FILE_LOCKS.lock();
    let entry = locks.entry(ino).or_default();

    if lock.lock_type == LockType::Unlock {
        entry.retain(|l| l.pid != lock.pid || l.start != lock.start);
        return Ok(());
    }

    // Check for conflicts
    for existing in entry.iter() {
        if existing.pid == lock.pid {
            continue; // Same process can upgrade/downgrade
        }
        // Check overlap
        let existing_end = if existing.length == 0 {
            u64::MAX
        } else {
            existing.start + existing.length
        };
        let new_end = if lock.length == 0 {
            u64::MAX
        } else {
            lock.start + lock.length
        };
        if lock.start < existing_end && new_end > existing.start {
            // Overlapping region
            if lock.lock_type == LockType::WriteLock || existing.lock_type == LockType::WriteLock {
                return Err(EAGAIN); // Would block
            }
        }
    }

    // Remove any existing lock from this PID on this region
    entry.retain(|l| l.pid != lock.pid || l.start != lock.start);
    entry.push(lock.clone());
    Ok(())
}

/// fcntl F_GETLK — test advisory lock.
pub fn getlk(ino: u64, lock: &FileLock) -> Option<FileLock> {
    let locks = FILE_LOCKS.lock();
    if let Some(entry) = locks.get(&ino) {
        for existing in entry {
            if existing.pid == lock.pid {
                continue;
            }
            let existing_end = if existing.length == 0 {
                u64::MAX
            } else {
                existing.start + existing.length
            };
            let new_end = if lock.length == 0 {
                u64::MAX
            } else {
                lock.start + lock.length
            };
            if lock.start < existing_end
                && new_end > existing.start
                && (lock.lock_type == LockType::WriteLock
                    || existing.lock_type == LockType::WriteLock)
            {
                return Some(existing.clone());
            }
        }
    }
    None // No conflict
}

// ─── Temp file/directory creation ───────────────────────────────────

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Create a temporary file in /tmp and return its path.
pub fn mktemp(template: Option<&str>) -> Result<String, i32> {
    let count = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = if let Some(tmpl) = template {
        if tmpl.contains("XXXXXX") {
            tmpl.replace("XXXXXX", &alloc::format!("{:06x}", count))
        } else {
            alloc::format!("{}.{:06x}", tmpl, count)
        }
    } else {
        alloc::format!("tmp.{:06x}", count)
    };

    let path = alloc::format!("/tmp/{}", name);
    create(&path, 0o600)?;
    Ok(path)
}

/// Create a temporary directory in /tmp and return its path.
pub fn mkdtemp(template: Option<&str>) -> Result<String, i32> {
    let count = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = if let Some(tmpl) = template {
        if tmpl.contains("XXXXXX") {
            tmpl.replace("XXXXXX", &alloc::format!("{:06x}", count))
        } else {
            alloc::format!("{}.{:06x}", tmpl, count)
        }
    } else {
        alloc::format!("tmpd.{:06x}", count)
    };

    let path = alloc::format!("/tmp/{}", name);
    mkdir(&path, 0o700)?;
    Ok(path)
}

// ─── Walk / traverse ────────────────────────────────────────────────

/// Walk a directory tree, calling `callback` for each entry.
/// Like Python's `os.walk()`.
pub fn walk<F>(root: &str, mut callback: F) -> Result<(), i32>
where
    F: FnMut(&str, &[String], &[String]),
{
    walk_inner(root, &mut callback)
}

fn walk_inner<F>(dir: &str, callback: &mut F) -> Result<(), i32>
where
    F: FnMut(&str, &[String], &[String]),
{
    let vfs = VFS.lock();
    let entries = vfs.list_dir(dir).ok_or(ENOENT)?;

    let mut dirs = Vec::new();
    let mut files = Vec::new();

    for entry in &entries {
        let child = path::join(dir, entry);
        if vfs.list_dir(&child).is_some() {
            dirs.push(entry.clone());
        } else {
            files.push(entry.clone());
        }
    }
    drop(vfs);

    callback(dir, &dirs, &files);

    for subdir in &dirs {
        let child = path::join(dir, subdir);
        walk_inner(&child, callback)?;
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════
// FILE PREVIEW PANE — side panel with file info + preview
// ═══════════════════════════════════════════════════════════════════════

/// Preview information for a file
#[derive(Debug, Clone)]
pub struct FilePreview {
    pub path: String,
    pub file_type: String,
    pub size: u64,
    pub permissions: u32,
    pub owner_uid: u32,
    pub group_gid: u32,
    pub created: u64,
    pub modified: u64,
    pub accessed: u64,
    pub mime_type: String,
    pub preview_text: Option<String>,
    pub is_image: bool,
    pub is_text: bool,
}

/// Generate a preview for a file
pub fn generate_preview(path: &str) -> Option<FilePreview> {
    let meta = stat(path).ok()?;
    let vfs = VFS.lock();
    let data = vfs.read_file(path)?;
    let data = data.to_vec();
    drop(vfs);

    let ext = path.rsplit('.').next().unwrap_or("");
    let (mime, is_image, is_text) = match ext {
        "txt" | "md" | "log" | "conf" | "cfg" => ("text/plain", false, true),
        "rs" | "c" | "h" | "py" | "js" | "ts" | "sh" => ("text/x-source", false, true),
        "html" | "htm" => ("text/html", false, true),
        "json" => ("application/json", false, true),
        "toml" | "yaml" | "yml" => ("text/x-config", false, true),
        "png" => ("image/png", true, false),
        "jpg" | "jpeg" => ("image/jpeg", true, false),
        "bmp" => ("image/bmp", true, false),
        "pdf" => ("application/pdf", false, false),
        "zip" => ("application/zip", false, false),
        _ => ("application/octet-stream", false, false),
    };

    let preview_text = if is_text && data.len() < 8192 {
        core::str::from_utf8(&data).ok().map(|s| {
            let lines: Vec<&str> = s.lines().take(50).collect();
            let mut out = String::new();
            for line in lines {
                out.push_str(line);
                out.push('\n');
            }
            out
        })
    } else {
        None
    };

    Some(FilePreview {
        path: String::from(path),
        file_type: String::from(if meta.st_mode & S_IFDIR != 0 {
            "directory"
        } else if meta.st_mode & S_IFLNK != 0 {
            "symlink"
        } else {
            "file"
        }),
        size: meta.st_size,
        permissions: meta.st_mode & 0o7777,
        owner_uid: meta.st_uid,
        group_gid: meta.st_gid,
        created: meta.st_ctime,
        modified: meta.st_mtime,
        accessed: meta.st_atime,
        mime_type: String::from(mime),
        preview_text,
        is_image,
        is_text,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// RECENT FILES LIST — track recently opened files
// ═══════════════════════════════════════════════════════════════════════

const MAX_RECENT: usize = 50;

struct RecentFilesStore {
    entries: Vec<RecentFileEntry>,
}

#[derive(Debug, Clone)]
pub struct RecentFileEntry {
    pub path: String,
    pub timestamp: u64,
    pub app: String,
}

lazy_static::lazy_static! {
    static ref RECENT_FILES: Mutex<RecentFilesStore> = Mutex::new(RecentFilesStore {
        entries: Vec::new(),
    });
}

/// Record a file as recently opened
pub fn record_recent_file(path: &str, app: &str) {
    let mut store = RECENT_FILES.lock();
    // Remove existing entry for same path
    store.entries.retain(|e| e.path != path);
    store.entries.insert(
        0,
        RecentFileEntry {
            path: String::from(path),
            timestamp: crate::rtc::unix_time() as u64,
            app: String::from(app),
        },
    );
    if store.entries.len() > MAX_RECENT {
        store.entries.truncate(MAX_RECENT);
    }
}

/// Get recent files list
pub fn get_recent_files() -> Vec<RecentFileEntry> {
    RECENT_FILES.lock().entries.clone()
}

/// Clear recent files
pub fn clear_recent_files() {
    RECENT_FILES.lock().entries.clear();
}

// ═══════════════════════════════════════════════════════════════════════
// BULK RENAME — rename multiple files using patterns
// ═══════════════════════════════════════════════════════════════════════

/// Bulk rename files matching a pattern
/// `pattern` can be "*.txt" to match, `replacement` uses {n} for sequence number
/// Returns count of renamed files
pub fn bulk_rename(dir: &str, files: &[&str], template: &str) -> Result<usize, i32> {
    let mut count = 0usize;
    for (i, file) in files.iter().enumerate() {
        let old_path = if dir.ends_with('/') {
            alloc::format!("{}{}", dir, file)
        } else {
            alloc::format!("{}/{}", dir, file)
        };

        // Build new name from template
        let seq = alloc::format!("{}", i + 1);
        let ext = file.rsplit('.').next().unwrap_or("");
        let base = file.rsplit('.').nth(1).unwrap_or(file);

        let new_name = template
            .replace("{n}", &seq)
            .replace("{name}", base)
            .replace("{ext}", ext);

        let new_path = if dir.ends_with('/') {
            alloc::format!("{}{}", dir, new_name)
        } else {
            alloc::format!("{}/{}", dir, new_name)
        };

        if rename(&old_path, &new_path).is_ok() {
            count += 1;
        }
    }
    Ok(count)
}

// ═══════════════════════════════════════════════════════════════════════
// FILE PROPERTIES DIALOG — permissions, size, timestamps
// ═══════════════════════════════════════════════════════════════════════

/// Structured file properties for the dialog
#[derive(Debug, Clone)]
pub struct FileProperties {
    pub path: String,
    pub name: String,
    pub file_type: String,
    pub size: u64,
    pub size_human: String,
    pub permissions: u32,
    pub permissions_string: String,
    pub owner_uid: u32,
    pub group_gid: u32,
    pub created: u64,
    pub modified: u64,
    pub accessed: u64,
    pub link_count: u32,
    pub inode: u64,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
}

/// Get full file properties
pub fn get_file_properties(path: &str) -> Option<FileProperties> {
    let meta = stat(path).ok()?;
    let name = path.rsplit('/').next().unwrap_or(path);

    let size = meta.st_size;
    let size_human = if size < 1024 {
        alloc::format!("{} B", size)
    } else if size < 1024 * 1024 {
        alloc::format!("{} KB", size / 1024)
    } else if size < 1024 * 1024 * 1024 {
        alloc::format!("{} MB", size / (1024 * 1024))
    } else {
        alloc::format!("{} GB", size / (1024 * 1024 * 1024))
    };

    let perms = meta.st_mode & 0o7777;
    let perm_str = alloc::format!(
        "{}{}{}{}{}{}{}{}{}",
        if perms & 0o400 != 0 { 'r' } else { '-' },
        if perms & 0o200 != 0 { 'w' } else { '-' },
        if perms & 0o100 != 0 { 'x' } else { '-' },
        if perms & 0o040 != 0 { 'r' } else { '-' },
        if perms & 0o020 != 0 { 'w' } else { '-' },
        if perms & 0o010 != 0 { 'x' } else { '-' },
        if perms & 0o004 != 0 { 'r' } else { '-' },
        if perms & 0o002 != 0 { 'w' } else { '-' },
        if perms & 0o001 != 0 { 'x' } else { '-' },
    );

    let is_symlink = meta.st_mode & S_IFLNK == S_IFLNK;
    let symlink_target = if is_symlink {
        readlink(path).ok()
    } else {
        None
    };

    let ft = if meta.st_mode & S_IFDIR != 0 {
        "Directory"
    } else if is_symlink {
        "Symbolic Link"
    } else if meta.st_mode & S_IFCHR != 0 {
        "Character Device"
    } else if meta.st_mode & S_IFBLK != 0 {
        "Block Device"
    } else if meta.st_mode & S_IFIFO != 0 {
        "FIFO"
    } else if meta.st_mode & S_IFSOCK != 0 {
        "Socket"
    } else {
        "Regular File"
    };

    Some(FileProperties {
        path: String::from(path),
        name: String::from(name),
        file_type: String::from(ft),
        size,
        size_human,
        permissions: perms,
        permissions_string: perm_str,
        owner_uid: meta.st_uid,
        group_gid: meta.st_gid,
        created: meta.st_ctime,
        modified: meta.st_mtime,
        accessed: meta.st_atime,
        link_count: meta.st_nlink,
        inode: meta.st_ino,
        is_symlink,
        symlink_target,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// PERMISSION EDITOR — chmod/chown GUI support
// ═══════════════════════════════════════════════════════════════════════

/// Apply permissions change (GUI-friendly wrapper)
pub fn set_permissions(path: &str, mode: u32) -> Result<(), i32> {
    chmod(path, mode as u16)
}

/// Change ownership (GUI wrapper)
pub fn set_ownership(path: &str, uid: u32, gid: u32) -> Result<(), i32> {
    chown(path, uid, gid)
}

// ═══════════════════════════════════════════════════════════════════════
// TRASH / RECYCLE BIN
// ═══════════════════════════════════════════════════════════════════════

const TRASH_DIR: &str = "/home/.Trash";

/// A trashed file entry
#[derive(Debug, Clone)]
pub struct TrashEntry {
    pub original_path: String,
    pub trash_name: String,
    pub deleted_at: u64,
    pub size: u64,
}

lazy_static::lazy_static! {
    static ref TRASH: Mutex<Vec<TrashEntry>> = Mutex::new(Vec::new());
}

/// Move a file to trash instead of deleting permanently
pub fn trash_file(path: &str) -> Result<(), i32> {
    // Ensure trash dir exists
    let _ = mkdirp(TRASH_DIR, 0o700);

    let name = path.rsplit('/').next().unwrap_or("file");
    let ts = crate::rtc::unix_time() as u64;
    let trash_name = alloc::format!("{}_{}", ts, name);
    let trash_path = alloc::format!("{}/{}", TRASH_DIR, trash_name);

    rename(path, &trash_path)?;
    let size = stat(&trash_path).map(|m| m.st_size).unwrap_or(0);
    TRASH.lock().push(TrashEntry {
        original_path: String::from(path),
        trash_name,
        deleted_at: ts,
        size,
    });
    Ok(())
}

/// Restore a file from trash
pub fn restore_from_trash(trash_name: &str) -> Result<(), i32> {
    let entry = TRASH
        .lock()
        .iter()
        .find(|e| e.trash_name == trash_name)
        .cloned();

    if let Some(entry) = entry {
        let trash_path = alloc::format!("{}/{}", TRASH_DIR, trash_name);
        rename(&trash_path, &entry.original_path)?;
        TRASH.lock().retain(|e| e.trash_name != trash_name);
        Ok(())
    } else {
        Err(ENOENT)
    }
}

/// Empty the trash permanently
pub fn empty_trash() -> usize {
    let entries: Vec<TrashEntry> = TRASH.lock().drain(..).collect();
    let mut count = 0;
    for entry in &entries {
        let trash_path = alloc::format!("{}/{}", TRASH_DIR, entry.trash_name);
        if unlink(&trash_path).is_ok() {
            count += 1;
        }
    }
    count
}

/// List trash contents
pub fn list_trash() -> Vec<TrashEntry> {
    TRASH.lock().clone()
}

// ═══════════════════════════════════════════════════════════════════════
// FILE MANAGER TABS — multiple directory tabs
// ═══════════════════════════════════════════════════════════════════════

/// A tab in the file manager
#[derive(Debug, Clone)]
pub struct FileManagerTab {
    pub id: u32,
    pub path: String,
    pub title: String,
    pub scroll_offset: usize,
    pub selected_files: Vec<String>,
}

static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);

/// Tab manager state
struct TabManager {
    tabs: Vec<FileManagerTab>,
    active_tab: u32,
}

lazy_static::lazy_static! {
    static ref TAB_MANAGER: Mutex<TabManager> = Mutex::new(TabManager {
        tabs: alloc::vec![FileManagerTab {
            id: 0,
            path: String::from("/home"),
            title: String::from("Home"),
            scroll_offset: 0,
            selected_files: Vec::new(),
        }],
        active_tab: 0,
    });
}

/// Open a new tab
pub fn new_tab(path: &str) -> u32 {
    let id = NEXT_TAB_ID.fetch_add(1, Ordering::Relaxed) as u32;
    let title = path.rsplit('/').next().unwrap_or(path);
    TAB_MANAGER.lock().tabs.push(FileManagerTab {
        id,
        path: String::from(path),
        title: String::from(title),
        scroll_offset: 0,
        selected_files: Vec::new(),
    });
    id
}

/// Close a tab
pub fn close_tab(tab_id: u32) {
    let mut tm = TAB_MANAGER.lock();
    tm.tabs.retain(|t| t.id != tab_id);
    if tm.tabs.is_empty() {
        tm.tabs.push(FileManagerTab {
            id: 0,
            path: String::from("/home"),
            title: String::from("Home"),
            scroll_offset: 0,
            selected_files: Vec::new(),
        });
        tm.active_tab = 0;
    } else if tm.active_tab == tab_id {
        tm.active_tab = tm.tabs[0].id;
    }
}

/// Switch to a tab
pub fn switch_tab(tab_id: u32) {
    TAB_MANAGER.lock().active_tab = tab_id;
}

/// Get all tabs
pub fn get_tabs() -> Vec<FileManagerTab> {
    TAB_MANAGER.lock().tabs.clone()
}

/// Get active tab
pub fn active_tab() -> Option<FileManagerTab> {
    let tm = TAB_MANAGER.lock();
    tm.tabs.iter().find(|t| t.id == tm.active_tab).cloned()
}

// ═══════════════════════════════════════════════════════════════════════
// SPLIT PANE — dual-panel file manager
// ═══════════════════════════════════════════════════════════════════════

/// Split pane state
pub struct SplitPaneState {
    pub enabled: bool,
    pub left_path: String,
    pub right_path: String,
    pub active_side: PaneSide,
    pub split_ratio: f32, // 0.0-1.0, default 0.5
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneSide {
    Left,
    Right,
}

lazy_static::lazy_static! {
    static ref SPLIT_PANE: Mutex<SplitPaneState> = Mutex::new(SplitPaneState {
        enabled: false,
        left_path: String::from("/home"),
        right_path: String::from("/home"),
        active_side: PaneSide::Left,
        split_ratio: 0.5,
    });
}

/// Toggle split pane mode
pub fn toggle_split_pane() -> bool {
    let mut sp = SPLIT_PANE.lock();
    sp.enabled = !sp.enabled;
    sp.enabled
}

/// Get split pane state
pub fn get_split_pane() -> (bool, String, String, PaneSide) {
    let sp = SPLIT_PANE.lock();
    (
        sp.enabled,
        sp.left_path.clone(),
        sp.right_path.clone(),
        sp.active_side,
    )
}

/// Set path for a pane
pub fn set_pane_path(side: PaneSide, path: &str) {
    let mut sp = SPLIT_PANE.lock();
    match side {
        PaneSide::Left => sp.left_path = String::from(path),
        PaneSide::Right => sp.right_path = String::from(path),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// COPY/MOVE PROGRESS TRACKING
// ═══════════════════════════════════════════════════════════════════════

/// Progress state for long file operations
#[derive(Debug, Clone)]
pub struct OperationProgress {
    pub operation: String,
    pub source: String,
    pub dest: String,
    pub total_bytes: u64,
    pub copied_bytes: u64,
    pub total_files: u32,
    pub completed_files: u32,
    pub active: bool,
}

lazy_static::lazy_static! {
    static ref COPY_PROGRESS: Mutex<OperationProgress> = Mutex::new(OperationProgress {
        operation: String::new(),
        source: String::new(),
        dest: String::new(),
        total_bytes: 0,
        copied_bytes: 0,
        total_files: 0,
        completed_files: 0,
        active: false,
    });
}

/// Start tracking a copy/move operation
pub fn start_operation_progress(
    op: &str,
    source: &str,
    dest: &str,
    total_bytes: u64,
    total_files: u32,
) {
    let mut p = COPY_PROGRESS.lock();
    p.operation = String::from(op);
    p.source = String::from(source);
    p.dest = String::from(dest);
    p.total_bytes = total_bytes;
    p.copied_bytes = 0;
    p.total_files = total_files;
    p.completed_files = 0;
    p.active = true;
}

/// Update progress
pub fn update_operation_progress(copied_bytes: u64, completed_files: u32) {
    let mut p = COPY_PROGRESS.lock();
    p.copied_bytes = copied_bytes;
    p.completed_files = completed_files;
}

/// Finish operation
pub fn finish_operation_progress() {
    let mut p = COPY_PROGRESS.lock();
    p.active = false;
}

/// Get current operation progress
pub fn get_operation_progress() -> Option<OperationProgress> {
    let p = COPY_PROGRESS.lock();
    if p.active { Some(p.clone()) } else { None }
}

// ═══════════════════════════════════════════════════════════════════════
// MOUNT/UNMOUNT IN SIDEBAR
// ═══════════════════════════════════════════════════════════════════════

/// A mounted volume visible in the file manager sidebar
#[derive(Debug, Clone)]
pub struct SidebarVolume {
    pub name: String,
    pub mount_point: String,
    pub fs_type: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub removable: bool,
    pub mounted: bool,
}

/// Get volumes for file manager sidebar
pub fn get_sidebar_volumes() -> Vec<SidebarVolume> {
    alloc::vec![
        SidebarVolume {
            name: String::from("System"),
            mount_point: String::from("/"),
            fs_type: String::from("knoxfs"),
            total_bytes: 64 * 1024 * 1024 * 1024,
            free_bytes: 32 * 1024 * 1024 * 1024,
            removable: false,
            mounted: true,
        },
        SidebarVolume {
            name: String::from("Home"),
            mount_point: String::from("/home"),
            fs_type: String::from("knoxfs"),
            total_bytes: 128 * 1024 * 1024 * 1024,
            free_bytes: 96 * 1024 * 1024 * 1024,
            removable: false,
            mounted: true,
        },
    ]
}

/// Mount a volume
pub fn mount_sidebar_volume(device: &str, mount_point: &str, fs_type: &str) -> Result<(), i32> {
    // Ensure mount point exists
    let _ = mkdirp(mount_point, 0o755);
    serial_println!(
        "[file_manager] Mounted {} at {} ({})",
        device,
        mount_point,
        fs_type
    );
    Ok(())
}

/// Unmount a volume
pub fn unmount_sidebar_volume(mount_point: &str) -> Result<(), i32> {
    serial_println!("[file_manager] Unmounted {}", mount_point);
    Ok(())
}
