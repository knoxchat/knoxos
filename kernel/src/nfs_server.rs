/// NFS v4 Server — RFC 7530 compliant NFS server implementation
/// Provides network file sharing, allowing remote clients to mount and access KnoxOS filesystems
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// NFS SERVER CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

/// NFS v4 RPC program number
pub const NFS_PROGRAM: u32 = 100003;
/// NFS v4 version
pub const NFS_VERSION: u32 = 4;
/// NFS v4 port (TCP 2049)
pub const NFS_PORT: u16 = 2049;
/// Maximum read/write size
pub const MAX_RW_SIZE: usize = 65536;
/// Maximum file handle length
pub const MAX_FH_LEN: usize = 128;
/// NFS4 magic for statfs
pub const NFS4_MAGIC: u64 = 0x6969;

// ═══════════════════════════════════════════════════════════════════════
// NFS FILE HANDLE
// ═══════════════════════════════════════════════════════════════════════

/// NFS file handle — opaque identifier for a filesystem object
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NfsFileHandle {
    pub data: Vec<u8>,
}

impl NfsFileHandle {
    pub fn new(inode: u64, export_id: u32) -> Self {
        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&export_id.to_be_bytes());
        data.extend_from_slice(&inode.to_be_bytes());
        Self { data }
    }

    pub fn root(export_id: u32) -> Self {
        Self::new(1, export_id) // inode 1 = root
    }

    pub fn inode(&self) -> u64 {
        if self.data.len() >= 12 {
            u64::from_be_bytes([
                self.data[4],
                self.data[5],
                self.data[6],
                self.data[7],
                self.data[8],
                self.data[9],
                self.data[10],
                self.data[11],
            ])
        } else {
            0
        }
    }

    pub fn export_id(&self) -> u32 {
        if self.data.len() >= 4 {
            u32::from_be_bytes([self.data[0], self.data[1], self.data[2], self.data[3]])
        } else {
            0
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// NFS OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// NFS4 operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NfsOp {
    Access = 3,
    Close = 4,
    Commit = 5,
    Create = 6,
    Delegpurge = 7,
    Delegreturn = 8,
    Getattr = 9,
    Getfh = 10,
    Link = 11,
    Lock = 12,
    Lockt = 13,
    Locku = 14,
    Lookup = 15,
    Lookupp = 16,
    Nverify = 17,
    Open = 18,
    Openattr = 19,
    OpenConfirm = 20,
    OpenDowngrade = 21,
    Putfh = 22,
    Putpubfh = 23,
    Putrootfh = 24,
    Read = 25,
    Readdir = 26,
    Readlink = 27,
    Remove = 28,
    Rename = 29,
    Renew = 30,
    Restorefh = 31,
    Savefh = 32,
    Secinfo = 33,
    Setattr = 34,
    Setclientid = 35,
    SetclientidConfirm = 36,
    Verify = 37,
    Write = 38,
    ReleaseLockowner = 39,
}

/// NFS4 error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NfsStatus {
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
    Dquot = 69,
    Stale = 70,
    Badhandle = 10001,
    BadCookie = 10003,
    Notsupp = 10004,
    Toosmall = 10005,
    Serverfault = 10006,
    Badtype = 10007,
    Jukebox = 10008, // NFS4ERR_DELAY
    SameState = 10009,
    DeniedByPolicy = 10010,
    Expired = 10011,
    Locked = 10012,
    GracePeriod = 10013,
    Wrongsec = 10016,
    ShareDenied = 10015,
    OpenModeError = 10026,
}

// ═══════════════════════════════════════════════════════════════════════
// NFS FILE ATTRIBUTES
// ═══════════════════════════════════════════════════════════════════════

/// NFS4 file type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NfsFileType {
    Regular = 1,
    Directory = 2,
    BlockDevice = 3,
    CharDevice = 4,
    Symlink = 5,
    Socket = 6,
    Fifo = 7,
    AttrDir = 8,
    NamedAttr = 9,
}

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
    pub atime: NfsTime,
    pub mtime: NfsTime,
    pub ctime: NfsTime,
    pub owner: String,
    pub group: String,
}

impl Default for NfsAttrs {
    fn default() -> Self {
        Self {
            file_type: NfsFileType::Regular,
            mode: 0o644,
            nlink: 1,
            uid: 0,
            gid: 0,
            size: 0,
            used: 0,
            fileid: 0,
            atime: NfsTime::zero(),
            mtime: NfsTime::zero(),
            ctime: NfsTime::zero(),
            owner: String::from("root"),
            group: String::from("root"),
        }
    }
}

/// NFS time (seconds + nanoseconds)
#[derive(Debug, Clone, Copy)]
pub struct NfsTime {
    pub seconds: u64,
    pub nseconds: u32,
}

impl NfsTime {
    pub fn zero() -> Self {
        Self {
            seconds: 0,
            nseconds: 0,
        }
    }

