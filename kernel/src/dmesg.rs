/// Kernel Ring Buffer (dmesg) — Linux-compatible kernel log
///
/// Implements:
///   - Fixed-size ring buffer for kernel messages
///   - Syslog priority levels (KERN_EMERG..KERN_DEBUG)
///   - Timestamped entries
///   - dmesg command output format
///   - /dev/kmsg read interface
///   - syslog(2) syscall support (SYSLOG_ACTION_READ, etc.)
use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Maximum number of log entries
const RING_BUFFER_SIZE: usize = 8192;

/// Maximum message length
const MAX_MSG_LEN: usize = 512;

/// Early boot log buffer size (pre-heap, static array)
const EARLY_LOG_CAPACITY: usize = 64;
const EARLY_MSG_LEN: usize = 128;

/// Syslog priority levels (matches Linux <linux/kern_levels.h>)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum LogLevel {
    Emergency = 0, // System is unusable
    Alert = 1,     // Action must be taken immediately
    Critical = 2,  // Critical conditions
    Error = 3,     // Error conditions
    Warning = 4,   // Warning conditions
    Notice = 5,    // Normal but significant condition
    Info = 6,      // Informational
    Debug = 7,     // Debug-level messages
}

impl LogLevel {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Emergency,
            1 => Self::Alert,
            2 => Self::Critical,
            3 => Self::Error,
            4 => Self::Warning,
            5 => Self::Notice,
            6 => Self::Info,
            7 => Self::Debug,
            _ => Self::Info,
        }
    }

    pub fn prefix(&self) -> &'static str {
        match self {
            Self::Emergency => "emerg",
            Self::Alert => "alert",
            Self::Critical => "crit",
            Self::Error => "err",
            Self::Warning => "warn",
            Self::Notice => "notice",
            Self::Info => "info",
            Self::Debug => "debug",
        }
    }
}

/// A single kernel log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Sequence number
    pub seq: u64,
    /// Timestamp in microseconds since boot
    pub timestamp_usec: u64,
    /// Log level
    pub level: LogLevel,
    /// Facility (kern, user, daemon, etc.)
    pub facility: &'static str,
    /// Message text
    pub message: String,
}

/// Global sequence counter
static LOG_SEQ: AtomicU64 = AtomicU64::new(0);

// ═══════════════════════════════════════════════════════════════════════
// EARLY BOOT LOG BUFFER (pre-heap, static memory)
// ═══════════════════════════════════════════════════════════════════════

use core::sync::atomic::AtomicUsize;

/// Static early boot log — fixed-size array usable before heap is initialized
struct EarlyLogEntry {
    level: u8,
    len: u8,
    msg: [u8; EARLY_MSG_LEN],
}

impl EarlyLogEntry {
    const fn empty() -> Self {
        Self {
            level: 6, // Info
            len: 0,
            msg: [0u8; EARLY_MSG_LEN],
        }
    }
}

/// Static early log buffer (no heap required)
static mut EARLY_LOG: [EarlyLogEntry; EARLY_LOG_CAPACITY] = {
    const EMPTY: EarlyLogEntry = EarlyLogEntry::empty();
    [EMPTY; EARLY_LOG_CAPACITY]
};
static EARLY_LOG_COUNT: AtomicUsize = AtomicUsize::new(0);
static EARLY_LOG_FLUSHED: AtomicBool = AtomicBool::new(false);

use core::sync::atomic::AtomicBool;

/// Log a message during early boot (before heap is available).
/// Uses a static fixed-size ring — safe to call before allocator init.
pub fn early_klog(level: LogLevel, message: &str) {
    let idx = EARLY_LOG_COUNT.fetch_add(1, Ordering::Relaxed);
    if idx >= EARLY_LOG_CAPACITY {
        return; // buffer full, discard
    }
    let bytes = message.as_bytes();
    let copy_len = bytes.len().min(EARLY_MSG_LEN);
    // Safety: single writer per index (atomic fetch_add ensures unique slot)
    unsafe {
        EARLY_LOG[idx].level = level as u8;
        EARLY_LOG[idx].msg[..copy_len].copy_from_slice(&bytes[..copy_len]);
        EARLY_LOG[idx].len = copy_len as u8;
    }
}

