/// Thread Support - POSIX-compatible threading primitives
/// Implements pthread-compatible threads, TLS, and futex synchronization
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::process::Pid;
use crate::serial_println;

/// Thread identifier
pub type Tid = u32;

/// Thread state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Running,
    Ready,
    Sleeping,
    Blocked,
    Terminated,
    Detached,
}

/// A thread within a process
#[derive(Debug, Clone)]
pub struct Thread {
    /// Thread ID (unique system-wide, like Linux)
    pub tid: Tid,
    /// Process ID this thread belongs to
    pub pid: Pid,
    /// Thread state
    pub state: ThreadState,
    /// Thread name (for debugging)
    pub name: String,
    /// Entry point function address
    pub entry_point: u64,
    /// Argument to entry function
    pub arg: u64,
    /// Stack pointer
    pub stack_ptr: u64,
    /// Stack size
    pub stack_size: usize,
    /// Thread-local storage base address
    pub tls_base: u64,
    /// Exit status (set when thread terminates)
    pub exit_status: Option<u64>,
    /// Detached flag (if true, auto-cleanup on exit)
    pub detached: bool,
    /// CPU affinity mask
    pub cpu_affinity: u64,
    /// Priority (nice value)
    pub priority: i32,
    /// Total CPU time used (ticks)
    pub cpu_time: u64,
}

impl Thread {
    pub fn new(tid: Tid, pid: Pid, entry_point: u64, arg: u64, stack_size: usize) -> Self {
        Self {
            tid,
            pid,
            state: ThreadState::Ready,
            name: alloc::format!("thread-{}", tid),
            entry_point,
            arg,
            stack_ptr: 0,
            stack_size,
            tls_base: 0,
            exit_status: None,
            detached: false,
            cpu_affinity: u64::MAX, // All CPUs
            priority: 0,
            cpu_time: 0,
        }
    }
}

/// Per-process thread group
pub struct ThreadGroup {
    pub pid: Pid,
    pub threads: Vec<Thread>,
    pub main_tid: Tid,
}

impl ThreadGroup {
    pub fn new(pid: Pid, main_tid: Tid) -> Self {
        Self {
            pid,
            threads: Vec::new(),
            main_tid,
        }
    }
}

static NEXT_TID: AtomicU32 = AtomicU32::new(100);

lazy_static::lazy_static! {
    /// Global thread table
    pub static ref THREAD_GROUPS: Mutex<BTreeMap<Pid, ThreadGroup>> = Mutex::new(BTreeMap::new());
}

/// Create a new thread (pthread_create equivalent)
pub fn thread_create(pid: Pid, entry_point: u64, arg: u64, stack_size: usize) -> Result<Tid, i32> {
    let tid = NEXT_TID.fetch_add(1, Ordering::Relaxed);

    let thread = Thread::new(tid, pid, entry_point, arg, stack_size);

    let mut groups = THREAD_GROUPS.lock();
    let group = groups
        .entry(pid)
        .or_insert_with(|| ThreadGroup::new(pid, tid));
    group.threads.push(thread);

    // Register with scheduler
    crate::scheduler::SCHEDULER.lock().add_process(tid, 0);

    serial_println!(
        "[KnoxOS] thread_create: PID {} -> TID {} (entry={:#x})",
        pid,
        tid,
        entry_point
    );
    Ok(tid)
}

/// Wait for a thread to finish (pthread_join equivalent)
pub fn thread_join(tid: Tid) -> Result<u64, i32> {
    let groups = THREAD_GROUPS.lock();
    for (_, group) in groups.iter() {
        if let Some(thread) = group.threads.iter().find(|t| t.tid == tid) {
            if thread.detached {
                return Err(-22); // EINVAL - can't join detached thread
            }
            if let Some(status) = thread.exit_status {
                return Ok(status);
            }
            // Would block here in real implementation
            return Err(-11); // EAGAIN
        }
    }
    Err(-3) // ESRCH
}

