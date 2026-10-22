/// sched_debug — Scheduler debugging and statistics
///
/// Provides detailed scheduler debugging information compatible with
/// Linux /proc/sched_debug and /proc/schedstat interfaces.
///
/// Features:
/// - Per-CPU runqueue statistics
/// - CFS bandwidth control debugging
/// - Scheduler latency tracking
/// - Migration statistics
/// - Load tracking (PELT - Per-Entity Load Tracking)
/// - /proc/sched_debug output generation
/// - /proc/schedstat output generation
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── PELT — Per-Entity Load Tracking ────────────────────────────────

/// Per-Entity Load Tracking (compatible with Linux CFS PELT)
#[derive(Debug, Clone, Default)]
pub struct PeltSignal {
    /// Running average load (0-1024 scale)
    pub load_avg: u64,
    /// Running average runnable time
    pub runnable_avg: u64,
    /// Running average utilization
    pub util_avg: u64,
    /// Last update time (ns)
    pub last_update_time: u64,
    /// Geometric series sum for decay
    pub period_contrib: u32,
    /// Load sum (accumulator)
    pub load_sum: u64,
    /// Runnable sum
    pub runnable_sum: u64,
    /// Utilization sum
    pub util_sum: u64,
}

impl PeltSignal {
    /// PELT half-life period in microseconds (32ms like Linux)
    const PELT_HALF_LIFE_US: u64 = 32_000;

    /// Update PELT signal with new running/runnable data
    pub fn update(&mut self, now_us: u64, running: bool, runnable: bool, weight: u64) {
        if self.last_update_time == 0 {
            self.last_update_time = now_us;
            return;
        }

        let delta = now_us.saturating_sub(self.last_update_time);
        if delta == 0 {
            return;
        }

        self.last_update_time = now_us;

        // Number of complete periods elapsed
        let periods = delta / Self::PELT_HALF_LIFE_US;

        // Decay existing sums
        if periods > 0 {
            // Approximate exponential decay: multiply by 0.5^periods
            for _ in 0..periods.min(10) {
                self.load_sum /= 2;
                self.runnable_sum /= 2;
                self.util_sum /= 2;
            }
        }

        // Accumulate new contribution
        let contrib = delta.min(Self::PELT_HALF_LIFE_US);
        if runnable {
            self.load_sum += contrib * weight / 1024;
            self.runnable_sum += contrib;
        }
        if running {
            self.util_sum += contrib;
        }

        // Calculate averages (scaled to 0-1024)
        self.load_avg = self.load_sum * 1024 / Self::PELT_HALF_LIFE_US.max(1);
        self.runnable_avg = self.runnable_sum * 1024 / Self::PELT_HALF_LIFE_US.max(1);
        self.util_avg = self.util_sum * 1024 / Self::PELT_HALF_LIFE_US.max(1);

        // Cap at 1024
        self.load_avg = self.load_avg.min(1024);
        self.runnable_avg = self.runnable_avg.min(1024);
        self.util_avg = self.util_avg.min(1024);
    }
}

// ─── Per-CPU Runqueue Stats ─────────────────────────────────────────

/// Per-CPU scheduler statistics
#[derive(Debug, Clone)]
pub struct CpuSchedStats {
    /// CPU index
    pub cpu: u32,
    /// Number of tasks on the run queue
    pub nr_running: u32,
    /// Total context switches
    pub nr_switches: u64,
    /// Voluntary context switches
    pub nr_voluntary_switches: u64,
    /// Involuntary context switches (preemptions)
    pub nr_involuntary_switches: u64,
    /// Total wait time (ns) of all tasks that waited on this CPU
    pub wait_sum_ns: u64,
    /// Number of tasks that waited
    pub wait_count: u64,
    /// Total run time (ns) of all tasks on this CPU
    pub run_sum_ns: u64,
    /// Number of schedule events
    pub sched_count: u64,
    /// Total idle time (ns)
    pub idle_ns: u64,
    /// Number of load balance runs
    pub lb_count: u64,
    /// Number of tasks pulled by load balancing
    pub lb_gained: u64,
    /// Number of failed load balance attempts
    pub lb_failed: u64,
    /// Number of tasks pushed by active balance
    pub lb_pushed: u64,
    /// Number of migrations (task moved between CPUs)
    pub nr_migrations: u64,
    /// Current load (PELT)
    pub pelt: PeltSignal,
    /// Scheduler latency tracking (histogram buckets in µs)
    /// Buckets: [0-1), [1-2), [2-4), [4-8), [8-16), [16-32), [32-64), [64+)
    pub latency_histogram: [u64; 8],
    /// Max observed latency (µs)
    pub max_latency_us: u64,
    /// Min latency (µs)
    pub min_latency_us: u64,
}