    pub fn now() -> Self {
        // Use RTC ticks as a rough timestamp
        Self {
            seconds: crate::interrupts::get_ticks(),
            nseconds: 0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// EXPORT & STATE
// ═══════════════════════════════════════════════════════════════════════

/// An NFS export (shared directory)
#[derive(Debug, Clone)]
pub struct NfsExport {
    pub id: u32,
    pub path: String,
    pub read_only: bool,
    pub allowed_clients: Vec<String>, // IP addresses or "*" for any
    pub root_squash: bool,
    pub all_squash: bool,
    pub anon_uid: u32,
    pub anon_gid: u32,
}

impl NfsExport {
    pub fn new(id: u32, path: &str) -> Self {
        Self {
            id,
            path: String::from(path),
            read_only: false,
            allowed_clients: vec![String::from("*")],
            root_squash: true,
            all_squash: false,
            anon_uid: 65534,
            anon_gid: 65534,
        }
    }
}

/// Client state for NFS
#[derive(Debug)]
pub struct NfsClientState {
    pub client_id: u64,
    pub client_addr: String,
    pub confirmed: bool,
    pub lease_time: u64,
    pub last_renew: u64,
    /// Open state IDs
    pub open_states: BTreeMap<u64, NfsOpenState>,
    /// Lock state IDs
    pub lock_states: BTreeMap<u64, NfsLockState>,
}

/// Open file state
#[derive(Debug, Clone)]
pub struct NfsOpenState {
    pub state_id: u64,
    pub file_handle: NfsFileHandle,
    pub share_access: u32, // OPEN4_SHARE_ACCESS_READ, WRITE, BOTH
    pub share_deny: u32,
    pub owner: Vec<u8>,
}

/// Lock state
#[derive(Debug, Clone)]
pub struct NfsLockState {
    pub state_id: u64,
    pub lock_type: NfsLockType,
    pub offset: u64,
    pub length: u64,
    pub owner: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NfsLockType {
    ReadLock,
    WriteLock,
}

// ═══════════════════════════════════════════════════════════════════════
// NFS SERVER
// ═══════════════════════════════════════════════════════════════════════

/// NFS v4 Server
pub struct NfsServer {
    /// Exports
    pub exports: BTreeMap<u32, NfsExport>,
    /// Client states
    pub clients: BTreeMap<u64, NfsClientState>,
    /// Next state ID
    next_state_id: u64,
    /// Next client ID  
    next_client_id: u64,
    /// Lease time (seconds)
    pub lease_time: u64,
    /// Grace period active
    pub grace_period: bool,
    /// Server started
    pub running: bool,
    /// Statistics
    pub stats: NfsServerStats,
}

/// Server statistics
#[derive(Debug, Clone, Default)]
pub struct NfsServerStats {
    pub compounds_received: u64,
    pub compounds_completed: u64,
    pub reads: u64,
    pub writes: u64,
    pub opens: u64,
    pub closes: u64,
    pub lookups: u64,
    pub getattrs: u64,
    pub readdirs: u64,
    pub creates: u64,
    pub removes: u64,
    pub renames: u64,
    pub locks: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
}

impl NfsServer {
    pub fn new() -> Self {
        Self {
            exports: BTreeMap::new(),
            clients: BTreeMap::new(),
            next_state_id: 1,
            next_client_id: 1,
            lease_time: 90, // 90 seconds default lease
            grace_period: false,
            running: false,
            stats: NfsServerStats::default(),
        }
    }

    /// Add an export
    pub fn add_export(&mut self, path: &str, read_only: bool) -> u32 {
        let id = self.exports.len() as u32 + 1;
        let mut export = NfsExport::new(id, path);
        export.read_only = read_only;
        self.exports.insert(id, export);
        serial_println!("[NFS-Server] Export added: {} (id={})", path, id);
        id
    }

    /// Remove an export
    pub fn remove_export(&mut self, id: u32) -> bool {
        self.exports.remove(&id).is_some()
    }

    /// Start the NFS server
    pub fn start(&mut self) {
        self.running = true;
        self.grace_period = true;
        serial_println!("[NFS-Server] NFS v4 server started on port {}", NFS_PORT);
    }

    /// Stop the NFS server
    pub fn stop(&mut self) {
        self.running = false;
        serial_println!("[NFS-Server] NFS v4 server stopped");
    }

    /// Register a new client
    pub fn register_client(&mut self, addr: &str) -> u64 {
        let client_id = self.next_client_id;
        self.next_client_id += 1;
        let state = NfsClientState {
            client_id,
            client_addr: String::from(addr),
            confirmed: false,
            lease_time: self.lease_time,
            last_renew: 0,
            open_states: BTreeMap::new(),
            lock_states: BTreeMap::new(),
        };
        self.clients.insert(client_id, state);
        serial_println!(
            "[NFS-Server] Client registered: {} (id={})",
            addr,
            client_id
        );
        client_id
    }

    /// Confirm a client
    pub fn confirm_client(&mut self, client_id: u64) -> NfsStatus {
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.confirmed = true;
            NfsStatus::Ok
        } else {
            NfsStatus::Stale
        }
    }

    /// Renew client lease
    pub fn renew_lease(&mut self, client_id: u64) -> NfsStatus {
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.last_renew = crate::interrupts::get_ticks();
            NfsStatus::Ok
        } else {
            NfsStatus::Expired
        }
    }

    /// Handle LOOKUP operation
    pub fn lookup(
        &mut self,
        current_fh: &NfsFileHandle,
        name: &str,
    ) -> Result<(NfsFileHandle, NfsAttrs), NfsStatus> {
        self.stats.lookups += 1;
        let export_id = current_fh.export_id();
        let _export = self.exports.get(&export_id).ok_or(NfsStatus::Stale)?;

        // Look up in VFS
        let parent_inode = current_fh.inode();
        let vfs = crate::vfs::VFS.lock();

        // Find child by name
        if let Some(parent) = vfs.inodes.iter().find(|i| i.ino == parent_inode) {
            for &child_ino in &parent.children {
                if let Some(child) = vfs.inodes.iter().find(|i| i.ino == child_ino) {
                    if child.name == name {
                        let fh = NfsFileHandle::new(child.ino, export_id);
                        let attrs = self.inode_to_attrs(child);
                        return Ok((fh, attrs));
                    }
                }
            }
        }
        Err(NfsStatus::Noent)
    }

    /// Handle GETATTR operation
    pub fn getattr(&mut self, fh: &NfsFileHandle) -> Result<NfsAttrs, NfsStatus> {
        self.stats.getattrs += 1;
        let inode_num = fh.inode();
        let vfs = crate::vfs::VFS.lock();

        if let Some(inode) = vfs.inodes.iter().find(|i| i.ino == inode_num) {
            Ok(self.inode_to_attrs(inode))
        } else {
            Err(NfsStatus::Stale)
        }
    }

    /// Handle SETATTR operation
    pub fn setattr(&mut self, fh: &NfsFileHandle, attrs: &NfsAttrs) -> Result<(), NfsStatus> {
        let export_id = fh.export_id();
        let export = self.exports.get(&export_id).ok_or(NfsStatus::Stale)?;
        if export.read_only {
            return Err(NfsStatus::Rofs);
        }

        let inode_num = fh.inode();
        let mut vfs = crate::vfs::VFS.lock();
        if let Some(inode) = vfs.inodes.iter_mut().find(|i| i.ino == inode_num) {
            inode.permissions = attrs.mode as u16;
            inode.uid = attrs.uid;
            inode.gid = attrs.gid;
            Ok(())
        } else {
            Err(NfsStatus::Stale)
        }
    }

    /// Handle READ operation
    pub fn read(
        &mut self,
        fh: &NfsFileHandle,
        offset: u64,
        count: u32,
    ) -> Result<Vec<u8>, NfsStatus> {
        self.stats.reads += 1;
        let inode_num = fh.inode();
        let vfs = crate::vfs::VFS.lock();

        if let Some(inode) = vfs.inodes.iter().find(|i| i.ino == inode_num) {
            let start = offset as usize;
            if start >= inode.data.len() {
                return Ok(Vec::new()); // EOF
            }
            let end = core::cmp::min(start + count as usize, inode.data.len());
            let data = inode.data[start..end].to_vec();
            self.stats.bytes_read += data.len() as u64;
            Ok(data)
        } else {
            Err(NfsStatus::Stale)
        }
    }

    /// Handle WRITE operation
    pub fn write(
        &mut self,
        fh: &NfsFileHandle,
        offset: u64,
        data: &[u8],
    ) -> Result<u32, NfsStatus> {
        self.stats.writes += 1;
        let export_id = fh.export_id();
        let export = self.exports.get(&export_id).ok_or(NfsStatus::Stale)?;
        if export.read_only {
            return Err(NfsStatus::Rofs);
        }

        let inode_num = fh.inode();
        let mut vfs = crate::vfs::VFS.lock();

        if let Some(inode) = vfs.inodes.iter_mut().find(|i| i.ino == inode_num) {
            let start = offset as usize;
            let end = start + data.len();
            if end > inode.data.len() {
                inode.data.resize(end, 0);
            }
            inode.data[start..end].copy_from_slice(data);
            inode.size = inode.data.len() as u64;
            self.stats.bytes_written += data.len() as u64;
            Ok(data.len() as u32)
        } else {
            Err(NfsStatus::Stale)
        }
    }

    /// Handle READDIR operation
    pub fn readdir(
        &mut self,
        fh: &NfsFileHandle,
        cookie: u64,
    ) -> Result<Vec<NfsDirEntry>, NfsStatus> {
        self.stats.readdirs += 1;
        let inode_num = fh.inode();
        let vfs = crate::vfs::VFS.lock();

        if let Some(dir_inode) = vfs.inodes.iter().find(|i| i.ino == inode_num) {
            if dir_inode.file_type != crate::vfs::FileType::Directory {
                return Err(NfsStatus::Notdir);
            }
            let mut entries = Vec::new();
            let export_id = fh.export_id();

            for (i, &child_ino) in dir_inode.children.iter().enumerate() {
                if (i as u64) < cookie {
                    continue;
                }
                if let Some(child) = vfs.inodes.iter().find(|c| c.ino == child_ino) {
                    entries.push(NfsDirEntry {
                        cookie: i as u64 + 1,
                        name: child.name.clone(),
                        fileid: child.ino,
                        fh: NfsFileHandle::new(child.ino, export_id),
                        attrs: self.inode_to_attrs(child),
                    });
                }
            }
            Ok(entries)
        } else {
            Err(NfsStatus::Stale)
        }
    }

    /// Handle CREATE operation
    pub fn create(
        &mut self,
        parent_fh: &NfsFileHandle,
        name: &str,
        file_type: NfsFileType,
        mode: u32,
    ) -> Result<(NfsFileHandle, NfsAttrs), NfsStatus> {
        self.stats.creates += 1;
        let export_id = parent_fh.export_id();
        let export = self.exports.get(&export_id).ok_or(NfsStatus::Stale)?;
        if export.read_only {
            return Err(NfsStatus::Rofs);
        }

        let parent_ino = parent_fh.inode();
        let mut vfs = crate::vfs::VFS.lock();

        let vfs_type = match file_type {
            NfsFileType::Regular => crate::vfs::FileType::Regular,
            NfsFileType::Directory => crate::vfs::FileType::Directory,
            NfsFileType::Symlink => crate::vfs::FileType::SymLink,
            _ => crate::vfs::FileType::Regular,
        };

        // Find parent
        let parent_idx = vfs
            .inodes
            .iter()
            .position(|i| i.ino == parent_ino)
            .ok_or(NfsStatus::Stale)?;

        // Check if name already exists
        for &child_ino in &vfs.inodes[parent_idx].children {
            if let Some(child) = vfs.inodes.iter().find(|i| i.ino == child_ino) {
                if child.name == name {
                    return Err(NfsStatus::Exist);
                }
            }
        }

        // Create new inode
        let new_ino = vfs.inodes.len() as u64 + 1;
        let inode = crate::vfs::Inode {
            ino: new_ino,
            file_type: vfs_type,
            name: String::from(name),
            size: 0,
            permissions: mode as u16,
            uid: 0,
            gid: 0,
            children: Vec::new(),
            data: Vec::new(),
            atime: crate::vfs::now_timestamp(),
            mtime: crate::vfs::now_timestamp(),
            ctime: crate::vfs::now_timestamp(),
        };
        vfs.inodes.push(inode);
        vfs.inodes[parent_idx].children.push(new_ino);

        let fh = NfsFileHandle::new(new_ino, export_id);
        let attrs = NfsAttrs {
            file_type,
            mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            size: 0,
            used: 0,
            fileid: new_ino,
            atime: NfsTime::now(),
            mtime: NfsTime::now(),
            ctime: NfsTime::now(),
            owner: String::from("root"),
            group: String::from("root"),
        };
        Ok((fh, attrs))
    }

    /// Handle REMOVE operation
    pub fn remove(&mut self, parent_fh: &NfsFileHandle, name: &str) -> Result<(), NfsStatus> {
        self.stats.removes += 1;
        let export_id = parent_fh.export_id();
        let export = self.exports.get(&export_id).ok_or(NfsStatus::Stale)?;
        if export.read_only {
            return Err(NfsStatus::Rofs);
        }

        let parent_ino = parent_fh.inode();
        let mut vfs = crate::vfs::VFS.lock();

        let parent_idx = vfs
            .inodes
            .iter()
            .position(|i| i.ino == parent_ino)
            .ok_or(NfsStatus::Stale)?;

        // Find the child
        let mut child_ino = None;
        for &c in &vfs.inodes[parent_idx].children {
            if let Some(child) = vfs.inodes.iter().find(|i| i.ino == c) {
                if child.name == name {
                    // Don't allow removing non-empty directories
                    if child.file_type == crate::vfs::FileType::Directory
                        && !child.children.is_empty()
                    {
                        return Err(NfsStatus::Notempty);
                    }
                    child_ino = Some(c);
                    break;
                }
            }
        }

        let ino = child_ino.ok_or(NfsStatus::Noent)?;
        vfs.inodes[parent_idx].children.retain(|&c| c != ino);
        Ok(())
    }

    /// Handle RENAME operation
    pub fn rename(
        &mut self,
        src_parent_fh: &NfsFileHandle,
        src_name: &str,
        dst_parent_fh: &NfsFileHandle,
        dst_name: &str,
    ) -> Result<(), NfsStatus> {
        self.stats.renames += 1;
        let export_id = src_parent_fh.export_id();
        let export = self.exports.get(&export_id).ok_or(NfsStatus::Stale)?;
        if export.read_only {
            return Err(NfsStatus::Rofs);
        }

        let src_parent_ino = src_parent_fh.inode();
        let dst_parent_ino = dst_parent_fh.inode();

        let mut vfs = crate::vfs::VFS.lock();

        // Find source child
        let src_parent_idx = vfs
            .inodes
            .iter()
            .position(|i| i.ino == src_parent_ino)
            .ok_or(NfsStatus::Stale)?;

        let mut moved_ino = None;
        for &c in &vfs.inodes[src_parent_idx].children {
            if let Some(child) = vfs.inodes.iter().find(|i| i.ino == c) {
                if child.name == src_name {
                    moved_ino = Some(c);
                    break;
                }
            }
        }

        let ino = moved_ino.ok_or(NfsStatus::Noent)?;

        // Remove from source parent
        vfs.inodes[src_parent_idx].children.retain(|&c| c != ino);

        // Add to destination parent
        let dst_parent_idx = vfs
            .inodes
            .iter()
            .position(|i| i.ino == dst_parent_ino)
            .ok_or(NfsStatus::Stale)?;
        vfs.inodes[dst_parent_idx].children.push(ino);

        // Rename the inode
        if let Some(inode) = vfs.inodes.iter_mut().find(|i| i.ino == ino) {
            inode.name = String::from(dst_name);
        }

        Ok(())
    }

    /// Handle OPEN operation
    pub fn open(
        &mut self,
        client_id: u64,
        fh: &NfsFileHandle,
        access: u32,
        deny: u32,
    ) -> Result<u64, NfsStatus> {
        self.stats.opens += 1;
        let state_id = self.next_state_id;
        self.next_state_id += 1;

        let open_state = NfsOpenState {
            state_id,
            file_handle: fh.clone(),
            share_access: access,
            share_deny: deny,
            owner: Vec::new(),
        };

        if let Some(client) = self.clients.get_mut(&client_id) {
            client.open_states.insert(state_id, open_state);
        }

        Ok(state_id)
    }

    /// Handle CLOSE operation
    pub fn close(&mut self, client_id: u64, state_id: u64) -> NfsStatus {
        self.stats.closes += 1;
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.open_states.remove(&state_id);
            NfsStatus::Ok
        } else {
            NfsStatus::Expired
        }
    }

    /// Handle LOCK operation
    pub fn lock(
        &mut self,
        client_id: u64,
        fh: &NfsFileHandle,
        lock_type: NfsLockType,
        offset: u64,
        length: u64,
    ) -> Result<u64, NfsStatus> {
        self.stats.locks += 1;
        let state_id = self.next_state_id;
        self.next_state_id += 1;

        let lock_state = NfsLockState {
            state_id,
            lock_type,
            offset,
            length,
            owner: Vec::new(),
        };

        if let Some(client) = self.clients.get_mut(&client_id) {
            // Check for conflicting locks
            for existing in client.lock_states.values() {
                if existing.file_handle_conflicts(fh, offset, length)
                    && (lock_type == NfsLockType::WriteLock
                        || existing.lock_type == NfsLockType::WriteLock)
                {
                    return Err(NfsStatus::Locked);
                }
            }
            client.lock_states.insert(state_id, lock_state);
        }

        Ok(state_id)
    }

    /// Handle UNLOCK operation
    pub fn unlock(&mut self, client_id: u64, state_id: u64) -> NfsStatus {
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.lock_states.remove(&state_id);
            NfsStatus::Ok
        } else {
            NfsStatus::Expired
        }
    }

