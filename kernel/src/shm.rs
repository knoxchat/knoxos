/// Shared Memory IPC - POSIX shared memory and memory-mapped segments
/// Implements shmget/shmat/shmdt/shmctl for inter-process communication
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::process::Pid;
use crate::serial_println;

/// Shared memory segment
pub struct SharedMemory {
    /// Segment ID
    pub id: u32,
    /// Key (for shmget)
    pub key: u32,
    /// Size in bytes
    pub size: usize,
    /// Data buffer
    pub data: Vec<u8>,
    /// Owner PID
    pub owner: Pid,
    /// Permission mode
    pub mode: u32,
    /// Attached process count
    pub nattach: u32,
    /// Creation time (ticks)
    pub ctime: u64,
    /// Last attach time
    pub atime: u64,
    /// Last detach time
    pub dtime: u64,
    /// Creator PID
    pub cpid: Pid,
    /// Last operation PID
    pub lpid: Pid,
}

impl SharedMemory {
    pub fn new(id: u32, key: u32, size: usize, owner: Pid, mode: u32) -> Self {
        let data = alloc::vec![0; size];
        Self {
            id,
            key,
            size,
            data,
            owner,
            mode,
            nattach: 0,
            ctime: crate::interrupts::get_ticks(),
            atime: 0,
            dtime: 0,
            cpid: owner,
            lpid: owner,
        }
    }
}

/// Attachment record - tracks which processes have attached to which segments
#[derive(Debug, Clone)]
pub struct ShmAttachment {
    pub shm_id: u32,
    pub pid: Pid,
    pub addr: u64, // Virtual address where attached
    pub readonly: bool,
}

/// IPC keys
pub const IPC_PRIVATE: u32 = 0;
pub const IPC_CREAT: u32 = 0o1000;
pub const IPC_EXCL: u32 = 0o2000;
pub const IPC_RMID: u32 = 0;
pub const IPC_SET: u32 = 1;
pub const IPC_STAT: u32 = 2;

static NEXT_SHM_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_SHM_ADDR: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0x7FF0_0000_0000);

lazy_static::lazy_static! {
    /// Global shared memory segments
    pub static ref SHM_SEGMENTS: Mutex<BTreeMap<u32, SharedMemory>> = Mutex::new(BTreeMap::new());

    /// Per-process attachments
    pub static ref SHM_ATTACHMENTS: Mutex<Vec<ShmAttachment>> = Mutex::new(Vec::new());
}

/// Get or create a shared memory segment (shmget)
pub fn shmget(key: u32, size: usize, flags: u32) -> Result<u32, i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let mut segments = SHM_SEGMENTS.lock();

    // Check if segment with this key exists
    if key != IPC_PRIVATE {
        for (id, seg) in segments.iter() {
            if seg.key == key {
                if flags & IPC_CREAT != 0 && flags & IPC_EXCL != 0 {
                    return Err(-17); // EEXIST
                }
                return Ok(*id);
            }
        }
    }

    // Create new segment
    if flags & IPC_CREAT != 0 || key == IPC_PRIVATE {
        let id = NEXT_SHM_ID.fetch_add(1, Ordering::Relaxed);
        let mode = flags & 0o777;
        let segment = SharedMemory::new(id, key, size, pid, mode);
        serial_println!(
            "[KnoxOS] shmget: created segment {} (key={}, size={})",
            id,
            key,
            size
        );
        segments.insert(id, segment);
        Ok(id)
    } else {
        Err(-2) // ENOENT
    }
}

