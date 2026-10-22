/// close_range — Efficient file descriptor range closing
///
/// Implements the Linux close_range() syscall for efficiently closing
/// a range of file descriptors. Also supports CLOSE_RANGE_CLOEXEC
/// and CLOSE_RANGE_UNSHARE.
///
/// Features:
/// - Close range [first, last] of file descriptors
/// - CLOSE_RANGE_UNSHARE: unshare fd table before closing
/// - CLOSE_RANGE_CLOEXEC: set close-on-exec instead of closing
/// - Used by init systems and daemon launchers
/// - Efficient O(n) implementation vs N close() calls
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── Constants ──────────────────────────────────────────────────────

pub const CLOSE_RANGE_UNSHARE: u32 = 1 << 1;
pub const CLOSE_RANGE_CLOEXEC: u32 = 1 << 2;

/// Maximum file descriptor number
pub const MAX_FD: u32 = 1_048_576; // 1M, matches Linux default

// ─── Data Structures ────────────────────────────────────────────────

/// Flags for a file descriptor
#[derive(Debug, Clone, Copy)]
pub struct FdFlags {
    /// Close on exec
    pub cloexec: bool,
    /// Non-blocking
    pub nonblock: bool,
    /// Whether the FD is open
    pub open: bool,
}

impl Default for FdFlags {
    fn default() -> Self {
        Self {
            cloexec: false,
            nonblock: false,
            open: true,
        }
    }
}

/// Per-process file descriptor table
#[derive(Debug, Clone)]
pub struct FdTable {
    /// Open file descriptors and their flags
    pub fds: BTreeMap<u32, FdFlags>,
    /// Next available fd
    pub next_fd: u32,
    /// Maximum fd allowed
    pub max_fd: u32,
    /// Whether this table is shared with another process
    pub shared: bool,
}

impl FdTable {
    pub fn new() -> Self {
        let mut fds = BTreeMap::new();
        // stdin/stdout/stderr
        fds.insert(0, FdFlags::default()); // stdin
        fds.insert(1, FdFlags::default()); // stdout
        fds.insert(2, FdFlags::default()); // stderr
        Self {
            fds,
            next_fd: 3,
            max_fd: MAX_FD,
            shared: false,
        }
    }

    /// Allocate a new fd
    pub fn alloc_fd(&mut self) -> Result<u32, i32> {
        let fd = self.next_fd;
        if fd >= self.max_fd {
            return Err(-24); // EMFILE
        }
        self.fds.insert(fd, FdFlags::default());
        self.next_fd = fd + 1;
        Ok(fd)
    }

    /// Close a single fd
    pub fn close_fd(&mut self, fd: u32) -> Result<(), i32> {
        if self.fds.remove(&fd).is_some() {
            Ok(())
        } else {
            Err(-9) // EBADF
        }
    }

    /// close_range() implementation
    pub fn close_range(&mut self, first: u32, last: u32, flags: u32) -> Result<(), i32> {
        // Validate range
        if first > last {
            return Err(-22); // EINVAL
        }

        let last = if last == u32::MAX { self.max_fd } else { last };

        if flags & CLOSE_RANGE_UNSHARE != 0 {
            // Unshare the fd table (copy-on-write semantics)
            self.shared = false;
        }

        if flags & CLOSE_RANGE_CLOEXEC != 0 {
            // Set close-on-exec flag instead of closing
            let fds_in_range: Vec<u32> = self.fds.range(first..=last).map(|(&fd, _)| fd).collect();
            for fd in fds_in_range {
                if let Some(flags) = self.fds.get_mut(&fd) {
                    flags.cloexec = true;
                }
            }
        } else {
            // Close all fds in range
            let fds_to_close: Vec<u32> = self.fds.range(first..=last).map(|(&fd, _)| fd).collect();
            for fd in fds_to_close {
                self.fds.remove(&fd);
            }
        }

        Ok(())
    }

    /// Close all close-on-exec fds (called during exec())
    pub fn close_cloexec(&mut self) {
        let cloexec_fds: Vec<u32> = self
            .fds
            .iter()
            .filter(|(_, flags)| flags.cloexec)
            .map(|(&fd, _)| fd)
            .collect();
        for fd in cloexec_fds {
            self.fds.remove(&fd);
        }
    }

    /// Get the number of open fds
    pub fn count_open(&self) -> usize {
        self.fds.len()
    }

    /// Check if an fd is open
    pub fn is_open(&self, fd: u32) -> bool {
        self.fds.contains_key(&fd)
    }

    /// Set cloexec flag on a specific fd
    pub fn set_cloexec(&mut self, fd: u32, cloexec: bool) -> Result<(), i32> {
        if let Some(flags) = self.fds.get_mut(&fd) {
            flags.cloexec = cloexec;
            Ok(())
        } else {
            Err(-9) // EBADF
        }
    }
}

// ─── Per-process FD tables ──────────────────────────────────────────

pub struct CloseRangeState {
    /// Per-PID fd tables
    pub tables: BTreeMap<u32, FdTable>,
    /// Stats
    pub stats: CloseRangeStats,
}

#[derive(Debug, Clone, Default)]
pub struct CloseRangeStats {
    pub close_range_calls: u64,
    pub fds_closed: u64,
    pub cloexec_set: u64,
}

lazy_static::lazy_static! {
    pub static ref CLOSE_RANGE: Mutex<CloseRangeState> = Mutex::new(CloseRangeState::new());
}

impl CloseRangeState {
    pub fn new() -> Self {
        let mut tables = BTreeMap::new();
        // Kernel init process (PID 0)
        tables.insert(0, FdTable::new());
        Self {
            tables,
            stats: CloseRangeStats::default(),
        }
    }

    /// Create fd table for a new process
    pub fn create_table(&mut self, pid: u32) {
        self.tables.insert(pid, FdTable::new());
    }

    /// Fork fd table (copy parent's fds to child)
    pub fn fork_table(&mut self, parent_pid: u32, child_pid: u32) {
        if let Some(parent) = self.tables.get(&parent_pid) {
            let child_table = parent.clone();
            self.tables.insert(child_pid, child_table);
        } else {
            self.tables.insert(child_pid, FdTable::new());
        }
    }

    /// Remove fd table for terminated process
    pub fn remove_table(&mut self, pid: u32) {
        self.tables.remove(&pid);
    }

    /// Syscall: close_range(first, last, flags)
    pub fn sys_close_range(
        &mut self,
        pid: u32,
        first: u32,
        last: u32,
        flags: u32,
    ) -> Result<(), i32> {
        let table = self.tables.get_mut(&pid).ok_or(-3i32)?; // ESRCH
        self.stats.close_range_calls += 1;

        let before = table.count_open();
        let result = table.close_range(first, last, flags);
        let after = table.count_open();

        if flags & CLOSE_RANGE_CLOEXEC != 0 {
            self.stats.cloexec_set += (before - after) as u64;
        } else {
            self.stats.fds_closed += (before - after) as u64;
        }

        result
    }
}

// ─── Public API ─────────────────────────────────────────────────────

pub fn close_range(pid: u32, first: u32, last: u32, flags: u32) -> Result<(), i32> {
    CLOSE_RANGE.lock().sys_close_range(pid, first, last, flags)
}

pub fn create_fd_table(pid: u32) {
    CLOSE_RANGE.lock().create_table(pid);
}

pub fn fork_fd_table(parent: u32, child: u32) {
    CLOSE_RANGE.lock().fork_table(parent, child);
}

pub fn init() {
    serial_println!(
        "[CLOSE_RANGE] File descriptor management initialized (close_range, CLOEXEC, UNSHARE)"
    );
}
