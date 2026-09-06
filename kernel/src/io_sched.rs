/// I/O Scheduler
///
/// Implements multiple I/O scheduling algorithms for block device request ordering.
/// Compatible with Linux block layer I/O scheduling.
///
/// Schedulers:
///   - None (FIFO / noop)
///   - Deadline (deadline-based with read/write queues)
///   - CFQ (Complete Fair Queuing - per-process fair scheduling)
///   - BFQ (Budget Fair Queuing - proportional share + low latency)
///   - mq-deadline (multi-queue deadline)
///   - kyber (latency-target based for fast SSDs)
///
/// Features:
///   - Request merging (front/back merge)
///   - Per-queue scheduling
///   - Priority-based I/O classes (real-time, best-effort, idle)
///   - I/O bandwidth throttling
///   - Latency tracking
///   - Queue depth management
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// I/O REQUEST
// ═══════════════════════════════════════════════════════════════════════

/// I/O request direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoDirection {
    Read,
    Write,
    Flush,
    Discard,
}

/// I/O priority class (Linux-compatible ioprio)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IoPriorityClass {
    None = 0,
    RealTime = 1,
    BestEffort = 2,
    Idle = 3,
}

/// I/O priority (class + level 0-7)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoPriority {
    pub class: IoPriorityClass,
    pub level: u8, // 0 (highest) to 7 (lowest)
}

impl IoPriority {
    pub fn new(class: IoPriorityClass, level: u8) -> Self {
        Self {
            class,
            level: level.min(7),
        }
    }

    pub fn default() -> Self {
        Self {
            class: IoPriorityClass::BestEffort,
            level: 4,
        }
    }

    pub fn encode(&self) -> u16 {
        ((self.class as u16) << 13) | (self.level as u16)
    }

    pub fn decode(val: u16) -> Self {
        let class = match (val >> 13) & 0x7 {
            1 => IoPriorityClass::RealTime,
            2 => IoPriorityClass::BestEffort,
            3 => IoPriorityClass::Idle,
            _ => IoPriorityClass::None,
        };
        Self {
            class,
            level: (val & 0x7) as u8,
        }
    }
}

/// Block I/O request
#[derive(Debug, Clone)]
pub struct IoRequest {
    pub id: u64,
    pub direction: IoDirection,
    pub sector: u64,
    pub nr_sectors: u32,
    pub priority: IoPriority,
    pub pid: u32,
    pub submitted_at: u64,
    pub deadline: u64,
    pub data_ptr: usize,
    pub completed: bool,
    pub error: Option<i32>,
}

impl IoRequest {
    pub fn new(direction: IoDirection, sector: u64, nr_sectors: u32) -> Self {
        let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            direction,
            sector,
            nr_sectors,
            priority: IoPriority::default(),
            pid: 0,
            submitted_at: rdtsc(),
            deadline: 0,
            data_ptr: 0,
            completed: false,
            error: None,
        }
    }

    /// Check if two requests are mergeable (adjacent sectors)
    pub fn can_merge_back(&self, other: &IoRequest) -> bool {
        self.direction == other.direction
            && self.sector + self.nr_sectors as u64 == other.sector
            && self.priority.class == other.priority.class
    }

    pub fn can_merge_front(&self, other: &IoRequest) -> bool {
        other.can_merge_back(self)
    }

    /// Merge another request into this one (back merge)
    pub fn merge_back(&mut self, other: &IoRequest) {
        self.nr_sectors += other.nr_sectors;
    }
}

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

// ═══════════════════════════════════════════════════════════════════════
// SCHEDULER TRAIT
// ═══════════════════════════════════════════════════════════════════════

/// I/O scheduler types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerType {
    None,       // noop / FIFO
    Deadline,   // Deadline scheduler
    Cfq,        // Complete Fair Queuing
    Bfq,        // Budget Fair Queuing
    MqDeadline, // Multi-queue deadline
    Kyber,      // Kyber (latency-target)
}

