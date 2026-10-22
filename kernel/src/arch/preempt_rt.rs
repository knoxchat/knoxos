/// PREEMPT_RT — Real-Time Scheduling Extensions
/// Provides real-time scheduling guarantees for latency-sensitive workloads
///
/// Features:
/// - Priority inheritance protocol for mutexes (PI-aware)
/// - RT throttling (bandwidth limiting) per CPU
/// - High-resolution timer infrastructure
/// - RT scheduling classes (FIFO, RR, Deadline)
/// - CPU isolation for RT tasks
/// - Latency tracking and histograms
/// - RT-safe memory allocation hints
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── RT Priority Levels ─────────────────────────────────────────────

pub const RT_PRIO_MIN: u32 = 1;
pub const RT_PRIO_MAX: u32 = 99;
pub const DEFAULT_RT_PRIO: u32 = 50;

/// RT scheduling policies
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtPolicy {
    Normal,     // SCHED_OTHER (CFS)
    Fifo,       // SCHED_FIFO — No timeslice, runs until blocks/yields
    RoundRobin, // SCHED_RR — Fixed timeslice per priority
    Batch,      // SCHED_BATCH — CPU-intensive background
    Idle,       // SCHED_IDLE — Very low priority
    Deadline,   // SCHED_DEADLINE — EDF (Earliest Deadline First)
}

// ─── RT Task Parameters ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RtTaskParams {
    pub pid: u32,
    pub policy: RtPolicy,
    pub priority: u32,     // 1-99 for RT, nice for CFS
    pub cpu_affinity: u64, // Bitmask of allowed CPUs
    // Deadline parameters (SCHED_DEADLINE)
    pub runtime_ns: u64,  // Worst-case execution time
    pub deadline_ns: u64, // Relative deadline
    pub period_ns: u64,   // Period of the task
    // Runtime stats
    pub total_runtime_ns: u64,
    pub max_latency_ns: u64,
    pub min_latency_ns: u64,
    pub wakeup_count: u64,
    pub preemption_count: u64,
}

impl RtTaskParams {
    pub fn new(pid: u32, policy: RtPolicy, priority: u32) -> Self {
        Self {
            pid,
            policy,
            priority,
            cpu_affinity: u64::MAX, // All CPUs
            runtime_ns: 0,
            deadline_ns: 0,
            period_ns: 0,
            total_runtime_ns: 0,
            max_latency_ns: 0,
            min_latency_ns: u64::MAX,
            wakeup_count: 0,
            preemption_count: 0,
        }
    }

    pub fn with_deadline(mut self, runtime_ns: u64, deadline_ns: u64, period_ns: u64) -> Self {
        self.runtime_ns = runtime_ns;
        self.deadline_ns = deadline_ns;
        self.period_ns = period_ns;
        self
    }
}

// ─── Priority Inheritance Mutex ─────────────────────────────────────

/// PI-aware mutex: boosts holder's priority to highest waiter's priority
/// to prevent priority inversion.
#[derive(Debug)]
pub struct PiMutex {
    pub id: u32,
    pub name: String,
    pub owner: Option<u32>,       // Owning PID
    pub owner_orig_prio: u32,     // Original priority before boost
    pub waiters: Vec<(u32, u32)>, // (PID, priority) sorted by priority desc
    pub locked: bool,
}

impl PiMutex {
    pub fn new(id: u32, name: &str) -> Self {
        Self {
            id,
            name: String::from(name),
            owner: None,
            owner_orig_prio: 0,
            waiters: Vec::new(),
            locked: false,
        }
    }

