/// aio — Asynchronous I/O support
/// Linux-compatible AIO (io_setup, io_submit, io_getevents, io_destroy)
/// Also includes io_uring-style submission/completion ring stubs
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// AIO operation codes
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u16)]
pub enum AioOpcode {
    Pread = 0,
    Pwrite = 1,
    Fsync = 2,
    Fdsync = 3,
    Noop = 6,
    Preadv = 7,
    Pwritev = 8,
}

impl AioOpcode {
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0 => Some(Self::Pread),
            1 => Some(Self::Pwrite),
            2 => Some(Self::Fsync),
            3 => Some(Self::Fdsync),
            6 => Some(Self::Noop),
            7 => Some(Self::Preadv),
            8 => Some(Self::Pwritev),
            _ => None,
        }
    }
}

/// IO Control Block (iocb) - submitted by user
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Iocb {
    pub aio_data: u64,       // User data returned in event
    pub aio_lio_opcode: u16, // Operation code
    pub aio_reqprio: i16,    // Request priority
    pub aio_fildes: u32,     // File descriptor
    pub aio_buf: u64,        // Buffer pointer
    pub aio_nbytes: u64,     // Number of bytes
    pub aio_offset: i64,     // File offset
    pub aio_flags: u32,
    pub aio_resfd: u32, // eventfd for notification
}

/// IO Event - returned to user on completion
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct IoEvent {
    pub data: u64, // Data from iocb.aio_data
    pub obj: u64,  // Pointer to original iocb
    pub res: i64,  // Result of operation
    pub res2: i64, // Secondary result
}

/// AIO context state
#[derive(Debug, Clone, Copy, PartialEq)]
enum AioState {
    Pending,
    Completed,
}

/// Submitted AIO request
#[derive(Debug, Clone)]
struct AioRequest {
    iocb: Iocb,
    state: AioState,
    result: i64,
}

/// AIO Context
struct AioContext {
    max_events: u32,
    requests: Vec<AioRequest>,
    completed: Vec<IoEvent>,
    pid: u32,
}

/// Next context ID
static NEXT_CTX_ID: Mutex<u64> = Mutex::new(1);

lazy_static::lazy_static! {
    static ref AIO_CONTEXTS: Mutex<BTreeMap<u64, AioContext>> = Mutex::new(BTreeMap::new());
}

/// Create a new AIO context
pub fn io_setup(max_events: u32) -> Result<u64, i32> {
    if max_events == 0 {
        return Err(-22); // EINVAL
    }

    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let mut next = NEXT_CTX_ID.lock();
    let ctx_id = *next;
    *next += 1;

    let ctx = AioContext {
        max_events,
        requests: Vec::new(),
        completed: Vec::new(),
        pid,
    };

    AIO_CONTEXTS.lock().insert(ctx_id, ctx);
    Ok(ctx_id)
}

/// Destroy an AIO context
pub fn io_destroy(ctx_id: u64) -> Result<(), i32> {
    AIO_CONTEXTS.lock().remove(&ctx_id).ok_or(-22)?; // EINVAL
    Ok(())
}

/// Submit I/O requests
pub fn io_submit(ctx_id: u64, iocbs: Vec<Iocb>) -> Result<i32, i32> {
    let mut contexts = AIO_CONTEXTS.lock();
    let ctx = contexts.get_mut(&ctx_id).ok_or(-22i32)?;

    if ctx.requests.len() + iocbs.len() > ctx.max_events as usize {
        return Err(-11); // EAGAIN
    }

    let submitted = iocbs.len();

    for iocb in iocbs {
        // Process the I/O request immediately (synchronous emulation)
        let result = process_iocb(&iocb);

        let event = IoEvent {
            data: iocb.aio_data,
            obj: 0, // Would be pointer to iocb
            res: result,
            res2: 0,
        };

        ctx.completed.push(event);
        ctx.requests.push(AioRequest {
            iocb,
            state: AioState::Completed,
            result,
        });
    }

    Ok(submitted as i32)
}