/// Statistics for an I/O scheduler
#[derive(Debug, Clone, Default)]
pub struct IoSchedStats {
    pub requests_submitted: u64,
    pub requests_completed: u64,
    pub requests_merged: u64,
    pub reads_dispatched: u64,
    pub writes_dispatched: u64,
    pub read_sectors: u64,
    pub write_sectors: u64,
    pub avg_latency_us: u64,
    pub max_latency_us: u64,
}

// ═══════════════════════════════════════════════════════════════════════
// NOOP SCHEDULER
// ═══════════════════════════════════════════════════════════════════════

/// Simple FIFO scheduler (no reordering)
pub struct NoopScheduler {
    queue: VecDeque<IoRequest>,
    stats: IoSchedStats,
}

impl NoopScheduler {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            stats: IoSchedStats::default(),
        }
    }

    pub fn submit(&mut self, req: IoRequest) {
        self.stats.requests_submitted += 1;
        self.queue.push_back(req);
    }

    pub fn dispatch(&mut self) -> Option<IoRequest> {
        let req = self.queue.pop_front()?;
        match req.direction {
            IoDirection::Read => {
                self.stats.reads_dispatched += 1;
                self.stats.read_sectors += req.nr_sectors as u64;
            }
            IoDirection::Write => {
                self.stats.writes_dispatched += 1;
                self.stats.write_sectors += req.nr_sectors as u64;
            }
            _ => {}
        }
        Some(req)
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn stats(&self) -> &IoSchedStats {
        &self.stats
    }
}

// ═══════════════════════════════════════════════════════════════════════
// DEADLINE SCHEDULER
// ═══════════════════════════════════════════════════════════════════════

/// Deadline scheduler - prevents starvation with per-request deadlines
pub struct DeadlineScheduler {
    /// Sorted by sector (for seeking efficiency)
    read_sorted: Vec<IoRequest>,
    write_sorted: Vec<IoRequest>,
    /// Sorted by deadline (FIFO with deadlines)
    read_fifo: VecDeque<IoRequest>,
    write_fifo: VecDeque<IoRequest>,
    /// Dispatch from reads vs writes
    read_deadline_ms: u64, // default 500ms
    write_deadline_ms: u64,  // default 5000ms
    writes_starved: u32,     // count of starved writes
    writes_starved_max: u32, // max before forcing write dispatch
    fifo_batch: u32,         // batch size for FIFO dispatch
    dispatched_reads: u32,
    dispatched_writes: u32,
    last_sector: u64,
    stats: IoSchedStats,
}

impl DeadlineScheduler {
    pub fn new() -> Self {
        Self {
            read_sorted: Vec::new(),
            write_sorted: Vec::new(),
            read_fifo: VecDeque::new(),
            write_fifo: VecDeque::new(),
            read_deadline_ms: 500,
            write_deadline_ms: 5000,
            writes_starved: 0,
            writes_starved_max: 2,
            fifo_batch: 16,
            dispatched_reads: 0,
            dispatched_writes: 0,
            last_sector: 0,
            stats: IoSchedStats::default(),
        }
    }

    pub fn submit(&mut self, mut req: IoRequest) {
        self.stats.requests_submitted += 1;

        let now = rdtsc();
        match req.direction {
            IoDirection::Read => {
                req.deadline = now + self.read_deadline_ms * 1_000_000; // approximate
                // Insert sorted by sector
                let pos = self
                    .read_sorted
                    .iter()
                    .position(|r| r.sector > req.sector)
                    .unwrap_or(self.read_sorted.len());
                self.read_sorted.insert(pos, req.clone());
                self.read_fifo.push_back(req);
            }
            IoDirection::Write => {
                req.deadline = now + self.write_deadline_ms * 1_000_000;
                let pos = self
                    .write_sorted
                    .iter()
                    .position(|r| r.sector > req.sector)
                    .unwrap_or(self.write_sorted.len());
                self.write_sorted.insert(pos, req.clone());
                self.write_fifo.push_back(req);
            }
            _ => {
                // Flush/discard go directly to read queue
                self.read_fifo.push_back(req);
            }
        }
    }

