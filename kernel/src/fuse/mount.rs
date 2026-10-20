//! FUSE mount table, loopback store, and kernel↔daemon request channel.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

use super::protocol::{FuseInHeader, FuseOpcode};

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

/// In-kernel loopback daemon: path → (ino, bytes) plus directory and symlink inos.
#[derive(Debug, Default)]
pub(super) struct LoopbackFs {
    pub(super) files: BTreeMap<String, (u64, Vec<u8>)>,
    pub(super) dirs: BTreeMap<String, u64>,
    /// name → (ino, target)
    pub(super) links: BTreeMap<String, (u64, String)>,
    /// name → (ino, mode) for special files (mknod)
    pub(super) nodes: BTreeMap<String, (u64, u32)>,
    /// nodeid → (xattr name → value)
    pub(super) xattrs: BTreeMap<u64, BTreeMap<String, Vec<u8>>>,
    pub(super) next_ino: u64,
}

lazy_static::lazy_static! {
    pub(super) static ref MOUNTS: Mutex<BTreeMap<String, FuseMount>> = Mutex::new(BTreeMap::new());
    pub(super) static ref LOOPBACK: Mutex<BTreeMap<String, LoopbackFs>> = Mutex::new(BTreeMap::new());
}

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
    LOOPBACK.lock().insert(
        String::from(mount_point),
        LoopbackFs {
            files: BTreeMap::new(),
            dirs: BTreeMap::new(),
            links: BTreeMap::new(),
            nodes: BTreeMap::new(),
            xattrs: BTreeMap::new(),
            next_ino: 2,
        },
    );
    Ok(())
}

/// Register a file the in-kernel FUSE daemon will serve.
pub fn loopback_put(mount_point: &str, name: &str, data: &[u8]) -> Result<u64, &'static str> {
    let mut lbs = LOOPBACK.lock();
    let lb = lbs.get_mut(mount_point).ok_or("Not a FUSE mount")?;
    let ino = lb.next_ino;
    lb.next_ino += 1;
    lb.files.insert(String::from(name), (ino, data.to_vec()));
    Ok(ino)
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

/// Send a request to the FUSE daemon
pub(super) fn send_request(
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

pub(super) fn take_response(mount_point: &str, unique: u64) -> Option<Vec<u8>> {
    let mut mounts = MOUNTS.lock();
    mounts.get_mut(mount_point)?.responses.remove(&unique)
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

/// List FUSE mounts
pub fn list_mounts() -> Vec<(String, u32, bool)> {
    MOUNTS
        .lock()
        .values()
        .map(|m| (m.mount_point.clone(), m.daemon_pid, m.connected))
        .collect()
}
