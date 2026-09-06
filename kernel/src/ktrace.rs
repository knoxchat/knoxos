/// Kernel Tracing Infrastructure (ftrace/kprobes)
///
/// Provides kernel-level tracing and profiling:
/// - Function entry/exit tracing
/// - Kprobes (dynamic probes at any kernel address)
/// - Tracepoints (static instrumentation points)
/// - Event filtering and per-CPU ring buffers
/// - Trace output formatting
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Trace Event Types ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TraceEventType {
    FunctionEntry,
    FunctionExit,
    Kprobe,
    Kretprobe,
    Tracepoint,
    SyscallEntry,
    SyscallExit,
    Irq,
    SoftIrq,
    SchedSwitch,
    SchedWakeup,
    MemAlloc,
    MemFree,
    BlockIo,
    NetRx,
    NetTx,
    PageFault,
    Custom,
}

impl TraceEventType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::FunctionEntry => "func_enter",
            Self::FunctionExit => "func_exit",
            Self::Kprobe => "kprobe",
            Self::Kretprobe => "kretprobe",
            Self::Tracepoint => "tracepoint",
            Self::SyscallEntry => "sys_enter",
            Self::SyscallExit => "sys_exit",
            Self::Irq => "irq",
            Self::SoftIrq => "softirq",
            Self::SchedSwitch => "sched_switch",
            Self::SchedWakeup => "sched_wakeup",
            Self::MemAlloc => "mem_alloc",
            Self::MemFree => "mem_free",
            Self::BlockIo => "block_io",
            Self::NetRx => "net_rx",
            Self::NetTx => "net_tx",
            Self::PageFault => "page_fault",
            Self::Custom => "custom",
        }
    }
}

// ─── Trace Events ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TraceEvent {
    pub timestamp_ns: u64,
    pub cpu: u8,
    pub pid: u32,
    pub tid: u32,
    pub event_type: TraceEventType,
    pub name: String,
    pub args: [u64; 4],
    pub retval: i64,
    pub depth: u16,
}

impl TraceEvent {
    pub fn format(&self) -> String {
        match self.event_type {
            TraceEventType::FunctionEntry => {
                alloc::format!(
                    "[{:>12}] CPU{} PID{:>5} {} {}(0x{:x}, 0x{:x})",
                    self.timestamp_ns / 1000,
                    self.cpu,
                    self.pid,
                    "  ".repeat(self.depth as usize).as_str(),
                    self.name,
                    self.args[0],
                    self.args[1]
                )
            }
            TraceEventType::FunctionExit => {
                alloc::format!(
                    "[{:>12}] CPU{} PID{:>5} {} {}() = {}",
                    self.timestamp_ns / 1000,
                    self.cpu,
                    self.pid,
                    "  ".repeat(self.depth as usize).as_str(),
                    self.name,
                    self.retval
                )
            }
            TraceEventType::SyscallEntry => {
                alloc::format!(
                    "[{:>12}] CPU{} PID{:>5} > syscall {}(0x{:x}, 0x{:x}, 0x{:x})",
                    self.timestamp_ns / 1000,
                    self.cpu,
                    self.pid,
                    self.name,
                    self.args[0],
                    self.args[1],
                    self.args[2]
                )
            }
            TraceEventType::SyscallExit => {
                alloc::format!(
                    "[{:>12}] CPU{} PID{:>5} < syscall {} = {}",
                    self.timestamp_ns / 1000,
                    self.cpu,
                    self.pid,
                    self.name,
                    self.retval
                )
            }
            TraceEventType::SchedSwitch => {
                alloc::format!(
                    "[{:>12}] CPU{} sched_switch: {} -> {}",
                    self.timestamp_ns / 1000,
                    self.cpu,
                    self.args[0],
                    self.args[1]
                )
            }
            _ => {
                alloc::format!(
                    "[{:>12}] CPU{} PID{:>5} {} {}",
                    self.timestamp_ns / 1000,
                    self.cpu,
                    self.pid,
                    self.event_type.name(),
                    self.name
                )
            }
        }
    }
}

// ─── Kprobe ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Kprobe {
    pub name: String,
    pub address: u64,
    pub enabled: bool,
    pub hit_count: u64,
    pub original_byte: u8,
}

// ─── Tracepoint ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Tracepoint {
    pub name: String,
    pub category: String,
    pub enabled: bool,
    pub hit_count: u64,
    pub format: String,
}

// ─── Per-CPU Ring Buffer ────────────────────────────────────────────