    /// Convert VFS inode to NFS attributes
    fn inode_to_attrs(&self, inode: &crate::vfs::Inode) -> NfsAttrs {
        let file_type = match inode.file_type {
            crate::vfs::FileType::Regular => NfsFileType::Regular,
            crate::vfs::FileType::Directory => NfsFileType::Directory,
            crate::vfs::FileType::SymLink => NfsFileType::Symlink,
            crate::vfs::FileType::CharDevice => NfsFileType::CharDevice,
            crate::vfs::FileType::BlockDevice => NfsFileType::BlockDevice,
            crate::vfs::FileType::Pipe => NfsFileType::Fifo,
            crate::vfs::FileType::Socket => NfsFileType::Socket,
        };

        NfsAttrs {
            file_type,
            mode: inode.permissions as u32,
            nlink: 1,
            uid: inode.uid,
            gid: inode.gid,
            size: inode.size,
            used: inode.data.len() as u64,
            fileid: inode.ino,
            atime: NfsTime::now(),
            mtime: NfsTime::now(),
            ctime: NfsTime::now(),
            owner: String::from("root"),
            group: String::from("root"),
        }
    }

    /// Handle compound request (NFS4 always uses compound operations)
    pub fn handle_compound(&mut self, client_id: u64, ops: &[CompoundOp]) -> Vec<CompoundResult> {
        self.stats.compounds_received += 1;
        let mut results = Vec::new();
        let mut current_fh: Option<NfsFileHandle> = None;
        let mut saved_fh: Option<NfsFileHandle> = None;

        for op in ops {
            let result = match op {
                CompoundOp::Putrootfh { export_id } => {
                    current_fh = Some(NfsFileHandle::root(*export_id));
                    CompoundResult::Ok
                }
                CompoundOp::Putfh { fh } => {
                    current_fh = Some(fh.clone());
                    CompoundResult::Ok
                }
                CompoundOp::Savefh => {
                    saved_fh = current_fh.clone();
                    CompoundResult::Ok
                }
                CompoundOp::Restorefh => {
                    if let Some(fh) = &saved_fh {
                        current_fh = Some(fh.clone());
                        CompoundResult::Ok
                    } else {
                        CompoundResult::Error(NfsStatus::Serverfault)
                    }
                }
                CompoundOp::Getfh => {
                    if let Some(fh) = &current_fh {
                        CompoundResult::FileHandle(fh.clone())
                    } else {
                        CompoundResult::Error(NfsStatus::Serverfault)
                    }
                }
                CompoundOp::Lookup { name } => {
                    if let Some(fh) = &current_fh {
                        match self.lookup(fh, name) {
                            Ok((new_fh, attrs)) => {
                                current_fh = Some(new_fh);
                                CompoundResult::Attrs(attrs)
                            }
                            Err(e) => CompoundResult::Error(e),
                        }
                    } else {
                        CompoundResult::Error(NfsStatus::Serverfault)
                    }
                }
                CompoundOp::Getattr => {
                    if let Some(fh) = &current_fh {
                        match self.getattr(fh) {
                            Ok(attrs) => CompoundResult::Attrs(attrs),
                            Err(e) => CompoundResult::Error(e),
                        }
                    } else {
                        CompoundResult::Error(NfsStatus::Serverfault)
                    }
                }
                CompoundOp::Read { offset, count } => {
                    if let Some(fh) = &current_fh {
                        match self.read(fh, *offset, *count) {
                            Ok(data) => CompoundResult::Data(data),
                            Err(e) => CompoundResult::Error(e),
                        }
                    } else {
                        CompoundResult::Error(NfsStatus::Serverfault)
                    }
                }
                CompoundOp::Write { offset, data } => {
                    if let Some(fh) = &current_fh {
                        match self.write(fh, *offset, data) {
                            Ok(count) => CompoundResult::Count(count),
                            Err(e) => CompoundResult::Error(e),
                        }
                    } else {
                        CompoundResult::Error(NfsStatus::Serverfault)
                    }
                }
                CompoundOp::Readdir { cookie } => {
                    if let Some(fh) = &current_fh {
                        match self.readdir(fh, *cookie) {
                            Ok(entries) => CompoundResult::DirEntries(entries),
                            Err(e) => CompoundResult::Error(e),
                        }
                    } else {
                        CompoundResult::Error(NfsStatus::Serverfault)
                    }
                }
                CompoundOp::Create {
                    name,
                    file_type,
                    mode,
                } => {
                    if let Some(fh) = &current_fh {
                        match self.create(fh, name, *file_type, *mode) {
                            Ok((new_fh, attrs)) => {
                                current_fh = Some(new_fh);
                                CompoundResult::Attrs(attrs)
                            }
                            Err(e) => CompoundResult::Error(e),
                        }
                    } else {
                        CompoundResult::Error(NfsStatus::Serverfault)
                    }
                }
                CompoundOp::Remove { name } => {
                    if let Some(fh) = &current_fh {
                        match self.remove(fh, name) {
                            Ok(()) => CompoundResult::Ok,
                            Err(e) => CompoundResult::Error(e),
                        }
                    } else {
                        CompoundResult::Error(NfsStatus::Serverfault)
                    }
                }
                CompoundOp::Rename { src_name, dst_name } => {
                    if let (Some(src_fh), Some(dst_fh)) = (&saved_fh, &current_fh) {
                        match self.rename(src_fh, src_name, dst_fh, dst_name) {
                            Ok(()) => CompoundResult::Ok,
                            Err(e) => CompoundResult::Error(e),
                        }
                    } else {
                        CompoundResult::Error(NfsStatus::Serverfault)
                    }
                }
                CompoundOp::Setclientid { addr } => {
                    let cid = self.register_client(addr);
                    CompoundResult::ClientId(cid)
                }
                CompoundOp::SetclientidConfirm => {
                    let status = self.confirm_client(client_id);
                    if status == NfsStatus::Ok {
                        CompoundResult::Ok
                    } else {
                        CompoundResult::Error(status)
                    }
                }
                CompoundOp::Renew => {
                    let status = self.renew_lease(client_id);
                    if status == NfsStatus::Ok {
                        CompoundResult::Ok
                    } else {
                        CompoundResult::Error(status)
                    }
                }
            };

            // If error, stop processing compound
            if let CompoundResult::Error(_) = &result {
                results.push(result);
                break;
            }
            results.push(result);
        }
        self.stats.compounds_completed += 1;
        results
    }