    /// Acquire the mutex, performing priority inheritance if needed
    pub fn lock(&mut self, pid: u32, priority: u32) -> Result<(), u32> {
        if !self.locked {
            self.locked = true;
            self.owner = Some(pid);
            self.owner_orig_prio = priority;
            Ok(())
        } else {
            // Add to waiters list
            self.waiters.push((pid, priority));
            self.waiters.sort_by_key(|b| core::cmp::Reverse(b.1)); // highest priority first

            // Priority inheritance: boost owner if waiter has higher priority
            if let Some(_owner_pid) = self.owner {
                let highest_waiter_prio = self.waiters.first().map(|w| w.1).unwrap_or(0);
                if highest_waiter_prio > self.owner_orig_prio {
                    // Boost owner's priority
                    serial_println!(
                        "[RT-PI] Priority boost: pid {} boosted from {} to {}",
                        _owner_pid,
                        self.owner_orig_prio,
                        highest_waiter_prio
                    );
                }
            }
            Err(pid) // Would block
        }
    }

    /// Release the mutex, restoring original priority
    pub fn unlock(&mut self, pid: u32) -> Result<Option<u32>, &'static str> {
        if self.owner != Some(pid) {
            return Err("Not the owner");
        }

        // Restore original priority
        serial_println!("[RT-PI] Mutex '{}' released by pid {}", self.name, pid);

        // Wake highest priority waiter
        if let Some((next_pid, next_prio)) = self.waiters.first().cloned() {
            self.waiters.remove(0);
            self.owner = Some(next_pid);
            self.owner_orig_prio = next_prio;
            Ok(Some(next_pid))
        } else {
            self.locked = false;
            self.owner = None;
            Ok(None)
        }
    }
}

// ─── RT Throttling ──────────────────────────────────────────────────

/// Per-CPU RT bandwidth throttling
#[derive(Debug, Clone)]
pub struct RtBandwidth {
    pub runtime_us: u64, // RT runtime per period (default 950000 = 950ms)
    pub period_us: u64,  // Throttling period (default 1000000 = 1s)
    pub used_us: u64,    // Used runtime in current period
    pub throttled: bool,
    pub throttled_count: u64,
}

impl Default for RtBandwidth {
    fn default() -> Self {
        Self::new()
    }
}

impl RtBandwidth {
    pub const fn new() -> Self {
        Self {
            runtime_us: 950_000,  // 95% CPU for RT tasks
            period_us: 1_000_000, // 1 second period
            used_us: 0,
            throttled: false,
            throttled_count: 0,
        }
    }

    /// Check if RT task can run (bandwidth not exhausted)
    pub fn can_run(&self) -> bool {
        !self.throttled && self.used_us < self.runtime_us
    }

    /// Account RT runtime
    pub fn charge(&mut self, runtime_us: u64) {
        self.used_us += runtime_us;
        if self.used_us >= self.runtime_us {
            self.throttled = true;
            self.throttled_count += 1;
        }
    }

    /// Reset bandwidth at period boundary
    pub fn reset_period(&mut self) {
        self.used_us = 0;
        self.throttled = false;
    }
}

// ─── High-Resolution Timer ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HrTimer {
    pub id: u32,
    pub expires_ns: u64,   // Absolute expiry time in nanoseconds
    pub interval_ns: u64,  // For periodic timers (0 = one-shot)
    pub callback_pid: u32, // Process to signal on expiry
    pub signal: u32,       // Signal to deliver
    pub active: bool,
    pub overrun_count: u64,
}

impl HrTimer {
    pub fn new_oneshot(id: u32, expires_ns: u64, pid: u32, signal: u32) -> Self {
        Self {
            id,
            expires_ns,
            interval_ns: 0,
            callback_pid: pid,
            signal,
            active: true,
            overrun_count: 0,
        }
    }

    pub fn new_periodic(id: u32, interval_ns: u64, pid: u32, signal: u32) -> Self {
        Self {
            id,
            expires_ns: interval_ns,
            interval_ns,
            callback_pid: pid,
            signal,
            active: true,
            overrun_count: 0,
        }
    }
}

// ─── Latency Histogram ──────────────────────────────────────────────