const RING_BUFFER_SIZE: usize = 8192;
const MAX_CPUS: usize = 64;

#[derive(Debug)]
pub struct CpuTraceBuffer {
    pub events: Vec<TraceEvent>,
    pub head: usize,
    pub tail: usize,
    pub overruns: u64,
    pub capacity: usize,
}

impl CpuTraceBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: Vec::with_capacity(capacity),
            head: 0,
            tail: 0,
            overruns: 0,
            capacity,
        }
    }

    pub fn push(&mut self, event: TraceEvent) {
        if self.events.len() >= self.capacity {
            self.events.remove(0);
            self.overruns += 1;
        }
        self.events.push(event);
    }

    pub fn drain(&mut self) -> Vec<TraceEvent> {
        core::mem::take(&mut self.events)
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

// ─── Trace Filter ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TraceFilter {
    pub pid_filter: Option<u32>,
    pub cpu_filter: Option<u8>,
    pub event_types: Vec<TraceEventType>,
    pub function_filter: Vec<String>,
    pub min_duration_ns: u64,
}

impl TraceFilter {
    pub fn new() -> Self {
        Self {
            pid_filter: None,
            cpu_filter: None,
            event_types: Vec::new(),
            function_filter: Vec::new(),
            min_duration_ns: 0,
        }
    }

    pub fn matches(&self, event: &TraceEvent) -> bool {
        if let Some(pid) = self.pid_filter {
            if event.pid != pid {
                return false;
            }
        }
        if let Some(cpu) = self.cpu_filter {
            if event.cpu != cpu {
                return false;
            }
        }
        if !self.event_types.is_empty() && !self.event_types.contains(&event.event_type) {
            return false;
        }
        if !self.function_filter.is_empty() {
            let matches_func = self
                .function_filter
                .iter()
                .any(|f| event.name.contains(f.as_str()));
            if !matches_func {
                return false;
            }
        }
        true
    }
}

// ─── Global State ───────────────────────────────────────────────────

static TRACING_ENABLED: AtomicBool = AtomicBool::new(false);
static EVENT_COUNTER: AtomicU64 = AtomicU64::new(0);

lazy_static::lazy_static! {
    static ref CPU_BUFFERS: Mutex<Vec<CpuTraceBuffer>> = {
        let mut buffers = Vec::new();
        for _ in 0..MAX_CPUS {
            buffers.push(CpuTraceBuffer::new(RING_BUFFER_SIZE));
        }
        Mutex::new(buffers)
    };
    static ref KPROBES: Mutex<BTreeMap<u64, Kprobe>> = Mutex::new(BTreeMap::new());
    static ref TRACEPOINTS: Mutex<BTreeMap<String, Tracepoint>> = Mutex::new(BTreeMap::new());
    static ref ACTIVE_FILTER: Mutex<TraceFilter> = Mutex::new(TraceFilter::new());
    static ref FUNCTION_GRAPH: Mutex<BTreeMap<u32, u16>> = Mutex::new(BTreeMap::new()); // pid → depth
}

// ─── Core Tracing Functions ────────────────────────────────────────

/// Record a trace event
pub fn trace_event(event: TraceEvent) {
    if !TRACING_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let filter = ACTIVE_FILTER.lock();
    if !filter.matches(&event) {
        return;
    }
    drop(filter);

    let cpu = event.cpu as usize;
    let mut buffers = CPU_BUFFERS.lock();
    if cpu < buffers.len() {
        buffers[cpu].push(event);
    }

    EVENT_COUNTER.fetch_add(1, Ordering::Relaxed);
}

/// Trace function entry
pub fn trace_function_entry(name: &str, pid: u32, cpu: u8, arg0: u64, arg1: u64) {
    let depth = {
        let mut graph = FUNCTION_GRAPH.lock();
        let d = graph.entry(pid).or_insert(0);
        let current = *d;
        *d += 1;
        current
    };

    trace_event(TraceEvent {
        timestamp_ns: crate::clock::monotonic_ns() as u64,
        cpu,
        pid,
        tid: pid,
        event_type: TraceEventType::FunctionEntry,
        name: String::from(name),
        args: [arg0, arg1, 0, 0],
        retval: 0,
        depth,
    });
}

