/// System Call Tracing — strace-compatible syscall tracing for debugging
///
/// Provides kernel-side tracing infrastructure that logs every syscall
/// invocation with arguments and return values, similar to Linux strace.
///
/// Features:
/// - Per-PID trace enable/disable
/// - Syscall argument decoding (paths, flags, modes)
/// - Return value logging
/// - Timing information (entry/exit timestamps)
/// - Trace buffer with configurable depth
/// - Filter by syscall number or category
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Maximum trace entries per process
const MAX_TRACE_ENTRIES: usize = 4096;

/// Global tracing enable flag
static TRACING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Trace sequence counter
static TRACE_SEQ: AtomicU64 = AtomicU64::new(0);

/// A single syscall trace entry
#[derive(Debug, Clone)]
pub struct TraceEntry {
    /// Sequence number
    pub seq: u64,
    /// Process ID
    pub pid: u32,
    /// Thread ID
    pub tid: u32,
    /// Syscall number
    pub syscall_nr: u64,
    /// Syscall name (decoded)
    pub name: String,
    /// Arguments (decoded to strings)
    pub args: Vec<String>,
    /// Return value
    pub ret: i64,
    /// Entry tick
    pub entry_tick: u64,
    /// Exit tick
    pub exit_tick: u64,
    /// Whether the call was successful
    pub success: bool,
}

/// Per-process trace state
#[derive(Debug)]
pub struct ProcessTrace {
    pub enabled: bool,
    pub entries: Vec<TraceEntry>,
    pub filter: TraceFilter,
    pub total_calls: u64,
    pub failed_calls: u64,
}

/// Trace filter configuration
#[derive(Debug, Clone)]
pub struct TraceFilter {
    /// If non-empty, only trace these syscall numbers
    pub include_syscalls: Vec<u64>,
    /// Exclude these syscall numbers from trace
    pub exclude_syscalls: Vec<u64>,
    /// Only trace calls that fail
    pub errors_only: bool,
    /// Trace signal delivery
    pub trace_signals: bool,
}

impl Default for TraceFilter {
    fn default() -> Self {
        Self {
            include_syscalls: Vec::new(),
            exclude_syscalls: Vec::new(),
            errors_only: false,
            trace_signals: true,
        }
    }
}

impl ProcessTrace {
    fn new() -> Self {
        Self {
            enabled: true,
            entries: Vec::new(),
            filter: TraceFilter::default(),
            total_calls: 0,
            failed_calls: 0,
        }
    }
}

lazy_static::lazy_static! {
    static ref TRACES: Mutex<BTreeMap<u32, ProcessTrace>> = Mutex::new(BTreeMap::new());
}

/// Initialize strace subsystem
pub fn init() {
    serial_println!("[KnoxOS] Syscall tracing (strace) subsystem initialized");
}

/// Enable tracing for a process
pub fn trace_enable(pid: u32) {
    let mut traces = TRACES.lock();
    traces.entry(pid).or_insert_with(ProcessTrace::new).enabled = true;
    TRACING_ENABLED.store(true, Ordering::SeqCst);
    serial_println!("[strace] Enabled tracing for PID {}", pid);
}

/// Disable tracing for a process
pub fn trace_disable(pid: u32) {
    let mut traces = TRACES.lock();
    if let Some(t) = traces.get_mut(&pid) {
        t.enabled = false;
    }
    // Check if any process is still traced
    let any_active = traces.values().any(|t| t.enabled);
    if !any_active {
        TRACING_ENABLED.store(false, Ordering::SeqCst);
    }
}

/// Check if tracing is globally active (fast path for syscall hot path)
#[inline]
pub fn is_active() -> bool {
    TRACING_ENABLED.load(Ordering::Relaxed)
}

