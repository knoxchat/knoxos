use alloc::collections::VecDeque;
/// Syslog - Kernel logging facility
/// Compatible with Linux syslog interface
/// Provides ring buffer logging with severity levels
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Syslog severity levels (RFC 5424)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Severity {
    Emergency = 0, // System is unusable
    Alert = 1,     // Action must be taken immediately
    Critical = 2,  // Critical conditions
    Error = 3,     // Error conditions
    Warning = 4,   // Warning conditions
    Notice = 5,    // Normal but significant condition
    Info = 6,      // Informational messages
    Debug = 7,     // Debug-level messages
}

/// Syslog facility codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Facility {
    Kern = 0,   // Kernel messages
    User = 1,   // User-level messages
    Mail = 2,   // Mail system
    Daemon = 3, // System daemons
    Auth = 4,   // Security/authorization
    Syslog = 5, // Syslog internal
    Lpr = 6,    // Line printer
    News = 7,   // Network news
    Cron = 9,   // Clock daemon
    Local0 = 16,
    Local1 = 17,
    Local2 = 18,
    Local3 = 19,
    Local4 = 20,
    Local5 = 21,
    Local6 = 22,
    Local7 = 23,
}

/// A log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: u64,
    pub severity: Severity,
    pub facility: Facility,
    pub message: String,
    pub pid: u32,
}

/// Maximum log ring buffer size
const MAX_LOG_ENTRIES: usize = 4096;

/// Minimum severity level to actually log
static LOG_LEVEL: Mutex<Severity> = Mutex::new(Severity::Debug);

/// Kernel log ring buffer (like Linux dmesg)
lazy_static::lazy_static! {
    static ref KERNEL_LOG: Mutex<VecDeque<LogEntry>> = Mutex::new(VecDeque::with_capacity(MAX_LOG_ENTRIES));
}

/// Log a kernel message
pub fn log(severity: Severity, facility: Facility, message: &str) {
    let min_level = *LOG_LEVEL.lock();
    if severity > min_level {
        return;
    }

    let entry = LogEntry {
        timestamp: crate::rtc::uptime_seconds(),
        severity,
        facility,
        message: String::from(message),
        pid: 0,
    };

    let mut log = KERNEL_LOG.lock();
    if log.len() >= MAX_LOG_ENTRIES {
        log.pop_front();
    }
    log.push_back(entry);
}

/// Log with formatting (like printk)
#[macro_export]
macro_rules! klog {
    ($severity:expr, $($arg:tt)*) => {
        $crate::syslog::log(
            $severity,
            $crate::syslog::Facility::Kern,
            &alloc::format!($($arg)*)
        )
    };
}

/// Convenience macros
#[macro_export]
macro_rules! klog_info {
    ($($arg:tt)*) => {
        $crate::klog!($crate::syslog::Severity::Info, $($arg)*)
    };
}

#[macro_export]
macro_rules! klog_warn {
    ($($arg:tt)*) => {
        $crate::klog!($crate::syslog::Severity::Warning, $($arg)*)
    };
}

#[macro_export]
macro_rules! klog_err {
    ($($arg:tt)*) => {
        $crate::klog!($crate::syslog::Severity::Error, $($arg)*)
    };
}

#[macro_export]
macro_rules! klog_debug {
    ($($arg:tt)*) => {
        $crate::klog!($crate::syslog::Severity::Debug, $($arg)*)
    };
}

/// Read the kernel log (like dmesg)
pub fn dmesg() -> Vec<LogEntry> {
    KERNEL_LOG.lock().iter().cloned().collect()
}

/// Read the last N log entries
pub fn dmesg_tail(count: usize) -> Vec<LogEntry> {
    let log = KERNEL_LOG.lock();
    let skip = if log.len() > count {
        log.len() - count
    } else {
        0
    };
    log.iter().skip(skip).cloned().collect()
}

/// Clear the kernel log
pub fn clear() {
    KERNEL_LOG.lock().clear();
}

/// Set the minimum log level
pub fn set_level(level: Severity) {
    *LOG_LEVEL.lock() = level;
}

/// Get the current log level
pub fn get_level() -> Severity {
    *LOG_LEVEL.lock()
}

/// Format a log entry for display
pub fn format_entry(entry: &LogEntry) -> String {
    let severity_str = match entry.severity {
        Severity::Emergency => "EMERG",
        Severity::Alert => "ALERT",
        Severity::Critical => "CRIT ",
        Severity::Error => "ERROR",
        Severity::Warning => "WARN ",
        Severity::Notice => "NOTE ",
        Severity::Info => "INFO ",
        Severity::Debug => "DEBUG",
    };

    alloc::format!(
        "[{:>8}.000000] {} {}",
        entry.timestamp,
        severity_str,
        entry.message
    )
}

/// Get log statistics
pub fn stats() -> (usize, usize) {
    let log = KERNEL_LOG.lock();
    (log.len(), MAX_LOG_ENTRIES)
}

/// Initialize syslog
pub fn init() {
    log(Severity::Info, Facility::Kern, "KnoxOS syslog initialized");
    log(Severity::Info, Facility::Kern, "Ring buffer: 4096 entries");
    crate::serial_println!(
        "[KnoxOS] Syslog initialized (ring buffer: {} entries)",
        MAX_LOG_ENTRIES
    );
}
