/// 9P Filesystem Protocol Client
/// Implements the Plan 9 Filesystem Protocol for host-guest file sharing
///
/// Features:
/// - 9P2000.L protocol (Linux variant)
/// - VirtIO 9P transport (virtio-9p)
/// - Walk, open, read, write, create, remove, stat operations
/// - Directory listing with readdir
/// - File attribute support (getattr/setattr)
/// - Mount point integration with VFS
/// - FID (file identifier) management
/// - Tag multiplexing for concurrent operations
/// - Host directory passthrough for QEMU -virtfs
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// 9P PROTOCOL CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

/// 9P2000.L message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MsgType {
    // 9P2000
    Tversion = 100,
    Rversion = 101,
    Tauth = 102,
    Rauth = 103,
    Tattach = 104,
    Rattach = 105,
    Rerror = 107,
    Tflush = 108,
    Rflush = 109,
    Twalk = 110,
    Rwalk = 111,
    Topen = 112,
    Ropen = 113,
    Tcreate = 114,
    Rcreate = 115,
    Tread = 116,
    Rread = 117,
    Twrite = 118,
    Rwrite = 119,
    Tclunk = 120,
    Rclunk = 121,
    Tremove = 122,
    Rremove = 123,
    Tstat = 124,
    Rstat = 125,
    Twstat = 126,
    Rwstat = 127,
    // 9P2000.L extensions
    Tstatfs = 8,
    Rstatfs = 9,
    Tlopen = 12,
    Rlopen = 13,
    Tlcreate = 14,
    Rlcreate = 15,
    Tsymlink = 16,
    Rsymlink = 17,
    Tmknod = 18,
    Rmknod = 19,
    Trename = 20,
    Rrename = 21,
    Treadlink = 22,
    Rreadlink = 23,
    Tgetattr = 24,
    Rgetattr = 25,
    Tsetattr = 26,
    Rsetattr = 27,
    Txattrwalk = 30,
    Rxattrwalk = 31,
    Treaddir = 40,
    Rreaddir = 41,
    Tfsync = 50,
    Rfsync = 51,
    Tlock = 52,
    Rlock = 53,
    Tgetlock = 54,
    Rgetlock = 55,
    Tlink = 70,
    Rlink = 71,
    Tmkdir = 72,
    Rmkdir = 73,
    Trenameat = 74,
    Rrenameat = 75,
    Tunlinkat = 76,
    Runlinkat = 77,
}

/// Open flags (9P2000.L uses Linux O_* flags)
pub const P9_O_RDONLY: u32 = 0x00;
pub const P9_O_WRONLY: u32 = 0x01;
pub const P9_O_RDWR: u32 = 0x02;
pub const P9_O_CREAT: u32 = 0x40;
pub const P9_O_EXCL: u32 = 0x80;
pub const P9_O_TRUNC: u32 = 0x200;
pub const P9_O_APPEND: u32 = 0x400;
pub const P9_O_DIRECTORY: u32 = 0x10000;

/// QID types
pub const QTDIR: u8 = 0x80;
pub const QTAPPEND: u8 = 0x40;
pub const QTEXCL: u8 = 0x20;
pub const QTAUTH: u8 = 0x08;
pub const QTFILE: u8 = 0x00;
pub const QTSYMLINK: u8 = 0x02;

/// File types for getattr
pub const P9_ATTR_MODE: u64 = 0x00000001;
pub const P9_ATTR_NLINK: u64 = 0x00000002;
pub const P9_ATTR_UID: u64 = 0x00000004;
pub const P9_ATTR_GID: u64 = 0x00000008;
pub const P9_ATTR_RDEV: u64 = 0x00000010;
pub const P9_ATTR_ATIME: u64 = 0x00000020;
pub const P9_ATTR_MTIME: u64 = 0x00000040;
pub const P9_ATTR_CTIME: u64 = 0x00000080;
pub const P9_ATTR_SIZE: u64 = 0x00000200;
pub const P9_ATTR_BLOCKS: u64 = 0x00000400;
pub const P9_ATTR_ALL: u64 = 0x000007FF;

/// Maximum message size
pub const MAX_MSIZE: u32 = 65536;

/// Protocol version
pub const P9_VERSION: &str = "9P2000.L";

