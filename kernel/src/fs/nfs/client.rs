use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::serial_println;

use super::compound::{NfsCompoundOp, NfsCompoundResult};
use super::types::{
    NFS_PORT, NFS4_ACCESS_DELETE, NFS4_ACCESS_EXECUTE, NFS4_ACCESS_EXTEND, NFS4_ACCESS_LOOKUP,
    NFS4_ACCESS_MODIFY, NFS4_ACCESS_READ, NFS4_SHARE_ACCESS_READ, NFS4_SHARE_ACCESS_WRITE,
    NFS4_SHARE_DENY_NONE, NfsAttrs, NfsDirEntry, NfsFileHandle, NfsFileType, NfsLock, NfsLockType,
    NfsStat, NfsStateId,
};
use super::util::{rdtsc, simple_hash};

const NFS4_ACCESS_ALL: u32 = NFS4_ACCESS_READ
    | NFS4_ACCESS_LOOKUP
    | NFS4_ACCESS_MODIFY
    | NFS4_ACCESS_EXTEND
    | NFS4_ACCESS_DELETE
    | NFS4_ACCESS_EXECUTE;
const NFS4_ACCESS_FILE: u32 = NFS4_ACCESS_READ | NFS4_ACCESS_MODIFY | NFS4_ACCESS_EXTEND;

fn nfs_lock_write(t: NfsLockType) -> bool {
    matches!(t, NfsLockType::WriteLt | NfsLockType::WriteW)
}

fn nfs_lock_conflicts(held: &NfsLock, want: &NfsLock) -> bool {
    let a_end = held.offset.saturating_add(held.length);
    let b_end = want.offset.saturating_add(want.length);
    let overlap = held.offset < b_end && want.offset < a_end;
    overlap && (nfs_lock_write(held.lock_type) || nfs_lock_write(want.lock_type))
}

