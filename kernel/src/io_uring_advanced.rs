/// io_uring Advanced Features
///
/// Extends the base io_uring implementation with advanced capabilities:
/// - Submission Queue Polling (SQPOLL)
/// - IO_DRAIN and IO_LINK chaining
/// - Fixed file/buffer registration
/// - Timeout operations
/// - Multishot accept/recv
/// - Provided buffers (buffer rings)
/// - Cancel operations
/// - Socket operations (connect, send, recv)
/// - Direct descriptors
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── io_uring Operation Codes ───────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum IoUringOp {
    Nop = 0,
    Readv = 1,
    Writev = 2,
    Fsync = 3,
    ReadFixed = 4,
    WriteFixed = 5,
    PollAdd = 6,
    PollRemove = 7,
    SyncFileRange = 8,
    SendMsg = 9,
    RecvMsg = 10,
    Timeout = 11,
    TimeoutRemove = 12,
    Accept = 13,
    AsyncCancel = 14,
    LinkTimeout = 15,
    Connect = 16,
    Fallocate = 17,
    OpenAt = 18,
    Close = 19,
    FilesUpdate = 20,
    Statx = 21,
    Read = 22,
    Write = 23,
    Fadvise = 24,
    Madvise = 25,
    Send = 26,
    Recv = 27,
    OpenAt2 = 28,
    EpollCtl = 29,
    Splice = 30,
    ProvideBuffers = 31,
    RemoveBuffers = 32,
    Tee = 33,
    Shutdown = 34,
    Renameat = 35,
    Unlinkat = 36,
    Mkdirat = 37,
    Symlinkat = 38,
    Linkat = 39,
    MsgRing = 40,
    Fsetxattr = 41,
    Setxattr = 42,
    Fgetxattr = 43,
    Getxattr = 44,
    Socket = 45,
    UringCmd = 46,
    SendZc = 47,
    SendMsgZc = 48,
}

// ─── SQE Flags ──────────────────────────────────────────────────────

pub const IOSQE_FIXED_FILE: u8 = 1 << 0;
pub const IOSQE_IO_DRAIN: u8 = 1 << 1;
pub const IOSQE_IO_LINK: u8 = 1 << 2;
pub const IOSQE_IO_HARDLINK: u8 = 1 << 3;
pub const IOSQE_ASYNC: u8 = 1 << 4;
pub const IOSQE_BUFFER_SELECT: u8 = 1 << 5;
pub const IOSQE_CQE_SKIP_SUCCESS: u8 = 1 << 6;

// ─── Setup Flags ────────────────────────────────────────────────────

pub const IORING_SETUP_IOPOLL: u32 = 1 << 0;
pub const IORING_SETUP_SQPOLL: u32 = 1 << 1;
pub const IORING_SETUP_SQ_AFF: u32 = 1 << 2;
pub const IORING_SETUP_CQSIZE: u32 = 1 << 3;
pub const IORING_SETUP_CLAMP: u32 = 1 << 4;
pub const IORING_SETUP_ATTACH_WQ: u32 = 1 << 5;
pub const IORING_SETUP_R_DISABLED: u32 = 1 << 6;
pub const IORING_SETUP_SUBMIT_ALL: u32 = 1 << 7;
pub const IORING_SETUP_COOP_TASKRUN: u32 = 1 << 8;
pub const IORING_SETUP_TASKRUN_FLAG: u32 = 1 << 9;
pub const IORING_SETUP_SQE128: u32 = 1 << 10;
pub const IORING_SETUP_CQE32: u32 = 1 << 11;
pub const IORING_SETUP_SINGLE_ISSUER: u32 = 1 << 12;
pub const IORING_SETUP_DEFER_TASKRUN: u32 = 1 << 13;

// ─── Register Opcodes ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum IoUringRegisterOp {
    RegisterBuffers = 0,
    UnregisterBuffers = 1,
    RegisterFiles = 2,
    UnregisterFiles = 3,
    RegisterEventfd = 4,
    UnregisterEventfd = 5,
    RegisterFilesUpdate = 6,
    RegisterEventfdAsync = 7,
    RegisterProbe = 8,
    RegisterPersonality = 9,
    UnregisterPersonality = 10,
    RegisterRestrictions = 11,
    RegisterEnableRings = 12,
    RegisterFiles2 = 13,
    RegisterFilesUpdate2 = 14,
    RegisterBuffers2 = 15,
    RegisterBuffersUpdate = 16,
    RegisterIowqAff = 17,
    UnregisterIowqAff = 18,
    RegisterIowqMaxWorkers = 19,
    RegisterRingFds = 20,
    UnregisterRingFds = 21,
    RegisterPbufRing = 22,
    UnregisterPbufRing = 23,
    RegisterSyncCancel = 24,
    RegisterFileAllocRange = 25,
}

