use alloc::collections::BTreeMap;
/// Scheduler - Preemptive round-robin scheduler with priority support
/// Provides Linux-compatible process scheduling with context switching
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::process::{PROCESS_TABLE, Pid, ProcessState};
use crate::serial_println;

/// Scheduler tick interval (timer interrupts between reschedules)
const SCHEDULER_TICKS: u64 = 5;

/// Time slice for normal priority (in ticks)
const DEFAULT_TIME_SLICE: u64 = 10;

/// CFS weight table indexed by (nice + 20).
/// Approximates Linux's sched_prio_to_weight[] — higher weight = more CPU.
/// nice -20 → index 0  (highest weight),  nice +19 → index 39 (lowest weight).
const CFS_WEIGHTS: [u64; 40] = [
    88761, 71755, 56483, 46273, 36291, // nice -20..-16
    29154, 23254, 18705, 14949, 11916, // nice -15..-11
    9548, 7620, 6100, 4904, 3906, // nice -10..-6
    3121, 2501, 1991, 1586, 1277, // nice  -5..-1
    1024, 820, 655, 526, 423, // nice   0.. 4
    335, 272, 215, 172, 137, // nice   5.. 9
    110, 87, 70, 56, 45, // nice  10..14
    36, 29, 23, 18, 15, // nice  15..19
];

/// Scheduling policy (Linux-compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SchedPolicy {
    /// Normal (CFS-like round-robin)
    Normal = 0,
    /// First-In First-Out real-time
    Fifo = 1,
    /// Round-Robin real-time
    RoundRobin = 2,
    /// Batch (CPU-intensive, low-priority)
    Batch = 3,
    /// Idle (only runs when nothing else wants to)
    Idle = 5,
    /// Deadline (EDF — Earliest Deadline First)
    Deadline = 6,
}

/// SCHED_DEADLINE parameters for a process
#[derive(Debug, Clone)]
pub struct DeadlineParams {
    /// Runtime budget per period (in ticks)
    pub runtime: u64,
    /// Period length (in ticks)
    pub period: u64,
    /// Absolute deadline relative to period start (in ticks)
    pub deadline: u64,
    /// Remaining runtime budget in current period
    pub remaining_runtime: u64,
    /// Absolute deadline of current job (tick at which job must complete)
    pub absolute_deadline: u64,
    /// Start of current period
    pub period_start: u64,
    /// Whether this task has overrun its budget
    pub throttled: bool,
}

/// Per-process scheduling info
#[derive(Debug, Clone)]
pub struct SchedInfo {
    pub pid: Pid,
    pub policy: SchedPolicy,
    pub priority: i32,      // Nice value: -20 (high) to 19 (low)
    pub time_slice: u64,    // Remaining ticks in this quantum
    pub total_runtime: u64, // Total ticks consumed
    pub vruntime: u64,      // Virtual runtime (for CFS-like fairness)
    pub cpu_affinity: u64,  // CPU affinity mask (bitmask of allowed CPUs)
    pub deadline_params: Option<DeadlineParams>, // SCHED_DEADLINE params
}

impl SchedInfo {
    pub fn new(pid: Pid, priority: i32) -> Self {
        Self {
            pid,
            policy: SchedPolicy::Normal,
            priority,
            time_slice: DEFAULT_TIME_SLICE,
            total_runtime: 0,
            vruntime: 0,
            cpu_affinity: u64::MAX, // all CPUs allowed
            deadline_params: None,
        }
    }

    /// Calculate effective time slice based on priority
    pub fn effective_time_slice(&self) -> u64 {
        // Higher priority (lower nice) = longer time slice
        let base = DEFAULT_TIME_SLICE as i64;
        let adjusted = base - self.priority as i64;
        adjusted.max(2) as u64
    }

    /// CFS weight derived from nice value.
    /// Linux CFS uses a piecewise table; we approximate with an exponential.
    /// Nice 0 → weight 1024, nice -20 → ~88761, nice +19 → ~15.
    pub fn weight(&self) -> u64 {
        // weight = 1024 * 1.25^(-nice)
        // Precomputed for the nice range [-20, 19] via integer approximation.
        let idx = (self.priority + 20).clamp(0, 39) as usize;
        CFS_WEIGHTS[idx]
    }
}