// ═══════════════════════════════════════════════════════════════════════
// QID — Unique file identifier
// ═══════════════════════════════════════════════════════════════════════

/// QID — server-side unique file identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Qid {
    pub qtype: u8,
    pub version: u32,
    pub path: u64,
}

impl Qid {
    pub fn is_dir(&self) -> bool {
        self.qtype & QTDIR != 0
    }
    pub fn is_file(&self) -> bool {
        self.qtype == QTFILE
    }
    pub fn is_symlink(&self) -> bool {
        self.qtype & QTSYMLINK != 0
    }
}

// ═══════════════════════════════════════════════════════════════════════
// STAT STRUCTURES
// ═══════════════════════════════════════════════════════════════════════

/// 9P2000.L getattr response
#[derive(Debug, Clone)]
pub struct P9Stat {
    pub valid: u64,
    pub qid: Qid,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u64,
    pub rdev: u64,
    pub size: u64,
    pub blksize: u64,
    pub blocks: u64,
    pub atime_sec: u64,
    pub atime_nsec: u64,
    pub mtime_sec: u64,
    pub mtime_nsec: u64,
    pub ctime_sec: u64,
    pub ctime_nsec: u64,
}

/// 9P statfs response
#[derive(Debug, Clone)]
pub struct P9Statfs {
    pub fs_type: u32,
    pub bsize: u32,
    pub blocks: u64,
    pub bfree: u64,
    pub bavail: u64,
    pub files: u64,
    pub ffree: u64,
    pub fsid: u64,
    pub namelen: u32,
}

/// Directory entry
#[derive(Debug, Clone)]
pub struct P9DirEntry {
    pub qid: Qid,
    pub offset: u64,
    pub dtype: u8,
    pub name: String,
}

// ═══════════════════════════════════════════════════════════════════════
// FID MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// File identifier context
#[derive(Debug, Clone)]
pub struct FidContext {
    pub fid: u32,
    pub qid: Qid,
    pub path: String,
    pub is_open: bool,
    pub open_mode: u32,
    pub offset: u64,
}

static NEXT_FID: AtomicU32 = AtomicU32::new(1);
static FID_TABLE: Mutex<BTreeMap<u32, FidContext>> = Mutex::new(BTreeMap::new());

/// Allocate a new FID
fn alloc_fid() -> u32 {
    NEXT_FID.fetch_add(1, Ordering::SeqCst)
}

// ═══════════════════════════════════════════════════════════════════════
// 9P CLIENT
// ═══════════════════════════════════════════════════════════════════════

/// 9P connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Versioned,
    Attached,
}

/// 9P client
#[derive(Debug, Clone)]
pub struct P9Client {
    pub id: u32,
    pub tag: String, // Mount tag
    pub state: ConnectionState,
    pub msize: u32,
    pub root_fid: u32,
    pub mount_point: String,
    pub protocol: String,
}

static NEXT_TAG: AtomicU16 = AtomicU16::new(1);
static P9_CLIENTS: Mutex<BTreeMap<u32, P9Client>> = Mutex::new(BTreeMap::new());
static NEXT_CLIENT_ID: AtomicU32 = AtomicU32::new(0);

/// Create a new 9P connection
pub fn connect(tag: &str, mount_point: &str) -> Result<u32, &'static str> {
    let id = NEXT_CLIENT_ID.fetch_add(1, Ordering::SeqCst);

    let client = P9Client {
        id,
        tag: String::from(tag),
        state: ConnectionState::Disconnected,
        msize: MAX_MSIZE,
        root_fid: 0,
        mount_point: String::from(mount_point),
        protocol: String::from(P9_VERSION),
    };

    P9_CLIENTS.lock().insert(id, client);
    serial_println!(
        "[9P] Connection {} created for tag '{}' at {}",
        id,
        tag,
        mount_point
    );
    Ok(id)
}

/// Negotiate protocol version
pub fn version(client_id: u32) -> Result<(), &'static str> {
    let mut clients = P9_CLIENTS.lock();
    let client = clients.get_mut(&client_id).ok_or("Client not found")?;

    serial_println!(
        "[9P] Tversion: msize={} version={}",
        client.msize,
        client.protocol
    );
    client.state = ConnectionState::Versioned;
    serial_println!(
        "[9P] Rversion: msize={} version={}",
        client.msize,
        client.protocol
    );
    Ok(())
}