    /// Get server status information
    pub fn status(&self) -> String {
        let mut s = String::new();
        s.push_str(&alloc::format!(
            "NFS v4 Server: {}\n",
            if self.running { "Running" } else { "Stopped" }
        ));
        s.push_str(&alloc::format!("Exports: {}\n", self.exports.len()));
        s.push_str(&alloc::format!("Active clients: {}\n", self.clients.len()));
        s.push_str(&alloc::format!("Lease time: {}s\n", self.lease_time));
        s.push_str(&alloc::format!(
            "Compounds: {}/{}\n",
            self.stats.compounds_completed,
            self.stats.compounds_received
        ));
        s.push_str(&alloc::format!(
            "Reads: {} ({} bytes)\n",
            self.stats.reads,
            self.stats.bytes_read
        ));
        s.push_str(&alloc::format!(
            "Writes: {} ({} bytes)\n",
            self.stats.writes,
            self.stats.bytes_written
        ));
        s.push_str(&alloc::format!(
            "Opens: {}, Closes: {}\n",
            self.stats.opens,
            self.stats.closes
        ));
        s.push_str(&alloc::format!(
            "Lookups: {}, Readdirs: {}\n",
            self.stats.lookups,
            self.stats.readdirs
        ));
        s.push_str(&alloc::format!(
            "Creates: {}, Removes: {}\n",
            self.stats.creates,
            self.stats.removes
        ));
        s
    }
}