/// Trace function exit
pub fn trace_function_exit(name: &str, pid: u32, cpu: u8, retval: i64) {
    let depth = {
        let mut graph = FUNCTION_GRAPH.lock();
        let d = graph.entry(pid).or_insert(0);
        if *d > 0 {
            *d -= 1;
        }
        *d
    };

    trace_event(TraceEvent {
        timestamp_ns: crate::clock::monotonic_ns() as u64,
        cpu,
        pid,
        tid: pid,
        event_type: TraceEventType::FunctionExit,
        name: String::from(name),
        args: [0; 4],
        retval,
        depth,
    });
}

/// Trace syscall entry
pub fn trace_syscall_enter(name: &str, pid: u32, cpu: u8, args: [u64; 4]) {
    trace_event(TraceEvent {
        timestamp_ns: crate::clock::monotonic_ns() as u64,
        cpu,
        pid,
        tid: pid,
        event_type: TraceEventType::SyscallEntry,
        name: String::from(name),
        args,
        retval: 0,
        depth: 0,
    });
}

/// Trace syscall exit
pub fn trace_syscall_exit(name: &str, pid: u32, cpu: u8, retval: i64) {
    trace_event(TraceEvent {
        timestamp_ns: crate::clock::monotonic_ns() as u64,
        cpu,
        pid,
        tid: pid,
        event_type: TraceEventType::SyscallExit,
        name: String::from(name),
        args: [0; 4],
        retval,
        depth: 0,
    });
}

/// Trace scheduler context switch
pub fn trace_sched_switch(from_pid: u32, to_pid: u32, cpu: u8) {
    trace_event(TraceEvent {
        timestamp_ns: crate::clock::monotonic_ns() as u64,
        cpu,
        pid: from_pid,
        tid: from_pid,
        event_type: TraceEventType::SchedSwitch,
        name: String::from("sched_switch"),
        args: [from_pid as u64, to_pid as u64, 0, 0],
        retval: 0,
        depth: 0,
    });
}

// ─── Kprobe Management ─────────────────────────────────────────────

/// Register a kprobe at a given address
pub fn register_kprobe(name: &str, address: u64) -> bool {
    let kprobe = Kprobe {
        name: String::from(name),
        address,
        enabled: true,
        hit_count: 0,
        original_byte: 0,
    };

    KPROBES.lock().insert(address, kprobe);
    serial_println!("[ktrace] Kprobe '{}' registered at 0x{:X}", name, address);
    true
}

/// Unregister a kprobe
pub fn unregister_kprobe(address: u64) -> bool {
    KPROBES.lock().remove(&address).is_some()
}

/// Handle kprobe hit
pub fn kprobe_hit(address: u64, pid: u32, cpu: u8) {
    let mut kprobes = KPROBES.lock();
    if let Some(kp) = kprobes.get_mut(&address) {
        kp.hit_count += 1;
        let name = kp.name.clone();
        drop(kprobes);

        trace_event(TraceEvent {
            timestamp_ns: crate::clock::monotonic_ns() as u64,
            cpu,
            pid,
            tid: pid,
            event_type: TraceEventType::Kprobe,
            name,
            args: [address, 0, 0, 0],
            retval: 0,
            depth: 0,
        });
    }
}

// ─── Tracepoint Management ─────────────────────────────────────────

/// Register a tracepoint
pub fn register_tracepoint(category: &str, name: &str, format: &str) {
    let key = alloc::format!("{}:{}", category, name);
    let tp = Tracepoint {
        name: String::from(name),
        category: String::from(category),
        enabled: false,
        hit_count: 0,
        format: String::from(format),
    };
    TRACEPOINTS.lock().insert(key, tp);
}

/// Enable/disable a tracepoint
pub fn set_tracepoint_enabled(category: &str, name: &str, enabled: bool) -> bool {
    let key = alloc::format!("{}:{}", category, name);
    if let Some(tp) = TRACEPOINTS.lock().get_mut(&key) {
        tp.enabled = enabled;
        true
    } else {
        false
    }
}

/// Fire a tracepoint
pub fn fire_tracepoint(category: &str, name: &str, pid: u32, cpu: u8, args: [u64; 4]) {
    let key = alloc::format!("{}:{}", category, name);
    let mut tps = TRACEPOINTS.lock();
    if let Some(tp) = tps.get_mut(&key) {
        if !tp.enabled {
            return;
        }
        tp.hit_count += 1;
        let tp_name = tp.name.clone();
        drop(tps);

        trace_event(TraceEvent {
            timestamp_ns: crate::clock::monotonic_ns() as u64,
            cpu,
            pid,
            tid: pid,
            event_type: TraceEventType::Tracepoint,
            name: tp_name,
            args,
            retval: 0,
            depth: 0,
        });
    }
}

