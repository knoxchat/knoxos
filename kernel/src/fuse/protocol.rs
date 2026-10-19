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
    Access = 34,
    Create = 35,
    Destroy = 38,
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