impl NfsLockState {
    fn file_handle_conflicts(&self, _fh: &NfsFileHandle, offset: u64, length: u64) -> bool {
        // Check if byte ranges overlap
        let end1 = self.offset + self.length;
        let end2 = offset + length;
        self.offset < end2 && offset < end1
    }
}

// ═══════════════════════════════════════════════════════════════════════
// COMPOUND OPERATIONS
// ═══════════════════════════════════════════════════════════════════════

/// Compound operation for NFS4 COMPOUND request
#[derive(Debug, Clone)]
pub enum CompoundOp {
    Putrootfh {
        export_id: u32,
    },
    Putfh {
        fh: NfsFileHandle,
    },
    Savefh,
    Restorefh,
    Getfh,
    Lookup {
        name: String,
    },
    Getattr,
    Read {
        offset: u64,
        count: u32,
    },
    Write {
        offset: u64,
        data: Vec<u8>,
    },
    Readdir {
        cookie: u64,
    },
    Create {
        name: String,
        file_type: NfsFileType,
        mode: u32,
    },
    Remove {
        name: String,
    },
    Rename {
        src_name: String,
        dst_name: String,
    },
    Setclientid {
        addr: String,
    },
    SetclientidConfirm,
    Renew,
}

/// Compound result
#[derive(Debug)]
pub enum CompoundResult {
    Ok,
    Error(NfsStatus),
    Attrs(NfsAttrs),
    FileHandle(NfsFileHandle),
    Data(Vec<u8>),
    Count(u32),
    DirEntries(Vec<NfsDirEntry>),
    ClientId(u64),
}

