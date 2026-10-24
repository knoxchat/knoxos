use alloc::string::String;
use alloc::vec::Vec;

use super::types::{
    NfsAttrs, NfsDirEntry, NfsFileHandle, NfsFileType, NfsLock, NfsStat, NfsStateId,
};

// ═══════════════════════════════════════════════════════════════════════
// COMPOUND OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// An NFS4 compound operation
#[derive(Debug, Clone)]
pub enum NfsCompoundOp {
    PutRootFh,
    PutPubFh,
    PutFh(NfsFileHandle),
    Lookup(String),
    LookupP,
    GetFh,
    GetAttr(Vec<u32>), // attribute bitmap
    SetAttr(NfsStateId, NfsAttrs),
    Access(u32),
    Open {
        share_access: u32,
        share_deny: u32,
        name: String,
        create: bool,
    },
    Close(NfsStateId),
    Read {
        state_id: NfsStateId,
        offset: u64,
        count: u32,
    },
    Write {
        state_id: NfsStateId,
        offset: u64,
        data: Vec<u8>,
        stable: bool,
    },
    ReadDir {
        cookie: u64,
        dircount: u32,
        maxcount: u32,
    },
    Remove(String),
    Rename {
        old_name: String,
        new_name: String,
    },
    Create {
        name: String,
        file_type: NfsFileType,
        attrs: NfsAttrs,
        link_text: String,
    },
    Link {
        new_name: String,
    },
    ReadLink,
    Commit {
        offset: u64,
        count: u32,
    },
    Lock(NfsLock),
    Unlock(NfsLock),
    LockTest(NfsLock),
    SaveFh,
    RestoreFh,
    Verify {
        size: u64,
    },
    Nverify {
        size: u64,
    },
    Renew(u64), // client_id
    SetClientId {
        client_name: String,
    },
    SetClientIdConfirm {
        client_id: u64,
    },
    ReleaseLockOwner {
        owner: String,
    },
    OpenConfirm {
        state_id: NfsStateId,
    },
    OpenDowngrade {
        state_id: NfsStateId,
        share_access: u32,
        share_deny: u32,
    },
    SecInfo {
        name: String,
    },
    OpenAttr {
        createdir: bool,
    },
    DelegPurge {
        client_id: u64,
    },
    DelegReturn {
        state_id: NfsStateId,
    },
}

/// Result of a compound operation
#[derive(Debug, Clone)]
pub enum NfsCompoundResult {
    Status(NfsStat),
    FileHandle(NfsFileHandle),
    Attrs(NfsAttrs),
    AccessResult {
        supported: u32,
        access: u32,
    },
    ReadResult {
        eof: bool,
        data: Vec<u8>,
    },
    WriteResult {
        count: u32,
        committed: bool,
    },
    DirEntries {
        entries: Vec<NfsDirEntry>,
        eof: bool,
    },
    OpenResult {
        state_id: NfsStateId,
        fh: NfsFileHandle,
    },
    LockResult {
        state_id: NfsStateId,
    },
    ClientId {
        client_id: u64,
        verifier: u64,
    },
    LinkResult {
        name: String,
    },
    ReadLinkResult {
        target: String,
    },
    SecInfo {
        flavors: Vec<u32>,
    },
}
