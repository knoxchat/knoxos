/// Process Resource Limits — Linux-compatible RLIMIT subsystem
///
/// Implements getrlimit/setrlimit/prlimit64 for per-process resource limits:
///   - RLIMIT_NOFILE (max open file descriptors)
///   - RLIMIT_NPROC  (max number of processes)
///   - RLIMIT_AS     (max address space size)
///   - RLIMIT_FSIZE  (max file size)
///   - RLIMIT_STACK  (max stack size)
///   - RLIMIT_CORE   (max core dump size)
///   - RLIMIT_DATA   (max data segment size)
///   - RLIMIT_CPU    (max CPU time in seconds)
///   - RLIMIT_MEMLOCK (max locked memory)
///   - RLIMIT_LOCKS  (max file locks)
///   - RLIMIT_SIGPENDING (max pending signals)
///   - RLIMIT_MSGQUEUE (max bytes in POSIX mqueues)
use alloc::collections::BTreeMap;
use spin::Mutex;

use crate::process::Pid;
use crate::serial_println;

/// Special value meaning "no limit"
pub const RLIM_INFINITY: u64 = u64::MAX;

/// Resource limit identifiers (matching Linux values)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Resource {
    /// Max CPU time in seconds
    Cpu = 0,
    /// Max file size (bytes)
    Fsize = 1,
    /// Max data segment size (bytes)
    Data = 2,
    /// Max stack size (bytes)
    Stack = 3,
    /// Max core dump size (bytes)
    Core = 4,
    /// Max resident set size (bytes)
    Rss = 5,
    /// Max number of processes
    Nproc = 6,
    /// Max number of open files
    Nofile = 7,
    /// Max locked memory (bytes)
    Memlock = 8,
    /// Max address space (bytes)
    As = 9,
    /// Max file locks held
    Locks = 10,
    /// Max pending signals
    Sigpending = 11,
    /// Max bytes in POSIX message queues
    Msgqueue = 12,
    /// Max nice priority
    Nice = 13,
    /// Max real-time priority
    Rtprio = 14,
    /// Max real-time timeout (microseconds)
    Rttime = 15,
}

impl Resource {
    pub fn from_u32(val: u32) -> Option<Resource> {
        match val {
            0 => Some(Resource::Cpu),
            1 => Some(Resource::Fsize),
            2 => Some(Resource::Data),
            3 => Some(Resource::Stack),
            4 => Some(Resource::Core),
            5 => Some(Resource::Rss),
            6 => Some(Resource::Nproc),
            7 => Some(Resource::Nofile),
            8 => Some(Resource::Memlock),
            9 => Some(Resource::As),
            10 => Some(Resource::Locks),
            11 => Some(Resource::Sigpending),
            12 => Some(Resource::Msgqueue),
            13 => Some(Resource::Nice),
            14 => Some(Resource::Rtprio),
            15 => Some(Resource::Rttime),
            _ => None,
        }
    }
}

/// A resource limit pair (soft, hard)
#[derive(Debug, Clone, Copy)]
pub struct Rlimit {
    /// Current (soft) limit — enforced limit
    pub rlim_cur: u64,
    /// Maximum (hard) limit — ceiling for soft limit
    pub rlim_max: u64,
}

impl Rlimit {
    pub const fn unlimited() -> Self {
        Rlimit {
            rlim_cur: RLIM_INFINITY,
            rlim_max: RLIM_INFINITY,
        }
    }

    pub const fn new(soft: u64, hard: u64) -> Self {
        Rlimit {
            rlim_cur: soft,
            rlim_max: hard,
        }
    }
}