// ─── Control ────────────────────────────────────────────────────────

/// Enable tracing
pub fn enable() {
    TRACING_ENABLED.store(true, Ordering::Relaxed);
    serial_println!("[ktrace] Tracing enabled");
}

/// Disable tracing
pub fn disable() {
    TRACING_ENABLED.store(false, Ordering::Relaxed);
    serial_println!("[ktrace] Tracing disabled");
}

/// Check if tracing is enabled
pub fn is_enabled() -> bool {
    TRACING_ENABLED.load(Ordering::Relaxed)
}

/// Set trace filter
pub fn set_filter(filter: TraceFilter) {
    *ACTIVE_FILTER.lock() = filter;
}

/// Clear all trace buffers
pub fn clear() {
    let mut buffers = CPU_BUFFERS.lock();
    for buf in buffers.iter_mut() {
        buf.events.clear();
    }
    EVENT_COUNTER.store(0, Ordering::Relaxed);
    serial_println!("[ktrace] Trace buffers cleared");
}

/// Read trace output (all CPUs, sorted by timestamp)
pub fn read_trace(limit: usize) -> Vec<TraceEvent> {
    let mut all_events = Vec::new();
    let mut buffers = CPU_BUFFERS.lock();

    for buf in buffers.iter_mut() {
        all_events.extend(buf.events.iter().cloned());
    }

    all_events.sort_by_key(|e| e.timestamp_ns);

    if all_events.len() > limit {
        all_events.truncate(limit);
    }

    all_events
}

/// Get trace buffer statistics
pub fn buffer_stats() -> Vec<(usize, usize, u64)> {
    let buffers = CPU_BUFFERS.lock();
    buffers
        .iter()
        .enumerate()
        .filter(|(_, b)| !b.is_empty() || b.overruns > 0)
        .map(|(cpu, b)| (cpu, b.len(), b.overruns))
        .collect()
}

/// Total events recorded
pub fn total_events() -> u64 {
    EVENT_COUNTER.load(Ordering::Relaxed)
}

/// Get kprobe count
pub fn kprobe_count() -> usize {
    KPROBES.lock().len()
}

/// Get tracepoint count
pub fn tracepoint_count() -> usize {
    TRACEPOINTS.lock().len()
}

// ─── Built-in Tracepoints ──────────────────────────────────────────

fn register_builtin_tracepoints() {
    register_tracepoint("sched", "sched_switch", "prev_pid=%lu next_pid=%lu");
    register_tracepoint("sched", "sched_wakeup", "pid=%lu target_cpu=%lu");
    register_tracepoint(
        "sched",
        "sched_process_fork",
        "parent_pid=%lu child_pid=%lu",
    );
    register_tracepoint("sched", "sched_process_exit", "pid=%lu");
    register_tracepoint("syscalls", "sys_enter", "nr=%lu");
    register_tracepoint("syscalls", "sys_exit", "nr=%lu ret=%ld");
    register_tracepoint("kmem", "kmalloc", "ptr=%lu bytes=%lu");
    register_tracepoint("kmem", "kfree", "ptr=%lu");
    register_tracepoint("irq", "irq_handler_entry", "irq=%lu");
    register_tracepoint("irq", "irq_handler_exit", "irq=%lu ret=%lu");
    register_tracepoint("block", "block_rq_issue", "dev=%lu sector=%lu");
    register_tracepoint("block", "block_rq_complete", "dev=%lu sector=%lu");
    register_tracepoint("net", "net_dev_xmit", "len=%lu");
    register_tracepoint("net", "netif_receive_skb", "len=%lu");
    register_tracepoint("page", "page_fault_user", "addr=%lu");
    register_tracepoint("page", "page_fault_kernel", "addr=%lu");
    register_tracepoint("signal", "signal_deliver", "sig=%lu pid=%lu");
    register_tracepoint("timer", "timer_expire", "timer=%lu");
}

// ─── Init ───────────────────────────────────────────────────────────

pub fn init() {
    register_builtin_tracepoints();
    serial_println!("[KnoxOS] Kernel tracing infrastructure initialized");
    serial_println!("[KnoxOS]   {} built-in tracepoints", tracepoint_count());
    serial_println!(
        "[KnoxOS]   Per-CPU ring buffers: {} entries × {} CPUs",
        RING_BUFFER_SIZE,
        MAX_CPUS
    );
    serial_println!("[KnoxOS]   Supports: ftrace, kprobes, tracepoints");
}
