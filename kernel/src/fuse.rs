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

/// In-kernel loopback daemon: path → (ino, bytes) plus directory and symlink inos.
#[derive(Debug, Default)]
struct LoopbackFs {
    files: BTreeMap<String, (u64, Vec<u8>)>,
    dirs: BTreeMap<String, u64>,
    /// name → (ino, target)
    links: BTreeMap<String, (u64, String)>,
    /// name → (ino, mode) for special files (mknod)
    nodes: BTreeMap<String, (u64, u32)>,
    next_ino: u64,
}

lazy_static::lazy_static! {
    static ref MOUNTS: Mutex<BTreeMap<String, FuseMount>> = Mutex::new(BTreeMap::new());
    static ref LOOPBACK: Mutex<BTreeMap<String, LoopbackFs>> = Mutex::new(BTreeMap::new());
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
    LOOPBACK.lock().insert(
        String::from(mount_point),
        LoopbackFs {
            files: BTreeMap::new(),
            dirs: BTreeMap::new(),
            links: BTreeMap::new(),
            nodes: BTreeMap::new(),
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

fn take_response(mount_point: &str, unique: u64) -> Option<Vec<u8>> {
    let mut mounts = MOUNTS.lock();
    mounts.get_mut(mount_point)?.responses.remove(&unique)
}

fn encode_entry(ino: u64, size: u64, mode: u32) -> Vec<u8> {
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

fn serve_loopback(mount_point: &str, opcode: FuseOpcode, unique: u64, req: &[u8]) {
    if opcode != FuseOpcode::Lookup {
        return;
    }
    let name = core::str::from_utf8(req).unwrap_or("");
    let (ino, size, mode) = {
        let lbs = LOOPBACK.lock();
        let Some(lb) = lbs.get(mount_point) else {
            return;
        };
        if let Some(&ino) = lb.dirs.get(name) {
            (ino, 0u64, 0o040755)
        } else if let Some(&(ino, ref data)) = lb.files.get(name) {
            (ino, data.len() as u64, 0o100644)
        } else if let Some(&(ino, ref target)) = lb.links.get(name) {
            (ino, target.len() as u64, 0o120777)
        } else if let Some(&(ino, mode)) = lb.nodes.get(name) {
            (ino, 0u64, mode)
        } else {
            return;
        }
    };
    let _ = write_response(mount_point, unique, encode_entry(ino, size, mode));
}

fn serve_loopback_read(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if let Some((_, data)) = lb.files.values().find(|(ino, _)| *ino == nodeid) {
        let payload = data.clone();
        drop(lbs);
        let _ = write_response(mount_point, unique, payload);
    }
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
    let unique = send_request(
        mount_point,
        FuseOpcode::Lookup,
        parent,
        name.as_bytes().to_vec(),
    )?;
    serve_loopback(mount_point, FuseOpcode::Lookup, unique, name.as_bytes());
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseEntry {
        nodeid: ino,
        generation: 1,
        attr: FuseAttr {
            ino,
            size,
            blocks: size.div_ceil(512),
            mode,
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
    let unique = send_request(mount_point, FuseOpcode::Read, nodeid, data)?;
    serve_loopback_read(mount_point, unique, nodeid);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let start = offset as usize;
    let end = start.saturating_add(size as usize).min(resp.len());
    if start >= resp.len() {
        Ok(Vec::new())
    } else {
        Ok(resp[start..end].to_vec())
    }
}

fn serve_loopback_write(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    if req.len() < 8 {
        return;
    }
    let offset = u64::from_le_bytes(req[0..8].try_into().unwrap_or([0; 8])) as usize;
    let payload = &req[8..];
    let written = {
        let mut lbs = LOOPBACK.lock();
        let Some(lb) = lbs.get_mut(mount_point) else {
            return;
        };
        let Some((_, data)) = lb.files.values_mut().find(|(ino, _)| *ino == nodeid) else {
            return;
        };
        let end = offset.saturating_add(payload.len());
        if end > data.len() {
            data.resize(end, 0);
        }
        if offset < end {
            data[offset..end].copy_from_slice(payload);
        }
        payload.len() as u32
    };
    let mut resp = Vec::with_capacity(8);
    resp.extend_from_slice(&written.to_le_bytes());
    resp.extend_from_slice(&0u32.to_le_bytes());
    let _ = write_response(mount_point, unique, resp);
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
    let unique = send_request(mount_point, FuseOpcode::Write, nodeid, req_data.clone())?;
    serve_loopback_write(mount_point, unique, nodeid, &req_data);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    if resp.len() < 4 {
        return Err("short write");
    }
    Ok(u32::from_le_bytes(resp[0..4].try_into().map_err(|_| "short write")?) as usize)
}

fn serve_loopback_unlink(mount_point: &str, unique: u64, name: &str) {
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    lb.files.remove(name);
    lb.links.remove(name);
    lb.nodes.remove(name);
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}

/// Handle a VFS unlink through FUSE.
pub fn fuse_unlink(mount_point: &str, parent: u64, name: &str) -> Result<(), &'static str> {
    let unique = send_request(
        mount_point,
        FuseOpcode::Unlink,
        parent,
        name.as_bytes().to_vec(),
    )?;
    serve_loopback_unlink(mount_point, unique, name);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}

fn serve_loopback_mkdir(mount_point: &str, unique: u64, name: &str) {
    let entry = {
        let mut lbs = LOOPBACK.lock();
        let Some(lb) = lbs.get_mut(mount_point) else {
            return;
        };
        let ino = lb.next_ino;
        lb.next_ino += 1;
        lb.dirs.insert(String::from(name), ino);
        encode_entry(ino, 0, 0o040755)
    };
    let _ = write_response(mount_point, unique, entry);
}

/// Handle a VFS mkdir through FUSE.
pub fn fuse_mkdir(mount_point: &str, parent: u64, name: &str) -> Result<FuseEntry, &'static str> {
    let unique = send_request(
        mount_point,
        FuseOpcode::Mkdir,
        parent,
        name.as_bytes().to_vec(),
    )?;
    serve_loopback_mkdir(mount_point, unique, name);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseEntry {
        nodeid: ino,
        generation: 1,
        attr: FuseAttr {
            ino,
            size,
            blocks: 0,
            mode,
            nlink: 2,
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

fn serve_loopback_rmdir(mount_point: &str, unique: u64, name: &str) {
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    lb.dirs.remove(name);
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}

/// Handle a VFS rmdir through FUSE.
pub fn fuse_rmdir(mount_point: &str, parent: u64, name: &str) -> Result<(), &'static str> {
    let unique = send_request(
        mount_point,
        FuseOpcode::Rmdir,
        parent,
        name.as_bytes().to_vec(),
    )?;
    serve_loopback_rmdir(mount_point, unique, name);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}

fn serve_loopback_rename(mount_point: &str, unique: u64, req: &[u8]) {
    let Ok(s) = core::str::from_utf8(req) else {
        return;
    };
    let Some((old, new)) = s.split_once('\0') else {
        return;
    };
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    if let Some(file) = lb.files.remove(old) {
        lb.files.insert(String::from(new), file);
    } else if let Some(ino) = lb.dirs.remove(old) {
        lb.dirs.insert(String::from(new), ino);
    } else if let Some(link) = lb.links.remove(old) {
        lb.links.insert(String::from(new), link);
    } else if let Some(node) = lb.nodes.remove(old) {
        lb.nodes.insert(String::from(new), node);
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}

/// Handle a VFS rename through FUSE. Request body is `old\0new`.
pub fn fuse_rename(
    mount_point: &str,
    parent: u64,
    old: &str,
    new: &str,
) -> Result<(), &'static str> {
    let mut data = Vec::with_capacity(old.len() + new.len() + 1);
    data.extend_from_slice(old.as_bytes());
    data.push(0);
    data.extend_from_slice(new.as_bytes());
    let unique = send_request(mount_point, FuseOpcode::Rename, parent, data.clone())?;
    serve_loopback_rename(mount_point, unique, &data);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}

fn serve_loopback_symlink(mount_point: &str, unique: u64, req: &[u8]) {
    let Ok(s) = core::str::from_utf8(req) else {
        return;
    };
    let Some((name, target)) = s.split_once('\0') else {
        return;
    };
    let entry = {
        let mut lbs = LOOPBACK.lock();
        let Some(lb) = lbs.get_mut(mount_point) else {
            return;
        };
        let ino = lb.next_ino;
        lb.next_ino += 1;
        lb.links
            .insert(String::from(name), (ino, String::from(target)));
        encode_entry(ino, target.len() as u64, 0o120777)
    };
    let _ = write_response(mount_point, unique, entry);
}

/// Handle a VFS symlink through FUSE. Request body is `name\0target`.
pub fn fuse_symlink(
    mount_point: &str,
    parent: u64,
    name: &str,
    target: &str,
) -> Result<FuseEntry, &'static str> {
    let mut data = Vec::with_capacity(name.len() + target.len() + 1);
    data.extend_from_slice(name.as_bytes());
    data.push(0);
    data.extend_from_slice(target.as_bytes());
    let unique = send_request(mount_point, FuseOpcode::Symlink, parent, data.clone())?;
    serve_loopback_symlink(mount_point, unique, &data);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseEntry {
        nodeid: ino,
        generation: 1,
        attr: FuseAttr {
            ino,
            size,
            blocks: 0,
            mode,
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

fn serve_loopback_readlink(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    if let Some((_, target)) = lb.links.values().find(|(ino, _)| *ino == nodeid) {
        let payload = target.as_bytes().to_vec();
        drop(lbs);
        let _ = write_response(mount_point, unique, payload);
    }
}

/// Handle a VFS readlink through FUSE.
pub fn fuse_readlink(mount_point: &str, nodeid: u64) -> Result<Vec<u8>, &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Readlink, nodeid, Vec::new())?;
    serve_loopback_readlink(mount_point, unique, nodeid);
    take_response(mount_point, unique).ok_or("would block")
}

fn serve_loopback_link(mount_point: &str, unique: u64, req: &[u8]) {
    let Ok(s) = core::str::from_utf8(req) else {
        return;
    };
    let Some((old, new)) = s.split_once('\0') else {
        return;
    };
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    if let Some(file) = lb.files.get(old).cloned() {
        lb.files.insert(String::from(new), file);
    } else if let Some(&ino) = lb.dirs.get(old) {
        lb.dirs.insert(String::from(new), ino);
    } else if let Some(link) = lb.links.get(old).cloned() {
        lb.links.insert(String::from(new), link);
    } else if let Some(&node) = lb.nodes.get(old) {
        lb.nodes.insert(String::from(new), node);
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, Vec::new());
}

/// Handle a VFS hard link through FUSE. Request body is `old\0new`.
pub fn fuse_link(mount_point: &str, parent: u64, old: &str, new: &str) -> Result<(), &'static str> {
    let mut data = Vec::with_capacity(old.len() + new.len() + 1);
    data.extend_from_slice(old.as_bytes());
    data.push(0);
    data.extend_from_slice(new.as_bytes());
    let unique = send_request(mount_point, FuseOpcode::Link, parent, data.clone())?;
    serve_loopback_link(mount_point, unique, &data);
    take_response(mount_point, unique).ok_or("would block")?;
    Ok(())
}

fn serve_loopback_create(mount_point: &str, unique: u64, name: &str) {
    let entry = {
        let mut lbs = LOOPBACK.lock();
        let Some(lb) = lbs.get_mut(mount_point) else {
            return;
        };
        let ino = lb.next_ino;
        lb.next_ino += 1;
        lb.files.insert(String::from(name), (ino, Vec::new()));
        encode_entry(ino, 0, 0o100644)
    };
    let _ = write_response(mount_point, unique, entry);
}

/// Handle a VFS create through FUSE.
pub fn fuse_create(mount_point: &str, parent: u64, name: &str) -> Result<FuseEntry, &'static str> {
    let unique = send_request(
        mount_point,
        FuseOpcode::Create,
        parent,
        name.as_bytes().to_vec(),
    )?;
    serve_loopback_create(mount_point, unique, name);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseEntry {
        nodeid: ino,
        generation: 1,
        attr: FuseAttr {
            ino,
            size,
            blocks: 0,
            mode,
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

fn serve_loopback_getattr(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    let entry = if let Some((_, data)) = lb.files.values().find(|(ino, _)| *ino == nodeid) {
        encode_entry(nodeid, data.len() as u64, 0o100644)
    } else if lb.dirs.values().any(|&ino| ino == nodeid) {
        encode_entry(nodeid, 0, 0o040755)
    } else if let Some((_, target)) = lb.links.values().find(|(ino, _)| *ino == nodeid) {
        encode_entry(nodeid, target.len() as u64, 0o120777)
    } else if let Some(&(_, mode)) = lb.nodes.values().find(|(ino, _)| *ino == nodeid) {
        encode_entry(nodeid, 0, mode)
    } else {
        return;
    };
    drop(lbs);
    let _ = write_response(mount_point, unique, entry);
}

/// Handle a VFS getattr through FUSE.
pub fn fuse_getattr(mount_point: &str, nodeid: u64) -> Result<FuseAttr, &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Getattr, nodeid, Vec::new())?;
    serve_loopback_getattr(mount_point, unique, nodeid);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseAttr {
        ino,
        size,
        blocks: size.div_ceil(512),
        mode,
        nlink: 1,
        uid: 0,
        gid: 0,
        atime: 0,
        mtime: 0,
        ctime: 0,
    })
}

fn serve_loopback_setattr(mount_point: &str, unique: u64, nodeid: u64, req: &[u8]) {
    if req.len() < 8 {
        return;
    }
    let size = u64::from_le_bytes(req[0..8].try_into().unwrap_or([0; 8]));
    let mut lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get_mut(mount_point) else {
        return;
    };
    let Some((_, data)) = lb.files.values_mut().find(|(ino, _)| *ino == nodeid) else {
        return;
    };
    data.resize(size as usize, 0);
    drop(lbs);
    let _ = write_response(mount_point, unique, encode_entry(nodeid, size, 0o100644));
}

/// Handle a VFS setattr through FUSE. Request body is the new size (u64 LE).
pub fn fuse_setattr(mount_point: &str, nodeid: u64, size: u64) -> Result<FuseAttr, &'static str> {
    let mut data = Vec::with_capacity(8);
    data.extend_from_slice(&size.to_le_bytes());
    let unique = send_request(mount_point, FuseOpcode::Setattr, nodeid, data.clone())?;
    serve_loopback_setattr(mount_point, unique, nodeid, &data);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseAttr {
        ino,
        size,
        blocks: size.div_ceil(512),
        mode,
        nlink: 1,
        uid: 0,
        gid: 0,
        atime: 0,
        mtime: 0,
        ctime: 0,
    })
}

fn encode_dirent(ino: u64, mode: u32, name: &str) -> Vec<u8> {
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

fn serve_loopback_readdir(mount_point: &str, unique: u64, nodeid: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    let is_root = nodeid == 1;
    let is_dir = lb.dirs.values().any(|&ino| ino == nodeid);
    if !is_root && !is_dir {
        return;
    }
    let mut out = Vec::new();
    if is_root {
        for (name, &(ino, _)) in &lb.files {
            out.extend_from_slice(&encode_dirent(ino, 0o100644, name));
        }
        for (name, &ino) in &lb.dirs {
            out.extend_from_slice(&encode_dirent(ino, 0o040755, name));
        }
        for (name, &(ino, _)) in &lb.links {
            out.extend_from_slice(&encode_dirent(ino, 0o120777, name));
        }
        for (name, &(ino, mode)) in &lb.nodes {
            out.extend_from_slice(&encode_dirent(ino, mode, name));
        }
    }
    drop(lbs);
    let _ = write_response(mount_point, unique, out);
}

/// Handle a VFS readdir through FUSE. Root nodeid 1 lists loopback names.
pub fn fuse_readdir(
    mount_point: &str,
    nodeid: u64,
) -> Result<Vec<(u64, u32, String)>, &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Readdir, nodeid, Vec::new())?;
    serve_loopback_readdir(mount_point, unique, nodeid);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    Ok(parse_fuse_dirents(&resp))
}

fn serve_loopback_mknod(mount_point: &str, unique: u64, req: &[u8]) {
    if req.len() < 4 {
        return;
    }
    let mode = u32::from_le_bytes(req[0..4].try_into().unwrap_or([0; 4]));
    let Ok(name) = core::str::from_utf8(&req[4..]) else {
        return;
    };
    let entry = {
        let mut lbs = LOOPBACK.lock();
        let Some(lb) = lbs.get_mut(mount_point) else {
            return;
        };
        let ino = lb.next_ino;
        lb.next_ino += 1;
        lb.nodes.insert(String::from(name), (ino, mode));
        encode_entry(ino, 0, mode)
    };
    let _ = write_response(mount_point, unique, entry);
}

/// Handle a VFS mknod through FUSE. Request body is mode (u32 LE) + name.
pub fn fuse_mknod(
    mount_point: &str,
    parent: u64,
    name: &str,
    mode: u32,
) -> Result<FuseEntry, &'static str> {
    let mut data = Vec::with_capacity(4 + name.len());
    data.extend_from_slice(&mode.to_le_bytes());
    data.extend_from_slice(name.as_bytes());
    let unique = send_request(mount_point, FuseOpcode::Mknod, parent, data.clone())?;
    serve_loopback_mknod(mount_point, unique, &data);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    let (ino, size, mode) = parse_fuse_entry(&resp).ok_or("short entry")?;
    Ok(FuseEntry {
        nodeid: ino,
        generation: 1,
        attr: FuseAttr {
            ino,
            size,
            blocks: 0,
            mode,
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

/// FUSE STATFS reply: bsize u32, blocks u64, bfree u64, files u64, namelen u32.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuseStatfs {
    pub bsize: u32,
    pub blocks: u64,
    pub bfree: u64,
    pub files: u64,
    pub namelen: u32,
}

fn encode_statfs(st: &FuseStatfs) -> Vec<u8> {
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

fn serve_loopback_statfs(mount_point: &str, unique: u64) {
    let lbs = LOOPBACK.lock();
    let Some(lb) = lbs.get(mount_point) else {
        return;
    };
    let files = (lb.files.len() + lb.dirs.len() + lb.links.len() + lb.nodes.len()) as u64;
    let st = FuseStatfs {
        bsize: 4096,
        blocks: 1024,
        bfree: 1024,
        files,
        namelen: 255,
    };
    drop(lbs);
    let _ = write_response(mount_point, unique, encode_statfs(&st));
}

/// Handle a VFS statfs through FUSE.
pub fn fuse_statfs(mount_point: &str) -> Result<FuseStatfs, &'static str> {
    let unique = send_request(mount_point, FuseOpcode::Statfs, 1, Vec::new())?;
    serve_loopback_statfs(mount_point, unique);
    let resp = take_response(mount_point, unique).ok_or("would block")?;
    parse_fuse_statfs(&resp).ok_or("short statfs")
}

/// List FUSE mounts
pub fn list_mounts() -> Vec<(String, u32, bool)> {
    MOUNTS
        .lock()
        .values()
        .map(|m| (m.mount_point.clone(), m.daemon_pid, m.connected))
        .collect()
}

pub const GATE_FUSE_MARKER: &str = "GATE_FUSE lookup";
const GATE_FUSE_PAYLOAD: &[u8] = b"knoxos-fuse\n";

fn lookup_self_test() -> bool {
    if mount("/mnt/fuse", 1).is_err() {
        serial_println!("[fuse] GATE_FUSE FAILED: mount");
        return false;
    }
    if loopback_put("/mnt/fuse", "gate.txt", GATE_FUSE_PAYLOAD).is_err() {
        serial_println!("[fuse] GATE_FUSE FAILED: put");
        return false;
    }
    let entry = match fuse_lookup("/mnt/fuse", 1, "gate.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE FAILED: lookup ({})", e);
            return false;
        }
    };
    match fuse_read("/mnt/fuse", entry.nodeid, 0, 64) {
        Ok(data)
            if data == GATE_FUSE_PAYLOAD && entry.attr.size == GATE_FUSE_PAYLOAD.len() as u64 =>
        {
            serial_println!(
                "[fuse] {} (LOOKUP+READ {} bytes ino={})",
                GATE_FUSE_MARKER,
                data.len(),
                entry.nodeid
            );
            true
        }
        other => {
            serial_println!(
                "[fuse] GATE_FUSE FAILED read_len={}",
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

pub const GATE_FUSE_WRITE_MARKER: &str = "GATE_FUSE write";
const GATE_FUSE_WRITE_PAYLOAD: &[u8] = b"knoxos-fuse-w\n";

/// WRITE opcode must store bytes that a subsequent READ returns.
pub fn write_self_test() -> bool {
    const MP: &str = "/mnt/fuse-w";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE write FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "gate.txt", b"old").is_err() {
        serial_println!("[fuse] GATE_FUSE write FAILED: put");
        return false;
    }
    let entry = match fuse_lookup(MP, 1, "gate.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE write FAILED: lookup ({})", e);
            return false;
        }
    };
    let written = match fuse_write(MP, entry.nodeid, 0, GATE_FUSE_WRITE_PAYLOAD) {
        Ok(n) => n,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE write FAILED: write ({})", e);
            return false;
        }
    };
    match fuse_read(MP, entry.nodeid, 0, 64) {
        Ok(data) if data == GATE_FUSE_WRITE_PAYLOAD && written == GATE_FUSE_WRITE_PAYLOAD.len() => {
            serial_println!(
                "[fuse] {} (WRITE+READ {} bytes ino={})",
                GATE_FUSE_WRITE_MARKER,
                data.len(),
                entry.nodeid
            );
            true
        }
        other => {
            serial_println!(
                "[fuse] GATE_FUSE write FAILED written={} read_len={}",
                written,
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

/// Initialize FUSE subsystem
pub const GATE_FUSE_UNLINK_MARKER: &str = "GATE_FUSE unlink";

/// UNLINK must drop the name so a later LOOKUP fails.
pub fn unlink_self_test() -> bool {
    const MP: &str = "/mnt/fuse-u";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE unlink FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "gone.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE unlink FAILED: put");
        return false;
    }
    if fuse_lookup(MP, 1, "gone.txt").is_err() {
        serial_println!("[fuse] GATE_FUSE unlink FAILED: lookup before");
        return false;
    }
    if let Err(e) = fuse_unlink(MP, 1, "gone.txt") {
        serial_println!("[fuse] GATE_FUSE unlink FAILED: unlink ({})", e);
        return false;
    }
    if fuse_lookup(MP, 1, "gone.txt").is_ok() {
        serial_println!("[fuse] GATE_FUSE unlink FAILED: name still present");
        return false;
    }
    serial_println!(
        "[fuse] {} (UNLINK then LOOKUP ENOENT)",
        GATE_FUSE_UNLINK_MARKER
    );
    true
}

pub const GATE_FUSE_MKDIR_MARKER: &str = "GATE_FUSE mkdir";

/// MKDIR must create a directory that LOOKUP returns with S_IFDIR.
pub fn mkdir_self_test() -> bool {
    const MP: &str = "/mnt/fuse-m";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE mkdir FAILED: mount ({})", e);
            return false;
        }
    }
    let entry = match fuse_mkdir(MP, 1, "gatedir") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE mkdir FAILED: mkdir ({})", e);
            return false;
        }
    };
    if entry.attr.mode & 0o170000 != 0o040000 {
        serial_println!(
            "[fuse] GATE_FUSE mkdir FAILED: mode={:#o} (want S_IFDIR)",
            entry.attr.mode
        );
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "gatedir") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE mkdir FAILED: lookup ({})", e);
            return false;
        }
    };
    if looked.nodeid != entry.nodeid || looked.attr.mode & 0o170000 != 0o040000 {
        serial_println!(
            "[fuse] GATE_FUSE mkdir FAILED: lookup ino={} mode={:#o}",
            looked.nodeid,
            looked.attr.mode
        );
        return false;
    }
    serial_println!(
        "[fuse] {} (MKDIR+LOOKUP S_IFDIR ino={})",
        GATE_FUSE_MKDIR_MARKER,
        entry.nodeid
    );
    true
}