/// Flush early boot log entries into the main ring buffer.
/// Called once after heap is initialized.
pub fn flush_early_log() {
    if EARLY_LOG_FLUSHED.swap(true, Ordering::SeqCst) {
        return; // already flushed
    }
    let count = EARLY_LOG_COUNT
        .load(Ordering::Relaxed)
        .min(EARLY_LOG_CAPACITY);
    for i in 0..count {
        let entry = unsafe { &EARLY_LOG[i] };
        let msg = core::str::from_utf8(&entry.msg[..entry.len as usize]).unwrap_or("<?>");
        klog(LogLevel::from_u8(entry.level), "kern", msg);
    }
    if count > 0 {
        serial_println!(
            "[dmesg] Flushed {} early boot messages into ring buffer",
            count
        );
    }
}

lazy_static::lazy_static! {
    static ref RING_BUFFER: Mutex<VecDeque<LogEntry>> = Mutex::new(VecDeque::new());
}

/// Initialize the kernel ring buffer
pub fn init() {
    // Log the first boot message
    klog(
        LogLevel::Info,
        "kern",
        "KnoxOS kernel ring buffer initialized",
    );
    klog(
        LogLevel::Info,
        "kern",
        "KnoxOS version 0.1.0 (rustc nightly)",
    );
    klog(LogLevel::Info, "kern", "Command line: console=ttyS0");
    serial_println!("[KnoxOS] Kernel ring buffer (dmesg) initialized");
}

/// Log a kernel message
pub fn klog(level: LogLevel, facility: &'static str, message: &str) {
    let seq = LOG_SEQ.fetch_add(1, Ordering::Relaxed);
    let ticks = crate::interrupts::get_ticks();
    // Convert ticks to microseconds (~55ms per tick for PIT at ~18.2 Hz)
    let timestamp_usec = ticks.wrapping_mul(54945);

    let mut msg = String::from(message);
    if msg.len() > MAX_MSG_LEN {
        msg.truncate(MAX_MSG_LEN);
    }

    let entry = LogEntry {
        seq,
        timestamp_usec,
        level,
        facility,
        message: msg,
    };

    let mut buf = RING_BUFFER.lock();
    if buf.len() >= RING_BUFFER_SIZE {
        buf.pop_front();
    }
    buf.push_back(entry);
}

/// Convenience macros for different log levels
pub fn kern_emerg(msg: &str) {
    klog(LogLevel::Emergency, "kern", msg);
}
pub fn kern_alert(msg: &str) {
    klog(LogLevel::Alert, "kern", msg);
}
pub fn kern_crit(msg: &str) {
    klog(LogLevel::Critical, "kern", msg);
}
pub fn kern_err(msg: &str) {
    klog(LogLevel::Error, "kern", msg);
}
pub fn kern_warning(msg: &str) {
    klog(LogLevel::Warning, "kern", msg);
}
pub fn kern_notice(msg: &str) {
    klog(LogLevel::Notice, "kern", msg);
}
pub fn kern_info(msg: &str) {
    klog(LogLevel::Info, "kern", msg);
}
pub fn kern_debug(msg: &str) {
    klog(LogLevel::Debug, "kern", msg);
}

/// Read all log entries (for dmesg command)
pub fn read_all() -> Vec<LogEntry> {
    let buf = RING_BUFFER.lock();
    buf.iter().cloned().collect()
}

/// Read log entries since a sequence number
pub fn read_since(since_seq: u64) -> Vec<LogEntry> {
    let buf = RING_BUFFER.lock();
    buf.iter().filter(|e| e.seq > since_seq).cloned().collect()
}

/// Read entries matching a minimum log level
pub fn read_level(min_level: LogLevel) -> Vec<LogEntry> {
    let buf = RING_BUFFER.lock();
    buf.iter()
        .filter(|e| (e.level as u8) <= (min_level as u8))
        .cloned()
        .collect()
}

/// Clear the ring buffer
pub fn clear() {
    let mut buf = RING_BUFFER.lock();
    buf.clear();
    klog(LogLevel::Info, "kern", "Ring buffer cleared");
}