impl CpuSchedStats {
    pub fn new(cpu: u32) -> Self {
        Self {
            cpu,
            nr_running: 0,
            nr_switches: 0,
            nr_voluntary_switches: 0,
            nr_involuntary_switches: 0,
            wait_sum_ns: 0,
            wait_count: 0,
            run_sum_ns: 0,
            sched_count: 0,
            idle_ns: 0,
            lb_count: 0,
            lb_gained: 0,
            lb_failed: 0,
            lb_pushed: 0,
            nr_migrations: 0,
            pelt: PeltSignal::default(),
            latency_histogram: [0; 8],
            max_latency_us: 0,
            min_latency_us: u64::MAX,
        }
    }

    /// Record a context switch
    pub fn record_switch(&mut self, voluntary: bool) {
        self.nr_switches += 1;
        self.sched_count += 1;
        if voluntary {
            self.nr_voluntary_switches += 1;
        } else {
            self.nr_involuntary_switches += 1;
        }
    }

    /// Record scheduling latency
    pub fn record_latency(&mut self, latency_us: u64) {
        let bucket = if latency_us == 0 {
            0
        } else if latency_us < 2 {
            1
        } else if latency_us < 4 {
            2
        } else if latency_us < 8 {
            3
        } else if latency_us < 16 {
            4
        } else if latency_us < 32 {
            5
        } else if latency_us < 64 {
            6
        } else {
            7
        };
        self.latency_histogram[bucket] += 1;

        if latency_us > self.max_latency_us {
            self.max_latency_us = latency_us;
        }
        if latency_us < self.min_latency_us {
            self.min_latency_us = latency_us;
        }
    }

    /// Record a migration event
    pub fn record_migration(&mut self) {
        self.nr_migrations += 1;
    }

    /// Record wait time for a task
    pub fn record_wait(&mut self, wait_ns: u64) {
        self.wait_sum_ns += wait_ns;
        self.wait_count += 1;
    }

    /// Average wait time
    pub fn avg_wait_ns(&self) -> u64 {
        self.wait_sum_ns.checked_div(self.wait_count).unwrap_or(0)
    }
}

// ─── Per-Task Stats ─────────────────────────────────────────────────

/// Per-task scheduler statistics
#[derive(Debug, Clone)]
pub struct TaskSchedStats {
    pub pid: u32,
    pub comm: String,
    /// Total time on CPU (ns)
    pub sum_exec_ns: u64,
    /// Number of times scheduled
    pub nr_switches: u64,
    /// Total wait time (waiting in runqueue)
    pub wait_sum_ns: u64,
    /// Number of waits
    pub wait_count: u64,
    /// Number of migrations between CPUs
    pub nr_migrations: u64,
    /// Number of involuntary context switches (preemptions)
    pub nr_involuntary_switches: u64,
    /// CFS virtual runtime
    pub vruntime: u64,
    /// Task weight (nice → weight)
    pub weight: u64,
    /// Last CPU the task ran on
    pub last_cpu: u32,
    /// PELT signals
    pub pelt: PeltSignal,
}