/// The kernel scheduler
pub struct Scheduler {
    /// Run queue - processes ready to execute
    run_queue: VecDeque<SchedInfo>,
    /// Currently running process
    current: Option<SchedInfo>,
    /// Wait queue - processes waiting for events
    wait_queue: Vec<SchedInfo>,
    /// Idle process PID
    idle_pid: Pid,
    /// Whether the scheduler is enabled
    enabled: bool,
    /// Tick counter for scheduling decisions
    tick_count: u64,
    /// Total context switches
    context_switches: u64,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            run_queue: VecDeque::new(),
            current: None,
            wait_queue: Vec::new(),
            idle_pid: 0,
            enabled: false,
            tick_count: 0,
            context_switches: 0,
        }
    }

    /// Add a process to the run queue
    pub fn add_process(&mut self, pid: Pid, priority: i32) {
        let mut info = SchedInfo::new(pid, priority);
        // CFS: set initial vruntime to the current minimum so new processes
        // don't starve existing ones (and don't get unfair CPU bursts).
        let min_vr = self
            .run_queue
            .iter()
            .map(|s| s.vruntime)
            .chain(self.current.iter().map(|s| s.vruntime))
            .min()
            .unwrap_or(0);
        info.vruntime = min_vr;
        self.run_queue.push_back(info);
    }

    /// Remove a process from all queues
    pub fn remove_process(&mut self, pid: Pid) {
        self.run_queue.retain(|s| s.pid != pid);
        self.wait_queue.retain(|s| s.pid != pid);
        if let Some(ref current) = self.current {
            if current.pid == pid {
                self.current = None;
            }
        }
    }

    /// Move a process to the wait queue (sleeping/blocked)
    pub fn block_process(&mut self, pid: Pid) {
        // Remove from run queue
        if let Some(pos) = self.run_queue.iter().position(|s| s.pid == pid) {
            let info = self.run_queue.remove(pos).unwrap();
            self.wait_queue.push(info);
        }
        // If it's the current process, force reschedule
        if let Some(ref current) = self.current {
            if current.pid == pid {
                let mut info = self.current.take().unwrap();
                info.time_slice = info.effective_time_slice(); // Reset time slice
                self.wait_queue.push(info);
            }
        }
    }

    /// Wake a process (move from wait queue to run queue)
    pub fn wake_process(&mut self, pid: Pid) {
        if let Some(pos) = self.wait_queue.iter().position(|s| s.pid == pid) {
            let info = self.wait_queue.remove(pos);
            self.run_queue.push_back(info);
        }
    }

    /// Set process priority (nice value)
    pub fn set_priority(&mut self, pid: Pid, priority: i32) {
        let priority = priority.clamp(-20, 19);
        for info in self.run_queue.iter_mut() {
            if info.pid == pid {
                info.priority = priority;
                return;
            }
        }
        if let Some(ref mut current) = self.current {
            if current.pid == pid {
                current.priority = priority;
            }
        }
    }

    /// Get the currently running process PID
    pub fn current_pid(&self) -> Option<Pid> {
        self.current.as_ref().map(|s| s.pid)
    }

    /// Timer tick - called from the timer interrupt handler
    /// Returns true if a context switch should occur
    pub fn tick(&mut self) -> bool {
        if !self.enabled {
            return false;
        }

        self.tick_count += 1;

        if let Some(ref mut current) = self.current {
            current.total_runtime += 1;
            // CFS: vruntime grows inversely proportional to weight.
            // delta_vruntime = delta_exec * (NICE_0_WEIGHT / weight)
            // We scale by 1024 (nice-0 weight) to keep integer arithmetic.
            let w = current.weight().max(1);
            current.vruntime += 1024 / w; // lower weight → faster vruntime growth

            if current.time_slice > 0 {
                current.time_slice -= 1;
            }

            // Time slice expired - need to reschedule
            if current.time_slice == 0 {
                return true;
            }

            // CFS preemption: if a process in the run queue has a much smaller
            // vruntime, preempt early to maintain fairness.
            if let Some(min_vr) = self.run_queue.iter().map(|s| s.vruntime).min() {
                if current.vruntime > min_vr + 4 {
                    return true; // Preempt: current has run ahead by >4 vruntime units
                }
            }
        } else {
            // No current process - need to schedule one
            return !self.run_queue.is_empty();
        }

        false
    }

    /// Pick the next process to run (CFS: smallest vruntime wins)
    /// Priority order: SCHED_DEADLINE (EDF) > SCHED_FIFO/RR > CFS Normal/Batch > Idle
    pub fn schedule(&mut self) -> Option<Pid> {
        // Put current process back in the run queue
        if let Some(mut current) = self.current.take() {
            current.time_slice = current.effective_time_slice();
            self.run_queue.push_back(current);
        }

        if self.run_queue.is_empty() {
            return Some(self.idle_pid);
        }

        // 1. SCHED_DEADLINE: pick earliest absolute deadline (EDF) that isn't throttled
        let dl_idx = self
            .run_queue
            .iter()
            .enumerate()
            .filter(|(_, s)| s.policy == SchedPolicy::Deadline)
            .filter(|(_, s)| s.deadline_params.as_ref().is_some_and(|d| !d.throttled))
            .min_by_key(|(_, s)| {
                s.deadline_params
                    .as_ref()
                    .map_or(u64::MAX, |d| d.absolute_deadline)
            })
            .map(|(i, _)| i);

        if let Some(idx) = dl_idx {
            if let Some(mut next) = self.run_queue.remove(idx) {
                next.time_slice = next
                    .deadline_params
                    .as_ref()
                    .map_or(next.effective_time_slice(), |d| d.remaining_runtime.max(1));
                let pid = next.pid;
                self.current = Some(next);
                self.context_switches += 1;
                return Some(pid);
            }
        }

        // 2. SCHED_FIFO / SCHED_RR: highest static priority wins
        let rt_idx = self
            .run_queue
            .iter()
            .enumerate()
            .filter(|(_, s)| s.policy == SchedPolicy::Fifo || s.policy == SchedPolicy::RoundRobin)
            .min_by_key(|(_, s)| s.priority) // lower nice = higher priority
            .map(|(i, _)| i);

        if let Some(idx) = rt_idx {
            if let Some(mut next) = self.run_queue.remove(idx) {
                next.time_slice = next.effective_time_slice();
                let pid = next.pid;
                self.current = Some(next);
                self.context_switches += 1;
                return Some(pid);
            }
        }

        // 3. CFS: pick the process with the smallest vruntime (skip Idle if others exist)
        let non_idle_count = self
            .run_queue
            .iter()
            .filter(|s| s.policy != SchedPolicy::Idle)
            .count();
        let min_idx = if non_idle_count > 0 {
            self.run_queue
                .iter()
                .enumerate()
                .filter(|(_, s)| s.policy != SchedPolicy::Idle)
                .min_by_key(|(_, s)| s.vruntime)
                .map(|(i, _)| i)
        } else {
            self.run_queue
                .iter()
                .enumerate()
                .min_by_key(|(_, s)| s.vruntime)
                .map(|(i, _)| i)
        };

        let min_idx = min_idx.unwrap();

        if let Some(mut next) = self.run_queue.remove(min_idx) {
            next.time_slice = next.effective_time_slice();
            let pid = next.pid;
            self.current = Some(next);
            self.context_switches += 1;
            Some(pid)
        } else {
            Some(self.idle_pid)
        }
    }

    /// Enable the scheduler
    pub fn enable(&mut self) {
        self.enabled = true;
        serial_println!("[KnoxOS] Scheduler enabled");
    }

    /// Get scheduler statistics
    pub fn stats(&self) -> SchedStats {
        SchedStats {
            run_queue_len: self.run_queue.len(),
            wait_queue_len: self.wait_queue.len(),
            context_switches: self.context_switches,
            current_pid: self.current.as_ref().map(|s| s.pid),
            tick_count: self.tick_count,
        }
    }
}