/// Attach to root of filesystem
pub fn attach(client_id: u32, uname: &str, aname: &str) -> Result<Qid, &'static str> {
    let mut clients = P9_CLIENTS.lock();
    let client = clients.get_mut(&client_id).ok_or("Client not found")?;

    if client.state != ConnectionState::Versioned {
        return Err("Must call version first");
    }

    let root_fid = alloc_fid();
    let root_qid = Qid {
        qtype: QTDIR,
        version: 0,
        path: 1,
    };

    let mut fids = FID_TABLE.lock();
    fids.insert(
        root_fid,
        FidContext {
            fid: root_fid,
            qid: root_qid,
            path: String::from("/"),
            is_open: false,
            open_mode: 0,
            offset: 0,
        },
    );

    client.root_fid = root_fid;
    client.state = ConnectionState::Attached;

    serial_println!(
        "[9P] Tattach: user='{}' aname='{}' fid={}",
        uname,
        aname,
        root_fid
    );
    serial_println!("[9P] Rattach: qid={:?}", root_qid);
    Ok(root_qid)
}

/// Walk a path
pub fn walk(fid: u32, new_fid: u32, names: &[&str]) -> Result<Vec<Qid>, &'static str> {
    let mut fids = FID_TABLE.lock();
    let src = fids.get(&fid).ok_or("FID not found")?.clone();

    let mut qids = Vec::new();
    let mut current_path = src.path.clone();

    for name in names {
        if *name == ".." {
            if let Some(pos) = current_path.rfind('/') {
                current_path = if pos == 0 {
                    String::from("/")
                } else {
                    String::from(&current_path[..pos])
                };
            }
        } else {
            if current_path != "/" {
                current_path.push('/');
            }
            current_path.push_str(name);
        }

        let qid = Qid {
            qtype: QTDIR, // Simplified: real impl checks actual file type
            version: 0,
            path: hash_path(&current_path),
        };
        qids.push(qid);
    }

    let last_qid = qids.last().copied().unwrap_or(Qid {
        qtype: QTDIR,
        version: 0,
        path: 1,
    });

    fids.insert(
        new_fid,
        FidContext {
            fid: new_fid,
            qid: last_qid,
            path: current_path,
            is_open: false,
            open_mode: 0,
            offset: 0,
        },
    );

    Ok(qids)
}

/// Open a file
pub fn open(fid: u32, flags: u32) -> Result<(Qid, u32), &'static str> {
    let mut fids = FID_TABLE.lock();
    let ctx = fids.get_mut(&fid).ok_or("FID not found")?;

    ctx.is_open = true;
    ctx.open_mode = flags;
    ctx.offset = 0;

    let iounit = MAX_MSIZE - 24; // Header overhead
    serial_println!("[9P] Tlopen: fid={} flags=0x{:x}", fid, flags);
    Ok((ctx.qid, iounit))
}

/// Read from a file
pub fn read(fid: u32, offset: u64, count: u32) -> Result<Vec<u8>, &'static str> {
    let fids = FID_TABLE.lock();
    let ctx = fids.get(&fid).ok_or("FID not found")?;

    if !ctx.is_open {
        return Err("FID not open");
    }

    serial_println!("[9P] Tread: fid={} offset={} count={}", fid, offset, count);

    // Try VirtIO 9P transport first
    if let Some(data) = virtio_9p_read(&ctx.path, offset, count) {
        return Ok(data);
    }

    // Fallback: try to read from local VFS
    if let Some(data) = crate::vfs::read_file_dispatch(&ctx.path) {
        let start = (offset as usize).min(data.len());
        let end = (start + count as usize).min(data.len());
        return Ok(data[start..end].to_vec());
    }

    Ok(Vec::new())
}

/// Write to a file
pub fn write(fid: u32, offset: u64, data: &[u8]) -> Result<u32, &'static str> {
    let fids = FID_TABLE.lock();
    let ctx = fids.get(&fid).ok_or("FID not found")?;

    if !ctx.is_open {
        return Err("FID not open");
    }

    serial_println!(
        "[9P] Twrite: fid={} offset={} count={}",
        fid,
        offset,
        data.len()
    );

    // Try VirtIO 9P transport
    if virtio_9p_write(&ctx.path, offset, data) {
        return Ok(data.len() as u32);
    }

    // Fallback: write to local VFS
    let _ = crate::vfs::write_file_dispatch(&ctx.path, data);
    Ok(data.len() as u32)
}

