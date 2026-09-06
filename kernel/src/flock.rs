/// flock — File locking support
/// Linux-compatible advisory file locking (flock, fcntl/F_SETLK, POSIX locks)
///
/// Supports:
/// - flock() — whole-file advisory locks
/// - POSIX record locks (fcntl F_SETLK/F_SETLKW/F_GETLK)
/// - OFD locks (Open File Description locks)
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Lock types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LockType {
    /// Shared (read) lock
    ReadLock,
    /// Exclusive (write) lock
    WriteLock,
    /// Unlock
    Unlock,
}

/// flock() operation constants
pub const LOCK_SH: i32 = 1; // Shared lock
pub const LOCK_EX: i32 = 2; // Exclusive lock
pub const LOCK_UN: i32 = 8; // Unlock
pub const LOCK_NB: i32 = 4; // Non-blocking

/// fcntl lock commands
pub const F_GETLK: i32 = 5;
pub const F_SETLK: i32 = 6;
pub const F_SETLKW: i32 = 7;
pub const F_OFD_GETLK: i32 = 36;
pub const F_OFD_SETLK: i32 = 37;
pub const F_OFD_SETLKW: i32 = 38;

/// POSIX flock structure (for fcntl record locking)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Flock {
    pub l_type: i16,   // Lock type: F_RDLCK, F_WRLCK, F_UNLCK
    pub l_whence: i16, // SEEK_SET, SEEK_CUR, SEEK_END
    pub l_start: i64,  // Starting offset
    pub l_len: i64,    // Length (0 = until EOF)
    pub l_pid: i32,    // PID of lock owner
}

/// F_RDLCK, F_WRLCK, F_UNLCK
pub const F_RDLCK: i16 = 0;
pub const F_WRLCK: i16 = 1;
pub const F_UNLCK: i16 = 2;

/// A file lock entry
#[derive(Debug, Clone)]
struct FileLock {
    lock_type: LockType,
    pid: u32,
    /// For record locks
    start: i64,
    /// Length (0 = until EOF)
    len: i64,
    /// Whether this is a whole-file flock vs POSIX record lock
    is_flock: bool,
}

impl FileLock {
    fn overlaps(&self, start: i64, len: i64) -> bool {
        let end1 = if self.len == 0 {
            i64::MAX
        } else {
            self.start + self.len
        };
        let end2 = if len == 0 { i64::MAX } else { start + len };
        self.start < end2 && start < end1
    }

    fn conflicts_with(&self, lock_type: LockType, pid: u32) -> bool {
        if self.pid == pid {
            return false; // Same process can always upgrade/downgrade
        }
        !matches!(
            (self.lock_type, lock_type),
            (LockType::ReadLock, LockType::ReadLock)
        )
    }
}

/// Global lock table: inode_id → Vec<FileLock>
lazy_static::lazy_static! {
    static ref FILE_LOCKS: Mutex<BTreeMap<u64, Vec<FileLock>>> = Mutex::new(BTreeMap::new());
}

/// Apply a whole-file flock()
pub fn flock(inode_id: u64, operation: i32) -> Result<(), i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let nonblock = (operation & LOCK_NB) != 0;
    let op = operation & !LOCK_NB;

    match op {
        LOCK_SH => acquire_flock(inode_id, LockType::ReadLock, pid, nonblock),
        LOCK_EX => acquire_flock(inode_id, LockType::WriteLock, pid, nonblock),
        LOCK_UN => release_flock(inode_id, pid),
        _ => Err(-22), // EINVAL
    }
}

fn acquire_flock(inode_id: u64, lock_type: LockType, pid: u32, nonblock: bool) -> Result<(), i32> {
    let mut locks = FILE_LOCKS.lock();
    let file_locks = locks.entry(inode_id).or_default();

    // Check for conflicts
    for lock in file_locks.iter() {
        if lock.is_flock && lock.conflicts_with(lock_type, pid) {
            if nonblock {
                return Err(-11); // EAGAIN
            }
            // Would block - for now return EAGAIN
            return Err(-11);
        }
    }

    // Remove any existing flock from this process
    file_locks.retain(|l| !(l.is_flock && l.pid == pid));

    // Add new lock
    file_locks.push(FileLock {
        lock_type,
        pid,
        start: 0,
        len: 0,
        is_flock: true,
    });

    Ok(())
}