/// Scheduler statistics
#[derive(Debug, Clone)]
pub struct SchedStats {
    pub run_queue_len: usize,
    pub wait_queue_len: usize,
    pub context_switches: u64,
    pub current_pid: Option<Pid>,
    pub tick_count: u64,
}

lazy_static::lazy_static! {
    pub static ref SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());
}

/// Atomic reschedule request flag — set by timer ISR, consumed by deferred handler.
/// This avoids locking the SCHEDULER mutex from interrupt context.
static NEED_RESCHED: AtomicBool = AtomicBool::new(false);

/// Atomic tick counter for the scheduler (updated locklessly from ISR)
static ISR_TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Remaining time slice for current process (decremented atomically from ISR)
static CURRENT_TIME_SLICE: AtomicU64 = AtomicU64::new(DEFAULT_TIME_SLICE);

/// Whether preemptive scheduling is enabled
static PREEMPTION_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable preemptive scheduling
pub fn enable_preemption() {
    PREEMPTION_ENABLED.store(true, Ordering::Release);
    serial_println!("[KnoxOS] Preemptive scheduling enabled");
}

/// Disable preemption temporarily (e.g., during critical sections)
pub fn disable_preemption() {
    PREEMPTION_ENABLED.store(false, Ordering::Release);
}

/// Check if preemption is enabled
pub fn is_preemption_enabled() -> bool {
    PREEMPTION_ENABLED.load(Ordering::Acquire)
}