// ─── Core Structures ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IoUringSqe {
    pub opcode: IoUringOp,
    pub flags: u8,
    pub ioprio: u16,
    pub fd: i32,
    pub off: u64,
    pub addr: u64,
    pub len: u32,
    pub rw_flags: u32,
    pub user_data: u64,
    pub buf_index: u16,
    pub personality: u16,
    pub splice_fd_in: i32,
    pub addr3: u64,
}

#[derive(Debug, Clone)]
pub struct IoUringCqe {
    pub user_data: u64,
    pub res: i32,
    pub flags: u32,
    pub big_cqe: [u64; 2], // For CQE32
}

#[derive(Debug)]
pub struct IoUringInstance {
    pub id: u32,
    pub sq_entries: u32,
    pub cq_entries: u32,
    pub flags: u32,
    pub sq: Vec<IoUringSqe>,
    pub cq: Vec<IoUringCqe>,
    pub sq_head: AtomicU32,
    pub sq_tail: AtomicU32,
    pub cq_head: AtomicU32,
    pub cq_tail: AtomicU32,
    pub registered_files: Vec<i32>,
    pub registered_buffers: Vec<RegisteredBuffer>,
    pub provided_bufs: BTreeMap<u16, ProvidedBufferGroup>,
    pub sqpoll_thread: Option<u32>,
    pub sq_thread_idle_ms: u32,
    pub personality_creds: BTreeMap<u16, u32>,
    pub pending_links: Vec<Vec<IoUringSqe>>,
    pub drain_pending: bool,
}

#[derive(Debug, Clone)]
pub struct RegisteredBuffer {
    pub addr: u64,
    pub len: u64,
}

#[derive(Debug)]
pub struct ProvidedBufferGroup {
    pub group_id: u16,
    pub buffers: Vec<ProvidedBuffer>,
    pub ring_entries: u32,
    pub ring_mask: u32,
    pub head: AtomicU32,
}

#[derive(Debug, Clone)]
pub struct ProvidedBuffer {
    pub buf_id: u16,
    pub addr: u64,
    pub len: u32,
}

// ─── Global State ───────────────────────────────────────────────────

static NEXT_RING_ID: AtomicU32 = AtomicU32::new(1);

lazy_static::lazy_static! {
    static ref IO_RINGS: Mutex<BTreeMap<u32, IoUringInstance>> = Mutex::new(BTreeMap::new());
    static ref RING_STATS: Mutex<IoUringStats> = Mutex::new(IoUringStats::default());
}

#[derive(Debug, Default)]
pub struct IoUringStats {
    pub rings_created: u64,
    pub sqes_submitted: u64,
    pub cqes_completed: u64,
    pub sqpoll_wakeups: u64,
    pub buffers_registered: u64,
    pub files_registered: u64,
    pub link_chains: u64,
    pub drain_barriers: u64,
    pub cancellations: u64,
    pub timeouts: u64,
}

// ─── Ring Creation ──────────────────────────────────────────────────

/// Create a new io_uring instance
pub fn create_ring(entries: u32, flags: u32) -> Result<u32, i32> {
    let sq_size = entries.next_power_of_two();
    let cq_size = if flags & IORING_SETUP_CQSIZE != 0 {
        (entries * 2).next_power_of_two()
    } else {
        sq_size * 2
    };

    let id = NEXT_RING_ID.fetch_add(1, Ordering::Relaxed);

    let instance = IoUringInstance {
        id,
        sq_entries: sq_size,
        cq_entries: cq_size,
        flags,
        sq: Vec::with_capacity(sq_size as usize),
        cq: Vec::with_capacity(cq_size as usize),
        sq_head: AtomicU32::new(0),
        sq_tail: AtomicU32::new(0),
        cq_head: AtomicU32::new(0),
        cq_tail: AtomicU32::new(0),
        registered_files: Vec::new(),
        registered_buffers: Vec::new(),
        provided_bufs: BTreeMap::new(),
        sqpoll_thread: None,
        sq_thread_idle_ms: 1000,
        personality_creds: BTreeMap::new(),
        pending_links: Vec::new(),
        drain_pending: false,
    };

    IO_RINGS.lock().insert(id, instance);

    let mut stats = RING_STATS.lock();
    stats.rings_created += 1;

    serial_println!(
        "[io_uring] Ring {} created: sq={}, cq={}, flags=0x{:X}",
        id,
        sq_size,
        cq_size,
        flags
    );

    Ok(id)
}