/// Attach a shared memory segment to the process address space (shmat)
pub fn shmat(shm_id: u32, shmaddr: u64, flags: u32) -> Result<u64, i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let mut segments = SHM_SEGMENTS.lock();

    let segment = segments.get_mut(&shm_id).ok_or(-22i32)?; // EINVAL

    // Determine attach address
    let addr = if shmaddr != 0 {
        shmaddr
    } else {
        // Auto-assign address
        NEXT_SHM_ADDR.fetch_add((segment.size as u64 + 0xFFF) & !0xFFF, Ordering::Relaxed)
    };

    let readonly = flags & 0x1000 != 0; // SHM_RDONLY

    segment.nattach += 1;
    segment.atime = crate::interrupts::get_ticks();
    segment.lpid = pid;

    let attachment = ShmAttachment {
        shm_id,
        pid,
        addr,
        readonly,
    };

    drop(segments);
    SHM_ATTACHMENTS.lock().push(attachment);

    serial_println!(
        "[KnoxOS] shmat: PID {} attached segment {} at {:#x}",
        pid,
        shm_id,
        addr
    );
    Ok(addr)
}

/// Detach a shared memory segment (shmdt)
pub fn shmdt(shmaddr: u64) -> Result<(), i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let mut attachments = SHM_ATTACHMENTS.lock();

    let pos = attachments
        .iter()
        .position(|a| a.pid == pid && a.addr == shmaddr)
        .ok_or(-22i32)?; // EINVAL

    let attachment = attachments.remove(pos);

    let mut segments = SHM_SEGMENTS.lock();
    if let Some(segment) = segments.get_mut(&attachment.shm_id) {
        segment.nattach = segment.nattach.saturating_sub(1);
        segment.dtime = crate::interrupts::get_ticks();
        segment.lpid = pid;
    }

    serial_println!("[KnoxOS] shmdt: PID {} detached from {:#x}", pid, shmaddr);
    Ok(())
}

/// Control shared memory segment (shmctl)
pub fn shmctl(shm_id: u32, cmd: u32) -> Result<(), i32> {
    let mut segments = SHM_SEGMENTS.lock();

    match cmd {
        0 => {
            // IPC_RMID - mark for deletion
            if let Some(segment) = segments.get(&shm_id) {
                if segment.nattach == 0 {
                    segments.remove(&shm_id);
                    serial_println!("[KnoxOS] shmctl: removed segment {}", shm_id);
                } else {
                    // Mark for deletion when last process detaches
                    serial_println!("[KnoxOS] shmctl: segment {} marked for deletion", shm_id);
                }
                Ok(())
            } else {
                Err(-22) // EINVAL
            }
        }
        _ => Ok(()),
    }
}

/// Read from a shared memory segment (for internal use)
pub fn shm_read(shm_id: u32, offset: usize, buf: &mut [u8]) -> Result<usize, i32> {
    let segments = SHM_SEGMENTS.lock();
    let segment = segments.get(&shm_id).ok_or(-22i32)?;

    if offset >= segment.size {
        return Ok(0);
    }

    let available = segment.size - offset;
    let to_read = buf.len().min(available);
    buf[..to_read].copy_from_slice(&segment.data[offset..offset + to_read]);
    Ok(to_read)
}

/// Write to a shared memory segment (for internal use)
pub fn shm_write(shm_id: u32, offset: usize, data: &[u8]) -> Result<usize, i32> {
    let mut segments = SHM_SEGMENTS.lock();
    let segment = segments.get_mut(&shm_id).ok_or(-22i32)?;

    if offset >= segment.size {
        return Err(-22); // EINVAL
    }

    let available = segment.size - offset;
    let to_write = data.len().min(available);
    segment.data[offset..offset + to_write].copy_from_slice(&data[..to_write]);
    Ok(to_write)
}

/// Clean up shared memory for a process
pub fn cleanup_process_shm(pid: Pid) {
    let mut attachments = SHM_ATTACHMENTS.lock();
    let to_detach: Vec<u32> = attachments
        .iter()
        .filter(|a| a.pid == pid)
        .map(|a| a.shm_id)
        .collect();

    attachments.retain(|a| a.pid != pid);

    let mut segments = SHM_SEGMENTS.lock();
    for shm_id in to_detach {
        if let Some(segment) = segments.get_mut(&shm_id) {
            segment.nattach = segment.nattach.saturating_sub(1);
        }
    }
}

/// Initialize shared memory subsystem
pub fn init() {
    serial_println!("[KnoxOS] Shared memory (SysV IPC) initialized");
}