pub const GATE_FUSE_RMDIR_MARKER: &str = "GATE_FUSE rmdir";

/// RMDIR must drop the directory so a later LOOKUP fails.
pub fn rmdir_self_test() -> bool {
    const MP: &str = "/mnt/fuse-rm";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE rmdir FAILED: mount ({})", e);
            return false;
        }
    }
    if fuse_mkdir(MP, 1, "gone").is_err() {
        serial_println!("[fuse] GATE_FUSE rmdir FAILED: mkdir");
        return false;
    }
    if fuse_lookup(MP, 1, "gone").is_err() {
        serial_println!("[fuse] GATE_FUSE rmdir FAILED: lookup before");
        return false;
    }
    if let Err(e) = fuse_rmdir(MP, 1, "gone") {
        serial_println!("[fuse] GATE_FUSE rmdir FAILED: rmdir ({})", e);
        return false;
    }
    if fuse_lookup(MP, 1, "gone").is_ok() {
        serial_println!("[fuse] GATE_FUSE rmdir FAILED: name still present");
        return false;
    }
    serial_println!(
        "[fuse] {} (RMDIR then LOOKUP ENOENT)",
        GATE_FUSE_RMDIR_MARKER
    );
    true
}

pub const GATE_FUSE_RENAME_MARKER: &str = "GATE_FUSE rename";