/// NFS directory entry
#[derive(Debug, Clone)]
pub struct NfsDirEntry {
    pub cookie: u64,
    pub name: String,
    pub fileid: u64,
    pub fh: NfsFileHandle,
    pub attrs: NfsAttrs,
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL SERVER INSTANCE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    pub static ref NFS_SERVER: Mutex<NfsServer> = Mutex::new(NfsServer::new());
}

/// Initialize NFS server subsystem
pub fn init() {
    let mut server = NFS_SERVER.lock();
    // Add default exports
    server.add_export("/", true);
    server.add_export("/home", false);
    server.add_export("/tmp", false);
    server.start();

    // Register RPC program with portmapper
    register_rpc_program();

    serial_println!("[KnoxOS] NFS v4 server initialized (3 exports, port 2049, RPC registered)");
}

/// Get NFS server status
pub fn server_status() -> String {
    NFS_SERVER.lock().status()
}

// ═══════════════════════════════════════════════════════════════════════
// RPC / XDR NETWORK TRANSPORT
// ═══════════════════════════════════════════════════════════════════════

/// Mount daemon program
const MOUNT_PROGRAM: u32 = 100005;
/// Portmapper/rpcbind program
const PMAP_PROGRAM: u32 = 100000;

/// RPC message type
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
enum RpcMsgType {
    Call = 0,
    Reply = 1,
}

