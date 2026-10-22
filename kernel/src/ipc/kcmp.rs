/// kcmp — Process comparison syscall
///
/// Implements the Linux kcmp() syscall for comparing kernel resources
/// between two processes. Used by CRIU (Checkpoint/Restore in
/// Userspace) and process deduplication tools.
///
/// Features:
/// - KCMP_FILE — compare file descriptors
/// - KCMP_VM — compare virtual memory (address spaces)
/// - KCMP_FILES — compare file descriptor tables
/// - KCMP_FS — compare filesystem information (root, cwd, umask)
/// - KCMP_SIGHAND — compare signal handler tables
/// - KCMP_IO — compare I/O contexts
/// - KCMP_SYSVSEM — compare SysV semaphore undo lists
/// - KCMP_EPOLL_TFD — compare epoll target file descriptors
use spin::Mutex;

use crate::serial_println;

// ─── Constants ──────────────────────────────────────────────────────

/// kcmp types
pub const KCMP_FILE: u32 = 0;
pub const KCMP_VM: u32 = 1;
pub const KCMP_FILES: u32 = 2;
pub const KCMP_FS: u32 = 3;
pub const KCMP_SIGHAND: u32 = 4;
pub const KCMP_IO: u32 = 5;
pub const KCMP_SYSVSEM: u32 = 6;
pub const KCMP_EPOLL_TFD: u32 = 7;

/// Comparison results
pub const KCMP_ORDER_EQUAL: i32 = 0;
pub const KCMP_ORDER_LESS: i32 = -1;
pub const KCMP_ORDER_GREATER: i32 = 1;

// ─── Data Structures ────────────────────────────────────────────────

/// Resource key used for ordering comparisons
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceKey(u64);

impl ResourceKey {
    /// Create a key from a pointer-like value (kernel address or ID)
    pub fn from_id(id: u64) -> Self {
        Self(id)
    }

    /// Create a key combining PID and resource index
    pub fn from_pid_idx(pid: u32, idx: u32) -> Self {
        Self(((pid as u64) << 32) | idx as u64)
    }
}

/// Epoll target file descriptor descriptor for KCMP_EPOLL_TFD
#[derive(Debug, Clone, Copy)]
pub struct KcmpEpollSlot {
    /// Target file descriptor number
    pub tfd: u32,
    /// Epoll file descriptor
    pub efd: u32,
    /// Slot index in epoll
    pub toff: u32,
}

// ─── Simulated process resources for comparison ─────────────────────

/// Simplified process resource snapshot for comparison
#[derive(Debug, Clone)]
pub struct ProcessResources {
    /// PID
    pub pid: u32,
    /// Address space ID (processes sharing VM via clone(CLONE_VM) have same ID)
    pub vm_id: u64,
    /// File descriptor table ID (CLONE_FILES → shared)
    pub files_id: u64,
    /// FS struct ID (CLONE_FS → shared)
    pub fs_id: u64,
    /// Signal handler table ID (CLONE_SIGHAND → shared)
    pub sighand_id: u64,
    /// I/O context ID (CLONE_IO → shared)
    pub io_id: u64,
    /// SysV semaphore undo list ID
    pub sysvsem_id: u64,
    /// Per-fd kernel file pointer IDs (fd → unique file ID)
    pub fd_file_ids: alloc::collections::BTreeMap<u32, u64>,
}

impl ProcessResources {
    pub fn new(pid: u32) -> Self {
        let base = pid as u64 * 1000;
        Self {
            pid,
            vm_id: base + 1,
            files_id: base + 2,
            fs_id: base + 3,
            sighand_id: base + 4,
            io_id: base + 5,
            sysvsem_id: base + 6,
            fd_file_ids: alloc::collections::BTreeMap::new(),
        }
    }
}

// ─── Global State ───────────────────────────────────────────────────

pub struct KcmpState {
    /// Per-PID resource snapshots
    pub resources: alloc::collections::BTreeMap<u32, ProcessResources>,
    /// Stats
    pub stats: KcmpStats,
}

#[derive(Debug, Clone, Default)]
pub struct KcmpStats {
    pub kcmp_calls: u64,
    pub equal_results: u64,
}

lazy_static::lazy_static! {
    pub static ref KCMP: Mutex<KcmpState> = Mutex::new(KcmpState::new());
}

impl KcmpState {
    pub fn new() -> Self {
        let mut resources = alloc::collections::BTreeMap::new();
        // Init process
        let mut init = ProcessResources::new(1);
        init.fd_file_ids.insert(0, 100); // stdin
        init.fd_file_ids.insert(1, 101); // stdout
        init.fd_file_ids.insert(2, 102); // stderr
        resources.insert(1, init);
        Self {
            resources,
            stats: KcmpStats::default(),
        }
    }

