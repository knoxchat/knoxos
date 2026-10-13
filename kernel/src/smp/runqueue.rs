use alloc::vec::Vec;
use core::sync::atomic::Ordering;

use spin::Mutex;

use super::state::{CPU_DATA, CPUS_STARTED, MAX_CPUS};

// ─── Per-CPU Run Queues ─────────────────────────────────────────────────

/// Per-CPU run queue for SMP-aware scheduling
struct PerCpuRunQueue {
    queue: alloc::collections::VecDeque<u32>, // PIDs
    load: u64,                                // Approximate load metric
}

impl PerCpuRunQueue {
    const fn new() -> Self {
        Self {
            queue: alloc::collections::VecDeque::new(),
            load: 0,
        }
    }
}

lazy_static::lazy_static! {
    /// Per-CPU run queues (one per CPU)
    static ref PER_CPU_RUNQUEUES: Mutex<Vec<PerCpuRunQueue>> = {
        let mut queues = Vec::new();
        for _ in 0..MAX_CPUS {
            queues.push(PerCpuRunQueue {
                queue: alloc::collections::VecDeque::new(),
                load: 0,
            });
        }
        Mutex::new(queues)
    };
}

/// Enqueue a process for SMP dispatch.
///
/// Unpinned tasks stay on BSP CFS so boot `run_until_desktop` still works.
/// Single-CPU affinity is honoured (Gate I3). Idle APs take unpinned work
/// from [`balance_load`] or an explicit [`enqueue_on_cpu`] (Gate I4).
pub fn enqueue_balanced(pid: u32) {
    let affinity = crate::scheduler::get_cpu_affinity(pid).unwrap_or(u64::MAX);
    if affinity.count_ones() == 1 {
        let cpu = affinity.trailing_zeros() as usize;
        enqueue_on_cpu(cpu, pid);
        crate::scheduler::set_placed_cpu(pid, cpu as u32);
    }
}

/// Enqueue a process on a specific CPU
pub fn enqueue_on_cpu(cpu: usize, pid: u32) {
    let mut queues = PER_CPU_RUNQUEUES.lock();
    if cpu < queues.len() && !queues[cpu].queue.contains(&pid) {
        queues[cpu].queue.push_back(pid);
        queues[cpu].load += 1;
    }
}

/// Dequeue the next process from a CPU's run queue
pub fn dequeue_from_cpu(cpu: usize) -> Option<u32> {
    let mut queues = PER_CPU_RUNQUEUES.lock();
    if cpu < queues.len() {
        if let Some(pid) = queues[cpu].queue.pop_front() {
            queues[cpu].load = queues[cpu].load.saturating_sub(1);
            return Some(pid);
        }
    }
    None
}

pub fn remove_from_runqueues(pid: u32) {
    let mut queues = PER_CPU_RUNQUEUES.lock();
    for q in queues.iter_mut() {
        let before = q.queue.len();
        q.queue.retain(|p| *p != pid);
        q.load = q.load.saturating_sub((before - q.queue.len()) as u64);
    }
}

/// CPU load balancing — place unpinned runnable work on idle APs.
/// Never steals a single-CPU-pinned task (Gate I3).
pub fn balance_load() {
    let online = CPUS_STARTED.load(Ordering::Relaxed) as usize;
    if online < 2 {
        return;
    }
    for cpu in 1..online.min(MAX_CPUS) {
        if !crate::smp::ap_in_idle(cpu as u32) {
            continue;
        }
        let Some(pid) = crate::scheduler::steal_unpinned_for_cpu(cpu as u32) else {
            continue;
        };
        enqueue_on_cpu(cpu, pid);
        crate::scheduler::set_placed_cpu(pid, cpu as u32);
        return;
    }
}

/// Get per-CPU load statistics
pub fn cpu_load_stats() -> Vec<(u32, u64, usize)> {
    let queues = PER_CPU_RUNQUEUES.lock();
    let cpus = CPU_DATA.lock();
    let online = CPUS_STARTED.load(Ordering::Relaxed) as usize;
    let mut stats = Vec::new();

    for i in 0..online.min(queues.len()) {
        stats.push((i as u32, queues[i].load, queues[i].queue.len()));
    }
    stats
}