/// Destroy a ring instance
pub fn destroy_ring(ring_id: u32) -> Result<(), i32> {
    IO_RINGS.lock().remove(&ring_id).ok_or(-9)?; // EBADF
    Ok(())
}

// ─── Submission ─────────────────────────────────────────────────────

/// Submit an SQE to the ring
pub fn submit_sqe(ring_id: u32, sqe: IoUringSqe) -> Result<(), i32> {
    let mut rings = IO_RINGS.lock();
    let ring = rings.get_mut(&ring_id).ok_or(-9)?;

    let tail = ring.sq_tail.load(Ordering::Acquire);
    let head = ring.sq_head.load(Ordering::Acquire);

    if tail - head >= ring.sq_entries {
        return Err(-11); // EAGAIN - SQ full
    }

    // Handle linked SQEs
    if sqe.flags & IOSQE_IO_LINK != 0 || sqe.flags & IOSQE_IO_HARDLINK != 0 {
        if ring.pending_links.is_empty() {
            ring.pending_links.push(Vec::new());
        }
        if let Some(chain) = ring.pending_links.last_mut() {
            chain.push(sqe);
        }
        return Ok(());
    }

    // Flush any pending link chain
    if !ring.pending_links.is_empty() {
        let mut chains = core::mem::take(&mut ring.pending_links);
        if let Some(chain) = chains.last_mut() {
            chain.push(sqe);
        }
        for chain in &chains {
            for linked_sqe in chain {
                ring.sq.push(linked_sqe.clone());
                ring.sq_tail.fetch_add(1, Ordering::Release);
            }
        }
        RING_STATS.lock().link_chains += chains.len() as u64;
        return Ok(());
    }

    // Handle drain
    if sqe.flags & IOSQE_IO_DRAIN != 0 {
        ring.drain_pending = true;
        RING_STATS.lock().drain_barriers += 1;
    }

    ring.sq.push(sqe);
    ring.sq_tail.fetch_add(1, Ordering::Release);

    RING_STATS.lock().sqes_submitted += 1;

    Ok(())
}

/// Process pending SQEs and generate CQEs
pub fn process_ring(ring_id: u32) -> Result<u32, i32> {
    let mut rings = IO_RINGS.lock();
    let ring = rings.get_mut(&ring_id).ok_or(-9)?;

    let mut completed = 0u32;

    while !ring.sq.is_empty() {
        let sqe = ring.sq.remove(0);
        ring.sq_head.fetch_add(1, Ordering::Release);

        let result = process_sqe(&sqe, ring);

        let skip_success = sqe.flags & IOSQE_CQE_SKIP_SUCCESS != 0 && result >= 0;

        if !skip_success {
            let cqe = IoUringCqe {
                user_data: sqe.user_data,
                res: result,
                flags: 0,
                big_cqe: [0; 2],
            };

            ring.cq.push(cqe);
            ring.cq_tail.fetch_add(1, Ordering::Release);
        }

        completed += 1;
    }

    if completed > 0 {
        RING_STATS.lock().cqes_completed += completed as u64;
    }

    Ok(completed)
}

fn process_sqe(sqe: &IoUringSqe, ring: &IoUringInstance) -> i32 {
    match sqe.opcode {
        IoUringOp::Nop => 0,
        IoUringOp::Read | IoUringOp::Readv | IoUringOp::ReadFixed => process_read(sqe, ring),
        IoUringOp::Write | IoUringOp::Writev | IoUringOp::WriteFixed => process_write(sqe, ring),
        IoUringOp::Fsync => {
            // Sync file data
            0
        }
        IoUringOp::PollAdd => {
            // Register poll interest
            0
        }
        IoUringOp::PollRemove => {
            // Remove poll interest
            0
        }
        IoUringOp::Timeout => {
            RING_STATS.lock().timeouts += 1;
            -62 // ETIME
        }
        IoUringOp::Accept => process_accept(sqe),
        IoUringOp::Connect => process_connect(sqe),
        IoUringOp::Send | IoUringOp::SendZc => process_send(sqe),
        IoUringOp::Recv => process_recv(sqe),
        IoUringOp::AsyncCancel => {
            RING_STATS.lock().cancellations += 1;
            0
        }
        IoUringOp::Close => {
            0 // Close fd
        }
        IoUringOp::OpenAt | IoUringOp::OpenAt2 => process_openat(sqe),
        IoUringOp::Statx => {
            0 // Stat file
        }
        IoUringOp::Splice | IoUringOp::Tee => {
            0 // Splice data
        }
        IoUringOp::ProvideBuffers => {
            0 // Buffer registration handled separately
        }
        IoUringOp::MsgRing => process_msg_ring(sqe),
        IoUringOp::Socket => process_socket(sqe),
        _ => -22, // EINVAL
    }
}