    pub fn dispatch(&mut self) -> Option<IoRequest> {
        let now = rdtsc();

        // Check if any read deadlines expired
        if let Some(front) = self.read_fifo.front() {
            if front.deadline <= now && !self.read_fifo.is_empty() {
                let req = self.read_fifo.pop_front()?;
                self.remove_from_sorted(&req, true);
                self.dispatched_reads += 1;
                self.writes_starved = self.writes_starved.saturating_add(1);
                return Some(req);
            }
        }

        // Check if any write deadlines expired
        if let Some(front) = self.write_fifo.front() {
            if front.deadline <= now && !self.write_fifo.is_empty() {
                let req = self.write_fifo.pop_front()?;
                self.remove_from_sorted(&req, false);
                self.dispatched_writes += 1;
                self.writes_starved = 0;
                return Some(req);
            }
        }

        // Prefer reads unless writes are starved
        if self.writes_starved >= self.writes_starved_max && !self.write_sorted.is_empty() {
            return self.dispatch_from_sorted(false);
        }

        if !self.read_sorted.is_empty() {
            self.writes_starved = self.writes_starved.saturating_add(1);
            return self.dispatch_from_sorted(true);
        }

        if !self.write_sorted.is_empty() {
            self.writes_starved = 0;
            return self.dispatch_from_sorted(false);
        }

        None
    }

    fn dispatch_from_sorted(&mut self, is_read: bool) -> Option<IoRequest> {
        let sorted = if is_read {
            &mut self.read_sorted
        } else {
            &mut self.write_sorted
        };

        if sorted.is_empty() {
            return None;
        }

        // Find closest sector to last dispatched
        let idx = sorted
            .iter()
            .position(|r| r.sector >= self.last_sector)
            .unwrap_or(0);

        let req = sorted.remove(idx);
        self.last_sector = req.sector + req.nr_sectors as u64;

        // Remove from FIFO
        if is_read {
            self.read_fifo.retain(|r| r.id != req.id);
            self.dispatched_reads += 1;
            self.stats.reads_dispatched += 1;
            self.stats.read_sectors += req.nr_sectors as u64;
        } else {
            self.write_fifo.retain(|r| r.id != req.id);
            self.dispatched_writes += 1;
            self.stats.writes_dispatched += 1;
            self.stats.write_sectors += req.nr_sectors as u64;
        }

        Some(req)
    }

    fn remove_from_sorted(&mut self, req: &IoRequest, is_read: bool) {
        let sorted = if is_read {
            &mut self.read_sorted
        } else {
            &mut self.write_sorted
        };
        sorted.retain(|r| r.id != req.id);
    }

    pub fn is_empty(&self) -> bool {
        self.read_sorted.is_empty()
            && self.write_sorted.is_empty()
            && self.read_fifo.is_empty()
            && self.write_fifo.is_empty()
    }