/// RENAME must move the name so LOOKUP finds the new one.
pub fn rename_self_test() -> bool {
    const MP: &str = "/mnt/fuse-rn";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE rename FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "old.txt", b"rn").is_err() {
        serial_println!("[fuse] GATE_FUSE rename FAILED: put");
        return false;
    }
    let before = match fuse_lookup(MP, 1, "old.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE rename FAILED: lookup old ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_rename(MP, 1, "old.txt", "new.txt") {
        serial_println!("[fuse] GATE_FUSE rename FAILED: rename ({})", e);
        return false;
    }
    if fuse_lookup(MP, 1, "old.txt").is_ok() {
        serial_println!("[fuse] GATE_FUSE rename FAILED: old name still present");
        return false;
    }
    let after = match fuse_lookup(MP, 1, "new.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE rename FAILED: lookup new ({})", e);
            return false;
        }
    };
    if after.nodeid != before.nodeid {
        serial_println!(
            "[fuse] GATE_FUSE rename FAILED: ino {} -> {}",
            before.nodeid,
            after.nodeid
        );
        return false;
    }
    serial_println!(
        "[fuse] {} (RENAME old→new ino={})",
        GATE_FUSE_RENAME_MARKER,
        after.nodeid
    );
    true
}

pub const GATE_FUSE_SYMLINK_MARKER: &str = "GATE_FUSE symlink";