/// Called from the timer interrupt handler — LOCK-FREE, interrupt-safe.
/// Decrements the current time slice and sets NEED_RESCHED when expired.
pub fn isr_timer_tick() {
    if !PREEMPTION_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    ISR_TICK_COUNT.fetch_add(1, Ordering::Relaxed);

    // Decrement time slice atomically
    let remaining = CURRENT_TIME_SLICE.load(Ordering::Relaxed);
    if remaining > 0 {
        CURRENT_TIME_SLICE.store(remaining - 1, Ordering::Relaxed);
    }
    if remaining <= 1 {
        // Time slice expired — request reschedule
        NEED_RESCHED.store(true, Ordering::Release);
    }
}

/// Check and clear the need_resched flag (called from deferred context after ISR).
/// Returns true if a context switch should be performed.
pub fn check_need_resched() -> bool {
    NEED_RESCHED.swap(false, Ordering::AcqRel)
}

/// Perform a deferred context switch — called with interrupts enabled,
/// after the timer ISR has returned. This is safe to lock the scheduler.
pub fn deferred_schedule() {
    if !is_preemption_enabled() {
        return;
    }
    if !NEED_RESCHED.swap(false, Ordering::AcqRel) {
        return;
    }
    CURRENT_TIME_SLICE.store(DEFAULT_TIME_SLICE, Ordering::Relaxed);

    let current = crate::context::current_pid();
    crate::signals::deliver_signals(current);

    let next = SCHEDULER.lock().schedule();
    if let Some(next_pid) = next {
        if next_pid != current && crate::context::has_runnable_context(next_pid) {
            unsafe {
                crate::context::switch_to(next_pid);
            }
        }
    }
}

/// Initialize the scheduler
/// Yield the current CPU time slice to other tasks
pub fn yield_now() {
    // In a cooperative scheduler, this would mark current as ready and switch
    // For now, just do a brief pause
    crate::arch_compat::instructions::interrupts::hlt();
}

pub fn init() {
    let mut sched = SCHEDULER.lock();
    sched.idle_pid = 0;
    // Desktop executor is the only runnable kernel "process" at boot.
    // Init (PID 1) has no RIP until userspace exists; idle is entered via
    // yield_to_idle() rather than the CFS run queue.
    sched.add_process(2, 0);
    serial_println!("[KnoxOS] Scheduler initialized (CFS fair scheduler + SMP-aware)");
}

/// Called from timer interrupt - check if we need to reschedule
pub fn timer_tick() -> bool {
    // NOTE: This function may be called from interrupt context.
    // Moved lock-heavy work (signal delivery, zombie reaping, load balancing)
    // to deferred_schedule() which runs from the main event loop.
    // Here we only do lock-free operations.
    isr_timer_tick();
    false
}

/// Get current running PID
pub fn current_pid() -> Option<Pid> {
    SCHEDULER.lock().current_pid()
}

/// Wake a specific process
pub fn wake_process(pid: Pid) {
    SCHEDULER.lock().wake_process(pid);
}

/// Block (sleep) the current process
pub fn sleep_current() {
    if let Some(pid) = current_pid() {
        SCHEDULER.lock().block_process(pid);
    }
}

/// Yield the current time slice
pub fn yield_current() {
    // Force a reschedule on next tick
    let mut sched = SCHEDULER.lock();
    sched.tick_count = SCHEDULER_TICKS;
}

/// Set scheduling policy for a process
pub fn set_policy(pid: Pid, policy: SchedPolicy) {
    let mut sched = SCHEDULER.lock();
    for entry in sched.run_queue.iter_mut() {
        if entry.pid == pid {
            entry.policy = policy;
            return;
        }
    }
}

/// Set priority for a process
pub fn set_priority(pid: Pid, priority: i32) {
    SCHEDULER.lock().set_priority(pid, priority);
}

