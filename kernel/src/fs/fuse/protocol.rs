//! FUSE protocol types, opcodes, and packed encode/decode.

use alloc::string::String;
use alloc::vec::Vec;

/// FUSE operation codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum FuseOpcode {
    Lookup = 1,
    Forget = 2,
    Getattr = 3,
    Setattr = 4,
    Readlink = 5,
    Symlink = 6,
    Mknod = 8,
    Mkdir = 9,
    Unlink = 10,
    Rmdir = 11,
    Rename = 12,
    Link = 13,
    Open = 14,
    Read = 15,
    Write = 16,
    Statfs = 17,
    Release = 18,
    Fsync = 20,
    Setxattr = 21,
    Getxattr = 22,
    Listxattr = 23,
    Removexattr = 24,
    Flush = 25,
    Init = 26,
    Opendir = 27,
    Readdir = 28,
    Releasedir = 29,
    Fsyncdir = 30,
    Getlk = 31,
    Setlk = 32,
    Setlkw = 33,
    Access = 34,
    Create = 35,
    Bmap = 37,
    Destroy = 38,
    Ioctl = 39,
    Poll = 40,
    Fallocate = 43,
    Readdirplus = 44,
    Rename2 = 45,
    Lseek = 46,
    CopyFileRange = 47,
    Syncfs = 50,
    Statx = 52,
}

/// Linux `FUSE_KERNEL_VERSION`.
pub const FUSE_KERNEL_VERSION: u32 = 7;
/// Linux `FUSE_KERNEL_MINOR_VERSION` (7.31 is FALLOCATE/LSEEK-era).
pub const FUSE_KERNEL_MINOR_VERSION: u32 = 31;

/// POSIX `R_OK` — FUSE ACCESS request/grant bit.
pub const FUSE_R_OK: u32 = 4;
/// POSIX `W_OK` — FUSE ACCESS request/grant bit.
pub const FUSE_W_OK: u32 = 2;
/// POSIX `X_OK` — FUSE ACCESS request/grant bit.
pub const FUSE_X_OK: u32 = 1;

/// Owner rwx bits of `mode` as FUSE ACCESS grant bits.
pub fn mode_to_access(mode: u32) -> u32 {
    let mut granted = 0;
    if mode & 0o400 != 0 {
        granted |= FUSE_R_OK;
    }
    if mode & 0o200 != 0 {
        granted |= FUSE_W_OK;
    }
    if mode & 0o100 != 0 {
        granted |= FUSE_X_OK;
    }
    granted
}

/// FUSE request header (kernel → userspace)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct FuseInHeader {
    pub len: u32,
    pub opcode: u32,
    pub unique: u64,
    pub nodeid: u64,
    pub uid: u32,
    pub gid: u32,
    pub pid: u32,
    pub padding: u32,
}

/// FUSE response header (userspace → kernel)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct FuseOutHeader {
    pub len: u32,
    pub error: i32,
    pub unique: u64,
}

/// FUSE file attributes
#[derive(Debug, Clone)]
pub struct FuseAttr {
    pub ino: u64,
    pub size: u64,
    pub blocks: u64,
    pub mode: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
}

/// FUSE entry (result of lookup/create)
#[derive(Debug, Clone)]
pub struct FuseEntry {
    pub nodeid: u64,
    pub generation: u64,
    pub attr: FuseAttr,
    pub attr_valid_secs: u64,
    pub entry_valid_secs: u64,
}

pub(super) fn encode_entry(ino: u64, size: u64, mode: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(20);
    out.extend_from_slice(&ino.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&mode.to_le_bytes());
    out
}

/// FUSE entry: ino u64, size u64, mode u32.
pub fn parse_fuse_entry(data: &[u8]) -> Option<(u64, u64, u32)> {
    if data.len() < 20 {
        return None;
    }
    let ino = u64::from_le_bytes(data[0..8].try_into().ok()?);
    let size = u64::from_le_bytes(data[8..16].try_into().ok()?);
    let mode = u32::from_le_bytes(data[16..20].try_into().ok()?);
    Some((ino, size, mode))
}

pub(super) fn encode_dirent(ino: u64, mode: u32, name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + name.len());
    out.extend_from_slice(&ino.to_le_bytes());
    out.extend_from_slice(&mode.to_le_bytes());
    out.extend_from_slice(&(name.len() as u32).to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out
}

