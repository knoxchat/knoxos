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

/// In-kernel loopback daemon: path → (ino, bytes) plus directory inos.
#[derive(Debug, Default)]
struct LoopbackFs {
    files: BTreeMap<String, (u64, Vec<u8>)>,
    dirs: BTreeMap<String, u64>,
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

/// Initialize FUSE subsystem
pub fn init() {
    serial_println!("[fuse] Filesystem in Userspace (FUSE) initialized");
    let _ = lookup_self_test();
    let _ = write_self_test();
    let _ = unlink_self_test();
    let _ = mkdir_self_test();
    let _ = rmdir_self_test();
    let _ = rename_self_test();
}