/// Add a process to the scheduler
pub fn add_process(pid: Pid, priority: i32) {
    SCHEDULER.lock().add_process(pid, priority);
    // Also enqueue on SMP per-CPU run queues for multi-core execution
    crate::smp::enqueue_balanced(pid);
}

/// Remove a process from the scheduler
pub fn remove_process(pid: Pid) {
    SCHEDULER.lock().remove_process(pid);
}

/// Check if a process exists in the scheduler (any state)
pub fn process_exists(pid: Pid) -> bool {
    let sched = SCHEDULER.lock();
    // Check run queue
    if sched.run_queue.iter().any(|e| e.pid == pid) {
        return true;
    }
    // Check wait queue (blocked/sleeping)
    if sched.wait_queue.iter().any(|e| e.pid == pid) {
        return true;
    }
    // Check if it's the currently running process
    if sched.current.as_ref().map(|c| c.pid) == Some(pid) {
        return true;
    }
    false
}

// ═══════════════════════════════════════════════════════════════════════
// SCHED_DEADLINE — Earliest Deadline First (EDF)
// ═══════════════════════════════════════════════════════════════════════

/// Configure SCHED_DEADLINE parameters for a process.
/// `runtime`, `period`, `deadline` are in ticks (1 tick ≈ 1ms at 1kHz).
/// Admission control: sum of (runtime/period) across all deadline tasks must be ≤ 1.0
pub fn set_deadline(
    pid: Pid,
    runtime: u64,
    period: u64,
    deadline: u64,
) -> Result<(), &'static str> {
    if runtime == 0 || period == 0 || deadline == 0 {
        return Err("Deadline parameters must be non-zero");
    }
    if runtime > deadline || deadline > period {
        return Err("Must satisfy: runtime <= deadline <= period");
    }

    let mut sched = SCHEDULER.lock();
    let tick = sched.tick_count;

    // Admission control: check total utilization
    let mut total_util_num: u64 = 0;
    let mut total_util_den: u64 = 1;
    for entry in sched.run_queue.iter().chain(sched.current.iter()) {
        if entry.policy == SchedPolicy::Deadline && entry.pid != pid {
            if let Some(ref dl) = entry.deadline_params {
                // Accumulate runtime/period using cross multiplication
                total_util_num = total_util_num * dl.period + dl.runtime * total_util_den;
                total_util_den *= dl.period;
            }
        }
    }
    // Add the new task's utilization
    total_util_num = total_util_num * period + runtime * total_util_den;
    total_util_den *= period;
    // Check if total utilization exceeds 1.0 (num/den > 1)
    if total_util_num > total_util_den {
        return Err("Admission control: total deadline utilization exceeds 100%");
    }

    let dl_params = DeadlineParams {
        runtime,
        period,
        deadline,
        remaining_runtime: runtime,
        absolute_deadline: tick + deadline,
        period_start: tick,
        throttled: false,
    };

    // Find the process and configure it
    let find_and_set = |entries: &mut VecDeque<SchedInfo>| -> bool {
        for entry in entries.iter_mut() {
            if entry.pid == pid {
                entry.policy = SchedPolicy::Deadline;
                entry.deadline_params = Some(dl_params.clone());
                return true;
            }
        }
        false
    };

    if !find_and_set(&mut sched.run_queue) {
        if let Some(ref mut current) = sched.current {
            if current.pid == pid {
                current.policy = SchedPolicy::Deadline;
                current.deadline_params = Some(dl_params);
                return Ok(());
            }
        }
        return Err("Process not found in scheduler");
    }

    Ok(())
}