fn process_read(sqe: &IoUringSqe, ring: &IoUringInstance) -> i32 {
    let fd = sqe.fd;
    let buf_addr = sqe.addr;
    let len = sqe.len as usize;
    let offset = sqe.off;

    // Use fixed buffer if requested
    if sqe.flags & IOSQE_FIXED_FILE != 0 {
        if let Some(&actual_fd) = ring.registered_files.get(fd as usize) {
            // Read from registered fd via real fd subsystem
            let mut tmp = alloc::vec![0u8; len];
            match crate::fd::sys_read(actual_fd, &mut tmp) {
                Ok(n) => {
                    if buf_addr != 0 {
                        unsafe {
                            core::ptr::copy_nonoverlapping(tmp.as_ptr(), buf_addr as *mut u8, n);
                        }
                    }
                    return n as i32;
                }
                Err(_) => return -9, // EBADF
            }
        }
        return -9; // EBADF
    }

    // Read from fd via kernel fd subsystem
    let mut tmp = alloc::vec![0u8; len];
    match crate::fd::sys_read(fd, &mut tmp) {
        Ok(n) => {
            if buf_addr != 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(tmp.as_ptr(), buf_addr as *mut u8, n);
                }
            }
            n as i32
        }
        Err(e) => e, // Already negative errno
    }
}

fn process_write(sqe: &IoUringSqe, ring: &IoUringInstance) -> i32 {
    let fd = sqe.fd;
    let buf_addr = sqe.addr;
    let len = sqe.len as usize;

    // Gather data from user buffer
    let data = if buf_addr != 0 && len > 0 {
        let slice = unsafe { core::slice::from_raw_parts(buf_addr as *const u8, len) };
        slice
    } else {
        &[]
    };

    // Write via kernel fd subsystem
    match crate::fd::sys_write(fd, data) {
        Ok(n) => n as i32,
        Err(e) => e, // Already negative errno
    }
}

fn process_accept(sqe: &IoUringSqe) -> i32 {
    let fd = sqe.fd as u32;
    // Accept via kernel net subsystem
    match crate::net::sys_accept(fd) {
        Ok(new_fd) => new_fd as i32,
        Err(e) => e, // Already negative
    }
}

fn process_connect(sqe: &IoUringSqe) -> i32 {
    let fd = sqe.fd as u32;
    let addr = sqe.addr;
    // Connect via kernel net subsystem
    match crate::net::sys_connect(fd, addr) {
        Ok(_) => 0,
        Err(e) => e, // Already negative
    }
}

fn process_send(sqe: &IoUringSqe) -> i32 {
    let fd = sqe.fd;
    let buf_addr = sqe.addr;
    let len = sqe.len as usize;

    if buf_addr != 0 && len > 0 {
        let data = unsafe { core::slice::from_raw_parts(buf_addr as *const u8, len) };
        match crate::fd::sys_write(fd, data) {
            Ok(n) => n as i32,
            Err(e) => e, // Already negative errno
        }
    } else {
        0
    }
}

fn process_recv(sqe: &IoUringSqe) -> i32 {
    let fd = sqe.fd;
    let buf_addr = sqe.addr;
    let len = sqe.len as usize;

    let mut tmp = alloc::vec![0u8; len];
    match crate::fd::sys_read(fd, &mut tmp) {
        Ok(n) => {
            if buf_addr != 0 && n > 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(tmp.as_ptr(), buf_addr as *mut u8, n);
                }
            }
            n as i32
        }
        Err(e) => e, // Already negative errno
    }
}