/// Packed FUSE dirents: ino u64, mode u32, namelen u32, name bytes.
pub fn parse_fuse_dirents(data: &[u8]) -> Vec<(u64, u32, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 16 <= data.len() {
        let ino = u64::from_le_bytes(data[i..i + 8].try_into().unwrap_or([0; 8]));
        let mode = u32::from_le_bytes(data[i + 8..i + 12].try_into().unwrap_or([0; 4]));
        let nlen = u32::from_le_bytes(data[i + 12..i + 16].try_into().unwrap_or([0; 4])) as usize;
        i += 16;
        if i + nlen > data.len() {
            break;
        }
        let Ok(name) = core::str::from_utf8(&data[i..i + nlen]) else {
            break;
        };
        out.push((ino, mode, String::from(name)));
        i += nlen;
    }
    out
}

/// FUSE STATFS reply: bsize u32, blocks u64, bfree u64, files u64, namelen u32.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuseStatfs {
    pub bsize: u32,
    pub blocks: u64,
    pub bfree: u64,
    pub files: u64,
    pub namelen: u32,
}

pub(super) fn encode_statfs(st: &FuseStatfs) -> Vec<u8> {
    let mut out = Vec::with_capacity(32);
    out.extend_from_slice(&st.bsize.to_le_bytes());
    out.extend_from_slice(&st.blocks.to_le_bytes());
    out.extend_from_slice(&st.bfree.to_le_bytes());
    out.extend_from_slice(&st.files.to_le_bytes());
    out.extend_from_slice(&st.namelen.to_le_bytes());
    out
}

/// Packed FUSE STATFS: bsize u32, blocks u64, bfree u64, files u64, namelen u32.
pub fn parse_fuse_statfs(data: &[u8]) -> Option<FuseStatfs> {
    if data.len() < 32 {
        return None;
    }
    Some(FuseStatfs {
        bsize: u32::from_le_bytes(data[0..4].try_into().ok()?),
        blocks: u64::from_le_bytes(data[4..12].try_into().ok()?),
        bfree: u64::from_le_bytes(data[12..20].try_into().ok()?),
        files: u64::from_le_bytes(data[20..28].try_into().ok()?),
        namelen: u32::from_le_bytes(data[28..32].try_into().ok()?),
    })
}

/// FUSE OPEN reply: fh u64, flags u32.
pub(super) fn encode_open(fh: u64, flags: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(&fh.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out
}

/// Packed FUSE OPEN: fh u64, flags u32.
pub fn parse_fuse_open(data: &[u8]) -> Option<(u64, u32)> {
    if data.len() < 12 {
        return None;
    }
    Some((
        u64::from_le_bytes(data[0..8].try_into().ok()?),
        u32::from_le_bytes(data[8..12].try_into().ok()?),
    ))
}

/// POSIX `F_RDLCK`.
pub const F_RDLCK: u32 = 0;
/// POSIX `F_WRLCK`.
pub const F_WRLCK: u32 = 1;
/// POSIX `F_UNLCK`.
pub const F_UNLCK: u32 = 2;

/// FUSE GETLK/SETLK file lock: start, end, type, pid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FuseFileLock {
    pub start: u64,
    pub end: u64,
    pub typ: u32,
    pub pid: u32,
}

pub(super) fn encode_lk(lk: &FuseFileLock) -> Vec<u8> {
    let mut out = Vec::with_capacity(24);
    out.extend_from_slice(&lk.start.to_le_bytes());
    out.extend_from_slice(&lk.end.to_le_bytes());
    out.extend_from_slice(&lk.typ.to_le_bytes());
    out.extend_from_slice(&lk.pid.to_le_bytes());
    out
}

/// Packed FUSE lock: start u64, end u64, type u32, pid u32.
pub fn parse_fuse_lk(data: &[u8]) -> Option<FuseFileLock> {
    if data.len() < 24 {
        return None;
    }
    Some(FuseFileLock {
        start: u64::from_le_bytes(data[0..8].try_into().ok()?),
        end: u64::from_le_bytes(data[8..16].try_into().ok()?),
        typ: u32::from_le_bytes(data[16..20].try_into().ok()?),
        pid: u32::from_le_bytes(data[20..24].try_into().ok()?),
    })
}

pub(super) fn locks_overlap(a: &FuseFileLock, b: &FuseFileLock) -> bool {
    a.start < b.end && b.start < a.end
}

pub(super) fn lock_conflicts(held: &FuseFileLock, want: &FuseFileLock) -> bool {
    if held.pid == want.pid || !locks_overlap(held, want) {
        return false;
    }
    held.typ == F_WRLCK || want.typ == F_WRLCK
}

pub(super) fn encode_bmap(block: u64) -> Vec<u8> {
    block.to_le_bytes().to_vec()
}

/// Packed FUSE BMAP reply: physical block u64.
pub fn parse_fuse_bmap(data: &[u8]) -> Option<u64> {
    if data.len() < 8 {
        return None;
    }
    Some(u64::from_le_bytes(data[0..8].try_into().ok()?))
}