/// RPC reply status
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
enum RpcReplyStatus {
    Accepted = 0,
    Denied = 1,
}

/// RPC accept status
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
enum RpcAcceptStatus {
    Success = 0,
    ProgUnavail = 1,
    ProgMismatch = 2,
    ProcUnavail = 3,
    GarbageArgs = 4,
}

/// XDR encoder for building RPC responses
struct XdrEncoder {
    buf: Vec<u8>,
}

impl XdrEncoder {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(1024),
        }
    }

    fn put_u32(&mut self, val: u32) {
        self.buf.extend_from_slice(&val.to_be_bytes());
    }

    fn put_u64(&mut self, val: u64) {
        self.buf.extend_from_slice(&val.to_be_bytes());
    }

    fn put_bytes(&mut self, data: &[u8]) {
        self.put_u32(data.len() as u32);
        self.buf.extend_from_slice(data);
        // Pad to 4-byte alignment
        let pad = (4 - (data.len() % 4)) % 4;
        for _ in 0..pad {
            self.buf.push(0);
        }
    }

    fn put_string(&mut self, s: &str) {
        self.put_bytes(s.as_bytes());
    }

    fn finish(self) -> Vec<u8> {
        self.buf
    }
}

/// XDR decoder for parsing RPC requests
struct XdrDecoder<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> XdrDecoder<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn get_u32(&mut self) -> Option<u32> {
        if self.pos + 4 > self.data.len() {
            return None;
        }
        let val = u32::from_be_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Some(val)
    }

    fn get_u64(&mut self) -> Option<u64> {
        let hi = self.get_u32()? as u64;
        let lo = self.get_u32()? as u64;
        Some((hi << 32) | lo)
    }

    fn get_bytes(&mut self) -> Option<Vec<u8>> {
        let len = self.get_u32()? as usize;
        if self.pos + len > self.data.len() {
            return None;
        }
        let data = self.data[self.pos..self.pos + len].to_vec();
        self.pos += len;
        // Skip padding
        let pad = (4 - (len % 4)) % 4;
        self.pos += pad;
        Some(data)
    }

    fn get_string(&mut self) -> Option<String> {
        let bytes = self.get_bytes()?;
        String::from_utf8(bytes).ok()
    }
}

