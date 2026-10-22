/// Wait Queue — Blocking I/O infrastructure for sleeping/waking tasks
///
/// Implements Linux-style wait queues for kernel synchronization:
///   - Processes can sleep waiting for a condition
///   - IRQs or other contexts can wake waiters
///   - Supports exclusive and non-exclusive wakeups
///   - Foundation for blocking read/write, socket accept, etc.
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;

use crate::process::Pid;
use crate::serial_println;

/// Wait queue entry mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitMode {
    /// Non-exclusive: all waiters are woken
    NonExclusive,
    /// Exclusive: only one waiter is woken (thundering herd prevention)
    Exclusive,
}

/// A single entry in a wait queue
#[derive(Debug, Clone)]
pub struct WaitQueueEntry {
    pub pid: Pid,
    pub mode: WaitMode,
    /// Whether this entry has been signaled
    pub signaled: bool,
}

/// A wait queue — list of sleeping processes
pub struct WaitQueue {
    entries: Mutex<VecDeque<WaitQueueEntry>>,
}

impl Default for WaitQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl WaitQueue {
    /// Create a new empty wait queue
    pub const fn new() -> Self {
        WaitQueue {
            entries: Mutex::new(VecDeque::new()),
        }
    }

    /// Add current process to wait queue and put it to sleep
    pub fn sleep(&self, mode: WaitMode) {
        let pid = crate::scheduler::current_pid().unwrap_or(1);
        let entry = WaitQueueEntry {
            pid,
            mode,
            signaled: false,
        };

        {
            let mut entries = self.entries.lock();
            entries.push_back(entry);
        }

        // Mark process as sleeping
        crate::scheduler::sleep_current();
    }

    /// Add a specific PID to the wait queue
    pub fn add_waiter(&self, pid: Pid, mode: WaitMode) {
        let entry = WaitQueueEntry {
            pid,
            mode,
            signaled: false,
        };
        let mut entries = self.entries.lock();
        entries.push_back(entry);
    }

    /// Remove a specific PID from the wait queue
    pub fn remove_waiter(&self, pid: Pid) {
        let mut entries = self.entries.lock();
        entries.retain(|e| e.pid != pid);
    }

    /// Wake one waiter (exclusive or first)
    pub fn wake_one(&self) -> Option<Pid> {
        let mut entries = self.entries.lock();

        // Find first exclusive waiter, or first waiter
        if let Some(idx) = entries.iter().position(|_| true) {
            let entry = entries.remove(idx).unwrap();
            crate::scheduler::wake_process(entry.pid);
            return Some(entry.pid);
        }
        None
    }

    /// Wake all non-exclusive waiters + one exclusive waiter
    pub fn wake_all(&self) -> usize {
        let mut entries = self.entries.lock();
        let mut woken = 0;
        let mut exclusive_woken = false;

        let mut remaining = VecDeque::new();

        while let Some(entry) = entries.pop_front() {
            match entry.mode {
                WaitMode::NonExclusive => {
                    crate::scheduler::wake_process(entry.pid);
                    woken += 1;
                }
                WaitMode::Exclusive => {
                    if !exclusive_woken {
                        crate::scheduler::wake_process(entry.pid);
                        woken += 1;
                        exclusive_woken = true;
                    } else {
                        remaining.push_back(entry);
                    }
                }
            }
        }

        *entries = remaining;
        woken
    }

    /// Wake up to N waiters
    pub fn wake_n(&self, n: usize) -> usize {
        let mut entries = self.entries.lock();
        let mut woken = 0;

        while woken < n {
            if let Some(entry) = entries.pop_front() {
                crate::scheduler::wake_process(entry.pid);
                woken += 1;
            } else {
                break;
            }
        }
        woken
    }

    /// Check if there are any waiters
    pub fn has_waiters(&self) -> bool {
        let entries = self.entries.lock();
        !entries.is_empty()
    }

    /// Number of waiters
    pub fn waiter_count(&self) -> usize {
        let entries = self.entries.lock();
        entries.len()
    }