/// Update deadline task accounting — call from deferred_schedule
pub fn update_deadline_tasks() {
    let mut sched = SCHEDULER.lock();
    let tick = sched.tick_count;

    let all_entries: Vec<usize> = (0..sched.run_queue.len()).collect();
    for i in all_entries {
        if sched.run_queue[i].policy != SchedPolicy::Deadline {
            continue;
        }
        if let Some(ref mut dl) = sched.run_queue[i].deadline_params {
            if tick >= dl.period_start + dl.period {
                dl.period_start = tick;
                dl.remaining_runtime = dl.runtime;
                dl.absolute_deadline = tick + dl.deadline;
                dl.throttled = false;
            }
        }
    }
    if let Some(ref mut current) = sched.current {
        if current.policy == SchedPolicy::Deadline {
            if let Some(ref mut dl) = current.deadline_params {
                if tick >= dl.period_start + dl.period {
                    dl.period_start = tick;
                    dl.remaining_runtime = dl.runtime;
                    dl.absolute_deadline = tick + dl.deadline;
                    dl.throttled = false;
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CPU AFFINITY
// ═══════════════════════════════════════════════════════════════════════

/// Set CPU affinity mask for a process (persists across exec)
pub fn set_cpu_affinity(pid: Pid, mask: u64) -> Result<(), &'static str> {
    if mask == 0 {
        return Err("CPU affinity mask must have at least one bit set");
    }
    let mut sched = SCHEDULER.lock();
    for entry in sched.run_queue.iter_mut() {
        if entry.pid == pid {
            entry.cpu_affinity = mask;
            return Ok(());
        }
    }
    if let Some(ref mut current) = sched.current {
        if current.pid == pid {
            current.cpu_affinity = mask;
            return Ok(());
        }
    }
    for entry in sched.wait_queue.iter_mut() {
        if entry.pid == pid {
            entry.cpu_affinity = mask;
            return Ok(());
        }
    }
    Err("Process not found")
}

/// Get CPU affinity mask for a process
pub fn get_cpu_affinity(pid: Pid) -> Option<u64> {
    let sched = SCHEDULER.lock();
    for entry in sched.run_queue.iter() {
        if entry.pid == pid {
            return Some(entry.cpu_affinity);
        }
    }
    if let Some(ref current) = sched.current {
        if current.pid == pid {
            return Some(current.cpu_affinity);
        }
    }
    for entry in sched.wait_queue.iter() {
        if entry.pid == pid {
            return Some(entry.cpu_affinity);
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════
// PROCESS ACCOUNTING
// ═══════════════════════════════════════════════════════════════════════

/// Per-process accounting statistics
#[derive(Debug, Clone, Default)]
pub struct ProcessAccounting {
    pub pid: Pid,
    pub user_time: u64,            // Ticks in user mode
    pub system_time: u64,          // Ticks in kernel mode
    pub voluntary_switches: u64,   // Voluntary context switches (sleep/wait)
    pub involuntary_switches: u64, // Involuntary (preempted by scheduler)
    pub peak_memory_kb: u64,       // Peak RSS in KB
    pub io_read_bytes: u64,        // Bytes read from disk
    pub io_write_bytes: u64,       // Bytes written to disk
    pub start_time: u64,           // Tick when process started
}

lazy_static::lazy_static! {
    static ref PROCESS_ACCOUNTING: Mutex<alloc::collections::BTreeMap<Pid, ProcessAccounting>> =
        Mutex::new(alloc::collections::BTreeMap::new());
}

/// Start accounting for a process
pub fn start_accounting(pid: Pid) {
    let mut acct = PROCESS_ACCOUNTING.lock();
    let tick = ISR_TICK_COUNT.load(Ordering::Relaxed);
    acct.insert(
        pid,
        ProcessAccounting {
            pid,
            start_time: tick,
            ..ProcessAccounting::default()
        },
    );
}

/// Record a voluntary context switch (process called sleep/wait)
pub fn record_voluntary_switch(pid: Pid) {
    if let Some(entry) = PROCESS_ACCOUNTING.lock().get_mut(&pid) {
        entry.voluntary_switches += 1;
    }
}

/// Record an involuntary context switch (preempted by scheduler)
pub fn record_involuntary_switch(pid: Pid) {
    if let Some(entry) = PROCESS_ACCOUNTING.lock().get_mut(&pid) {
        entry.involuntary_switches += 1;
    }
}

/// Record I/O bytes
pub fn record_io(pid: Pid, read_bytes: u64, write_bytes: u64) {
    if let Some(entry) = PROCESS_ACCOUNTING.lock().get_mut(&pid) {
        entry.io_read_bytes += read_bytes;
        entry.io_write_bytes += write_bytes;
    }
}

/// Get accounting stats for a process
pub fn get_accounting(pid: Pid) -> Option<ProcessAccounting> {
    PROCESS_ACCOUNTING.lock().get(&pid).cloned()
}

/// Remove accounting when process exits
pub fn remove_accounting(pid: Pid) -> Option<ProcessAccounting> {
    PROCESS_ACCOUNTING.lock().remove(&pid)
}