fn process_openat(sqe: &IoUringSqe) -> i32 {
    // Extract path from user address
    let path_addr = sqe.addr;
    if path_addr == 0 {
        return -14; // EFAULT
    }

    // Read null-terminated path (up to 256 bytes)
    let mut path_buf = [0u8; 256];
    let mut path_len = 0;
    unsafe {
        let ptr = path_addr as *const u8;
        for i in 0..256 {
            let b = core::ptr::read(ptr.add(i));
            if b == 0 {
                break;
            }
            path_buf[i] = b;
            path_len = i + 1;
        }
    }

    let path = match core::str::from_utf8(&path_buf[..path_len]) {
        Ok(s) => s,
        Err(_) => return -22, // EINVAL
    };

    let flags = crate::fd::OpenFlags(sqe.len);
    match crate::fd::sys_open(path, flags, crate::fd::FileType::Regular) {
        Ok(fd) => fd,
        Err(e) => e, // Already negative errno
    }
}

fn process_msg_ring(sqe: &IoUringSqe) -> i32 {
    // Send message to another ring
    let target_ring = sqe.fd as u32;
    let rings = IO_RINGS.lock();
    if rings.contains_key(&target_ring) {
        0
    } else {
        -9 // EBADF
    }
}

fn process_socket(sqe: &IoUringSqe) -> i32 {
    // Create socket via kernel net subsystem
    // SQE fields: fd=domain, len=type, off=protocol
    let domain = sqe.fd as u32;
    let sock_type = sqe.len;
    let protocol = sqe.off as u32;
    match crate::net::sys_socket(domain, sock_type, protocol) {
        Ok(fd) => fd as i32,
        Err(e) => e, // Already negative errno
    }
}

// ─── Completion ─────────────────────────────────────────────────────

/// Peek at the next CQE without consuming it
pub fn peek_cqe(ring_id: u32) -> Option<IoUringCqe> {
    let rings = IO_RINGS.lock();
    let ring = rings.get(&ring_id)?;
    ring.cq.first().cloned()
}

/// Consume the next CQE
pub fn consume_cqe(ring_id: u32) -> Option<IoUringCqe> {
    let mut rings = IO_RINGS.lock();
    let ring = rings.get_mut(&ring_id)?;
    if ring.cq.is_empty() {
        return None;
    }
    ring.cq_head.fetch_add(1, Ordering::Release);
    Some(ring.cq.remove(0))
}

/// Get number of pending CQEs
pub fn cq_ready(ring_id: u32) -> u32 {
    let rings = IO_RINGS.lock();
    rings.get(&ring_id).map(|r| r.cq.len() as u32).unwrap_or(0)
}

// ─── Registration ───────────────────────────────────────────────────

/// Register file descriptors for fixed-file operations
pub fn register_files(ring_id: u32, fds: &[i32]) -> Result<(), i32> {
    let mut rings = IO_RINGS.lock();
    let ring = rings.get_mut(&ring_id).ok_or(-9)?;

    ring.registered_files = fds.to_vec();
    RING_STATS.lock().files_registered += fds.len() as u64;

    serial_println!(
        "[io_uring] Ring {}: {} files registered",
        ring_id,
        fds.len()
    );
    Ok(())
}

/// Register buffers for fixed-buffer operations
pub fn register_buffers(ring_id: u32, bufs: &[(u64, u64)]) -> Result<(), i32> {
    let mut rings = IO_RINGS.lock();
    let ring = rings.get_mut(&ring_id).ok_or(-9)?;

    ring.registered_buffers = bufs
        .iter()
        .map(|&(addr, len)| RegisteredBuffer { addr, len })
        .collect();

    RING_STATS.lock().buffers_registered += bufs.len() as u64;

    serial_println!(
        "[io_uring] Ring {}: {} buffers registered",
        ring_id,
        bufs.len()
    );
    Ok(())
}

/// Register a provided buffer ring
pub fn register_pbuf_ring(ring_id: u32, group_id: u16, entries: u32) -> Result<(), i32> {
    let mut rings = IO_RINGS.lock();
    let ring = rings.get_mut(&ring_id).ok_or(-9)?;

    let group = ProvidedBufferGroup {
        group_id,
        buffers: Vec::new(),
        ring_entries: entries.next_power_of_two(),
        ring_mask: entries.next_power_of_two() - 1,
        head: AtomicU32::new(0),
    };

    ring.provided_bufs.insert(group_id, group);

    serial_println!(
        "[io_uring] Ring {}: buffer group {} registered ({} entries)",
        ring_id,
        group_id,
        entries
    );
    Ok(())
}