    /// Get list of waiting PIDs
    pub fn waiting_pids(&self) -> Vec<Pid> {
        let entries = self.entries.lock();
        entries.iter().map(|e| e.pid).collect()
    }
}

/// A completion — one-shot event that multiple waiters can wait on
pub struct Completion {
    done: Mutex<bool>,
    waiters: WaitQueue,
}

impl Default for Completion {
    fn default() -> Self {
        Self::new()
    }
}

impl Completion {
    pub const fn new() -> Self {
        Completion {
            done: Mutex::new(false),
            waiters: WaitQueue::new(),
        }
    }

    /// Wait for completion (blocks until complete() is called)
    pub fn wait(&self) {
        {
            let done = self.done.lock();
            if *done {
                return; // Already completed
            }
        }
        self.waiters.sleep(WaitMode::NonExclusive);
    }

    /// Signal completion (wakes all waiters)
    pub fn complete(&self) {
        {
            let mut done = self.done.lock();
            *done = true;
        }
        self.waiters.wake_all();
    }

    /// Signal one waiter
    pub fn complete_one(&self) {
        {
            let mut done = self.done.lock();
            *done = true;
        }
        self.waiters.wake_one();
    }

    /// Reset completion for reuse
    pub fn reset(&self) {
        let mut done = self.done.lock();
        *done = false;
    }

    /// Check if completed without blocking
    pub fn is_done(&self) -> bool {
        *self.done.lock()
    }
}

/// A simple semaphore built on wait queues
pub struct Semaphore {
    count: Mutex<i32>,
    waiters: WaitQueue,
}

impl Semaphore {
    pub const fn new(initial: i32) -> Self {
        Semaphore {
            count: Mutex::new(initial),
            waiters: WaitQueue::new(),
        }
    }

    /// Decrement semaphore (blocks if count <= 0)
    pub fn down(&self) {
        loop {
            {
                let mut count = self.count.lock();
                if *count > 0 {
                    *count -= 1;
                    return;
                }
            }
            self.waiters.sleep(WaitMode::Exclusive);
        }
    }

    /// Try to decrement without blocking
    pub fn try_down(&self) -> bool {
        let mut count = self.count.lock();
        if *count > 0 {
            *count -= 1;
            true
        } else {
            false
        }
    }

    /// Increment semaphore (wakes one waiter)
    pub fn up(&self) {
        {
            let mut count = self.count.lock();
            *count += 1;
        }
        self.waiters.wake_one();
    }
}

/// RW semaphore — allows multiple readers or single writer
pub struct RwSemaphore {
    /// Positive = readers, -1 = writer, 0 = free
    state: Mutex<i32>,
    read_waiters: WaitQueue,
    write_waiters: WaitQueue,
}

impl Default for RwSemaphore {
    fn default() -> Self {
        Self::new()
    }
}

impl RwSemaphore {
    pub const fn new() -> Self {
        RwSemaphore {
            state: Mutex::new(0),
            read_waiters: WaitQueue::new(),
            write_waiters: WaitQueue::new(),
        }
    }

    /// Acquire read lock
    pub fn read_lock(&self) {
        loop {
            {
                let mut state = self.state.lock();
                if *state >= 0 {
                    *state += 1;
                    return;
                }
            }
            self.read_waiters.sleep(WaitMode::NonExclusive);
        }
    }

    /// Release read lock
    pub fn read_unlock(&self) {
        let wake_writer = {
            let mut state = self.state.lock();
            *state -= 1;
            *state == 0
        };
        if wake_writer {
            self.write_waiters.wake_one();
        }
    }

    /// Acquire write lock
    pub fn write_lock(&self) {
        loop {
            {
                let mut state = self.state.lock();
                if *state == 0 {
                    *state = -1;
                    return;
                }
            }
            self.write_waiters.sleep(WaitMode::Exclusive);
        }
    }

    /// Release write lock
    pub fn write_unlock(&self) {
        {
            let mut state = self.state.lock();
            *state = 0;
        }
        // Prefer waking readers over writers
        if self.read_waiters.has_waiters() {
            self.read_waiters.wake_all();
        } else {
            self.write_waiters.wake_one();
        }
    }
}