/// Terminate the calling thread (pthread_exit equivalent)
pub fn thread_exit(tid: Tid, status: u64) {
    let mut groups = THREAD_GROUPS.lock();
    for (_, group) in groups.iter_mut() {
        if let Some(thread) = group.threads.iter_mut().find(|t| t.tid == tid) {
            thread.state = ThreadState::Terminated;
            thread.exit_status = Some(status);

            if thread.detached {
                // Auto-cleanup
                group.threads.retain(|t| t.tid != tid);
            }

            serial_println!("[KnoxOS] thread_exit: TID {} status={}", tid, status);
            break;
        }
    }

    // Remove from scheduler
    crate::scheduler::SCHEDULER.lock().remove_process(tid);
}

/// Detach a thread (pthread_detach equivalent)
pub fn thread_detach(tid: Tid) -> Result<(), i32> {
    let mut groups = THREAD_GROUPS.lock();
    for (_, group) in groups.iter_mut() {
        if let Some(thread) = group.threads.iter_mut().find(|t| t.tid == tid) {
            thread.detached = true;
            thread.state = ThreadState::Detached;
            return Ok(());
        }
    }
    Err(-3) // ESRCH
}

/// Set thread name
pub fn thread_set_name(tid: Tid, name: &str) -> Result<(), i32> {
    let mut groups = THREAD_GROUPS.lock();
    for (_, group) in groups.iter_mut() {
        if let Some(thread) = group.threads.iter_mut().find(|t| t.tid == tid) {
            thread.name = String::from(name);
            return Ok(());
        }
    }
    Err(-3) // ESRCH
}

/// Get thread count for a process
pub fn thread_count(pid: Pid) -> usize {
    THREAD_GROUPS
        .lock()
        .get(&pid)
        .map(|g| g.threads.len())
        .unwrap_or(1) // Main thread always exists
}

// ═══════════════════════════════════════════════════════════════════════
// FUTEX (Fast Userspace Mutex)
// ═══════════════════════════════════════════════════════════════════════

/// Futex operation codes
pub const FUTEX_WAIT: u32 = 0;
pub const FUTEX_WAKE: u32 = 1;
pub const FUTEX_WAIT_PRIVATE: u32 = 128;
pub const FUTEX_WAKE_PRIVATE: u32 = 129;

/// A futex waiter
#[derive(Debug, Clone)]
pub struct FutexWaiter {
    pub pid: Pid,
    pub tid: Tid,
    pub addr: u64,
}

lazy_static::lazy_static! {
    /// Global futex wait queue: address -> list of waiters
    pub static ref FUTEX_WAITERS: Mutex<BTreeMap<u64, Vec<FutexWaiter>>> =
        Mutex::new(BTreeMap::new());
}

/// Futex wait - block if *addr == expected_val
pub fn futex_wait(addr: u64, expected_val: u32) -> Result<(), i32> {
    let pid = crate::scheduler::current_pid().unwrap_or(0);

    // Check current value
    let current = unsafe { *(addr as *const u32) };
    if current != expected_val {
        return Err(-11); // EAGAIN - value changed
    }

    // Add to wait queue
    let mut waiters = FUTEX_WAITERS.lock();
    let queue = waiters.entry(addr).or_default();
    queue.push(FutexWaiter {
        pid,
        tid: pid,
        addr,
    });

    // Block the process
    drop(waiters);
    crate::scheduler::SCHEDULER.lock().block_process(pid);

    Ok(())
}

/// Futex wake - wake up to `count` waiters on addr
pub fn futex_wake(addr: u64, count: u32) -> Result<u32, i32> {
    let mut waiters = FUTEX_WAITERS.lock();
    let mut woken = 0u32;

    if let Some(queue) = waiters.get_mut(&addr) {
        let to_wake = (count as usize).min(queue.len());
        for _ in 0..to_wake {
            if let Some(waiter) = queue.pop() {
                crate::scheduler::SCHEDULER.lock().wake_process(waiter.pid);
                woken += 1;
            }
        }
        if queue.is_empty() {
            waiters.remove(&addr);
        }
    }

    Ok(woken)
}