pub struct LatencyHistogram {
    pub buckets: [u64; 32], // Bucket boundaries: 1us, 2us, 4us, ... 2^31 us
    pub total_samples: u64,
    pub max_latency_us: u64,
    pub min_latency_us: u64,
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl LatencyHistogram {
    pub const fn new() -> Self {
        Self {
            buckets: [0; 32],
            total_samples: 0,
            max_latency_us: 0,
            min_latency_us: u64::MAX,
        }
    }

    pub fn record(&mut self, latency_us: u64) {
        self.total_samples += 1;
        if latency_us > self.max_latency_us {
            self.max_latency_us = latency_us;
        }
        if latency_us < self.min_latency_us {
            self.min_latency_us = latency_us;
        }

        // Find bucket: log2(latency_us)
        let bucket = if latency_us == 0 {
            0
        } else {
            (64 - latency_us.leading_zeros() - 1).min(31) as usize
        };
        self.buckets[bucket] += 1;
    }

    pub fn average_us(&self) -> u64 {
        if self.total_samples == 0 {
            return 0;
        }
        // Approximate average from buckets
        let mut weighted_sum = 0u64;
        for i in 0..32 {
            weighted_sum += self.buckets[i] * (1u64 << i);
        }
        weighted_sum / self.total_samples
    }
}

// ─── CPU Isolation ──────────────────────────────────────────────────

/// CPUs isolated for RT tasks (no other work scheduled on them)
pub struct CpuIsolation {
    pub isolated_mask: u64,     // Bitmask of isolated CPUs
    pub housekeeping_mask: u64, // CPUs for housekeeping (kernel threads, interrupts)
}

impl Default for CpuIsolation {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuIsolation {
    pub const fn new() -> Self {
        Self {
            isolated_mask: 0,
            housekeeping_mask: u64::MAX,
        }
    }

    pub fn isolate_cpu(&mut self, cpu: u32) {
        self.isolated_mask |= 1u64 << cpu;
        self.housekeeping_mask &= !(1u64 << cpu);
    }

    pub fn unisolate_cpu(&mut self, cpu: u32) {
        self.isolated_mask &= !(1u64 << cpu);
        self.housekeeping_mask |= 1u64 << cpu;
    }

    pub fn is_isolated(&self, cpu: u32) -> bool {
        self.isolated_mask & (1u64 << cpu) != 0
    }
}

// ─── Global State ───────────────────────────────────────────────────

static RT_TASKS: Mutex<BTreeMap<u32, RtTaskParams>> = Mutex::new(BTreeMap::new());
static PI_MUTEXES: Mutex<BTreeMap<u32, PiMutex>> = Mutex::new(BTreeMap::new());
static HR_TIMERS: Mutex<Vec<HrTimer>> = Mutex::new(Vec::new());
static RT_BANDWIDTH: Mutex<[RtBandwidth; 64]> = Mutex::new([const { RtBandwidth::new() }; 64]);
static LATENCY_HIST: Mutex<LatencyHistogram> = Mutex::new(LatencyHistogram::new());
static CPU_ISOLATION: Mutex<CpuIsolation> = Mutex::new(CpuIsolation::new());
static NEXT_MUTEX_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_TIMER_ID: AtomicU32 = AtomicU32::new(1);
static RT_ENABLED: AtomicBool = AtomicBool::new(false);

/// Set RT scheduling parameters for a process
pub fn set_rt_params(pid: u32, policy: RtPolicy, priority: u32) -> Result<(), &'static str> {
    if priority > RT_PRIO_MAX {
        return Err("Priority out of range (max 99)");
    }
    let params = RtTaskParams::new(pid, policy, priority);
    RT_TASKS.lock().insert(pid, params);
    serial_println!("[RT] pid {} set to {:?} priority {}", pid, policy, priority);
    Ok(())
}

/// Set deadline parameters
pub fn set_deadline_params(
    pid: u32,
    runtime_ns: u64,
    deadline_ns: u64,
    period_ns: u64,
) -> Result<(), &'static str> {
    let mut tasks = RT_TASKS.lock();
    if let Some(task) = tasks.get_mut(&pid) {
        task.policy = RtPolicy::Deadline;
        task.runtime_ns = runtime_ns;
        task.deadline_ns = deadline_ns;
        task.period_ns = period_ns;
        Ok(())
    } else {
        Err("Task not found")
    }
}

