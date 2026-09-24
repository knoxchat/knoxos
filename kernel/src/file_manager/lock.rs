//! Advisory POSIX file locks and inotify-style event stubs.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::constants::EAGAIN;

// ─── Watch / notification stub ──────────────────────────────────────

/// Filesystem event types (for inotify-style watchers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsEvent {
    Create,
    Delete,
    Modify,
    MovedFrom,
    MovedTo,
    Attrib,
    Open,
    Close,
}

/// Filesystem event.
#[derive(Debug, Clone)]
pub struct FsNotification {
    pub event: FsEvent,
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

// ─── File locking ───────────────────────────────────────────────────

/// Advisory file lock type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockType {
    /// F_RDLCK — shared lock
    ReadLock,
    /// F_WRLCK — exclusive lock
    WriteLock,
    /// F_UNLCK — unlock
    Unlock,
}

/// Advisory lock on a file region.
#[derive(Debug, Clone)]
pub struct FileLock {
    pub lock_type: LockType,
    pub start: u64,
    pub length: u64, // 0 = to end of file
    pub pid: u32,
}

lazy_static::lazy_static! {
    static ref FILE_LOCKS: Mutex<BTreeMap<u64, Vec<FileLock>>> = Mutex::new(BTreeMap::new());
}

/// fcntl F_SETLK — set advisory lock.
pub fn setlk(ino: u64, lock: &FileLock) -> Result<(), i32> {
    let mut locks = FILE_LOCKS.lock();
    let entry = locks.entry(ino).or_default();

    if lock.lock_type == LockType::Unlock {
        entry.retain(|l| l.pid != lock.pid || l.start != lock.start);
        return Ok(());
    }

    // Check for conflicts
    for existing in entry.iter() {
        if existing.pid == lock.pid {
            continue; // Same process can upgrade/downgrade
        }
        // Check overlap
        let existing_end = if existing.length == 0 {
            u64::MAX
        } else {
            existing.start + existing.length
        };
        let new_end = if lock.length == 0 {
            u64::MAX
        } else {
            lock.start + lock.length
        };
        if lock.start < existing_end && new_end > existing.start {
            // Overlapping region
            if lock.lock_type == LockType::WriteLock || existing.lock_type == LockType::WriteLock {
                return Err(EAGAIN); // Would block
            }
        }
    }

    // Remove any existing lock from this PID on this region
    entry.retain(|l| l.pid != lock.pid || l.start != lock.start);
    entry.push(lock.clone());
    Ok(())
}

/// fcntl F_GETLK — test advisory lock.
pub fn getlk(ino: u64, lock: &FileLock) -> Option<FileLock> {
    let locks = FILE_LOCKS.lock();
    if let Some(entry) = locks.get(&ino) {
        for existing in entry {
            if existing.pid == lock.pid {
                continue;
            }
            let existing_end = if existing.length == 0 {
                u64::MAX
            } else {
                existing.start + existing.length
            };
            let new_end = if lock.length == 0 {
                u64::MAX
            } else {
                lock.start + lock.length
            };
            if lock.start < existing_end
                && new_end > existing.start
                && (lock.lock_type == LockType::WriteLock
                    || existing.lock_type == LockType::WriteLock)
            {
                return Some(existing.clone());
            }
        }
    }
    None // No conflict
}