/// SYMLINK must create a link that LOOKUP returns with S_IFLNK and READLINK returns the target.
pub fn symlink_self_test() -> bool {
    const MP: &str = "/mnt/fuse-sl";
    const TARGET: &str = "/tmp/gate-target";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE symlink FAILED: mount ({})", e);
            return false;
        }
    }
    let entry = match fuse_symlink(MP, 1, "gate.link", TARGET) {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE symlink FAILED: symlink ({})", e);
            return false;
        }
    };
    if entry.attr.mode & 0o170000 != 0o120000 {
        serial_println!(
            "[fuse] GATE_FUSE symlink FAILED: mode={:#o} (want S_IFLNK)",
            entry.attr.mode
        );
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "gate.link") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE symlink FAILED: lookup ({})", e);
            return false;
        }
    };
    if looked.nodeid != entry.nodeid || looked.attr.mode & 0o170000 != 0o120000 {
        serial_println!(
            "[fuse] GATE_FUSE symlink FAILED: lookup ino={} mode={:#o}",
            looked.nodeid,
            looked.attr.mode
        );
        return false;
    }
    match fuse_readlink(MP, entry.nodeid) {
        Ok(got) if got == TARGET.as_bytes() => {
            serial_println!(
                "[fuse] {} (SYMLINK+READLINK ino={})",
                GATE_FUSE_SYMLINK_MARKER,
                entry.nodeid
            );
            true
        }
        other => {
            serial_println!(
                "[fuse] GATE_FUSE symlink FAILED: target len={}",
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

pub const GATE_FUSE_LINK_MARKER: &str = "GATE_FUSE link";

/// LINK must add a name that LOOKUP finds with the same inode.
pub fn link_self_test() -> bool {
    const MP: &str = "/mnt/fuse-ln";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE link FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "old.txt", b"ln").is_err() {
        serial_println!("[fuse] GATE_FUSE link FAILED: put");
        return false;
    }
    let before = match fuse_lookup(MP, 1, "old.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE link FAILED: lookup old ({})", e);
            return false;
        }
    };
    if let Err(e) = fuse_link(MP, 1, "old.txt", "new.txt") {
        serial_println!("[fuse] GATE_FUSE link FAILED: link ({})", e);
        return false;
    }
    let after = match fuse_lookup(MP, 1, "new.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE link FAILED: lookup new ({})", e);
            return false;
        }
    };
    if after.nodeid != before.nodeid {
        serial_println!(
            "[fuse] GATE_FUSE link FAILED: ino {} -> {}",
            before.nodeid,
            after.nodeid
        );
        return false;
    }
    if fuse_lookup(MP, 1, "old.txt").is_err() {
        serial_println!("[fuse] GATE_FUSE link FAILED: old name missing");
        return false;
    }
    serial_println!(
        "[fuse] {} (LINK old+new ino={})",
        GATE_FUSE_LINK_MARKER,
        after.nodeid
    );
    true
}