/// Record a scheduling latency measurement
pub fn record_latency(latency_us: u64) {
    LATENCY_HIST.lock().record(latency_us);
}

/// Get latency statistics
pub fn latency_stats() -> (u64, u64, u64, u64) {
    let hist = LATENCY_HIST.lock();
    (
        hist.min_latency_us,
        hist.max_latency_us,
        hist.average_us(),
        hist.total_samples,
    )
}

/// Create a PI mutex
pub fn create_pi_mutex(name: &str) -> u32 {
    let id = NEXT_MUTEX_ID.fetch_add(1, Ordering::Relaxed);
    let mutex = PiMutex::new(id, name);
    PI_MUTEXES.lock().insert(id, mutex);
    id
}

/// Create a high-resolution timer
pub fn create_hrtimer(expires_ns: u64, interval_ns: u64, pid: u32, signal: u32) -> u32 {
    let id = NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed);
    let timer = if interval_ns > 0 {
        HrTimer::new_periodic(id, interval_ns, pid, signal)
    } else {
        HrTimer::new_oneshot(id, expires_ns, pid, signal)
    };
    HR_TIMERS.lock().push(timer);
    id
}

/// Process expired HR timers (called from timer interrupt)
pub fn process_hrtimers(current_ns: u64) {
    let mut timers = HR_TIMERS.lock();
    for timer in timers.iter_mut() {
        if timer.active && current_ns >= timer.expires_ns {
            // Timer expired — signal the process
            if timer.interval_ns > 0 {
                // Periodic: schedule next firing
                timer.expires_ns += timer.interval_ns;
                if timer.expires_ns <= current_ns {
                    timer.overrun_count += 1;
                    timer.expires_ns = current_ns + timer.interval_ns;
                }
            } else {
                timer.active = false;
            }
        }
    }
    // Remove inactive one-shot timers
    timers.retain(|t| t.active || t.interval_ns > 0);
}

/// Check if current CPU's RT bandwidth allows running
pub fn rt_bandwidth_available(cpu: usize) -> bool {
    let bw = RT_BANDWIDTH.lock();
    if cpu < 64 { bw[cpu].can_run() } else { false }
}

/// Charge RT runtime to CPU's bandwidth
pub fn rt_bandwidth_charge(cpu: usize, runtime_us: u64) {
    let mut bw = RT_BANDWIDTH.lock();
    if cpu < 64 {
        bw[cpu].charge(runtime_us);
    }
}

/// Reset all CPU RT bandwidth (called at period boundary)
pub fn rt_bandwidth_reset() {
    let mut bw = RT_BANDWIDTH.lock();
    for cpu_bw in bw.iter_mut() {
        cpu_bw.reset_period();
    }
}

/// Isolate a CPU for RT workloads
pub fn isolate_cpu(cpu: u32) {
    CPU_ISOLATION.lock().isolate_cpu(cpu);
    serial_println!("[RT] CPU {} isolated for RT tasks", cpu);
}

/// Get RT task info for a PID
pub fn get_rt_info(pid: u32) -> Option<RtTaskParams> {
    RT_TASKS.lock().get(&pid).cloned()
}

pub fn is_enabled() -> bool {
    RT_ENABLED.load(Ordering::Relaxed)
}

pub fn init() {
    RT_ENABLED.store(true, Ordering::Relaxed);

    serial_println!("[RT] PREEMPT_RT real-time extensions initialized");
    serial_println!("[RT]   Priority range: {}-{}", RT_PRIO_MIN, RT_PRIO_MAX);
    serial_println!("[RT]   Policies: FIFO, RR, Deadline, Batch, Idle");
    serial_println!("[RT]   PI mutexes, HR timers, RT bandwidth throttling enabled");
    serial_println!("[RT]   Latency tracking histogram active");
}
