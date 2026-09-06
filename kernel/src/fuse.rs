/// FUSE — Filesystem in Userspace
///
/// Allows userspace processes to provide filesystem implementations
/// via a well-defined kernel↔userspace protocol. The kernel handles
/// VFS integration while the userspace daemon handles the actual storage.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// FUSE PROTOCOL
// ═══════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════
// FUSE MOUNT STATE
// ═══════════════════════════════════════════════════════════════════════

/// A mounted FUSE filesystem
#[derive(Debug)]
pub struct FuseMount {
    pub mount_point: String,
    pub daemon_pid: u32,
    pub max_readahead: u32,
    pub max_write: u32,
    pub flags: u32,
    /// Pending requests waiting for userspace response
    pub pending: BTreeMap<u64, FuseRequest>,
    /// Completed responses from userspace
    pub responses: BTreeMap<u64, Vec<u8>>,
    /// Next unique request ID
    pub next_unique: u64,
    pub connected: bool,
}

/// A pending FUSE request
#[derive(Debug, Clone)]
pub struct FuseRequest {
    pub header: FuseInHeader,
    pub data: Vec<u8>,
}

lazy_static::lazy_static! {
    static ref MOUNTS: Mutex<BTreeMap<String, FuseMount>> = Mutex::new(BTreeMap::new());
}

// ═══════════════════════════════════════════════════════════════════════
// MOUNT / UNMOUNT
// ═══════════════════════════════════════════════════════════════════════

/// Mount a FUSE filesystem
pub fn mount(mount_point: &str, daemon_pid: u32) -> Result<(), &'static str> {
    let mut mounts = MOUNTS.lock();
    if mounts.contains_key(mount_point) {
        return Err("Mount point already in use");
    }

    let fm = FuseMount {
        mount_point: String::from(mount_point),
        daemon_pid,
        max_readahead: 131072,
        max_write: 131072,
        flags: 0,
        pending: BTreeMap::new(),
        responses: BTreeMap::new(),
        next_unique: 1,
        connected: true,
    };

    serial_println!(
        "[fuse] Mounted at '{}' (daemon pid {})",
        mount_point,
        daemon_pid
    );
    mounts.insert(String::from(mount_point), fm);
    Ok(())
}

/// Unmount a FUSE filesystem
pub fn unmount(mount_point: &str) -> Result<(), &'static str> {
    let mut mounts = MOUNTS.lock();
    let fm = mounts.get_mut(mount_point).ok_or("Not a FUSE mount")?;
    fm.connected = false;
    mounts.remove(mount_point);
    serial_println!("[fuse] Unmounted '{}'", mount_point);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// KERNEL → USERSPACE REQUESTS
// ═══════════════════════════════════════════════════════════════════════

/// Send a request to the FUSE daemon
fn send_request(
    mount_point: &str,
    opcode: FuseOpcode,
    nodeid: u64,
    data: Vec<u8>,
) -> Result<u64, &'static str> {
    let mut mounts = MOUNTS.lock();
    let fm = mounts.get_mut(mount_point).ok_or("Not a FUSE mount")?;

    if !fm.connected {
        return Err("FUSE daemon disconnected");
    }

    let unique = fm.next_unique;
    fm.next_unique += 1;

    let header = FuseInHeader {
        len: (core::mem::size_of::<FuseInHeader>() + data.len()) as u32,
        opcode: opcode as u32,
        unique,
        nodeid,
        uid: 0,
        gid: 0,
        pid: 0,
        padding: 0,
    };

    fm.pending.insert(unique, FuseRequest { header, data });
    Ok(unique)
}

/// FUSE daemon reads the next pending request (via /dev/fuse read)
pub fn read_request(mount_point: &str) -> Option<FuseRequest> {
    let mut mounts = MOUNTS.lock();
    let fm = mounts.get_mut(mount_point)?;

    // Return the first pending request
    let key = *fm.pending.keys().next()?;
    fm.pending.remove(&key)
}

/// FUSE daemon writes a response (via /dev/fuse write)
pub fn write_response(mount_point: &str, unique: u64, data: Vec<u8>) -> Result<(), &'static str> {
    let mut mounts = MOUNTS.lock();
    let fm = mounts.get_mut(mount_point).ok_or("Not a FUSE mount")?;
    fm.responses.insert(unique, data);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// VFS INTEGRATION
// ═══════════════════════════════════════════════════════════════════════

/// Handle a VFS lookup through FUSE
pub fn fuse_lookup(mount_point: &str, parent: u64, name: &str) -> Result<FuseEntry, &'static str> {
    let _unique = send_request(
        mount_point,
        FuseOpcode::Lookup,
        parent,
        name.as_bytes().to_vec(),
    )?;
    // In a real implementation, wait for the response from the daemon
    Ok(FuseEntry {
        nodeid: 2,
        generation: 1,
        attr: FuseAttr {
            ino: 2,
            size: 0,
            blocks: 0,
            mode: 0o100644,
            nlink: 1,
            uid: 0,
            gid: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        },
        attr_valid_secs: 1,
        entry_valid_secs: 1,
    })
}

/// Handle a VFS read through FUSE
pub fn fuse_read(
    mount_point: &str,
    nodeid: u64,
    offset: u64,
    size: u32,
) -> Result<Vec<u8>, &'static str> {
    let mut data = Vec::with_capacity(16);
    data.extend_from_slice(&offset.to_le_bytes());
    data.extend_from_slice(&(size as u64).to_le_bytes());
    let _unique = send_request(mount_point, FuseOpcode::Read, nodeid, data)?;
    Ok(Vec::new()) // Wait for response
}

/// Handle a VFS write through FUSE
pub fn fuse_write(
    mount_point: &str,
    nodeid: u64,
    offset: u64,
    data: &[u8],
) -> Result<usize, &'static str> {
    let mut req_data = Vec::with_capacity(8 + data.len());
    req_data.extend_from_slice(&offset.to_le_bytes());
    req_data.extend_from_slice(data);
    let _unique = send_request(mount_point, FuseOpcode::Write, nodeid, req_data)?;
    Ok(data.len())
}

/// List FUSE mounts
pub fn list_mounts() -> Vec<(String, u32, bool)> {
    MOUNTS
        .lock()
        .values()
        .map(|m| (m.mount_point.clone(), m.daemon_pid, m.connected))
        .collect()
}

/// Initialize FUSE subsystem
pub fn init() {
    serial_println!("[fuse] Filesystem in Userspace (FUSE) initialized");
}