pub const GATE_FUSE_CREATE_MARKER: &str = "GATE_FUSE create";

/// CREATE must make a regular file that LOOKUP returns with S_IFREG.
pub fn create_self_test() -> bool {
    const MP: &str = "/mnt/fuse-c";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE create FAILED: mount ({})", e);
            return false;
        }
    }
    let entry = match fuse_create(MP, 1, "gatec.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE create FAILED: create ({})", e);
            return false;
        }
    };
    if entry.attr.mode & 0o170000 != 0o100000 {
        serial_println!(
            "[fuse] GATE_FUSE create FAILED: mode={:#o} (want S_IFREG)",
            entry.attr.mode
        );
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "gatec.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE create FAILED: lookup ({})", e);
            return false;
        }
    };
    if looked.nodeid != entry.nodeid || looked.attr.mode & 0o170000 != 0o100000 {
        serial_println!(
            "[fuse] GATE_FUSE create FAILED: lookup ino={} mode={:#o}",
            looked.nodeid,
            looked.attr.mode
        );
        return false;
    }
    serial_println!(
        "[fuse] {} (CREATE+LOOKUP S_IFREG ino={})",
        GATE_FUSE_CREATE_MARKER,
        entry.nodeid
    );
    true
}

pub const GATE_FUSE_GETATTR_MARKER: &str = "GATE_FUSE getattr";
const GATE_FUSE_GETATTR_PAYLOAD: &[u8] = b"knoxos-fuse-g\n";