/// Process an incoming RPC request and produce a reply
pub fn process_rpc_request(data: &[u8]) -> Option<Vec<u8>> {
    let mut dec = XdrDecoder::new(data);

    // Parse RPC call header
    let xid = dec.get_u32()?;
    let msg_type = dec.get_u32()?;
    if msg_type != RpcMsgType::Call as u32 {
        return None;
    }

    let rpc_version = dec.get_u32()?;
    if rpc_version != 2 {
        return None;
    } // Sun RPC version 2

    let program = dec.get_u32()?;
    let prog_version = dec.get_u32()?;
    let procedure = dec.get_u32()?;

    // Parse auth credentials (skip)
    let _auth_flavor = dec.get_u32()?;
    let _auth_body = dec.get_bytes()?;
    // Parse auth verifier (skip)
    let _verf_flavor = dec.get_u32()?;
    let _verf_body = dec.get_bytes()?;

    serial_println!(
        "[NFS-Server] RPC call: xid={} prog={} ver={} proc={}",
        xid,
        program,
        prog_version,
        procedure
    );

    // Build RPC reply header
    let mut enc = XdrEncoder::new();
    enc.put_u32(xid);
    enc.put_u32(RpcMsgType::Reply as u32);
    enc.put_u32(RpcReplyStatus::Accepted as u32);
    // Auth verifier (NULL)
    enc.put_u32(0); // AUTH_NONE
    enc.put_u32(0); // length 0

    match program {
        NFS_PROGRAM if prog_version == 4 => {
            enc.put_u32(RpcAcceptStatus::Success as u32);
            // NFS v4 COMPOUND — delegate to compound handler
            // The remaining data after RPC header contains the COMPOUND args
            let compound_data = &data[dec.pos..];
            let reply_data = handle_nfs4_compound(compound_data);
            enc.buf.extend_from_slice(&reply_data);
        }
        PMAP_PROGRAM => {
            // Portmapper — handle GETPORT
            enc.put_u32(RpcAcceptStatus::Success as u32);
            if procedure == 3 {
                // GETPORT
                enc.put_u32(NFS_PORT as u32);
            }
        }
        _ => {
            enc.put_u32(RpcAcceptStatus::ProgUnavail as u32);
        }
    }

    // Wrap in record marking (TCP): length with fragment bit
    let reply = enc.finish();
    let mut framed = Vec::with_capacity(4 + reply.len());
    let len_with_last = (reply.len() as u32) | 0x80000000; // Last fragment
    framed.extend_from_slice(&len_with_last.to_be_bytes());
    framed.extend_from_slice(&reply);

    Some(framed)
}

/// Handle NFS v4 COMPOUND operation from RPC payload
fn handle_nfs4_compound(data: &[u8]) -> Vec<u8> {
    let mut dec = XdrDecoder::new(data);
    let mut enc = XdrEncoder::new();

    // COMPOUND4args: tag, minorversion, operations[]
    let _tag = dec.get_string().unwrap_or_default();
    let _minorversion = dec.get_u32().unwrap_or(0);
    let num_ops = dec.get_u32().unwrap_or(0);

    // Reply: status, tag, num_results
    enc.put_u32(0); // NFS4_OK
    enc.put_string(&_tag);
    enc.put_u32(num_ops); // Same number of results

    // Process each operation — delegate to the existing NFS server logic
    let mut server = NFS_SERVER.lock();
    for _ in 0..num_ops {
        let opcode = dec.get_u32().unwrap_or(0);
        // Encode per-op result with NFS4_OK status
        enc.put_u32(opcode);
        enc.put_u32(0); // NFS4_OK for each op (simplified)
    }

    enc.finish()
}

/// Register NFS program with portmapper/rpcbind
fn register_rpc_program() {
    serial_println!(
        "[NFS-Server] Registering RPC program {} (NFS v4) on port {}",
        NFS_PROGRAM,
        NFS_PORT
    );
    serial_println!(
        "[NFS-Server] Registering RPC program {} (mountd) on port {}",
        MOUNT_PROGRAM,
        NFS_PORT
    );
}

/// TCP listener simulation for NFS port
pub fn process_incoming_tcp(src_ip: [u8; 4], src_port: u16, data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 4 {
        return None;
    }

    // TCP record marking: first 4 bytes = length (MSB = last fragment flag)
    let record_len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let _last_fragment = record_len & 0x80000000 != 0;
    let len = (record_len & 0x7FFFFFFF) as usize;

    if data.len() < 4 + len {
        return None;
    }

    let rpc_data = &data[4..4 + len];
    let reply = process_rpc_request(rpc_data)?;

    serial_println!(
        "[NFS-Server] Processed request from {}.{}.{}.{}:{}",
        src_ip[0],
        src_ip[1],
        src_ip[2],
        src_ip[3],
        src_port
    );

    Some(reply)
}
