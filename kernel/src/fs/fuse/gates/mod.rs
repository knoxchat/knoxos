//! GATE_FUSE serial markers and known-answer self-tests.
//!
//! Each test mounts a loopback FUSE fs, runs one opcode path, and checks
//! status or bytes. Boot `fuse::init()` and `vfs_tests` call the same
//! `*_self_test` functions; serial markers stay `GATE_FUSE <name>`.
//!
//! Split by FUSE surface:
//!   io     — LOOKUP, WRITE
//!   names  — UNLINK, RENAME, RENAME2, SYMLINK, LINK, CREATE, MKNOD
//!   dir    — MKDIR, RMDIR, READDIR, READDIRPLUS, OPENDIR, RELEASEDIR, FSYNCDIR
//!   attr   — GETATTR, SETATTR, STATFS, STATX, ACCESS
//!   xattr  — SETXATTR, GETXATTR, LISTXATTR, REMOVEXATTR
//!   file   — OPEN, RELEASE, FLUSH, FSYNC, FALLOCATE, LSEEK, COPY_FILE_RANGE
//!   lock   — GETLK, SETLK, SETLKW
//!   misc   — BMAP, DESTROY, IOCTL, POLL, SYNCFS, INIT, FORGET
mod attr;
mod dir;
mod file;
mod io;
mod lock;
mod misc;
mod names;
mod xattr;

pub use attr::*;
pub use dir::*;
pub use file::*;
pub use io::*;
pub use lock::*;
pub use misc::*;
pub use names::*;
pub use xattr::*;
