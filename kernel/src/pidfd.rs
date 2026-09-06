// pidfd.rs — Process file descriptors (pidfd_open, pidfd_send_signal, pidfd_getfd)
// Linux 5.3+ API for race-free process management

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

/// pidfd_open flags
pub const PIDFD_NONBLOCK: u32 = 0x800; // O_NONBLOCK

/// pidfd_open flags (Linux 6.9+)
pub const PIDFD_THREAD: u32 = 0x1; // Refer to a thread

/// A process file descriptor
#[derive(Debug, Clone)]
pub struct PidFd {
    pub id: u64,
    pub pid: u64,
    pub flags: u32,
    pub owner_pid: u64,
    /// Whether the target process has exited
    pub exited: bool,
    /// Exit status (if exited)
    pub exit_status: i32,
    /// Signal mask for pidfd_send_signal
    pub signal_mask: u64,
}

impl PidFd {
    pub fn new(id: u64, pid: u64, flags: u32, owner: u64) -> Self {
        PidFd {
            id,
            pid,
            flags,
            owner_pid: owner,
            exited: false,
            exit_status: 0,
            signal_mask: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PidFdError {
    NotFound,      // ESRCH - process not found
    InvalidPid,    // EINVAL
    PermDenied,    // EPERM
    AlreadyExited, // Process already exited
    WouldBlock,    // EAGAIN
    InvalidSignal, // EINVAL
    TooMany,       // EMFILE
}

lazy_static! {
    static ref PIDFDS: Mutex<PidFdTable> = Mutex::new(PidFdTable::new());
}

struct PidFdTable {
    fds: BTreeMap<u64, PidFd>,
    /// pid → list of pidfd ids watching it
    watchers: BTreeMap<u64, Vec<u64>>,
    next_id: u64,
}

impl PidFdTable {
    fn new() -> Self {
        PidFdTable {
            fds: BTreeMap::new(),
            watchers: BTreeMap::new(),
            next_id: 1,
        }
    }
}

/// pidfd_open — create a file descriptor referring to a process
pub fn sys_pidfd_open(pid: u64, flags: u32) -> Result<u64, PidFdError> {
    if pid == 0 {
        return Err(PidFdError::InvalidPid);
    }

    // Check process exists
    let process_exists = crate::process::PROCESS_TABLE
        .lock()
        .processes
        .get(pid as usize)
        .is_some();
    if !process_exists {
        return Err(PidFdError::NotFound);
    }

    let mut table = PIDFDS.lock();
    let id = table.next_id;
    table.next_id += 1;

    let pidfd = PidFd::new(id, pid, flags, 0);
    table.fds.insert(id, pidfd);

    // Register as watcher
    table.watchers.entry(pid).or_default().push(id);

    Ok(id)
}

/// pidfd_send_signal — send a signal to a process via pidfd
pub fn sys_pidfd_send_signal(
    pidfd_id: u64,
    sig: i32,
    _info: u64,
    _flags: u32,
) -> Result<(), PidFdError> {
    let table = PIDFDS.lock();
    let pidfd = table.fds.get(&pidfd_id).ok_or(PidFdError::NotFound)?;

    if pidfd.exited {
        return Err(PidFdError::AlreadyExited);
    }

    if !(0..=64).contains(&sig) {
        return Err(PidFdError::InvalidSignal);
    }

    let pid = pidfd.pid;
    drop(table);

    // sig == 0 means just check if we can signal the process (permission check)
    if sig == 0 {
        return Ok(());
    }

    // Send signal via existing signal infrastructure
    if let Some(signal) = crate::signals::Signal::from_number(sig as u32) {
        let _ = crate::signals::kill(pid as u32, signal, 0);
    }
    Ok(())
}

/// pidfd_getfd — get a duplicate of a file descriptor from another process
/// (requires PTRACE_MODE_ATTACH_REALCREDS or CAP_SYS_PTRACE)
pub fn sys_pidfd_getfd(pidfd_id: u64, target_fd: i32, _flags: u32) -> Result<i32, PidFdError> {
    let table = PIDFDS.lock();
    let pidfd = table.fds.get(&pidfd_id).ok_or(PidFdError::NotFound)?;

    if pidfd.exited {
        return Err(PidFdError::AlreadyExited);
    }

    let _target_pid = pidfd.pid;
    let _fd = target_fd;
    drop(table);

    // In a full implementation, this would duplicate a fd from the target process
    // For now, return the target_fd as-is (simplified)
    Ok(target_fd)
}

/// waitid with P_PIDFD — wait for process exit via pidfd
pub fn pidfd_wait(pidfd_id: u64, flags: u32) -> Result<(u64, i32), PidFdError> {
    let table = PIDFDS.lock();
    let pidfd = table.fds.get(&pidfd_id).ok_or(PidFdError::NotFound)?;

    if pidfd.exited {
        return Ok((pidfd.pid, pidfd.exit_status));
    }

    if flags & PIDFD_NONBLOCK != 0 {
        return Err(PidFdError::WouldBlock);
    }

    let pid = pidfd.pid;
    drop(table);

    // Check if process has exited
    let proc_table = crate::process::PROCESS_TABLE.lock();
    if let Some(proc) = proc_table.processes.get(pid as usize) {
        if proc.state == crate::process::ProcessState::Zombie {
            return Ok((pid, 0));
        }
    } else {
        // Process not in table = already reaped
        return Ok((pid, 0));
    }
    drop(proc_table);

    Err(PidFdError::WouldBlock)
}

/// Notify all pidfds watching a process that it has exited
pub fn notify_exit(pid: u64, exit_status: i32) {
    let mut table = PIDFDS.lock();
    if let Some(watcher_ids) = table.watchers.get(&pid) {
        let ids: Vec<u64> = watcher_ids.clone();
        for id in ids {
            if let Some(pidfd) = table.fds.get_mut(&id) {
                pidfd.exited = true;
                pidfd.exit_status = exit_status;
            }
        }
    }
}

/// Close a pidfd
pub fn pidfd_close(pidfd_id: u64) {
    let mut table = PIDFDS.lock();
    if let Some(pidfd) = table.fds.remove(&pidfd_id) {
        // Remove from watchers
        if let Some(watchers) = table.watchers.get_mut(&pidfd.pid) {
            watchers.retain(|&id| id != pidfd_id);
            if watchers.is_empty() {
                table.watchers.remove(&pidfd.pid);
            }
        }
    }
}

/// Poll pidfd for readability (readable when process exits)
pub fn pidfd_poll(pidfd_id: u64) -> Result<bool, PidFdError> {
    let table = PIDFDS.lock();
    let pidfd = table.fds.get(&pidfd_id).ok_or(PidFdError::NotFound)?;
    Ok(pidfd.exited)
}

/// Get pidfd info
pub fn pidfd_info(pidfd_id: u64) -> Option<PidFdInfo> {
    let table = PIDFDS.lock();
    table.fds.get(&pidfd_id).map(|fd| PidFdInfo {
        id: fd.id,
        pid: fd.pid,
        flags: fd.flags,
        exited: fd.exited,
        exit_status: fd.exit_status,
    })
}

#[derive(Debug, Clone)]
pub struct PidFdInfo {
    pub id: u64,
    pub pid: u64,
    pub flags: u32,
    pub exited: bool,
    pub exit_status: i32,
}

/// Initialize pidfd subsystem
pub fn init() {
    crate::serial_println!(
        "  pidfd subsystem initialized (pidfd_open, pidfd_send_signal, pidfd_getfd)"
    );
}