    /// Register resources for a new process
    pub fn register_process(&mut self, pid: u32) {
        self.resources.insert(pid, ProcessResources::new(pid));
    }

    /// Register a forked process (shares some resources based on clone flags)
    pub fn register_fork(
        &mut self,
        parent: u32,
        child: u32,
        clone_vm: bool,
        clone_files: bool,
        clone_fs: bool,
        clone_sighand: bool,
        clone_io: bool,
    ) {
        if let Some(parent_res) = self.resources.get(&parent) {
            let mut child_res = ProcessResources::new(child);
            if clone_vm {
                child_res.vm_id = parent_res.vm_id;
            }
            if clone_files {
                child_res.files_id = parent_res.files_id;
                child_res.fd_file_ids = parent_res.fd_file_ids.clone();
            }
            if clone_fs {
                child_res.fs_id = parent_res.fs_id;
            }
            if clone_sighand {
                child_res.sighand_id = parent_res.sighand_id;
            }
            if clone_io {
                child_res.io_id = parent_res.io_id;
            }
            self.resources.insert(child, child_res);
        } else {
            self.resources.insert(child, ProcessResources::new(child));
        }
    }

    /// Remove resources for a terminated process
    pub fn unregister_process(&mut self, pid: u32) {
        self.resources.remove(&pid);
    }

    /// kcmp(pid1, pid2, type, idx1, idx2)
    pub fn sys_kcmp(
        &mut self,
        pid1: u32,
        pid2: u32,
        cmp_type: u32,
        idx1: u64,
        idx2: u64,
    ) -> Result<i32, i32> {
        let res1 = self.resources.get(&pid1).ok_or(-3i32)?; // ESRCH
        let res2 = self.resources.get(&pid2).ok_or(-3i32)?;

        self.stats.kcmp_calls += 1;

        let result = match cmp_type {
            KCMP_FILE => {
                // Compare if fd idx1 in pid1 points to same file as fd idx2 in pid2
                let file1 = res1.fd_file_ids.get(&(idx1 as u32)).copied().unwrap_or(0);
                let file2 = res2.fd_file_ids.get(&(idx2 as u32)).copied().unwrap_or(0);
                compare_keys(ResourceKey::from_id(file1), ResourceKey::from_id(file2))
            }
            KCMP_VM => compare_keys(
                ResourceKey::from_id(res1.vm_id),
                ResourceKey::from_id(res2.vm_id),
            ),
            KCMP_FILES => compare_keys(
                ResourceKey::from_id(res1.files_id),
                ResourceKey::from_id(res2.files_id),
            ),
            KCMP_FS => compare_keys(
                ResourceKey::from_id(res1.fs_id),
                ResourceKey::from_id(res2.fs_id),
            ),
            KCMP_SIGHAND => compare_keys(
                ResourceKey::from_id(res1.sighand_id),
                ResourceKey::from_id(res2.sighand_id),
            ),
            KCMP_IO => compare_keys(
                ResourceKey::from_id(res1.io_id),
                ResourceKey::from_id(res2.io_id),
            ),
            KCMP_SYSVSEM => compare_keys(
                ResourceKey::from_id(res1.sysvsem_id),
                ResourceKey::from_id(res2.sysvsem_id),
            ),
            KCMP_EPOLL_TFD => {
                // idx1 is target fd in pid1, idx2 points to KcmpEpollSlot struct
                // Simplified: compare by fd value
                compare_keys(
                    ResourceKey::from_pid_idx(pid1, idx1 as u32),
                    ResourceKey::from_pid_idx(pid2, idx2 as u32),
                )
            }
            _ => return Err(-22), // EINVAL
        };

        if result == KCMP_ORDER_EQUAL {
            self.stats.equal_results += 1;
        }

        Ok(result)
    }
}

/// Compare two resource keys
fn compare_keys(k1: ResourceKey, k2: ResourceKey) -> i32 {
    if k1 == k2 {
        KCMP_ORDER_EQUAL
    } else if k1 < k2 {
        KCMP_ORDER_LESS
    } else {
        KCMP_ORDER_GREATER
    }
}

// ─── Public API ─────────────────────────────────────────────────────

pub fn sys_kcmp(pid1: u32, pid2: u32, cmp_type: u32, idx1: u64, idx2: u64) -> Result<i32, i32> {
    KCMP.lock().sys_kcmp(pid1, pid2, cmp_type, idx1, idx2)
}

pub fn register_process(pid: u32) {
    KCMP.lock().register_process(pid);
}

pub fn unregister_process(pid: u32) {
    KCMP.lock().unregister_process(pid);
}

pub fn init() {
    serial_println!(
        "[KCMP] Process comparison subsystem initialized (FILE, VM, FILES, FS, SIGHAND, IO, SYSVSEM, EPOLL_TFD)"
    );
}