/// GETATTR must return size and S_IFREG for a created file.
pub fn getattr_self_test() -> bool {
    const MP: &str = "/mnt/fuse-g";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE getattr FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "gattr.txt", GATE_FUSE_GETATTR_PAYLOAD).is_err() {
        serial_println!("[fuse] GATE_FUSE getattr FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "gattr.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE getattr FAILED: lookup ({})", e);
            return false;
        }
    };
    let attr = match fuse_getattr(MP, looked.nodeid) {
        Ok(a) => a,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE getattr FAILED: getattr ({})", e);
            return false;
        }
    };
    if attr.ino != looked.nodeid
        || attr.size != GATE_FUSE_GETATTR_PAYLOAD.len() as u64
        || attr.mode & 0o170000 != 0o100000
    {
        serial_println!(
            "[fuse] GATE_FUSE getattr FAILED: ino={} size={} mode={:#o}",
            attr.ino,
            attr.size,
            attr.mode
        );
        return false;
    }
    serial_println!(
        "[fuse] {} (GETATTR size={} S_IFREG ino={})",
        GATE_FUSE_GETATTR_MARKER,
        attr.size,
        attr.ino
    );
    true
}

pub const GATE_FUSE_SETATTR_MARKER: &str = "GATE_FUSE setattr";
const GATE_FUSE_SETATTR_PAYLOAD: &[u8] = b"knoxos-fuse-sa\n";
const GATE_FUSE_SETATTR_SIZE: u64 = 5;