/// Process a single iocb
fn process_iocb(iocb: &Iocb) -> i64 {
    let opcode = match AioOpcode::from_u16(iocb.aio_lio_opcode) {
        Some(op) => op,
        None => return -22, // EINVAL
    };

    match opcode {
        AioOpcode::Pread => {
            // Read from fd at offset
            let pid = crate::scheduler::current_pid().unwrap_or(1);
            let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
            if let Some(fd_table) = tables.get_mut(&pid) {
                let buf = unsafe {
                    core::slice::from_raw_parts_mut(
                        iocb.aio_buf as *mut u8,
                        iocb.aio_nbytes as usize,
                    )
                };
                match fd_table.read(iocb.aio_fildes as i32, buf) {
                    Ok(n) => n as i64,
                    Err(e) => e as i64,
                }
            } else {
                -9 // EBADF
            }
        }
        AioOpcode::Pwrite => {
            let pid = crate::scheduler::current_pid().unwrap_or(1);
            let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
            if let Some(fd_table) = tables.get_mut(&pid) {
                let buf = unsafe {
                    core::slice::from_raw_parts(iocb.aio_buf as *const u8, iocb.aio_nbytes as usize)
                };
                match fd_table.write(iocb.aio_fildes as i32, buf) {
                    Ok(n) => n as i64,
                    Err(e) => e as i64,
                }
            } else {
                -9 // EBADF
            }
        }
        AioOpcode::Fsync | AioOpcode::Fdsync => {
            // No-op for now (everything is in RAM)
            0
        }
        AioOpcode::Noop => 0,
        _ => -22, // EINVAL
    }
}

/// Get completed events
pub fn io_getevents(ctx_id: u64, min_nr: i32, max_nr: i32) -> Result<Vec<IoEvent>, i32> {
    let mut contexts = AIO_CONTEXTS.lock();
    let ctx = contexts.get_mut(&ctx_id).ok_or(-22i32)?;

    if ctx.completed.is_empty() && min_nr > 0 {
        return Err(-11); // EAGAIN (would block)
    }

    let count = core::cmp::min(ctx.completed.len(), max_nr as usize);
    let events: Vec<IoEvent> = ctx.completed.drain(..count).collect();

    Ok(events)
}

// ═══════════════════════════════════════════════════════════════════════
// io_uring style interface stubs
// ═══════════════════════════════════════════════════════════════════════

/// io_uring setup parameters
#[derive(Debug, Clone)]
#[repr(C)]
pub struct IoUringParams {
    pub sq_entries: u32,
    pub cq_entries: u32,
    pub flags: u32,
    pub sq_thread_cpu: u32,
    pub sq_thread_idle: u32,
    pub features: u32,
    pub resv: [u32; 4],
}

/// io_uring instance
struct IoUring {
    params: IoUringParams,
    pid: u32,
    sq: Vec<Iocb>,
    cq: Vec<IoEvent>,
}

lazy_static::lazy_static! {
    static ref IO_URING_INSTANCES: Mutex<BTreeMap<i32, IoUring>> = Mutex::new(BTreeMap::new());
    static ref NEXT_URING_FD: Mutex<i32> = Mutex::new(2000);
}

/// io_uring_setup
pub fn io_uring_setup(entries: u32, params: &mut IoUringParams) -> Result<i32, i32> {
    if entries == 0 || entries > 4096 {
        return Err(-22); // EINVAL
    }

    params.sq_entries = entries.next_power_of_two();
    params.cq_entries = (entries * 2).next_power_of_two();
    params.features = 0x1; // IORING_FEAT_SINGLE_MMAP

    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let mut next = NEXT_URING_FD.lock();
    let fd = *next;
    *next += 1;

    let ring = IoUring {
        params: params.clone(),
        pid,
        sq: Vec::with_capacity(params.sq_entries as usize),
        cq: Vec::with_capacity(params.cq_entries as usize),
    };

    IO_URING_INSTANCES.lock().insert(fd, ring);
    Ok(fd)
}

/// io_uring_enter
pub fn io_uring_enter(fd: i32, to_submit: u32, min_complete: u32, flags: u32) -> Result<i32, i32> {
    let mut instances = IO_URING_INSTANCES.lock();
    let ring = instances.get_mut(&fd).ok_or(-9i32)?; // EBADF

    // Process submissions
    let submitted = core::cmp::min(to_submit as usize, ring.sq.len());
    let requests: Vec<Iocb> = ring.sq.drain(..submitted).collect();

    for iocb in &requests {
        let result = process_iocb(iocb);
        ring.cq.push(IoEvent {
            data: iocb.aio_data,
            obj: 0,
            res: result,
            res2: 0,
        });
    }

    Ok(submitted as i32)
}

pub fn init() {
    serial_println!("[KnoxOS] Async I/O (AIO + io_uring) initialized");
}
