use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

// ═══════════════════════════════════════════════════════════════════════
// NFS CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

pub const NFS4_PROGRAM: u32 = 100003;
pub const NFS4_VERSION: u32 = 4;
pub const NFS_PORT: u16 = 2049;

/// NFS4 error codes (nfsstat4)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NfsStat {
    Ok = 0,
    Perm = 1,
    Noent = 2,
    Io = 5,
    Nxio = 6,
    Access = 13,
    Exist = 17,
    Xdev = 18,
    Notdir = 20,
    Isdir = 21,
    Inval = 22,
    Fbig = 27,
    Nospc = 28,
    Rofs = 30,
    Nametoolong = 63,
    Notempty = 66,
    Stale = 70,
    BadHandle = 10001,
    NotSupp = 10004,
    ServerFault = 10006,
    BadType = 10007,
    Delay = 10008,
    Denied = 10010,
    Expired = 10011,
    Locked = 10012,
    Grace = 10013,
    FhExpired = 10014,
    ShareDenied = 10015,
    WrongSec = 10016,
    ClidInUse = 10017,
    Moved = 10019,
    /// NFS4ERR_NOT_SAME — VERIFY found different attributes.
    NotSameAttr = 10018,
    NoFileHandle = 10020,
    MinorVersMismatch = 10021,
    StaleClientId = 10022,
    StaleStateId = 10023,
    OldStateId = 10024,
    BadStateId = 10025,
    BadSeqId = 10026,
    NotSame = 10027,
    LockRange = 10028,
    Symlink = 10029,
    Attrnotsupp = 10032,
    LocksHeld = 10037,
    OpenMode = 10038,
    BadOwner = 10039,
    BadChar = 10040,
    BadName = 10041,
    Serverfault = 10042,
    Badiomode = 10049,
}

/// NFS4 operation codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NfsOp {
    Access = 3,
    Close = 4,
    Commit = 5,
    Create = 6,
    DelegPurge = 7,
    DelegReturn = 8,
    GetAttr = 9,
    GetFh = 10,
    Link = 11,
    Lock = 12,
    LockT = 13,
    LockU = 14,
    Lookup = 15,
    LookupP = 16,
    Nverify = 17,
    Open = 18,
    OpenAttr = 19,
    OpenConfirm = 20,
    OpenDowngrade = 21,
    PutFh = 22,
    PutPubFh = 23,
    PutRootFh = 24,
    Read = 25,
    ReadDir = 26,
    ReadLink = 27,
    Remove = 28,
    Rename = 29,
    Renew = 30,
    RestoreFh = 31,
    SaveFh = 32,
    SecInfo = 33,
    SetAttr = 34,
    SetClientId = 35,
    SetClientIdConfirm = 36,
    Verify = 37,
    Write = 38,
    ReleaseLockOwner = 39,
}

/// NFS4 file type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NfsFileType {
    Regular,
    Directory,
    BlockDevice,
    CharDevice,
    SymLink,
    Socket,
    Fifo,
    AttrDir,
    NamedAttr,
}

/// NFS4 access bits
pub const NFS4_ACCESS_READ: u32 = 0x01;
pub const NFS4_ACCESS_LOOKUP: u32 = 0x02;
pub const NFS4_ACCESS_MODIFY: u32 = 0x04;
pub const NFS4_ACCESS_EXTEND: u32 = 0x08;
pub const NFS4_ACCESS_DELETE: u32 = 0x10;
pub const NFS4_ACCESS_EXECUTE: u32 = 0x20;

/// NFS4 open flags
pub const NFS4_OPEN4_CREATE: u32 = 0x01;

/// NFS4 share access
pub const NFS4_SHARE_ACCESS_READ: u32 = 0x01;
pub const NFS4_SHARE_ACCESS_WRITE: u32 = 0x02;
pub const NFS4_SHARE_ACCESS_BOTH: u32 = 0x03;

/// NFS4 share deny
pub const NFS4_SHARE_DENY_NONE: u32 = 0x00;
pub const NFS4_SHARE_DENY_READ: u32 = 0x01;
pub const NFS4_SHARE_DENY_WRITE: u32 = 0x02;
pub const NFS4_SHARE_DENY_BOTH: u32 = 0x03;

// ═══════════════════════════════════════════════════════════════════════
// CORE DATA STRUCTURES
// ═══════════════════════════════════════════════════════════════════════

/// NFS file handle (opaque bytes, max 128)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NfsFileHandle(pub Vec<u8>);

impl NfsFileHandle {
    pub fn new(data: Vec<u8>) -> Self {
        Self(data)
    }

    pub fn root() -> Self {
        Self(vec![0, 0, 0, 1]) // Simplified root FH
    }
}

/// NFS state ID (for stateful operations)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NfsStateId {
    pub seqid: u32,
    pub other: [u8; 12],
}

impl NfsStateId {
    pub fn new(seqid: u32) -> Self {
        let mut other = [0u8; 12];
        let id = NEXT_STATE_ID.fetch_add(1, Ordering::Relaxed);
        other[0..4].copy_from_slice(&id.to_le_bytes());
        Self { seqid, other }
    }

    pub fn anonymous() -> Self {
        Self {
            seqid: 0,
            other: [0; 12],
        }
    }
}

static NEXT_STATE_ID: AtomicU32 = AtomicU32::new(1);

/// NFS4 file attributes
#[derive(Debug, Clone)]
pub struct NfsAttrs {
    pub file_type: NfsFileType,
    pub mode: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub used: u64,
    pub fileid: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub space_free: u64,
    pub space_total: u64,
    pub owner: String,
    pub group: String,
}

impl NfsAttrs {
    pub fn new_dir(fileid: u64) -> Self {
        Self {
            file_type: NfsFileType::Directory,
            mode: 0o755,
            nlink: 2,
            uid: 0,
            gid: 0,
            size: 4096,
            used: 4096,
            fileid,
            atime: 0,
            mtime: 0,
            ctime: 0,
            space_free: 0,
            space_total: 0,
            owner: String::from("root"),
            group: String::from("root"),
        }
    }

    pub fn new_file(fileid: u64, size: u64) -> Self {
        Self {
            file_type: NfsFileType::Regular,
            mode: 0o644,
            nlink: 1,
            uid: 0,
            gid: 0,
            size,
            used: size,
            fileid,
            atime: 0,
            mtime: 0,
            ctime: 0,
            space_free: 0,
            space_total: 0,
            owner: String::from("root"),
            group: String::from("root"),
        }
    }
}

/// NFS4 directory entry
#[derive(Debug, Clone)]
pub struct NfsDirEntry {
    pub name: String,
    pub fileid: u64,
    pub cookie: u64,
    pub attrs: NfsAttrs,
}

/// NFS lock type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NfsLockType {
    ReadLt = 1,
    WriteLt = 2,
    ReadW = 3,  // read + wait
    WriteW = 4, // write + wait
}

/// NFS4 lock
#[derive(Debug, Clone)]
pub struct NfsLock {
    pub lock_type: NfsLockType,
    pub offset: u64,
    pub length: u64,
    pub owner: String,
    pub state_id: NfsStateId,
}