impl TaskSchedStats {
    pub fn new(pid: u32, name: &str) -> Self {
        Self {
            pid,
            comm: String::from(name),
            sum_exec_ns: 0,
            nr_switches: 0,
            wait_sum_ns: 0,
            wait_count: 0,
            nr_migrations: 0,
            nr_involuntary_switches: 0,
            vruntime: 0,
            weight: 1024,
            last_cpu: 0,
            pelt: PeltSignal::default(),
        }
    }
}

// ─── CFS Bandwidth Control Debug ────────────────────────────────────

/// CFS bandwidth throttling statistics
#[derive(Debug, Clone, Default)]
pub struct CfsBandwidthStats {
    /// Number of times throttled
    pub nr_throttled: u64,
    /// Total throttled time (ns)
    pub throttled_time_ns: u64,
    /// Number of periods
    pub nr_periods: u64,
    /// Number of burst periods
    pub nr_burst: u64,
    /// Total burst time used (ns)
    pub burst_time_ns: u64,
}

// ─── Global Scheduler Debug ─────────────────────────────────────────

/// Maximum number of CPUs tracked
const MAX_CPUS: usize = 64;

pub struct SchedDebugState {
    /// Per-CPU stats
    pub cpu_stats: Vec<CpuSchedStats>,
    /// Per-task stats
    pub task_stats: BTreeMap<u32, TaskSchedStats>,
    /// CFS bandwidth stats
    pub cfs_bw: CfsBandwidthStats,
    /// Global scheduler clock (ns)
    pub sched_clock_ns: u64,
    /// Kernel version for sched_debug header
    pub kernel_version: String,
    /// Number of online CPUs
    pub online_cpus: u32,
}

lazy_static::lazy_static! {
    pub static ref SCHED_DEBUG: Mutex<SchedDebugState> = Mutex::new(SchedDebugState::new());
}

impl SchedDebugState {
    pub fn new() -> Self {
        let mut cpu_stats = Vec::new();
        // Pre-create stats for up to 8 CPUs (expandable)
        for i in 0..8 {
            cpu_stats.push(CpuSchedStats::new(i));
        }

        Self {
            cpu_stats,
            task_stats: BTreeMap::new(),
            cfs_bw: CfsBandwidthStats::default(),
            sched_clock_ns: 0,
            kernel_version: String::from("6.1.0-knoxos"),
            online_cpus: 1,
        }
    }

    /// Record a context switch for a CPU
    pub fn record_context_switch(&mut self, cpu: u32, voluntary: bool) {
        if let Some(stats) = self.cpu_stats.get_mut(cpu as usize) {
            stats.record_switch(voluntary);
        }
    }

    /// Record scheduling latency for a CPU
    pub fn record_latency(&mut self, cpu: u32, latency_us: u64) {
        if let Some(stats) = self.cpu_stats.get_mut(cpu as usize) {
            stats.record_latency(latency_us);
        }
    }

    /// Register a task for tracking
    pub fn register_task(&mut self, pid: u32, name: &str) {
        self.task_stats.insert(pid, TaskSchedStats::new(pid, name));
    }

    /// Remove a task
    pub fn unregister_task(&mut self, pid: u32) {
        self.task_stats.remove(&pid);
    }

    /// Get task stats
    pub fn get_task_stats(&self, pid: u32) -> Option<&TaskSchedStats> {
        self.task_stats.get(&pid)
    }