/// Decode a syscall number to its name
fn syscall_name(nr: u64) -> &'static str {
    match nr {
        0 => "read",
        1 => "write",
        2 => "open",
        3 => "close",
        4 => "stat",
        5 => "fstat",
        6 => "lstat",
        7 => "poll",
        8 => "lseek",
        9 => "mmap",
        10 => "mprotect",
        11 => "munmap",
        12 => "brk",
        13 => "rt_sigaction",
        14 => "rt_sigprocmask",
        16 => "ioctl",
        19 => "readv",
        20 => "writev",
        21 => "access",
        22 => "pipe",
        23 => "select",
        24 => "sched_yield",
        32 => "dup",
        33 => "dup2",
        35 => "nanosleep",
        39 => "getpid",
        41 => "socket",
        42 => "connect",
        43 => "accept",
        44 => "sendto",
        45 => "recvfrom",
        56 => "clone",
        57 => "fork",
        59 => "execve",
        60 => "exit",
        61 => "wait4",
        62 => "kill",
        63 => "uname",
        72 => "fcntl",
        73 => "flock",
        79 => "getcwd",
        80 => "chdir",
        83 => "mkdir",
        84 => "rmdir",
        87 => "unlink",
        96 => "gettimeofday",
        99 => "sysinfo",
        102 => "getuid",
        104 => "getgid",
        110 => "getppid",
        202 => "futex",
        217 => "getdents64",
        228 => "clock_gettime",
        231 => "exit_group",
        257 => "openat",
        302 => "prlimit64",
        318 => "getrandom",
        _ => "unknown",
    }
}

/// Record a syscall entry (before execution)
pub fn trace_entry(pid: u32, syscall_nr: u64, args: &[u64; 6]) {
    if !TRACING_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let mut traces = TRACES.lock();
    let trace = match traces.get_mut(&pid) {
        Some(t) if t.enabled => t,
        _ => return,
    };

    // Apply filters
    if !trace.filter.include_syscalls.is_empty()
        && !trace.filter.include_syscalls.contains(&syscall_nr)
    {
        return;
    }
    if trace.filter.exclude_syscalls.contains(&syscall_nr) {
        return;
    }

    let seq = TRACE_SEQ.fetch_add(1, Ordering::Relaxed);
    let name = String::from(syscall_name(syscall_nr));

    // Decode arguments based on syscall
    let decoded_args = decode_args(syscall_nr, args);

    let entry = TraceEntry {
        seq,
        pid,
        tid: pid, // simplified
        syscall_nr,
        name,
        args: decoded_args,
        ret: 0,
        entry_tick: crate::interrupts::get_ticks(),
        exit_tick: 0,
        success: true,
    };

    if trace.entries.len() >= MAX_TRACE_ENTRIES {
        trace.entries.remove(0);
    }
    trace.entries.push(entry);
    trace.total_calls += 1;
}

/// Record a syscall exit (after execution)
pub fn trace_exit(pid: u32, syscall_nr: u64, ret: i64) {
    if !TRACING_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let mut traces = TRACES.lock();
    let trace = match traces.get_mut(&pid) {
        Some(t) if t.enabled => t,
        _ => return,
    };

    // Find the last pending entry for this syscall
    if let Some(entry) = trace
        .entries
        .iter_mut()
        .rev()
        .find(|e| e.syscall_nr == syscall_nr && e.exit_tick == 0)
    {
        entry.ret = ret;
        entry.exit_tick = crate::interrupts::get_ticks();
        entry.success = ret >= 0;
        if !entry.success {
            trace.failed_calls += 1;
        }

        // Log to serial for real-time tracing
        serial_println!(
            "[strace] {}({}) = {}{}",
            entry.name,
            entry.args.join(", "),
            ret,
            if entry.success { "" } else { " (error)" },
        );
    }
}

/// Decode syscall arguments to human-readable strings
fn decode_args(syscall_nr: u64, args: &[u64; 6]) -> Vec<String> {
    match syscall_nr {
        // read(fd, buf, count)
        0 => alloc::vec![
            format!("{}", args[0]),
            format!("0x{:x}", args[1]),
            format!("{}", args[2]),
        ],
        // write(fd, buf, count)
        1 => alloc::vec![
            format!("{}", args[0]),
            format!("0x{:x}", args[1]),
            format!("{}", args[2]),
        ],
        // open(path, flags, mode)
        2 => {
            let path = unsafe { read_user_string_safe(args[0]) };
            alloc::vec![
                format!("\"{}\"", path),
                format_open_flags(args[1] as u32),
                format!("{:#o}", args[2]),
            ]
        }
        // close(fd)
        3 => alloc::vec![format!("{}", args[0])],
        // mmap(addr, length, prot, flags, fd, offset)
        9 => alloc::vec![
            format!("0x{:x}", args[0]),
            format!("{}", args[1]),
            format_mmap_prot(args[2] as u32),
            format_mmap_flags(args[3] as u32),
            format!("{}", args[4] as i64),
            format!("{}", args[5]),
        ],
        // Default: just show raw hex values
        _ => args.iter().map(|a| format!("0x{:x}", a)).collect(),
    }
}

