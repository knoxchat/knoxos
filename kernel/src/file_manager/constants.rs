//! Linux-compatible error codes, filesystem limits, *at() flags, and mode bits.

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