    pub fn stats(&self) -> &IoSchedStats {
        &self.stats
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CFQ SCHEDULER
// ═══════════════════════════════════════════════════════════════════════

/// Per-process I/O queue for CFQ
struct CfqQueue {
    pid: u32,
    priority: IoPriority,
    requests: VecDeque<IoRequest>,
    time_slice: u64,
    time_used: u64,
    dispatched: u64,
}

/// Complete Fair Queuing scheduler
pub struct CfqScheduler {
    queues: BTreeMap<u32, CfqQueue>, // per-PID queues
    active_pid: Option<u32>,
    time_slice_ms: u64,
    round_robin_order: Vec<u32>,
    rr_index: usize,
    stats: IoSchedStats,
}

impl CfqScheduler {
    pub fn new() -> Self {
        Self {
            queues: BTreeMap::new(),
            active_pid: None,
            time_slice_ms: 100,
            round_robin_order: Vec::new(),
            rr_index: 0,
            stats: IoSchedStats::default(),
        }
    }

    pub fn submit(&mut self, req: IoRequest) {
        self.stats.requests_submitted += 1;
        let pid = req.pid;

        let queue = self.queues.entry(pid).or_insert_with(|| CfqQueue {
            pid,
            priority: req.priority,
            requests: VecDeque::new(),
            time_slice: 0,
            time_used: 0,
            dispatched: 0,
        });

        // Try back merge
        if let Some(last) = queue.requests.back_mut() {
            if last.can_merge_back(&req) {
                last.merge_back(&req);
                self.stats.requests_merged += 1;
                return;
            }
        }

        queue.requests.push_back(req);

        if !self.round_robin_order.contains(&pid) {
            self.round_robin_order.push(pid);
        }
    }

    pub fn dispatch(&mut self) -> Option<IoRequest> {
        if self.round_robin_order.is_empty() {
            return None;
        }

        // Select active process in round-robin
        let pid = self.select_next_pid()?;

        let queue = self.queues.get_mut(&pid)?;
        let req = queue.requests.pop_front()?;

        queue.dispatched += 1;

        match req.direction {
            IoDirection::Read => {
                self.stats.reads_dispatched += 1;
                self.stats.read_sectors += req.nr_sectors as u64;
            }
            IoDirection::Write => {
                self.stats.writes_dispatched += 1;
                self.stats.write_sectors += req.nr_sectors as u64;
            }
            _ => {}
        }

        // Clean up empty queues
        if queue.requests.is_empty() {
            self.queues.remove(&pid);
            self.round_robin_order.retain(|&p| p != pid);
            if self.rr_index >= self.round_robin_order.len() && !self.round_robin_order.is_empty() {
                self.rr_index = 0;
            }
        }

        Some(req)
    }

    fn select_next_pid(&mut self) -> Option<u32> {
        if self.round_robin_order.is_empty() {
            return None;
        }

        // Sort by priority class first, then round-robin within class
        // For simplicity, we just do plain round-robin here
        if self.rr_index >= self.round_robin_order.len() {
            self.rr_index = 0;
        }
        let pid = self.round_robin_order[self.rr_index];
        self.rr_index += 1;
        Some(pid)
    }

    pub fn is_empty(&self) -> bool {
        self.queues.values().all(|q| q.requests.is_empty())
    }

    pub fn stats(&self) -> &IoSchedStats {
        &self.stats
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BFQ SCHEDULER
// ═══════════════════════════════════════════════════════════════════════

/// Per-process budget queue for BFQ
struct BfqQueue {
    pid: u32,
    priority: IoPriority,
    requests: VecDeque<IoRequest>,
    budget: u64,      // sectors allowed in this time slice
    budget_used: u64, // sectors consumed
    weight: u32,      // proportional weight (100 default)
    last_dispatch: u64,
}

/// Budget Fair Queuing scheduler (improved CFQ)
pub struct BfqScheduler {
    queues: BTreeMap<u32, BfqQueue>,
    active_pid: Option<u32>,
    default_budget: u64,
    budget_timeout_ms: u64,
    low_latency: bool,
    rr_order: Vec<u32>,
    rr_index: usize,
    stats: IoSchedStats,
}

impl BfqScheduler {
    pub fn new() -> Self {
        Self {
            queues: BTreeMap::new(),
            active_pid: None,
            default_budget: 128, // sectors
            budget_timeout_ms: 125,
            low_latency: true,
            rr_order: Vec::new(),
            rr_index: 0,
            stats: IoSchedStats::default(),
        }
    }

    fn weight_for_priority(prio: &IoPriority) -> u32 {
        match prio.class {
            IoPriorityClass::RealTime => 500 - (prio.level as u32 * 50),
            IoPriorityClass::BestEffort => 100 + ((4 - prio.level.min(4)) as u32 * 25),
            IoPriorityClass::Idle => 10,
            IoPriorityClass::None => 100,
        }
    }

    pub fn submit(&mut self, req: IoRequest) {
        self.stats.requests_submitted += 1;
        let pid = req.pid;

        let queue = self.queues.entry(pid).or_insert_with(|| {
            let weight = Self::weight_for_priority(&req.priority);
            BfqQueue {
                pid,
                priority: req.priority,
                requests: VecDeque::new(),
                budget: 0,
                budget_used: 0,
                weight,
                last_dispatch: 0,
            }
        });

        // Attempt merge
        if let Some(last) = queue.requests.back_mut() {
            if last.can_merge_back(&req) {
                last.merge_back(&req);
                self.stats.requests_merged += 1;
                return;
            }
        }

        queue.requests.push_back(req);

        if !self.rr_order.contains(&pid) {
            self.rr_order.push(pid);
        }
    }

    pub fn dispatch(&mut self) -> Option<IoRequest> {
        if self.rr_order.is_empty() {
            return None;
        }

        // Select process with budget
        let pid = self.select_next_pid()?;

        let queue = self.queues.get_mut(&pid)?;

        // Allocate budget if needed
        if queue.budget == 0 {
            queue.budget = self.default_budget * queue.weight as u64 / 100;
            queue.budget_used = 0;
        }

        let req = queue.requests.pop_front()?;
        queue.budget_used += req.nr_sectors as u64;
        queue.last_dispatch = rdtsc();

        match req.direction {
            IoDirection::Read => {
                self.stats.reads_dispatched += 1;
                self.stats.read_sectors += req.nr_sectors as u64;
            }
            IoDirection::Write => {
                self.stats.writes_dispatched += 1;
                self.stats.write_sectors += req.nr_sectors as u64;
            }
            _ => {}
        }

        // Check if budget exhausted
        if queue.budget_used >= queue.budget || queue.requests.is_empty() {
            queue.budget = 0;
            queue.budget_used = 0;
            if queue.requests.is_empty() {
                self.queues.remove(&pid);
                self.rr_order.retain(|&p| p != pid);
            }
            // Move to next process
            if self.rr_index >= self.rr_order.len() && !self.rr_order.is_empty() {
                self.rr_index = 0;
            }
        }

        Some(req)
    }

    fn select_next_pid(&mut self) -> Option<u32> {
        if self.rr_order.is_empty() {
            return None;
        }
        if self.rr_index >= self.rr_order.len() {
            self.rr_index = 0;
        }
        let pid = self.rr_order[self.rr_index];
        self.rr_index += 1;
        Some(pid)
    }

    pub fn is_empty(&self) -> bool {
        self.queues.values().all(|q| q.requests.is_empty())
    }

    pub fn stats(&self) -> &IoSchedStats {
        &self.stats
    }
}

// ═══════════════════════════════════════════════════════════════════════
// KYBER SCHEDULER
// ═══════════════════════════════════════════════════════════════════════

/// Kyber scheduler - latency-target based for fast devices (NVMe/SSD)
pub struct KyberScheduler {
    /// Read queue - latency sensitive
    read_queue: VecDeque<IoRequest>,
    /// Write queue - throughput oriented
    write_queue: VecDeque<IoRequest>,
    /// Discard queue
    discard_queue: VecDeque<IoRequest>,
    /// Target latencies
    read_lat_target_us: u64,
    write_lat_target_us: u64,
    /// Queue depth limits (auto-tuned)
    read_depth: u32,
    write_depth: u32,
    discard_depth: u32,
    /// Current in-flight
    read_inflight: u32,
    write_inflight: u32,
    discard_inflight: u32,
    stats: IoSchedStats,
}

impl KyberScheduler {
    pub fn new() -> Self {
        Self {
            read_queue: VecDeque::new(),
            write_queue: VecDeque::new(),
            discard_queue: VecDeque::new(),
            read_lat_target_us: 2000,   // 2ms read target
            write_lat_target_us: 10000, // 10ms write target
            read_depth: 256,
            write_depth: 128,
            discard_depth: 64,
            read_inflight: 0,
            write_inflight: 0,
            discard_inflight: 0,
            stats: IoSchedStats::default(),
        }
    }

    pub fn submit(&mut self, req: IoRequest) {
        self.stats.requests_submitted += 1;
        match req.direction {
            IoDirection::Read => self.read_queue.push_back(req),
            IoDirection::Write => self.write_queue.push_back(req),
            IoDirection::Discard => self.discard_queue.push_back(req),
            IoDirection::Flush => self.write_queue.push_back(req),
        }
    }

    pub fn dispatch(&mut self) -> Option<IoRequest> {
        // Priority: reads first (latency sensitive), then writes, then discards
        if self.read_inflight < self.read_depth {
            if let Some(req) = self.read_queue.pop_front() {
                self.read_inflight += 1;
                self.stats.reads_dispatched += 1;
                self.stats.read_sectors += req.nr_sectors as u64;
                return Some(req);
            }
        }

        if self.write_inflight < self.write_depth {
            if let Some(req) = self.write_queue.pop_front() {
                self.write_inflight += 1;
                self.stats.writes_dispatched += 1;
                self.stats.write_sectors += req.nr_sectors as u64;
                return Some(req);
            }
        }

        if self.discard_inflight < self.discard_depth {
            if let Some(req) = self.discard_queue.pop_front() {
                self.discard_inflight += 1;
                return Some(req);
            }
        }

        None
    }

    /// Called when a request completes - adjusts queue depths
    pub fn complete(&mut self, req: &IoRequest, latency_us: u64) {
        self.stats.requests_completed += 1;

        match req.direction {
            IoDirection::Read => {
                self.read_inflight = self.read_inflight.saturating_sub(1);
                if latency_us > self.read_lat_target_us {
                    self.read_depth = (self.read_depth * 3 / 4).max(1);
                } else {
                    self.read_depth = (self.read_depth + 1).min(1024);
                }
            }
            IoDirection::Write | IoDirection::Flush => {
                self.write_inflight = self.write_inflight.saturating_sub(1);
                if latency_us > self.write_lat_target_us {
                    self.write_depth = (self.write_depth * 3 / 4).max(1);
                } else {
                    self.write_depth = (self.write_depth + 1).min(512);
                }
            }
            IoDirection::Discard => {
                self.discard_inflight = self.discard_inflight.saturating_sub(1);
            }
        }

        // Update latency stats
        if latency_us > self.stats.max_latency_us {
            self.stats.max_latency_us = latency_us;
        }
        let completed = self.stats.requests_completed;
        self.stats.avg_latency_us =
            (self.stats.avg_latency_us * (completed - 1) + latency_us) / completed;
    }

    pub fn is_empty(&self) -> bool {
        self.read_queue.is_empty() && self.write_queue.is_empty() && self.discard_queue.is_empty()
    }

    pub fn stats(&self) -> &IoSchedStats {
        &self.stats
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MQ-DEADLINE SCHEDULER
// ═══════════════════════════════════════════════════════════════════════

/// Multi-queue deadline scheduler (default for modern Linux)
pub struct MqDeadlineScheduler {
    /// Per hardware-queue schedulers
    hw_queues: Vec<DeadlineScheduler>,
    num_queues: usize,
    stats: IoSchedStats,
}

impl MqDeadlineScheduler {
    pub fn new(num_queues: usize) -> Self {
        let mut hw_queues = Vec::new();
        for _ in 0..num_queues {
            hw_queues.push(DeadlineScheduler::new());
        }
        Self {
            hw_queues,
            num_queues,
            stats: IoSchedStats::default(),
        }
    }

    pub fn submit(&mut self, req: IoRequest) {
        self.stats.requests_submitted += 1;
        // Hash PID to queue for locality
        let queue_idx = (req.pid as usize) % self.num_queues;
        self.hw_queues[queue_idx].submit(req);
    }

    pub fn dispatch(&mut self, queue_idx: usize) -> Option<IoRequest> {
        if queue_idx >= self.num_queues {
            return None;
        }
        let req = self.hw_queues[queue_idx].dispatch()?;
        match req.direction {
            IoDirection::Read => {
                self.stats.reads_dispatched += 1;
                self.stats.read_sectors += req.nr_sectors as u64;
            }
            IoDirection::Write => {
                self.stats.writes_dispatched += 1;
                self.stats.write_sectors += req.nr_sectors as u64;
            }
            _ => {}
        }
        Some(req)
    }

    pub fn is_empty(&self) -> bool {
        self.hw_queues.iter().all(|q| q.is_empty())
    }

    pub fn stats(&self) -> &IoSchedStats {
        &self.stats
    }
}

// ═══════════════════════════════════════════════════════════════════════
// I/O BANDWIDTH THROTTLE
// ═══════════════════════════════════════════════════════════════════════

/// Per-cgroup bandwidth limit
#[derive(Debug, Clone)]
pub struct BandwidthLimit {
    pub cgroup_id: u64,
    pub read_bps: u64, // bytes per second (0 = unlimited)
    pub write_bps: u64,
    pub read_iops: u64, // I/O operations per second (0 = unlimited)
    pub write_iops: u64,
    pub read_bytes_window: u64,
    pub write_bytes_window: u64,
    pub read_ops_window: u64,
    pub write_ops_window: u64,
    pub window_start: u64,
}

impl BandwidthLimit {
    pub fn new(cgroup_id: u64) -> Self {
        Self {
            cgroup_id,
            read_bps: 0,
            write_bps: 0,
            read_iops: 0,
            write_iops: 0,
            read_bytes_window: 0,
            write_bytes_window: 0,
            read_ops_window: 0,
            write_ops_window: 0,
            window_start: rdtsc(),
        }
    }

    /// Check if a request should be throttled
    pub fn should_throttle(&self, req: &IoRequest) -> bool {
        match req.direction {
            IoDirection::Read => {
                if self.read_bps > 0 && self.read_bytes_window >= self.read_bps {
                    return true;
                }
                if self.read_iops > 0 && self.read_ops_window >= self.read_iops {
                    return true;
                }
            }
            IoDirection::Write => {
                if self.write_bps > 0 && self.write_bytes_window >= self.write_bps {
                    return true;
                }
                if self.write_iops > 0 && self.write_ops_window >= self.write_iops {
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    /// Account I/O
    pub fn account(&mut self, req: &IoRequest) {
        let bytes = req.nr_sectors as u64 * 512;
        match req.direction {
            IoDirection::Read => {
                self.read_bytes_window += bytes;
                self.read_ops_window += 1;
            }
            IoDirection::Write => {
                self.write_bytes_window += bytes;
                self.write_ops_window += 1;
            }
            _ => {}
        }
    }

    /// Reset window (call once per second)
    pub fn reset_window(&mut self) {
        self.read_bytes_window = 0;
        self.write_bytes_window = 0;
        self.read_ops_window = 0;
        self.write_ops_window = 0;
        self.window_start = rdtsc();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL SCHEDULER MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Currently active scheduler per block device
struct DeviceScheduler {
    device_name: String,
    sched_type: SchedulerType,
    noop: Option<NoopScheduler>,
    deadline: Option<DeadlineScheduler>,
    cfq: Option<CfqScheduler>,
    bfq: Option<BfqScheduler>,
    kyber: Option<KyberScheduler>,
    mq_deadline: Option<MqDeadlineScheduler>,
    bandwidth_limits: BTreeMap<u64, BandwidthLimit>,
}

impl DeviceScheduler {
    fn new(device_name: &str, sched_type: SchedulerType) -> Self {
        let mut sched = Self {
            device_name: String::from(device_name),
            sched_type,
            noop: None,
            deadline: None,
            cfq: None,
            bfq: None,
            kyber: None,
            mq_deadline: None,
            bandwidth_limits: BTreeMap::new(),
        };

        match sched_type {
            SchedulerType::None => sched.noop = Some(NoopScheduler::new()),
            SchedulerType::Deadline => sched.deadline = Some(DeadlineScheduler::new()),
            SchedulerType::Cfq => sched.cfq = Some(CfqScheduler::new()),
            SchedulerType::Bfq => sched.bfq = Some(BfqScheduler::new()),
            SchedulerType::Kyber => sched.kyber = Some(KyberScheduler::new()),
            SchedulerType::MqDeadline => sched.mq_deadline = Some(MqDeadlineScheduler::new(4)),
        }

        sched
    }

    fn submit(&mut self, req: IoRequest) {
        match self.sched_type {
            SchedulerType::None => self.noop.as_mut().unwrap().submit(req),
            SchedulerType::Deadline => self.deadline.as_mut().unwrap().submit(req),
            SchedulerType::Cfq => self.cfq.as_mut().unwrap().submit(req),
            SchedulerType::Bfq => self.bfq.as_mut().unwrap().submit(req),
            SchedulerType::Kyber => self.kyber.as_mut().unwrap().submit(req),
            SchedulerType::MqDeadline => self.mq_deadline.as_mut().unwrap().submit(req),
        }
    }

    fn dispatch(&mut self) -> Option<IoRequest> {
        match self.sched_type {
            SchedulerType::None => self.noop.as_mut()?.dispatch(),
            SchedulerType::Deadline => self.deadline.as_mut()?.dispatch(),
            SchedulerType::Cfq => self.cfq.as_mut()?.dispatch(),
            SchedulerType::Bfq => self.bfq.as_mut()?.dispatch(),
            SchedulerType::Kyber => self.kyber.as_mut()?.dispatch(),
            SchedulerType::MqDeadline => self.mq_deadline.as_mut()?.dispatch(0),
        }
    }
}

lazy_static::lazy_static! {
    static ref SCHEDULERS: Mutex<BTreeMap<String, DeviceScheduler>> = Mutex::new(BTreeMap::new());
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Register a block device with a scheduler
pub fn register_device(device_name: &str, sched_type: SchedulerType) {
    let sched = DeviceScheduler::new(device_name, sched_type);
    SCHEDULERS.lock().insert(String::from(device_name), sched);
    serial_println!(
        "[IO_SCHED] Registered device '{}' with {:?} scheduler",
        device_name,
        sched_type
    );
}

/// Change scheduler for a device
pub fn set_scheduler(device_name: &str, sched_type: SchedulerType) {
    let mut scheds = SCHEDULERS.lock();
    if let Some(old) = scheds.remove(device_name) {
        let new_sched = DeviceScheduler::new(device_name, sched_type);
        scheds.insert(String::from(device_name), new_sched);
        serial_println!(
            "[IO_SCHED] Changed scheduler for '{}': {:?} -> {:?}",
            device_name,
            old.sched_type,
            sched_type
        );
    }
}

/// Submit an I/O request
pub fn submit_io(device_name: &str, req: IoRequest) {
    let mut scheds = SCHEDULERS.lock();
    if let Some(sched) = scheds.get_mut(device_name) {
        sched.submit(req);
    }
}

/// Dispatch next request for a device
pub fn dispatch_io(device_name: &str) -> Option<IoRequest> {
    let mut scheds = SCHEDULERS.lock();
    scheds.get_mut(device_name)?.dispatch()
}

/// List available schedulers
pub fn available_schedulers() -> Vec<&'static str> {
    vec!["none", "deadline", "cfq", "bfq", "mq-deadline", "kyber"]
}

/// Get current scheduler for device
pub fn current_scheduler(device_name: &str) -> Option<SchedulerType> {
    let scheds = SCHEDULERS.lock();
    scheds.get(device_name).map(|s| s.sched_type)
}

/// Procfs info
pub fn proc_io_sched_info() -> String {
    let scheds = SCHEDULERS.lock();
    let mut info = String::from("I/O Schedulers:\n");
    for (name, sched) in scheds.iter() {
        info.push_str(&alloc::format!("  {} -> {:?}\n", name, sched.sched_type));
    }
    if scheds.is_empty() {
        info.push_str("  (no devices registered)\n");
    }
    info
}

fn rdtsc() -> u64 {
    #[cfg(target_arch = "x86_64")]
    return crate::arch_compat::read_tsc();
    #[cfg(not(target_arch = "x86_64"))]
    return 0;
}

/// Initialize I/O scheduler subsystem
pub fn init() {
    if INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    INITIALIZED.store(true, Ordering::Relaxed);

    // Register default virtual devices
    register_device("vda", SchedulerType::MqDeadline);
    register_device("vdb", SchedulerType::Kyber);

    serial_println!(
        "[KnoxOS] I/O scheduler subsystem initialized (none, deadline, cfq, bfq, mq-deadline, kyber)"
    );
}