/// Per-process resource limits
#[derive(Debug, Clone)]
pub struct ProcessLimits {
    pub limits: BTreeMap<u32, Rlimit>,
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessLimits {
    /// Create default limits (matching reasonable Linux defaults)
    pub fn new() -> Self {
        let mut limits = BTreeMap::new();
        // RLIMIT_CPU: unlimited
        limits.insert(Resource::Cpu as u32, Rlimit::unlimited());
        // RLIMIT_FSIZE: unlimited
        limits.insert(Resource::Fsize as u32, Rlimit::unlimited());
        // RLIMIT_DATA: unlimited
        limits.insert(Resource::Data as u32, Rlimit::unlimited());
        // RLIMIT_STACK: 8 MiB (matches Linux default)
        limits.insert(
            Resource::Stack as u32,
            Rlimit::new(8 * 1024 * 1024, RLIM_INFINITY),
        );
        // RLIMIT_CORE: 0 (no core dumps by default)
        limits.insert(Resource::Core as u32, Rlimit::new(0, RLIM_INFINITY));
        // RLIMIT_RSS: unlimited
        limits.insert(Resource::Rss as u32, Rlimit::unlimited());
        // RLIMIT_NPROC: 4096
        limits.insert(Resource::Nproc as u32, Rlimit::new(4096, 4096));
        // RLIMIT_NOFILE: 1024 soft, 4096 hard (matches Linux)
        limits.insert(Resource::Nofile as u32, Rlimit::new(1024, 4096));
        // RLIMIT_MEMLOCK: 64 KiB (matches Linux default)
        limits.insert(Resource::Memlock as u32, Rlimit::new(65536, 65536));
        // RLIMIT_AS: unlimited
        limits.insert(Resource::As as u32, Rlimit::unlimited());
        // RLIMIT_LOCKS: unlimited
        limits.insert(Resource::Locks as u32, Rlimit::unlimited());
        // RLIMIT_SIGPENDING: 4096
        limits.insert(Resource::Sigpending as u32, Rlimit::new(4096, 4096));
        // RLIMIT_MSGQUEUE: 819200 (matches Linux)
        limits.insert(Resource::Msgqueue as u32, Rlimit::new(819200, 819200));
        // RLIMIT_NICE: 0
        limits.insert(Resource::Nice as u32, Rlimit::new(0, 0));
        // RLIMIT_RTPRIO: 0
        limits.insert(Resource::Rtprio as u32, Rlimit::new(0, 0));
        // RLIMIT_RTTIME: unlimited
        limits.insert(Resource::Rttime as u32, Rlimit::unlimited());

        ProcessLimits { limits }
    }

    /// Get a resource limit
    pub fn get(&self, resource: Resource) -> Rlimit {
        self.limits
            .get(&(resource as u32))
            .copied()
            .unwrap_or(Rlimit::unlimited())
    }

    /// Set a resource limit
    pub fn set(&mut self, resource: Resource, limit: Rlimit) -> Result<(), i32> {
        // Hard limit can only be raised by root
        if let Some(current) = self.limits.get(&(resource as u32)) {
            if limit.rlim_max > current.rlim_max {
                let uid = crate::users::get_current_uid();
                if uid != 0 {
                    return Err(-1); // EPERM
                }
            }
        }
        // Soft limit cannot exceed hard limit
        if limit.rlim_cur > limit.rlim_max {
            return Err(-22); // EINVAL
        }
        self.limits.insert(resource as u32, limit);
        Ok(())
    }

    /// Check if a value is within the soft limit
    pub fn check(&self, resource: Resource, value: u64) -> bool {
        let limit = self.get(resource);
        if limit.rlim_cur == RLIM_INFINITY {
            return true;
        }
        value <= limit.rlim_cur
    }
}

/// Global per-process limits table
lazy_static::lazy_static! {
    static ref PROCESS_LIMITS: Mutex<BTreeMap<Pid, ProcessLimits>> =
        Mutex::new(BTreeMap::new());
}

/// Initialize limits for a process (called on process creation)
pub fn init_process(pid: Pid) {
    let mut table = PROCESS_LIMITS.lock();
    table.insert(pid, ProcessLimits::new());
}

/// Remove limits for a process (called on process exit)
pub fn cleanup_process(pid: Pid) {
    let mut table = PROCESS_LIMITS.lock();
    table.remove(&pid);
}

/// Inherit limits from parent (called on fork)
pub fn inherit_limits(parent_pid: Pid, child_pid: Pid) {
    let mut table = PROCESS_LIMITS.lock();
    if let Some(parent_limits) = table.get(&parent_pid).cloned() {
        table.insert(child_pid, parent_limits);
    } else {
        table.insert(child_pid, ProcessLimits::new());
    }
}

/// Get a resource limit for a process
pub fn getrlimit(pid: Pid, resource: u32) -> Result<Rlimit, i32> {
    let res = Resource::from_u32(resource).ok_or(-22i32)?; // EINVAL
    let table = PROCESS_LIMITS.lock();
    let limits = table.get(&pid).ok_or(-3i32)?; // ESRCH
    Ok(limits.get(res))
}

/// Set a resource limit for a process
pub fn setrlimit(pid: Pid, resource: u32, new_limit: Rlimit) -> Result<(), i32> {
    let res = Resource::from_u32(resource).ok_or(-22i32)?; // EINVAL
    let mut table = PROCESS_LIMITS.lock();
    let limits = table.get_mut(&pid).ok_or(-3i32)?; // ESRCH
    limits.set(res, new_limit)
}

/// prlimit64 — get and/or set limits for an arbitrary process
pub fn prlimit64(
    pid: Pid,
    resource: u32,
    new_limit: Option<Rlimit>,
    old_limit: &mut Option<Rlimit>,
) -> Result<(), i32> {
    let res = Resource::from_u32(resource).ok_or(-22i32)?;
    let mut table = PROCESS_LIMITS.lock();
    let limits = table.get_mut(&pid).ok_or(-3i32)?;

    // Get old limit
    *old_limit = Some(limits.get(res));

    // Set new limit if provided
    if let Some(new) = new_limit {
        limits.set(res, new)?;
    }
    Ok(())
}

/// Check if a process can open more file descriptors
pub fn check_nofile(pid: Pid, current_count: u64) -> bool {
    let table = PROCESS_LIMITS.lock();
    if let Some(limits) = table.get(&pid) {
        limits.check(Resource::Nofile, current_count)
    } else {
        true // No limits configured = allowed
    }
}

/// Check if a user can create more processes
pub fn check_nproc(pid: Pid, current_count: u64) -> bool {
    let table = PROCESS_LIMITS.lock();
    if let Some(limits) = table.get(&pid) {
        limits.check(Resource::Nproc, current_count)
    } else {
        true
    }
}

/// Initialize the rlimit subsystem
pub fn init() {
    // Init limits for PID 1 (init/kernel)
    init_process(1);
    serial_println!("[KnoxOS] Resource limits subsystem initialized");
}