/// SETATTR must change size so a later GETATTR/READ sees the new length.
pub fn setattr_self_test() -> bool {
    const MP: &str = "/mnt/fuse-sa";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setattr FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "sattr.txt", GATE_FUSE_SETATTR_PAYLOAD).is_err() {
        serial_println!("[fuse] GATE_FUSE setattr FAILED: put");
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "sattr.txt") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setattr FAILED: lookup ({})", e);
            return false;
        }
    };
    let attr = match fuse_setattr(MP, looked.nodeid, GATE_FUSE_SETATTR_SIZE) {
        Ok(a) => a,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setattr FAILED: setattr ({})", e);
            return false;
        }
    };
    if attr.ino != looked.nodeid || attr.size != GATE_FUSE_SETATTR_SIZE {
        serial_println!(
            "[fuse] GATE_FUSE setattr FAILED: ino={} size={}",
            attr.ino,
            attr.size
        );
        return false;
    }
    let got = match fuse_getattr(MP, looked.nodeid) {
        Ok(a) => a,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE setattr FAILED: getattr ({})", e);
            return false;
        }
    };
    match fuse_read(MP, looked.nodeid, 0, 64) {
        Ok(data)
            if got.size == GATE_FUSE_SETATTR_SIZE
                && data == GATE_FUSE_SETATTR_PAYLOAD[..GATE_FUSE_SETATTR_SIZE as usize] =>
        {
            serial_println!(
                "[fuse] {} (SETATTR size={} ino={})",
                GATE_FUSE_SETATTR_MARKER,
                got.size,
                got.ino
            );
            true
        }
        other => {
            serial_println!(
                "[fuse] GATE_FUSE setattr FAILED: getattr_size={} read_len={}",
                got.size,
                other.map(|d| d.len()).unwrap_or(0)
            );
            false
        }
    }
}