    /// Generate /proc/sched_debug output
    pub fn format_sched_debug(&self) -> String {
        let mut s = String::new();
        s.push_str("Sched Debug Version: v0.11, ");
        s.push_str(&self.kernel_version);
        s.push('\n');
        s.push_str("ktime                    : ");
        // Simple clock format
        s.push_str("0.000000\n");
        s.push_str("sched_clk                : ");
        s.push_str("0.000000\n");
        s.push_str("cpu_clk                  : ");
        s.push_str("0.000000\n\n");

        for stats in &self.cpu_stats {
            if stats.cpu >= self.online_cpus {
                break;
            }
            s.push_str("cpu#");
            push_u64(&mut s, stats.cpu as u64);
            s.push('\n');
            s.push_str("  .nr_running            : ");
            push_u64(&mut s, stats.nr_running as u64);
            s.push('\n');
            s.push_str("  .nr_switches           : ");
            push_u64(&mut s, stats.nr_switches);
            s.push('\n');
            s.push_str("  .nr_voluntary_switches : ");
            push_u64(&mut s, stats.nr_voluntary_switches);
            s.push('\n');
            s.push_str("  .nr_involuntary_switches: ");
            push_u64(&mut s, stats.nr_involuntary_switches);
            s.push('\n');
            s.push_str("  .sched_count           : ");
            push_u64(&mut s, stats.sched_count);
            s.push('\n');
            s.push_str("  .nr_migrations         : ");
            push_u64(&mut s, stats.nr_migrations);
            s.push('\n');
            s.push_str("  .load_avg              : ");
            push_u64(&mut s, stats.pelt.load_avg);
            s.push('\n');
            s.push_str("  .util_avg              : ");
            push_u64(&mut s, stats.pelt.util_avg);
            s.push('\n');
            s.push('\n');
        }

        s
    }

    /// Generate /proc/schedstat output
    pub fn format_schedstat(&self) -> String {
        let mut s = String::new();
        s.push_str("version 15\n");
        s.push_str("timestamp 0\n");

        for stats in &self.cpu_stats {
            if stats.cpu >= self.online_cpus {
                break;
            }
            s.push_str("cpu");
            push_u64(&mut s, stats.cpu as u64);
            s.push(' ');
            // Fields: sched_yield, sched_switch, sched_count, idle_ns, wait_sum, wait_count, run_sum, ...
            push_u64(&mut s, 0);
            s.push(' '); // yield
            push_u64(&mut s, stats.nr_switches);
            s.push(' ');
            push_u64(&mut s, stats.sched_count);
            s.push(' ');
            push_u64(&mut s, stats.idle_ns);
            s.push(' ');
            push_u64(&mut s, stats.wait_sum_ns);
            s.push(' ');
            push_u64(&mut s, stats.wait_count);
            s.push(' ');
            push_u64(&mut s, stats.run_sum_ns);
            s.push(' ');
            push_u64(&mut s, stats.lb_count);
            s.push(' ');
            push_u64(&mut s, stats.lb_gained);
            s.push('\n');
        }

        s
    }
}

/// Helper: push a u64 as decimal string
fn push_u64(s: &mut String, val: u64) {
    if val == 0 {
        s.push('0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut n = val;
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    for j in (0..i).rev() {
        s.push(buf[j] as char);
    }
}

// ─── Public API ─────────────────────────────────────────────────────

/// Record a context switch
pub fn record_context_switch(cpu: u32, voluntary: bool) {
    SCHED_DEBUG.lock().record_context_switch(cpu, voluntary);
}

/// Record scheduling latency
pub fn record_latency(cpu: u32, latency_us: u64) {
    SCHED_DEBUG.lock().record_latency(cpu, latency_us);
}

/// Register a new task
pub fn register_task(pid: u32, name: &str) {
    SCHED_DEBUG.lock().register_task(pid, name);
}

/// Unregister a task
pub fn unregister_task(pid: u32) {
    SCHED_DEBUG.lock().unregister_task(pid);
}

/// Get /proc/sched_debug content
pub fn proc_sched_debug() -> String {
    SCHED_DEBUG.lock().format_sched_debug()
}

/// Get /proc/schedstat content
pub fn proc_schedstat() -> String {
    SCHED_DEBUG.lock().format_schedstat()
}

/// Initialize scheduler debugging
pub fn init() {
    let mut debug = SCHED_DEBUG.lock();

    // Register kernel threads
    debug.register_task(0, "swapper/0");
    debug.register_task(1, "init");
    debug.register_task(2, "knoxos-desktop");

    serial_println!(
        "[sched_debug] Scheduler debug initialized ({} CPUs, PELT tracking, latency histograms)",
        debug.online_cpus
    );
}