/// Get current ring buffer size
pub fn size() -> usize {
    RING_BUFFER.lock().len()
}

/// Format dmesg output (matches Linux dmesg format)
pub fn format_dmesg() -> String {
    let entries = read_all();
    let mut output = String::new();
    for entry in &entries {
        let secs = entry.timestamp_usec / 1_000_000;
        let usecs = entry.timestamp_usec % 1_000_000;
        output.push_str(&format!("[{:>5}.{:06}] {}\n", secs, usecs, entry.message,));
    }
    output
}

/// Format dmesg with level prefix (dmesg --level=...)
pub fn format_dmesg_with_level() -> String {
    let entries = read_all();
    let mut output = String::new();
    for entry in &entries {
        let secs = entry.timestamp_usec / 1_000_000;
        let usecs = entry.timestamp_usec % 1_000_000;
        output.push_str(&format!(
            "<{}>[{:>5}.{:06}] {}: {}\n",
            entry.level as u8, secs, usecs, entry.facility, entry.message,
        ));
    }
    output
}

/// Read /dev/kmsg format (for userspace readers)
pub fn format_kmsg(since_seq: u64) -> String {
    let entries = read_since(since_seq);
    let mut output = String::new();
    for entry in &entries {
        // /dev/kmsg format: priority,sequence,timestamp,-;message
        let priority = entry.level as u8; // facility=kern (0)
        output.push_str(&format!(
            "{},{},{},{},-;{}\n",
            priority, entry.seq, entry.timestamp_usec, entry.facility, entry.message,
        ));
    }
    output
}

/// syslog(2) action constants
pub const SYSLOG_ACTION_CLOSE: i32 = 0;
pub const SYSLOG_ACTION_OPEN: i32 = 1;
pub const SYSLOG_ACTION_READ: i32 = 2;
pub const SYSLOG_ACTION_READ_ALL: i32 = 3;
pub const SYSLOG_ACTION_READ_CLEAR: i32 = 4;
pub const SYSLOG_ACTION_CLEAR: i32 = 5;
pub const SYSLOG_ACTION_CONSOLE_OFF: i32 = 6;
pub const SYSLOG_ACTION_CONSOLE_ON: i32 = 7;
pub const SYSLOG_ACTION_CONSOLE_LEVEL: i32 = 8;
pub const SYSLOG_ACTION_SIZE_UNREAD: i32 = 9;
pub const SYSLOG_ACTION_SIZE_BUFFER: i32 = 10;

/// Handle syslog(2) syscall
pub fn handle_syslog_syscall(action: i32, buf_ptr: u64, len: usize) -> i64 {
    match action {
        SYSLOG_ACTION_READ_ALL | SYSLOG_ACTION_READ => {
            let output = format_dmesg();
            let bytes = output.as_bytes();
            let copy_len = bytes.len().min(len);
            if buf_ptr != 0 && copy_len > 0 {
                let dest = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, copy_len) };
                dest.copy_from_slice(&bytes[..copy_len]);
            }
            copy_len as i64
        }
        SYSLOG_ACTION_READ_CLEAR => {
            let output = format_dmesg();
            let bytes = output.as_bytes();
            let copy_len = bytes.len().min(len);
            if buf_ptr != 0 && copy_len > 0 {
                let dest = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, copy_len) };
                dest.copy_from_slice(&bytes[..copy_len]);
            }
            clear();
            copy_len as i64
        }
        SYSLOG_ACTION_CLEAR => {
            clear();
            0
        }
        SYSLOG_ACTION_SIZE_UNREAD => {
            let buf = RING_BUFFER.lock();
            buf.iter().map(|e| e.message.len() + 32).sum::<usize>() as i64
        }
        SYSLOG_ACTION_SIZE_BUFFER => RING_BUFFER_SIZE as i64,
        SYSLOG_ACTION_CONSOLE_OFF
        | SYSLOG_ACTION_CONSOLE_ON
        | SYSLOG_ACTION_CONSOLE_LEVEL
        | SYSLOG_ACTION_CLOSE
        | SYSLOG_ACTION_OPEN => 0,
        _ => -22, // -EINVAL
    }
}