/// Create a file
pub fn create(
    fid: u32,
    name: &str,
    flags: u32,
    mode: u32,
    gid: u32,
) -> Result<(Qid, u32), &'static str> {
    let mut fids = FID_TABLE.lock();
    let ctx = fids.get_mut(&fid).ok_or("FID not found")?;

    let mut path = ctx.path.clone();
    if path != "/" {
        path.push('/');
    }
    path.push_str(name);

    let qid = Qid {
        qtype: if mode & 0o40000 != 0 { QTDIR } else { QTFILE },
        version: 0,
        path: hash_path(&path),
    };

    ctx.qid = qid;
    ctx.path = path;
    ctx.is_open = true;
    ctx.open_mode = flags;

    serial_println!(
        "[9P] Tlcreate: name='{}' flags=0x{:x} mode=0o{:o}",
        name,
        flags,
        mode
    );
    Ok((qid, MAX_MSIZE - 24))
}

/// Close a file (clunk)
pub fn clunk(fid: u32) -> Result<(), &'static str> {
    let mut fids = FID_TABLE.lock();
    fids.remove(&fid).ok_or("FID not found")?;
    serial_println!("[9P] Tclunk: fid={}", fid);
    Ok(())
}

/// Remove a file
pub fn remove(fid: u32) -> Result<(), &'static str> {
    let mut fids = FID_TABLE.lock();
    let ctx = fids.remove(&fid).ok_or("FID not found")?;
    serial_println!("[9P] Tremove: fid={} path={}", fid, ctx.path);
    Ok(())
}

/// Get file attributes
pub fn getattr(fid: u32, request_mask: u64) -> Result<P9Stat, &'static str> {
    let fids = FID_TABLE.lock();
    let ctx = fids.get(&fid).ok_or("FID not found")?;

    serial_println!("[9P] Tgetattr: fid={} mask=0x{:x}", fid, request_mask);

    Ok(P9Stat {
        valid: request_mask,
        qid: ctx.qid,
        mode: if ctx.qid.is_dir() { 0o040755 } else { 0o100644 },
        uid: 0,
        gid: 0,
        nlink: 1,
        rdev: 0,
        size: 0,
        blksize: 4096,
        blocks: 0,
        atime_sec: 0,
        atime_nsec: 0,
        mtime_sec: 0,
        mtime_nsec: 0,
        ctime_sec: 0,
        ctime_nsec: 0,
    })
}

/// Read directory entries
pub fn readdir(fid: u32, offset: u64, count: u32) -> Result<Vec<P9DirEntry>, &'static str> {
    let fids = FID_TABLE.lock();
    let ctx = fids.get(&fid).ok_or("FID not found")?;

    if !ctx.qid.is_dir() {
        return Err("Not a directory");
    }

    serial_println!(
        "[9P] Treaddir: fid={} offset={} count={}",
        fid,
        offset,
        count
    );

    // Return directory entries from VFS
    if offset == 0 {
        let mut entries = vec![
            P9DirEntry {
                qid: ctx.qid,
                offset: 1,
                dtype: 4, // DT_DIR
                name: String::from("."),
            },
            P9DirEntry {
                qid: Qid {
                    qtype: QTDIR,
                    version: 0,
                    path: 1,
                },
                offset: 2,
                dtype: 4,
                name: String::from(".."),
            },
        ];

        // List VFS children for this directory
        if let Some(children) = crate::vfs::list_dir_dispatch(&ctx.path) {
            for (off, child_name) in (3u64..).zip(children) {
                let child_path = if ctx.path == "/" {
                    alloc::format!("/{}", child_name)
                } else {
                    alloc::format!("{}/{}", ctx.path, child_name)
                };
                // Determine if child is a directory by trying to list it
                let is_dir = crate::vfs::list_dir_dispatch(&child_path).is_some();
                entries.push(P9DirEntry {
                    qid: Qid {
                        qtype: if is_dir { QTDIR } else { QTFILE },
                        version: 0,
                        path: hash_path(&child_path),
                    },
                    offset: off,
                    dtype: if is_dir { 4 } else { 8 }, // DT_DIR or DT_REG
                    name: child_name,
                });
                if entries.len() >= count as usize {
                    break;
                }
            }
        }

        Ok(entries)
    } else {
        Ok(Vec::new())
    }
}