pub(super) fn encode_ioctl(result: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    out.extend_from_slice(&result.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

/// Packed FUSE IOCTL reply: result i32 plus flags/iov counts.
pub fn parse_fuse_ioctl(data: &[u8]) -> Option<i32> {
    if data.len() < 4 {
        return None;
    }
    Some(i32::from_le_bytes(data[0..4].try_into().ok()?))
}

/// Linux `POLLIN`.
pub const POLLIN: u32 = 0x0001;
/// Linux `POLLOUT`.
pub const POLLOUT: u32 = 0x0004;

pub(super) fn encode_poll(revents: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(&revents.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

/// Packed FUSE POLL reply: revents u32 plus padding.
pub fn parse_fuse_poll(data: &[u8]) -> Option<u32> {
    if data.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes(data[0..4].try_into().ok()?))
}

/// Linux `RENAME_NOREPLACE` — fail if the destination exists.
pub const RENAME_NOREPLACE: u32 = 1 << 0;

pub(super) fn encode_direntplus(ino: u64, size: u64, mode: u32, name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(24 + name.len());
    out.extend_from_slice(&ino.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&mode.to_le_bytes());
    out.extend_from_slice(&(name.len() as u32).to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out
}

/// POSIX `SEEK_SET`.
pub const SEEK_SET: u32 = 0;
/// POSIX `SEEK_END`.
pub const SEEK_END: u32 = 2;

pub(super) fn encode_lseek(offset: u64) -> Vec<u8> {
    offset.to_le_bytes().to_vec()
}

/// Packed FUSE LSEEK reply: offset u64.
pub fn parse_fuse_lseek(data: &[u8]) -> Option<u64> {
    if data.len() < 8 {
        return None;
    }
    Some(u64::from_le_bytes(data[0..8].try_into().ok()?))
}

pub(super) fn encode_copy_file_range(copied: u64) -> Vec<u8> {
    copied.to_le_bytes().to_vec()
}

/// Packed FUSE COPY_FILE_RANGE reply: bytes copied u64.
pub fn parse_fuse_copy_file_range(data: &[u8]) -> Option<u64> {
    if data.len() < 8 {
        return None;
    }
    Some(u64::from_le_bytes(data[0..8].try_into().ok()?))
}

/// Linux `STATX_BASIC_STATS` (type, mode, nlink, uid, gid, atime, mtime, ctime, ino, size, blocks).
pub const STATX_BASIC_STATS: u32 = 0x0000_07ff;

pub(super) fn encode_statx(ino: u64, size: u64, mode: u32, mask: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(24);
    out.extend_from_slice(&ino.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&mode.to_le_bytes());
    out.extend_from_slice(&mask.to_le_bytes());
    out
}

pub(super) fn encode_init(major: u32, minor: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(&major.to_le_bytes());
    out.extend_from_slice(&minor.to_le_bytes());
    out
}

/// Packed FUSE INIT reply: major u32, minor u32.
pub fn parse_fuse_init(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 8 {
        return None;
    }
    Some((
        u32::from_le_bytes(data[0..4].try_into().ok()?),
        u32::from_le_bytes(data[4..8].try_into().ok()?),
    ))
}

/// Packed FUSE STATX: ino u64, size u64, mode u32, mask u32.
pub fn parse_fuse_statx(data: &[u8]) -> Option<(u64, u64, u32, u32)> {
    if data.len() < 24 {
        return None;
    }
    Some((
        u64::from_le_bytes(data[0..8].try_into().ok()?),
        u64::from_le_bytes(data[8..16].try_into().ok()?),
        u32::from_le_bytes(data[16..20].try_into().ok()?),
        u32::from_le_bytes(data[20..24].try_into().ok()?),
    ))
}

/// Packed FUSE READDIRPLUS: ino u64, size u64, mode u32, namelen u32, name bytes.
pub fn parse_fuse_direntplus(data: &[u8]) -> Vec<(u64, u64, u32, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 24 <= data.len() {
        let ino = u64::from_le_bytes(data[i..i + 8].try_into().unwrap_or([0; 8]));
        let size = u64::from_le_bytes(data[i + 8..i + 16].try_into().unwrap_or([0; 8]));
        let mode = u32::from_le_bytes(data[i + 16..i + 20].try_into().unwrap_or([0; 4]));
        let nlen = u32::from_le_bytes(data[i + 20..i + 24].try_into().unwrap_or([0; 4])) as usize;
        i += 24;
        if i + nlen > data.len() {
            break;
        }
        let Ok(name) = core::str::from_utf8(&data[i..i + nlen]) else {
            break;
        };
        out.push((ino, size, mode, String::from(name)));
        i += nlen;
    }
    out
}