fn apply_write(file: &mut Vec<u8>, offset: u64, data: &[u8]) {
    let start = offset as usize;
    let end = start.saturating_add(data.len());
    if end > file.len() {
        file.resize(end, 0);
    }
    if start < end {
        file[start..end].copy_from_slice(data);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// NFS CLIENT
// ═══════════════════════════════════════════════════════════════════════

/// NFS4 mount options
#[derive(Debug, Clone)]
pub struct NfsMountOptions {
    pub server: [u8; 4], // IPv4 address
    pub port: u16,
    pub path: String, // export path on server
    pub mount_point: String,
    pub rsize: u32,      // read size (default 1MB)
    pub wsize: u32,      // write size (default 1MB)
    pub timeo: u32,      // timeout deciseconds
    pub retrans: u32,    // number of retries
    pub hard: bool,      // hard mount (retry forever)
    pub tcp: bool,       // TCP transport (vs UDP)
    pub ac_timeout: u64, // attribute cache timeout (seconds)
}

impl NfsMountOptions {
    pub fn default(server: [u8; 4], path: &str, mount_point: &str) -> Self {
        Self {
            server,
            port: NFS_PORT,
            path: String::from(path),
            mount_point: String::from(mount_point),
            rsize: 1048576,
            wsize: 1048576,
            timeo: 600,
            retrans: 3,
            hard: true,
            tcp: true,
            ac_timeout: 60,
        }
    }
}

/// Cached file entry
#[derive(Debug, Clone)]
struct CachedEntry {
    fh: NfsFileHandle,
    attrs: NfsAttrs,
    cached_at: u64,
}

/// NFS4 client
pub struct NfsClient {
    options: NfsMountOptions,
    client_id: u64,
    next_xid: AtomicU32,
    root_fh: NfsFileHandle,
    current_fh: Option<NfsFileHandle>,
    saved_fh: Option<NfsFileHandle>,
    open_files: BTreeMap<NfsFileHandle, (NfsStateId, u32)>,
    attr_cache: BTreeMap<String, CachedEntry>,
    dir_cache: BTreeMap<String, Vec<NfsDirEntry>>,
    locks: Vec<(NfsFileHandle, NfsLock)>,
    connected: bool,
    pub(super) export_files: BTreeMap<String, Vec<u8>>,
    export_links: BTreeMap<String, String>,
    export_dirs: BTreeSet<String>,
    uncommitted: BTreeMap<String, Vec<u8>>,
    fh_names: BTreeMap<NfsFileHandle, String>,
    fh_parents: BTreeMap<NfsFileHandle, NfsFileHandle>,
    pub(super) client_id_confirmed: bool,
}

impl NfsClient {
    pub fn new(options: NfsMountOptions) -> Self {
        Self {
            options,
            client_id: 0,
            next_xid: AtomicU32::new(1),
            root_fh: NfsFileHandle::root(),
            current_fh: None,
            saved_fh: None,
            open_files: BTreeMap::new(),
            attr_cache: BTreeMap::new(),
            dir_cache: BTreeMap::new(),
            locks: Vec::new(),
            connected: false,
            export_files: BTreeMap::new(),
            export_links: BTreeMap::new(),
            export_dirs: BTreeSet::new(),
            uncommitted: BTreeMap::new(),
            fh_names: BTreeMap::new(),
            fh_parents: BTreeMap::new(),
            client_id_confirmed: false,
        }
    }

    /// Connect to NFS server
    pub fn connect(&mut self) -> Result<(), NfsStat> {
        serial_println!(
            "[NFS] Connecting to {}.{}.{}.{}:{}",
            self.options.server[0],
            self.options.server[1],
            self.options.server[2],
            self.options.server[3],
            self.options.port
        );

        // In a real implementation, this would:
        // 1. Establish TCP connection
        // 2. Send NULL RPC for portmap
        // 3. SETCLIENTID compound
        // 4. SETCLIENTID_CONFIRM

        self.client_id = rdtsc() & 0xFFFFFFFF;
        self.connected = true;

        serial_println!("[NFS] Connected, client_id = {:#x}", self.client_id);
        Ok(())
    }

    /// Disconnect from server
    pub fn disconnect(&mut self) {
        // Close all open files
        self.open_files.clear();
        self.attr_cache.clear();
        self.dir_cache.clear();
        self.connected = false;
        serial_println!("[NFS] Disconnected");
    }

    /// Execute compound operation
    pub fn compound(&mut self, tag: &str, ops: &[NfsCompoundOp]) -> Vec<NfsCompoundResult> {
        let mut results = Vec::new();

        for op in ops {
            let result = self.execute_op(op);
            results.push(result);
        }

        results
    }

    fn execute_op(&mut self, op: &NfsCompoundOp) -> NfsCompoundResult {
        match op {
            NfsCompoundOp::PutRootFh => {
                self.current_fh = Some(self.root_fh.clone());
                NfsCompoundResult::Status(NfsStat::Ok)
            }
            NfsCompoundOp::PutPubFh => {
                self.current_fh = Some(self.root_fh.clone());
                NfsCompoundResult::Status(NfsStat::Ok)
            }
            NfsCompoundOp::PutFh(fh) => {
                if *fh == self.root_fh || self.fh_names.contains_key(fh) {
                    self.current_fh = Some(fh.clone());
                    NfsCompoundResult::Status(NfsStat::Ok)
                } else {
                    NfsCompoundResult::Status(NfsStat::Stale)
                }
            }
            NfsCompoundOp::GetFh => match &self.current_fh {
                Some(fh) => NfsCompoundResult::FileHandle(fh.clone()),
                None => NfsCompoundResult::Status(NfsStat::NoFileHandle),
            },
            NfsCompoundOp::Lookup(name) => {
                if !self.export_files.contains_key(name)
                    && !self.export_links.contains_key(name)
                    && !self.export_dirs.contains(name)
                {
                    return NfsCompoundResult::Status(NfsStat::Noent);
                }
                let parent = self.current_fh.clone();
                let mut child_fh = parent.as_ref().map(|fh| fh.0.clone()).unwrap_or_default();
                child_fh.extend_from_slice(name.as_bytes());
                let hash = simple_hash(&child_fh);
                let fh = NfsFileHandle::new(hash.to_le_bytes().to_vec());
                self.fh_names.insert(fh.clone(), name.clone());
                if let Some(parent) = parent {
                    self.fh_parents.insert(fh.clone(), parent);
                }
                self.current_fh = Some(fh);
                NfsCompoundResult::Status(NfsStat::Ok)
            }
            NfsCompoundOp::LookupP => {
                let parent = self
                    .current_fh
                    .as_ref()
                    .and_then(|fh| self.fh_parents.get(fh).cloned());
                match parent {
                    Some(fh) => {
                        self.current_fh = Some(fh);
                        NfsCompoundResult::Status(NfsStat::Ok)
                    }
                    None => NfsCompoundResult::Status(NfsStat::Noent),
                }
            }
            NfsCompoundOp::GetAttr(_bitmap) => {
                let fileid = self
                    .current_fh
                    .as_ref()
                    .map(|fh| simple_hash(&fh.0))
                    .unwrap_or(0);
                let size = self
                    .current_fh
                    .as_ref()
                    .and_then(|fh| self.fh_names.get(fh))
                    .and_then(|n| self.export_files.get(n))
                    .map(|d| d.len() as u64)
                    .unwrap_or(0);
                NfsCompoundResult::Attrs(NfsAttrs::new_file(fileid, size))
            }
            NfsCompoundOp::SetAttr(state_id, attrs) => {
                let _ = state_id;
                let name = self
                    .current_fh
                    .as_ref()
                    .and_then(|fh| self.fh_names.get(fh).cloned());
                match name {
                    Some(n) if self.export_files.contains_key(&n) => {
                        if let Some(file) = self.export_files.get_mut(&n) {
                            file.resize(attrs.size as usize, 0);
                        }
                        NfsCompoundResult::Status(NfsStat::Ok)
                    }
                    Some(_) => NfsCompoundResult::Status(NfsStat::Inval),
                    None => NfsCompoundResult::Status(NfsStat::NoFileHandle),
                }
            }
            NfsCompoundOp::Access(requested) => {
                if self.current_fh.is_none() {
                    return NfsCompoundResult::Status(NfsStat::NoFileHandle);
                }
                let name = self
                    .current_fh
                    .as_ref()
                    .and_then(|fh| self.fh_names.get(fh));
                let is_dir = name.map(|n| self.export_dirs.contains(n)).unwrap_or(true);
                let granted = if is_dir {
                    NFS4_ACCESS_ALL
                } else {
                    NFS4_ACCESS_FILE
                };
                NfsCompoundResult::AccessResult {
                    supported: NFS4_ACCESS_ALL,
                    access: *requested & granted,
                }
            }
            NfsCompoundOp::Open {
                share_access,
                share_deny,
                name,
                create,
            } => {
                let _ = (share_access, share_deny);
                let mut fh_data = self
                    .current_fh
                    .as_ref()
                    .map(|fh| fh.0.clone())
                    .unwrap_or_default();
                fh_data.extend_from_slice(name.as_bytes());
                let hash = simple_hash(&fh_data);
                let fh = NfsFileHandle::new(hash.to_le_bytes().to_vec());
                let state_id = NfsStateId::new(1);
                self.open_files
                    .insert(fh.clone(), (state_id, *share_access));
                self.fh_names.insert(fh.clone(), name.clone());
                if *create {
                    self.export_files.entry(name.clone()).or_default();
                }
                self.current_fh = Some(fh.clone());
                NfsCompoundResult::OpenResult { state_id, fh }
            }
            NfsCompoundOp::Close(state_id) => {
                let Some(fh) = self.current_fh.clone() else {
                    return NfsCompoundResult::Status(NfsStat::NoFileHandle);
                };
                match self.open_files.get(&fh) {
                    Some((open_sid, _)) if open_sid == state_id => {
                        self.open_files.remove(&fh);
                        NfsCompoundResult::Status(NfsStat::Ok)
                    }
                    _ => NfsCompoundResult::Status(NfsStat::BadStateId),
                }
            }
            NfsCompoundOp::Read {
                state_id,
                offset,
                count,
            } => {
                let _ = state_id;
                let data = self
                    .current_fh
                    .as_ref()
                    .and_then(|fh| self.fh_names.get(fh))
                    .and_then(|n| self.export_files.get(n))
                    .map(|d| {
                        let start = (*offset as usize).min(d.len());
                        let end = (start + *count as usize).min(d.len());
                        d[start..end].to_vec()
                    })
                    .unwrap_or_default();
                NfsCompoundResult::ReadResult { eof: true, data }
            }
            NfsCompoundOp::Write {
                state_id,
                offset,
                data,
                stable,
            } => {
                let _ = state_id;
                let count = if let Some(name) = self
                    .current_fh
                    .as_ref()
                    .and_then(|fh| self.fh_names.get(fh).cloned())
                {
                    if *stable {
                        let file = self.export_files.entry(name.clone()).or_default();
                        apply_write(file, *offset, data);
                        self.uncommitted.remove(&name);
                    } else {
                        let mut pending = self
                            .uncommitted
                            .get(&name)
                            .cloned()
                            .or_else(|| self.export_files.get(&name).cloned())
                            .unwrap_or_default();
                        apply_write(&mut pending, *offset, data);
                        self.uncommitted.insert(name, pending);
                    }
                    data.len() as u32
                } else {
                    0
                };
                NfsCompoundResult::WriteResult {
                    count,
                    committed: *stable,
                }
            }
            NfsCompoundOp::ReadDir {
                cookie,
                dircount,
                maxcount,
            } => {
                let _ = (dircount, maxcount);
                let mut entries = Vec::new();
                let mut next = 1u64;
                for (name, data) in &self.export_files {
                    if next > *cookie {
                        entries.push(NfsDirEntry {
                            name: name.clone(),
                            fileid: simple_hash(name.as_bytes()),
                            cookie: next,
                            attrs: NfsAttrs::new_file(
                                simple_hash(name.as_bytes()),
                                data.len() as u64,
                            ),
                        });
                    }
                    next += 1;
                }
                for (name, target) in &self.export_links {
                    if self.export_files.contains_key(name) {
                        continue;
                    }
                    if next > *cookie {
                        entries.push(NfsDirEntry {
                            name: name.clone(),
                            fileid: simple_hash(name.as_bytes()),
                            cookie: next,
                            attrs: NfsAttrs::new_file(
                                simple_hash(name.as_bytes()),
                                target.len() as u64,
                            ),
                        });
                    }
                    next += 1;
                }
                NfsCompoundResult::DirEntries { entries, eof: true }
            }
            NfsCompoundOp::Remove(name) => {
                let file = self.export_files.remove(name);
                let link = self.export_links.remove(name);
                let dir = self.export_dirs.remove(name);
                self.uncommitted.remove(name);
                if file.is_none() && link.is_none() && !dir {
                    NfsCompoundResult::Status(NfsStat::Noent)
                } else {
                    self.fh_names.retain(|_, n| n != name);
                    NfsCompoundResult::Status(NfsStat::Ok)
                }
            }
            NfsCompoundOp::Rename { old_name, new_name } => {
                let file = self.export_files.remove(old_name);
                let link = self.export_links.remove(old_name);
                let dir = self.export_dirs.remove(old_name);
                let pending = self.uncommitted.remove(old_name);
                if file.is_none() && link.is_none() && !dir {
                    NfsCompoundResult::Status(NfsStat::Noent)
                } else {
                    if let Some(data) = file {
                        self.export_files.insert(new_name.clone(), data);
                    }
                    if let Some(target) = link {
                        self.export_links.insert(new_name.clone(), target);
                    }
                    if dir {
                        self.export_dirs.insert(new_name.clone());
                    }
                    if let Some(data) = pending {
                        self.uncommitted.insert(new_name.clone(), data);
                    }
                    for name in self.fh_names.values_mut() {
                        if name == old_name {
                            *name = new_name.clone();
                        }
                    }
                    NfsCompoundResult::Status(NfsStat::Ok)
                }
            }
            NfsCompoundOp::Create {
                name,
                file_type,
                attrs,
                link_text,
            } => {
                let _ = attrs;
                if self.export_files.contains_key(name) || self.export_links.contains_key(name) {
                    NfsCompoundResult::Status(NfsStat::Exist)
                } else {
                    if matches!(file_type, NfsFileType::SymLink) {
                        self.export_links.insert(name.clone(), link_text.clone());
                        self.export_files
                            .insert(name.clone(), link_text.as_bytes().to_vec());
                    } else {
                        self.export_files.insert(name.clone(), Vec::new());
                        if matches!(file_type, NfsFileType::Directory) {
                            self.export_dirs.insert(name.clone());
                        }
                    }
                    let mut child_fh = self
                        .current_fh
                        .as_ref()
                        .map(|fh| fh.0.clone())
                        .unwrap_or_default();
                    child_fh.extend_from_slice(name.as_bytes());
                    let hash = simple_hash(&child_fh);
                    let fh = NfsFileHandle::new(hash.to_le_bytes().to_vec());
                    self.fh_names.insert(fh.clone(), name.clone());
                    self.current_fh = Some(fh);
                    if matches!(file_type, NfsFileType::Directory) {
                        NfsCompoundResult::Attrs(NfsAttrs::new_dir(hash))
                    } else {
                        NfsCompoundResult::Status(NfsStat::Ok)
                    }
                }
            }
            NfsCompoundOp::Link { new_name } => {
                let src_name = self
                    .saved_fh
                    .as_ref()
                    .and_then(|fh| self.fh_names.get(fh).cloned());
                match src_name {
                    Some(src)
                        if self.export_files.contains_key(&src)
                            || self.export_links.contains_key(&src) =>
                    {
                        if let Some(data) = self.export_files.get(&src).cloned() {
                            self.export_files.insert(new_name.clone(), data);
                        }
                        if let Some(target) = self.export_links.get(&src).cloned() {
                            self.export_links.insert(new_name.clone(), target);
                        }
                        if self.export_dirs.contains(&src) {
                            self.export_dirs.insert(new_name.clone());
                        }
                        NfsCompoundResult::LinkResult {
                            name: new_name.clone(),
                        }
                    }
                    Some(_) => NfsCompoundResult::Status(NfsStat::Noent),
                    None => NfsCompoundResult::Status(NfsStat::NoFileHandle),
                }
            }
            NfsCompoundOp::ReadLink => {
                let target = self
                    .current_fh
                    .as_ref()
                    .and_then(|fh| self.fh_names.get(fh))
                    .and_then(|n| self.export_links.get(n).cloned());
                match target {
                    Some(t) => NfsCompoundResult::ReadLinkResult { target: t },
                    None => NfsCompoundResult::Status(NfsStat::Inval),
                }
            }
            NfsCompoundOp::Commit { offset, count } => {
                let name = self
                    .current_fh
                    .as_ref()
                    .and_then(|fh| self.fh_names.get(fh).cloned());
                match name {
                    Some(n) => {
                        if let Some(pending) = self.uncommitted.remove(&n) {
                            let file = self.export_files.entry(n).or_default();
                            if *count == 0 {
                                *file = pending;
                            } else {
                                let start = *offset as usize;
                                let end = start.saturating_add(*count as usize).min(pending.len());
                                if end > file.len() {
                                    file.resize(end, 0);
                                }
                                if start < end {
                                    file[start..end].copy_from_slice(&pending[start..end]);
                                }
                            }
                        }
                        NfsCompoundResult::Status(NfsStat::Ok)
                    }
                    None => NfsCompoundResult::Status(NfsStat::NoFileHandle),
                }
            }
            NfsCompoundOp::Lock(lock) => {
                let Some(fh) = self.current_fh.clone() else {
                    return NfsCompoundResult::Status(NfsStat::NoFileHandle);
                };
                if self
                    .locks
                    .iter()
                    .any(|(held_fh, held)| *held_fh == fh && nfs_lock_conflicts(held, lock))
                {
                    return NfsCompoundResult::Status(NfsStat::Denied);
                }
                self.locks.push((fh, lock.clone()));
                NfsCompoundResult::LockResult {
                    state_id: NfsStateId::new(1),
                }
            }
            NfsCompoundOp::Unlock(lock) => {
                let Some(fh) = self.current_fh.clone() else {
                    return NfsCompoundResult::Status(NfsStat::NoFileHandle);
                };
                let before = self.locks.len();
                self.locks.retain(|(held_fh, held)| {
                    *held_fh != fh
                        || held.offset != lock.offset
                        || held.length != lock.length
                        || held.owner != lock.owner
                });
                if self.locks.len() == before {
                    NfsCompoundResult::Status(NfsStat::Denied)
                } else {
                    NfsCompoundResult::Status(NfsStat::Ok)
                }
            }
            NfsCompoundOp::LockTest(lock) => {
                let Some(fh) = self.current_fh.clone() else {
                    return NfsCompoundResult::Status(NfsStat::NoFileHandle);
                };
                let conflict = self
                    .locks
                    .iter()
                    .any(|(held_fh, held)| *held_fh == fh && nfs_lock_conflicts(held, lock));
                if conflict {
                    NfsCompoundResult::Status(NfsStat::Denied)
                } else {
                    NfsCompoundResult::Status(NfsStat::Ok)
                }
            }
            NfsCompoundOp::SaveFh => {
                self.saved_fh = self.current_fh.clone();
                NfsCompoundResult::Status(NfsStat::Ok)
            }
            NfsCompoundOp::RestoreFh => match self.saved_fh.clone() {
                Some(fh) => {
                    self.current_fh = Some(fh);
                    NfsCompoundResult::Status(NfsStat::Ok)
                }
                None => NfsCompoundResult::Status(NfsStat::NoFileHandle),
            },
            NfsCompoundOp::Verify { size } => {
                if self.current_fh.is_none() {
                    return NfsCompoundResult::Status(NfsStat::NoFileHandle);
                }
                let actual = self
                    .current_fh
                    .as_ref()
                    .and_then(|fh| self.fh_names.get(fh))
                    .and_then(|n| self.export_files.get(n))
                    .map(|d| d.len() as u64)
                    .unwrap_or(0);
                if actual == *size {
                    NfsCompoundResult::Status(NfsStat::Ok)
                } else {
                    NfsCompoundResult::Status(NfsStat::NotSameAttr)
                }
            }
            NfsCompoundOp::Nverify { size } => {
                if self.current_fh.is_none() {
                    return NfsCompoundResult::Status(NfsStat::NoFileHandle);
                }
                let actual = self
                    .current_fh
                    .as_ref()
                    .and_then(|fh| self.fh_names.get(fh))
                    .and_then(|n| self.export_files.get(n))
                    .map(|d| d.len() as u64)
                    .unwrap_or(0);
                if actual == *size {
                    NfsCompoundResult::Status(NfsStat::NotSame)
                } else {
                    NfsCompoundResult::Status(NfsStat::Ok)
                }
            }
            NfsCompoundOp::Renew(client_id) => {
                if self.client_id_confirmed && self.client_id != 0 && *client_id == self.client_id {
                    NfsCompoundResult::Status(NfsStat::Ok)
                } else {
                    NfsCompoundResult::Status(NfsStat::StaleClientId)
                }
            }
            NfsCompoundOp::SetClientId { client_name } => {
                self.client_id = simple_hash(client_name.as_bytes());
                self.client_id_confirmed = false;
                NfsCompoundResult::ClientId {
                    client_id: self.client_id,
                    verifier: rdtsc(),
                }
            }
            NfsCompoundOp::SetClientIdConfirm { client_id } => {
                if self.client_id != 0 && *client_id == self.client_id {
                    self.client_id_confirmed = true;
                    NfsCompoundResult::Status(NfsStat::Ok)
                } else {
                    NfsCompoundResult::Status(NfsStat::StaleClientId)
                }
            }
            NfsCompoundOp::ReleaseLockOwner { owner } => {
                let Some(fh) = self.current_fh.clone() else {
                    return NfsCompoundResult::Status(NfsStat::NoFileHandle);
                };
                self.locks
                    .retain(|(held_fh, held)| *held_fh != fh || held.owner != *owner);
                NfsCompoundResult::Status(NfsStat::Ok)
            }
            NfsCompoundOp::OpenConfirm { state_id } => {
                let Some(fh) = self.current_fh.clone() else {
                    return NfsCompoundResult::Status(NfsStat::NoFileHandle);
                };
                match self.open_files.get(&fh) {
                    Some((open_sid, _)) if open_sid == state_id => NfsCompoundResult::OpenResult {
                        state_id: *open_sid,
                        fh,
                    },
                    _ => NfsCompoundResult::Status(NfsStat::BadStateId),
                }
            }
            NfsCompoundOp::OpenDowngrade {
                state_id,
                share_access,
                share_deny,
            } => {
                let _ = share_deny;
                let Some(fh) = self.current_fh.clone() else {
                    return NfsCompoundResult::Status(NfsStat::NoFileHandle);
                };
                match self.open_files.get_mut(&fh) {
                    Some((open_sid, share)) if *open_sid == *state_id => {
                        if share_access & *share != *share_access {
                            return NfsCompoundResult::Status(NfsStat::Inval);
                        }
                        *share = *share_access;
                        NfsCompoundResult::OpenResult {
                            state_id: *open_sid,
                            fh,
                        }
                    }
                    _ => NfsCompoundResult::Status(NfsStat::BadStateId),
                }
            }
        }
    }

    // ─── High-level file operations ────────────────────────────────────

    /// Read a file by path
    pub fn read_file(&mut self, path: &str) -> Result<Vec<u8>, NfsStat> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        // Build compound: PUTROOTFH, LOOKUP..., OPEN, READ, CLOSE
        let mut ops = vec![NfsCompoundOp::PutRootFh];
        for part in &parts[..parts.len().saturating_sub(1)] {
            ops.push(NfsCompoundOp::Lookup(String::from(*part)));
        }

        if let Some(filename) = parts.last() {
            ops.push(NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_READ,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from(*filename),
                create: false,
            });
        }

        let state_id = NfsStateId::anonymous();
        ops.push(NfsCompoundOp::Read {
            state_id,
            offset: 0,
            count: self.options.rsize,
        });

        let results = self.compound("read", &ops);

        // Extract data from results
        for result in &results {
            if let NfsCompoundResult::ReadResult { data, .. } = result {
                return Ok(data.clone());
            }
        }

        Err(NfsStat::Io)
    }

    /// Write a file by path
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> Result<u32, NfsStat> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        let mut ops = vec![NfsCompoundOp::PutRootFh];
        for part in &parts[..parts.len().saturating_sub(1)] {
            ops.push(NfsCompoundOp::Lookup(String::from(*part)));
        }

        if let Some(filename) = parts.last() {
            ops.push(NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_WRITE,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from(*filename),
                create: true,
            });
        }

        let state_id = NfsStateId::anonymous();
        ops.push(NfsCompoundOp::Write {
            state_id,
            offset: 0,
            data: data.to_vec(),
            stable: true,
        });

        let results = self.compound("write", &ops);

        for result in &results {
            if let NfsCompoundResult::WriteResult { count, .. } = result {
                return Ok(*count);
            }
        }

        Err(NfsStat::Io)
    }

    /// List directory
    pub fn readdir(&mut self, path: &str) -> Result<Vec<NfsDirEntry>, NfsStat> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        let mut ops = vec![NfsCompoundOp::PutRootFh];
        for part in &parts {
            ops.push(NfsCompoundOp::Lookup(String::from(*part)));
        }
        ops.push(NfsCompoundOp::ReadDir {
            cookie: 0,
            dircount: 8192,
            maxcount: 32768,
        });

        let results = self.compound("readdir", &ops);

        for result in &results {
            if let NfsCompoundResult::DirEntries { entries, .. } = result {
                return Ok(entries.clone());
            }
        }

        Err(NfsStat::Io)
    }

    /// Get file attributes
    pub fn stat(&mut self, path: &str) -> Result<NfsAttrs, NfsStat> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        let mut ops = vec![NfsCompoundOp::PutRootFh];
        for part in &parts {
            ops.push(NfsCompoundOp::Lookup(String::from(*part)));
        }
        ops.push(NfsCompoundOp::GetAttr(vec![0, 1]));

        let results = self.compound("stat", &ops);

        for result in &results {
            if let NfsCompoundResult::Attrs(attrs) = result {
                return Ok(attrs.clone());
            }
        }

        Err(NfsStat::Io)
    }

    /// Remove file
    pub fn remove(&mut self, path: &str) -> Result<(), NfsStat> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return Err(NfsStat::Inval);
        }

        let mut ops = vec![NfsCompoundOp::PutRootFh];
        for part in &parts[..parts.len() - 1] {
            ops.push(NfsCompoundOp::Lookup(String::from(*part)));
        }
        ops.push(NfsCompoundOp::Remove(String::from(*parts.last().unwrap())));

        let results = self.compound("remove", &ops);
        // Check last result for status
        if let Some(NfsCompoundResult::Status(stat)) = results.last() {
            if *stat == NfsStat::Ok {
                return Ok(());
            }
            return Err(*stat);
        }
        Ok(())
    }

    /// Make directory
    pub fn mkdir(&mut self, path: &str) -> Result<(), NfsStat> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return Err(NfsStat::Inval);
        }

        let mut ops = vec![NfsCompoundOp::PutRootFh];
        for part in &parts[..parts.len() - 1] {
            ops.push(NfsCompoundOp::Lookup(String::from(*part)));
        }
        ops.push(NfsCompoundOp::Create {
            name: String::from(*parts.last().unwrap()),
            file_type: NfsFileType::Directory,
            attrs: NfsAttrs::new_dir(0),
            link_text: String::new(),
        });

        let results = self.compound("mkdir", &ops);
        Ok(())
    }
}