// ═══════════════════════════════════════════════════════════════════════
// THREAD-LOCAL STORAGE (TLS)
// ═══════════════════════════════════════════════════════════════════════

/// TLS key type
pub type TlsKey = u32;

static NEXT_TLS_KEY: AtomicU32 = AtomicU32::new(1);

lazy_static::lazy_static! {
    /// TLS data: (tid, key) -> value
    pub static ref TLS_DATA: Mutex<BTreeMap<(Tid, TlsKey), u64>> = Mutex::new(BTreeMap::new());

    /// TLS destructors: key -> destructor function address
    pub static ref TLS_DESTRUCTORS: Mutex<BTreeMap<TlsKey, u64>> = Mutex::new(BTreeMap::new());
}

/// Create a TLS key (pthread_key_create)
pub fn tls_key_create(destructor: u64) -> Result<TlsKey, i32> {
    let key = NEXT_TLS_KEY.fetch_add(1, Ordering::Relaxed);
    if destructor != 0 {
        TLS_DESTRUCTORS.lock().insert(key, destructor);
    }
    Ok(key)
}

/// Delete a TLS key (pthread_key_delete)
pub fn tls_key_delete(key: TlsKey) -> Result<(), i32> {
    TLS_DESTRUCTORS.lock().remove(&key);
    // Remove all data for this key
    TLS_DATA.lock().retain(|(_, k), _| *k != key);
    Ok(())
}

/// Set TLS value (pthread_setspecific)
pub fn tls_set(tid: Tid, key: TlsKey, value: u64) -> Result<(), i32> {
    TLS_DATA.lock().insert((tid, key), value);
    Ok(())
}

/// Get TLS value (pthread_getspecific)
pub fn tls_get(tid: Tid, key: TlsKey) -> u64 {
    TLS_DATA.lock().get(&(tid, key)).copied().unwrap_or(0)
}

/// Clean up TLS for a terminated thread
pub fn tls_cleanup_thread(tid: Tid) {
    TLS_DATA.lock().retain(|(t, _), _| *t != tid);
}

/// Clean up all threads for a process
pub fn cleanup_process_threads(pid: Pid) {
    let mut groups = THREAD_GROUPS.lock();
    if let Some(group) = groups.remove(&pid) {
        for thread in &group.threads {
            crate::scheduler::SCHEDULER
                .lock()
                .remove_process(thread.tid);
            tls_cleanup_thread(thread.tid);
        }
    }
}

/// Initialize threading subsystem
/// Create a new thread (wrapper for thread_create)
/// entry_point: function pointer, arg: argument to function, stack_hint: stack size or tid hint
pub fn create_thread(entry_point: u64, arg: u64, stack_hint: u64) -> Result<Tid, i32> {
    thread_create(0, entry_point, arg, stack_hint as usize)
}

/// Join a thread (wrapper for thread_join, accepts u64 for PthreadT compat)
pub fn join_thread(tid: u64) -> Result<u64, i32> {
    thread_join(tid as Tid)
}

pub fn init() {
    // Create main thread group for init processes
    let mut groups = THREAD_GROUPS.lock();
    let mut init_group = ThreadGroup::new(1, 1);
    init_group.threads.push(Thread::new(1, 1, 0, 0, 0));
    groups.insert(1, init_group);

    let mut desktop_group = ThreadGroup::new(2, 2);
    desktop_group.threads.push(Thread::new(2, 2, 0, 0, 0));
    groups.insert(2, desktop_group);

    serial_println!("[KnoxOS] Thread support initialized (POSIX threads)");
    serial_println!("[KnoxOS] Futex synchronization initialized");
    serial_println!("[KnoxOS] Thread-local storage initialized");
}