fn release_flock(inode_id: u64, pid: u32) -> Result<(), i32> {
    let mut locks = FILE_LOCKS.lock();
    if let Some(file_locks) = locks.get_mut(&inode_id) {
        file_locks.retain(|l| !(l.is_flock && l.pid == pid));
        if file_locks.is_empty() {
            locks.remove(&inode_id);
        }
    }
    Ok(())
}

/// Apply a POSIX record lock (fcntl F_SETLK/F_SETLKW)
pub fn posix_lock(inode_id: u64, cmd: i32, flock_info: &mut Flock) -> Result<(), i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(0);

    match cmd {
        F_GETLK | F_OFD_GETLK => getlk(inode_id, flock_info, pid),
        F_SETLK | F_OFD_SETLK => setlk(inode_id, flock_info, pid, false),
        F_SETLKW | F_OFD_SETLKW => setlk(inode_id, flock_info, pid, true),
        _ => Err(-22),
    }
}

fn getlk(inode_id: u64, flock_info: &mut Flock, pid: u32) -> Result<(), i32> {
    let lock_type = match flock_info.l_type {
        F_RDLCK => LockType::ReadLock,
        F_WRLCK => LockType::WriteLock,
        _ => return Err(-22),
    };

    let locks = FILE_LOCKS.lock();
    if let Some(file_locks) = locks.get(&inode_id) {
        for lock in file_locks {
            if !lock.is_flock
                && lock.overlaps(flock_info.l_start, flock_info.l_len)
                && lock.conflicts_with(lock_type, pid)
            {
                flock_info.l_type = match lock.lock_type {
                    LockType::ReadLock => F_RDLCK,
                    LockType::WriteLock => F_WRLCK,
                    LockType::Unlock => F_UNLCK,
                };
                flock_info.l_start = lock.start;
                flock_info.l_len = lock.len;
                flock_info.l_pid = lock.pid as i32;
                return Ok(());
            }
        }
    }

    // No conflicting lock found
    flock_info.l_type = F_UNLCK;
    Ok(())
}

fn setlk(inode_id: u64, flock_info: &Flock, pid: u32, blocking: bool) -> Result<(), i32> {
    let lock_type = match flock_info.l_type {
        F_RDLCK => LockType::ReadLock,
        F_WRLCK => LockType::WriteLock,
        F_UNLCK => LockType::Unlock,
        _ => return Err(-22),
    };

    let mut locks = FILE_LOCKS.lock();
    let file_locks = locks.entry(inode_id).or_default();

    if lock_type == LockType::Unlock {
        // Remove locks from this process in the specified range
        file_locks.retain(|l| {
            !(l.pid == pid && !l.is_flock && l.overlaps(flock_info.l_start, flock_info.l_len))
        });
        if file_locks.is_empty() {
            locks.remove(&inode_id);
        }
        return Ok(());
    }

    // Check for conflicts
    for lock in file_locks.iter() {
        if !lock.is_flock
            && lock.overlaps(flock_info.l_start, flock_info.l_len)
            && lock.conflicts_with(lock_type, pid)
        {
            if !blocking {
                return Err(-11); // EAGAIN
            }
            return Err(-11); // Would block
        }
    }

    // Remove any existing locks from this process in the range
    file_locks.retain(|l| {
        !(l.pid == pid && !l.is_flock && l.overlaps(flock_info.l_start, flock_info.l_len))
    });

    // Add new lock
    file_locks.push(FileLock {
        lock_type,
        pid,
        start: flock_info.l_start,
        len: flock_info.l_len,
        is_flock: false,
    });

    Ok(())
}

/// Release all locks held by a process (called on process exit)
pub fn release_all_locks(pid: u32) {
    let mut locks = FILE_LOCKS.lock();
    let mut empty_keys = Vec::new();

    for (inode_id, file_locks) in locks.iter_mut() {
        file_locks.retain(|l| l.pid != pid);
        if file_locks.is_empty() {
            empty_keys.push(*inode_id);
        }
    }

    for key in empty_keys {
        locks.remove(&key);
    }
}

pub fn init() {
    serial_println!("[KnoxOS] File locking (flock + POSIX locks) initialized");
}