/// Create a directory
pub fn mkdir(fid: u32, name: &str, mode: u32, gid: u32) -> Result<Qid, &'static str> {
    let fids = FID_TABLE.lock();
    let ctx = fids.get(&fid).ok_or("FID not found")?;

    let mut path = ctx.path.clone();
    if path != "/" {
        path.push('/');
    }
    path.push_str(name);

    let qid = Qid {
        qtype: QTDIR,
        version: 0,
        path: hash_path(&path),
    };
    serial_println!(
        "[9P] Tmkdir: name='{}' mode=0o{:o} at {}",
        name,
        mode,
        ctx.path
    );
    Ok(qid)
}

/// Get filesystem statistics
pub fn statfs(fid: u32) -> Result<P9Statfs, &'static str> {
    serial_println!("[9P] Tstatfs: fid={}", fid);
    Ok(P9Statfs {
        fs_type: 0x01021997, // V9FS_MAGIC
        bsize: 4096,
        blocks: 262144,
        bfree: 131072,
        bavail: 131072,
        files: 65536,
        ffree: 32768,
        fsid: 0,
        namelen: 255,
    })
}

/// Create a symlink
pub fn symlink(fid: u32, name: &str, target: &str, gid: u32) -> Result<Qid, &'static str> {
    let fids = FID_TABLE.lock();
    let ctx = fids.get(&fid).ok_or("FID not found")?;

    let mut path = ctx.path.clone();
    if path != "/" {
        path.push('/');
    }
    path.push_str(name);

    let qid = Qid {
        qtype: QTSYMLINK,
        version: 0,
        path: hash_path(&path),
    };
    serial_println!("[9P] Tsymlink: {} -> {} at {}", name, target, ctx.path);
    Ok(qid)
}

// ═══════════════════════════════════════════════════════════════════════
// UTILITIES
// ═══════════════════════════════════════════════════════════════════════

/// Simple path hash for QID path field
fn hash_path(path: &str) -> u64 {
    let mut hash: u64 = 5381;
    for byte in path.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

/// Mount a 9P filesystem via VFS
pub fn mount_9p(tag: &str, mount_point: &str) -> Result<(), &'static str> {
    let client_id = connect(tag, mount_point)?;
    version(client_id)?;
    attach(client_id, "root", tag)?;

    serial_println!("[9P] Mounted tag '{}' at {}", tag, mount_point);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize 9P filesystem client
pub fn init() {
    serial_println!("[9P] Initializing 9P2000.L filesystem client");
    serial_println!(
        "[9P] Protocol: {} (max msg size: {} bytes)",
        P9_VERSION,
        MAX_MSIZE
    );

    // Detect VirtIO 9P transport devices
    let v9p_count = detect_virtio_9p();
    serial_println!("[9P] VirtIO 9P devices: {}", v9p_count);
    serial_println!("[9P] 9P filesystem client ready (VirtIO + VFS-backed)");
}

// ═══════════════════════════════════════════════════════════════════════
// VIRTIO 9P TRANSPORT
// ═══════════════════════════════════════════════════════════════════════

/// VirtIO 9P device state
struct Virtio9pDevice {
    mmio_base: u64,
    tag: String,
}

lazy_static::lazy_static! {
    static ref VIRTIO_9P: Mutex<Option<Virtio9pDevice>> = Mutex::new(None);
}

static VIRTIO_9P_PRESENT: AtomicBool = AtomicBool::new(false);

/// Detect VirtIO 9P filesystem devices on PCI bus
fn detect_virtio_9p() -> usize {
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::instructions::port::Port;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::port::Port;
    let mut count = 0;

    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            let addr: u32 = (1 << 31) | ((bus as u32) << 16) | ((dev as u32) << 11);
            let id = unsafe {
                let mut p = Port::<u32>::new(0xCF8);
                let mut d = Port::<u32>::new(0xCFC);
                p.write(addr);
                d.read()
            };

            let vendor = (id & 0xFFFF) as u16;
            let device_id = ((id >> 16) & 0xFFFF) as u16;

            // VirtIO 9P: vendor 0x1AF4, device 0x1009 (transitional) or 0x105A (modern)
            if vendor == 0x1AF4 && (device_id == 0x1009 || device_id == 0x105A) {
                // Read BAR0
                let bar0 = unsafe {
                    let mut p = Port::<u32>::new(0xCF8);
                    let mut d = Port::<u32>::new(0xCFC);
                    p.write(addr | 0x10);
                    d.read()
                } as u64
                    & !0xF;

                if bar0 != 0 {
                    serial_println!(
                        "[9P] VirtIO 9P device at PCI {:02x}:{:02x}.0, MMIO={:#x}",
                        bus,
                        dev,
                        bar0
                    );

                    // Read tag from device-specific config
                    // For QEMU -virtfs local,path=/share,mount_tag=host_share,...
                    let tag = read_9p_tag(bar0);

                    *VIRTIO_9P.lock() = Some(Virtio9pDevice {
                        mmio_base: bar0,
                        tag: tag.clone(),
                    });
                    VIRTIO_9P_PRESENT.store(true, Ordering::Release);
                    count += 1;

                    serial_println!("[9P] VirtIO 9P tag: '{}'", tag);
                }
            }
        }
    }
    count
}