pub const GATE_FUSE_READDIR_MARKER: &str = "GATE_FUSE readdir";

/// READDIR of the root must list created files, directories, and names.
pub fn readdir_self_test() -> bool {
    const MP: &str = "/mnt/fuse-rd";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE readdir FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "a.txt", b"a").is_err() {
        serial_println!("[fuse] GATE_FUSE readdir FAILED: put");
        return false;
    }
    if fuse_mkdir(MP, 1, "bdir").is_err() {
        serial_println!("[fuse] GATE_FUSE readdir FAILED: mkdir");
        return false;
    }
    if fuse_create(MP, 1, "c.txt").is_err() {
        serial_println!("[fuse] GATE_FUSE readdir FAILED: create");
        return false;
    }
    let ents = match fuse_readdir(MP, 1) {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE readdir FAILED: readdir ({})", e);
            return false;
        }
    };
    let names: Vec<&str> = ents.iter().map(|(_, _, n)| n.as_str()).collect();
    if names.contains(&"a.txt") && names.contains(&"bdir") && names.contains(&"c.txt") {
        serial_println!(
            "[fuse] {} (READDIR {} names)",
            GATE_FUSE_READDIR_MARKER,
            ents.len()
        );
        true
    } else {
        serial_println!("[fuse] GATE_FUSE readdir FAILED: names={}", names.len());
        false
    }
}

pub const GATE_FUSE_MKNOD_MARKER: &str = "GATE_FUSE mknod";
const GATE_FUSE_MKNOD_MODE: u32 = 0o020666;

/// MKNOD must create a special file LOOKUP returns with S_IFCHR.
pub fn mknod_self_test() -> bool {
    const MP: &str = "/mnt/fuse-mn";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE mknod FAILED: mount ({})", e);
            return false;
        }
    }
    let entry = match fuse_mknod(MP, 1, "gate.dev", GATE_FUSE_MKNOD_MODE) {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE mknod FAILED: mknod ({})", e);
            return false;
        }
    };
    if entry.attr.mode & 0o170000 != 0o020000 {
        serial_println!(
            "[fuse] GATE_FUSE mknod FAILED: mode={:#o} (want S_IFCHR)",
            entry.attr.mode
        );
        return false;
    }
    let looked = match fuse_lookup(MP, 1, "gate.dev") {
        Ok(e) => e,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE mknod FAILED: lookup ({})", e);
            return false;
        }
    };
    if looked.nodeid != entry.nodeid || looked.attr.mode & 0o170000 != 0o020000 {
        serial_println!(
            "[fuse] GATE_FUSE mknod FAILED: lookup ino={} mode={:#o}",
            looked.nodeid,
            looked.attr.mode
        );
        return false;
    }
    serial_println!(
        "[fuse] {} (MKNOD+LOOKUP S_IFCHR ino={})",
        GATE_FUSE_MKNOD_MARKER,
        entry.nodeid
    );
    true
}

pub const GATE_FUSE_STATFS_MARKER: &str = "GATE_FUSE statfs";

/// STATFS must report bsize 4096 and count created names.
pub fn statfs_self_test() -> bool {
    const MP: &str = "/mnt/fuse-sf";
    match mount(MP, 1) {
        Ok(()) | Err("Mount point already in use") => {}
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE statfs FAILED: mount ({})", e);
            return false;
        }
    }
    if loopback_put(MP, "sf.txt", b"x").is_err() {
        serial_println!("[fuse] GATE_FUSE statfs FAILED: put");
        return false;
    }
    if fuse_mkdir(MP, 1, "sfdir").is_err() {
        serial_println!("[fuse] GATE_FUSE statfs FAILED: mkdir");
        return false;
    }
    let st = match fuse_statfs(MP) {
        Ok(s) => s,
        Err(e) => {
            serial_println!("[fuse] GATE_FUSE statfs FAILED: statfs ({})", e);
            return false;
        }
    };
    if st.bsize == 4096 && st.namelen == 255 && st.files >= 2 && st.blocks == 1024 {
        serial_println!(
            "[fuse] {} (STATFS bsize={} files={})",
            GATE_FUSE_STATFS_MARKER,
            st.bsize,
            st.files
        );
        true
    } else {
        serial_println!(
            "[fuse] GATE_FUSE statfs FAILED: bsize={} namelen={} files={} blocks={}",
            st.bsize,
            st.namelen,
            st.files,
            st.blocks
        );
        false
    }
}

/// Initialize FUSE subsystem
pub fn init() {
    serial_println!("[fuse] Filesystem in Userspace (FUSE) initialized");
    let _ = lookup_self_test();
    let _ = write_self_test();
    let _ = unlink_self_test();
    let _ = mkdir_self_test();
    let _ = rmdir_self_test();
    let _ = rename_self_test();
    let _ = symlink_self_test();
    let _ = link_self_test();
    let _ = create_self_test();
    let _ = getattr_self_test();
    let _ = setattr_self_test();
    let _ = readdir_self_test();
    let _ = mknod_self_test();
    let _ = statfs_self_test();
}