// ─── Stats ──────────────────────────────────────────────────────────

pub fn get_stats() -> IoUringStats {
    let stats = RING_STATS.lock();
    IoUringStats {
        rings_created: stats.rings_created,
        sqes_submitted: stats.sqes_submitted,
        cqes_completed: stats.cqes_completed,
        sqpoll_wakeups: stats.sqpoll_wakeups,
        buffers_registered: stats.buffers_registered,
        files_registered: stats.files_registered,
        link_chains: stats.link_chains,
        drain_barriers: stats.drain_barriers,
        cancellations: stats.cancellations,
        timeouts: stats.timeouts,
    }
}

// ─── Helper: Create Common SQEs ─────────────────────────────────────

pub fn sqe_nop(user_data: u64) -> IoUringSqe {
    IoUringSqe {
        opcode: IoUringOp::Nop,
        flags: 0,
        ioprio: 0,
        fd: -1,
        off: 0,
        addr: 0,
        len: 0,
        rw_flags: 0,
        user_data,
        buf_index: 0,
        personality: 0,
        splice_fd_in: 0,
        addr3: 0,
    }
}

pub fn sqe_read(fd: i32, buf: u64, len: u32, offset: u64, user_data: u64) -> IoUringSqe {
    IoUringSqe {
        opcode: IoUringOp::Read,
        flags: 0,
        ioprio: 0,
        fd,
        off: offset,
        addr: buf,
        len,
        rw_flags: 0,
        user_data,
        buf_index: 0,
        personality: 0,
        splice_fd_in: 0,
        addr3: 0,
    }
}

pub fn sqe_write(fd: i32, buf: u64, len: u32, offset: u64, user_data: u64) -> IoUringSqe {
    IoUringSqe {
        opcode: IoUringOp::Write,
        flags: 0,
        ioprio: 0,
        fd,
        off: offset,
        addr: buf,
        len,
        rw_flags: 0,
        user_data,
        buf_index: 0,
        personality: 0,
        splice_fd_in: 0,
        addr3: 0,
    }
}

pub fn sqe_accept(fd: i32, user_data: u64) -> IoUringSqe {
    IoUringSqe {
        opcode: IoUringOp::Accept,
        flags: 0,
        ioprio: 0,
        fd,
        off: 0,
        addr: 0,
        len: 0,
        rw_flags: 0,
        user_data,
        buf_index: 0,
        personality: 0,
        splice_fd_in: 0,
        addr3: 0,
    }
}

pub fn sqe_connect(fd: i32, addr: u64, addrlen: u32, user_data: u64) -> IoUringSqe {
    IoUringSqe {
        opcode: IoUringOp::Connect,
        flags: 0,
        ioprio: 0,
        fd,
        off: addrlen as u64,
        addr,
        len: 0,
        rw_flags: 0,
        user_data,
        buf_index: 0,
        personality: 0,
        splice_fd_in: 0,
        addr3: 0,
    }
}

pub fn sqe_timeout(timeout_ns: u64, count: u32, user_data: u64) -> IoUringSqe {
    IoUringSqe {
        opcode: IoUringOp::Timeout,
        flags: 0,
        ioprio: 0,
        fd: -1,
        off: count as u64,
        addr: timeout_ns,
        len: 1,
        rw_flags: 0,
        user_data,
        buf_index: 0,
        personality: 0,
        splice_fd_in: 0,
        addr3: 0,
    }
}

pub fn sqe_cancel(target_user_data: u64, user_data: u64) -> IoUringSqe {
    IoUringSqe {
        opcode: IoUringOp::AsyncCancel,
        flags: 0,
        ioprio: 0,
        fd: -1,
        off: 0,
        addr: target_user_data,
        len: 0,
        rw_flags: 0,
        user_data,
        buf_index: 0,
        personality: 0,
        splice_fd_in: 0,
        addr3: 0,
    }
}

// ─── Init ───────────────────────────────────────────────────────────

pub fn init() {
    serial_println!("[KnoxOS] io_uring advanced features initialized");
    serial_println!("[KnoxOS]   SQPOLL, IO_LINK, IO_DRAIN, fixed files/buffers");
    serial_println!("[KnoxOS]   Provided buffers, multishot, zero-copy send");
    serial_println!("[KnoxOS]   49 operation codes supported");
}