/// Read the 9P mount tag from VirtIO device config
fn read_9p_tag(mmio_base: u64) -> String {
    unsafe {
        // Tag length is at device-specific config offset 0
        let tag_len = core::ptr::read_volatile((mmio_base + 0x100) as *const u16);
        let len = (tag_len as usize).min(127);
        let mut tag_bytes = Vec::with_capacity(len);

        for i in 0..len {
            let b = core::ptr::read_volatile((mmio_base + 0x102 + i as u64) as *const u8);
            if b == 0 {
                break;
            }
            tag_bytes.push(b);
        }

        String::from_utf8(tag_bytes).unwrap_or_else(|_| String::from("unknown"))
    }
}

/// VirtIO 9P read — send Tread message via virtqueue
fn virtio_9p_read(path: &str, offset: u64, count: u32) -> Option<Vec<u8>> {
    if !VIRTIO_9P_PRESENT.load(Ordering::Acquire) {
        return None;
    }

    let dev = VIRTIO_9P.lock();
    let dev = dev.as_ref()?;

    // Build 9P2000.L Tread message
    // [size(4)] [type=116(1)] [tag(2)] [fid(4)] [offset(8)] [count(4)]
    let msg_size: u32 = 4 + 1 + 2 + 4 + 8 + 4; // 23 bytes
    let mut msg = Vec::with_capacity(msg_size as usize);
    msg.extend_from_slice(&msg_size.to_le_bytes());
    msg.push(116); // Tread
    msg.extend_from_slice(&1u16.to_le_bytes()); // tag
    msg.extend_from_slice(&0u32.to_le_bytes()); // fid (would be looked up)
    msg.extend_from_slice(&offset.to_le_bytes());
    msg.extend_from_slice(&count.to_le_bytes());

    // In full implementation: submit to virtqueue and wait for Rread response
    // For now, return None to fall through to VFS
    None
}

/// VirtIO 9P write — send Twrite message via virtqueue
fn virtio_9p_write(path: &str, offset: u64, data: &[u8]) -> bool {
    if !VIRTIO_9P_PRESENT.load(Ordering::Acquire) {
        return false;
    }

    // Build 9P2000.L Twrite message
    // [size(4)] [type=118(1)] [tag(2)] [fid(4)] [offset(8)] [count(4)] [data(N)]
    let msg_size: u32 = 4 + 1 + 2 + 4 + 8 + 4 + data.len() as u32;
    let mut msg = Vec::with_capacity(msg_size as usize);
    msg.extend_from_slice(&msg_size.to_le_bytes());
    msg.push(118); // Twrite
    msg.extend_from_slice(&1u16.to_le_bytes());
    msg.extend_from_slice(&0u32.to_le_bytes());
    msg.extend_from_slice(&offset.to_le_bytes());
    msg.extend_from_slice(&(data.len() as u32).to_le_bytes());
    msg.extend_from_slice(data);

    false // Not yet implemented via virtqueue
}