fn format_open_flags(flags: u32) -> String {
    let mut parts = Vec::new();
    if flags & 0x3 == 0 {
        parts.push("O_RDONLY");
    }
    if flags & 0x1 != 0 {
        parts.push("O_WRONLY");
    }
    if flags & 0x2 != 0 {
        parts.push("O_RDWR");
    }
    if flags & 0x40 != 0 {
        parts.push("O_CREAT");
    }
    if flags & 0x80 != 0 {
        parts.push("O_EXCL");
    }
    if flags & 0x200 != 0 {
        parts.push("O_TRUNC");
    }
    if flags & 0x400 != 0 {
        parts.push("O_APPEND");
    }
    if flags & 0x800 != 0 {
        parts.push("O_NONBLOCK");
    }
    if parts.is_empty() {
        format!("{:#x}", flags)
    } else {
        parts.join("|")
    }
}

fn format_mmap_prot(prot: u32) -> String {
    let mut parts = Vec::new();
    if prot == 0 {
        return String::from("PROT_NONE");
    }
    if prot & 0x1 != 0 {
        parts.push("PROT_READ");
    }
    if prot & 0x2 != 0 {
        parts.push("PROT_WRITE");
    }
    if prot & 0x4 != 0 {
        parts.push("PROT_EXEC");
    }
    parts.join("|")
}

fn format_mmap_flags(flags: u32) -> String {
    let mut parts = Vec::new();
    if flags & 0x1 != 0 {
        parts.push("MAP_SHARED");
    }
    if flags & 0x2 != 0 {
        parts.push("MAP_PRIVATE");
    }
    if flags & 0x10 != 0 {
        parts.push("MAP_FIXED");
    }
    if flags & 0x20 != 0 {
        parts.push("MAP_ANONYMOUS");
    }
    if parts.is_empty() {
        format!("{:#x}", flags)
    } else {
        parts.join("|")
    }
}

/// Safe user string read (won't panic on bad pointers)
unsafe fn read_user_string_safe(ptr: u64) -> String {
    if ptr == 0 || ptr > 0x0000_7FFF_FFFF_FFFF {
        return String::from("(null)");
    }
    let base = ptr as *const u8;
    let mut len = 0usize;
    while len < 256 {
        let byte = core::ptr::read_volatile(base.add(len));
        if byte == 0 {
            break;
        }
        len += 1;
    }
    let bytes = core::slice::from_raw_parts(base, len);
    core::str::from_utf8(bytes)
        .unwrap_or("<invalid utf8>")
        .into()
}

/// Get trace entries for a process
pub fn get_trace(pid: u32) -> Vec<TraceEntry> {
    let traces = TRACES.lock();
    traces
        .get(&pid)
        .map(|t| t.entries.clone())
        .unwrap_or_default()
}

/// Get trace statistics for a process
pub fn get_stats(pid: u32) -> Option<(u64, u64)> {
    let traces = TRACES.lock();
    traces.get(&pid).map(|t| (t.total_calls, t.failed_calls))
}

/// Clear trace buffer for a process
pub fn clear_trace(pid: u32) {
    let mut traces = TRACES.lock();
    if let Some(t) = traces.get_mut(&pid) {
        t.entries.clear();
        t.total_calls = 0;
        t.failed_calls = 0;
    }
}

/// Set trace filter
pub fn set_filter(pid: u32, filter: TraceFilter) {
    let mut traces = TRACES.lock();
    if let Some(t) = traces.get_mut(&pid) {
        t.filter = filter;
    }
}

/// Format trace output like strace
pub fn format_trace(pid: u32) -> String {
    let traces = TRACES.lock();
    let trace = match traces.get(&pid) {
        Some(t) => t,
        None => return String::from("(no trace data)\n"),
    };

    let mut output = String::new();
    for entry in &trace.entries {
        let duration = entry.exit_tick.saturating_sub(entry.entry_tick);
        output.push_str(&format!(
            "[{:>6}] {}({}) = {} <{} ticks>\n",
            entry.seq,
            entry.name,
            entry.args.join(", "),
            entry.ret,
            duration,
        ));
    }
    if output.is_empty() {
        output.push_str("(no trace entries)\n");
    }
    output.push_str(&format!(
        "--- {} calls ({} errors) ---\n",
        trace.total_calls, trace.failed_calls
    ));
    output
}
