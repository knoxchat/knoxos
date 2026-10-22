/// Audit Logging — Linux-compatible security audit subsystem
/// Records security-relevant events for compliance and forensics
///
/// Implements a simplified version of Linux's audit framework:
///   - Syscall auditing (entry/exit logging)
///   - File access auditing
///   - Process lifecycle events
///   - Authentication events
///   - Security policy violations
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::process::Pid;
use crate::serial_println;

/// Audit event types (matching Linux audit message types)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum AuditEventType {
    /// Syscall entry/exit
    Syscall = 1300,
    /// File access
    Path = 1302,
    /// Process creation
    ProcCreate = 1309,
    /// Process exit
    ProcExit = 1310,
    /// User authentication
    UserAuth = 1100,
    /// User login
    UserLogin = 1112,
    /// User logout
    UserLogout = 1113,
    /// Capability use
    CapabilityUse = 1328,
    /// Security policy load
    PolicyLoad = 1403,
    /// Seccomp action
    Seccomp = 1326,
    /// SELinux AVC denial
    AvcDenial = 1400,
    /// Anomaly detection
    Anomaly = 1701,
    /// Configuration change
    ConfigChange = 1305,
    /// Filesystem mount
    Mount = 1315,
    /// Network connection
    NetConnect = 1316,
    /// Custom KnoxOS event
    Custom = 2000,
}

/// Audit event severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuditSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl core::fmt::Display for AuditSeverity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARN"),
            Self::Error => write!(f, "ERROR"),
            Self::Critical => write!(f, "CRIT"),
        }
    }
}

/// An audit log event
#[derive(Debug, Clone)]
pub struct AuditEvent {
    /// Unique event ID
    pub id: u64,
    /// Event type
    pub event_type: AuditEventType,
    /// Severity level
    pub severity: AuditSeverity,
    /// Timestamp (tick count)
    pub timestamp: u64,
    /// Process ID that triggered the event
    pub pid: Pid,
    /// User ID
    pub uid: u32,
    /// Event message
    pub message: String,
    /// Success/failure
    pub success: bool,
    /// Syscall number (if applicable)
    pub syscall_nr: Option<u64>,
    /// Extra key-value fields
    pub fields: Vec<(String, String)>,
}

/// Audit configuration
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// Whether auditing is enabled
    pub enabled: bool,
    /// Minimum severity to log
    pub min_severity: AuditSeverity,
    /// Whether to log syscalls
    pub log_syscalls: bool,
    /// Syscall numbers to audit (empty = audit all)
    pub syscall_filter: Vec<u64>,
    /// Whether to log file accesses
    pub log_file_access: bool,
    /// File paths to watch
    pub watch_paths: Vec<String>,
    /// Maximum events to retain in memory
    pub max_events: usize,
    /// Whether to log to serial console
    pub log_to_serial: bool,
    /// Whether to persist audit log to disk
    pub persist_to_disk: bool,
    /// Path for persistent audit log file
    pub log_file_path: String,
    /// Maximum log file size in bytes before rotation
    pub max_log_size: usize,
    /// Number of rotated log files to keep
    pub log_rotate_count: u8,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_severity: AuditSeverity::Warning,
            log_syscalls: false,
            syscall_filter: Vec::new(),
            log_file_access: false,
            watch_paths: Vec::new(),
            max_events: 8192,
            log_to_serial: true,
            persist_to_disk: true,
            log_file_path: String::from("/var/log/audit/audit.log"),
            max_log_size: 8 * 1024 * 1024, // 8 MB
            log_rotate_count: 4,
        }
    }
}

/// Audit subsystem state
struct AuditState {
    config: AuditConfig,
    events: Vec<AuditEvent>,
    next_id: u64,
    /// Per-process audit context (login UID tracking)
    login_uids: BTreeMap<Pid, u32>,
    /// Pending events to flush to disk
    pending_flush: Vec<String>,
    /// Current log file size estimate (bytes)
    current_log_size: usize,
}

lazy_static::lazy_static! {
    static ref AUDIT: Mutex<AuditState> = Mutex::new(AuditState {
        config: AuditConfig::default(),
        events: Vec::new(),
        next_id: 1,
        login_uids: BTreeMap::new(),
        pending_flush: Vec::new(),
        current_log_size: 0,
    });
}

/// Log an audit event
pub fn log_event(
    event_type: AuditEventType,
    severity: AuditSeverity,
    pid: Pid,
    uid: u32,
    success: bool,
    message: &str,
) -> u64 {
    let mut state = AUDIT.lock();

    if !state.config.enabled {
        return 0;
    }

    if severity < state.config.min_severity {
        return 0;
    }

    let id = state.next_id;
    state.next_id += 1;

    let event = AuditEvent {
        id,
        event_type,
        severity,
        timestamp: crate::interrupts::get_ticks(),
        pid,
        uid,
        message: String::from(message),
        success,
        syscall_nr: None,
        fields: Vec::new(),
    };

    if state.config.log_to_serial {
        serial_println!(
            "[audit] id={} type={:?} pid={} uid={} {} {}",
            id,
            event_type,
            pid,
            uid,
            if success { "success" } else { "failure" },
            message
        );
    }

    // Evict old events if at capacity
    if state.events.len() >= state.config.max_events {
        state.events.remove(0);
    }

    // Persist to disk
    let formatted = alloc::format!(
        "type={:?} msg=audit({}:{}): pid={} uid={} {} res={}",
        event.event_type,
        event.timestamp,
        id,
        pid,
        uid,
        message,
        if success { "success" } else { "failed" }
    );
    persist_event(&mut state, formatted);

    state.events.push(event);
    id
}

/// Log a syscall audit event
pub fn log_syscall(
    pid: Pid,
    uid: u32,
    syscall_nr: u64,
    args: [u64; 6],
    result: i64,
    success: bool,
) {
    let mut state = AUDIT.lock();

    if !state.config.enabled || !state.config.log_syscalls {
        return;
    }

    // Check syscall filter
    if !state.config.syscall_filter.is_empty() && !state.config.syscall_filter.contains(&syscall_nr)
    {
        return;
    }

    let id = state.next_id;
    state.next_id += 1;

    let message = alloc::format!(
        "syscall={} args=[{:#x},{:#x},{:#x},{:#x},{:#x},{:#x}] result={}",
        syscall_nr,
        args[0],
        args[1],
        args[2],
        args[3],
        args[4],
        args[5],
        result
    );

    let mut event = AuditEvent {
        id,
        event_type: AuditEventType::Syscall,
        severity: if success {
            AuditSeverity::Info
        } else {
            AuditSeverity::Warning
        },
        timestamp: crate::interrupts::get_ticks(),
        pid,
        uid,
        message,
        success,
        syscall_nr: Some(syscall_nr),
        fields: Vec::new(),
    };

    event
        .fields
        .push((String::from("arch"), String::from("x86_64")));
    event
        .fields
        .push((String::from("exit"), alloc::format!("{}", result)));

    if state.events.len() >= state.config.max_events {
        state.events.remove(0);
    }

    state.events.push(event);
}

/// Log a file access event
pub fn log_file_access(pid: Pid, uid: u32, path: &str, access_type: &str, success: bool) {
    let state = AUDIT.lock();
    if !state.config.enabled || !state.config.log_file_access {
        return;
    }

    // Check if path is in watch list
    if !state.config.watch_paths.is_empty() {
        let watched = state
            .config
            .watch_paths
            .iter()
            .any(|w| path.starts_with(w.as_str()));
        if !watched {
            return;
        }
    }
    drop(state);

    let message = alloc::format!("path=\"{}\" access={}", path, access_type);
    log_event(
        AuditEventType::Path,
        AuditSeverity::Info,
        pid,
        uid,
        success,
        &message,
    );
}

/// Log a process creation event
pub fn log_process_create(pid: Pid, ppid: Pid, name: &str) {
    let message = alloc::format!("ppid={} name=\"{}\"", ppid, name);
    log_event(
        AuditEventType::ProcCreate,
        AuditSeverity::Info,
        pid,
        0,
        true,
        &message,
    );
}

/// Log a process exit event
pub fn log_process_exit(pid: Pid, exit_code: i32) {
    let message = alloc::format!("exit_code={}", exit_code);
    log_event(
        AuditEventType::ProcExit,
        AuditSeverity::Info,
        pid,
        0,
        true,
        &message,
    );
}

/// Log an authentication event
pub fn log_auth(pid: Pid, uid: u32, username: &str, success: bool) {
    let message = alloc::format!(
        "user=\"{}\" {}",
        username,
        if success {
            "authenticated"
        } else {
            "auth_failed"
        }
    );
    log_event(
        AuditEventType::UserAuth,
        if success {
            AuditSeverity::Info
        } else {
            AuditSeverity::Warning
        },
        pid,
        uid,
        success,
        &message,
    );
}

/// Log a capability use
pub fn log_capability(pid: Pid, uid: u32, cap: u32, granted: bool) {
    let message = alloc::format!(
        "capability={} {}",
        cap,
        if granted { "granted" } else { "denied" }
    );
    log_event(
        AuditEventType::CapabilityUse,
        if granted {
            AuditSeverity::Info
        } else {
            AuditSeverity::Warning
        },
        pid,
        uid,
        granted,
        &message,
    );
}

/// Log a security policy violation (SELinux AVC denial)
pub fn log_avc_denial(pid: Pid, uid: u32, source: &str, target: &str, permission: &str) {
    let message = alloc::format!(
        "avc: denied {{ {} }} scontext={} tcontext={}",
        permission,
        source,
        target
    );
    log_event(
        AuditEventType::AvcDenial,
        AuditSeverity::Warning,
        pid,
        uid,
        false,
        &message,
    );
}

/// Set the login UID for a process (auid)
pub fn set_login_uid(pid: Pid, uid: u32) {
    AUDIT.lock().login_uids.insert(pid, uid);
}

/// Get the login UID for a process
pub fn get_login_uid(pid: Pid) -> Option<u32> {
    AUDIT.lock().login_uids.get(&pid).copied()
}

/// Query audit events by type
pub fn query_events(event_type: Option<AuditEventType>, limit: usize) -> Vec<AuditEvent> {
    let state = AUDIT.lock();
    let iter = state.events.iter().rev();
    let filtered: Vec<AuditEvent> = if let Some(et) = event_type {
        iter.filter(|e| e.event_type == et)
            .take(limit)
            .cloned()
            .collect()
    } else {
        iter.take(limit).cloned().collect()
    };
    filtered
}

/// Get all audit events
pub fn get_all_events() -> Vec<AuditEvent> {
    AUDIT.lock().events.clone()
}

/// Clear all audit events
pub fn clear_events() {
    AUDIT.lock().events.clear();
}

/// Update audit configuration
pub fn set_config(config: AuditConfig) {
    AUDIT.lock().config = config;
}

/// Get current audit configuration
pub fn get_config() -> AuditConfig {
    AUDIT.lock().config.clone()
}

/// Enable syscall auditing for specific syscalls
pub fn enable_syscall_audit(syscall_numbers: &[u64]) {
    let mut state = AUDIT.lock();
    state.config.log_syscalls = true;
    state.config.syscall_filter = syscall_numbers.to_vec();
}

/// Add a file path to the audit watch list
pub fn add_watch(path: &str) {
    let mut state = AUDIT.lock();
    state.config.log_file_access = true;
    state.config.watch_paths.push(String::from(path));
}

/// Get audit statistics
pub fn stats() -> (usize, u64) {
    let state = AUDIT.lock();
    (state.events.len(), state.next_id - 1)
}

/// Format an audit event for display
pub fn format_event(event: &AuditEvent) -> String {
    let mut s = alloc::format!(
        "type={:?} msg=audit({}:{}): pid={} uid={} {}",
        event.event_type,
        event.timestamp,
        event.id,
        event.pid,
        event.uid,
        event.message,
    );
    if !event.fields.is_empty() {
        for (key, val) in &event.fields {
            s.push_str(&alloc::format!(" {}={}", key, val));
        }
    }
    s
}

// ═══════════════════════════════════════════════════════════════════════
// PERSISTENT AUDIT LOG — writes events to /var/log/audit/audit.log
// ═══════════════════════════════════════════════════════════════════════

/// Write a single formatted audit line to the persistent log file on disk.
fn persist_event(state: &mut AuditState, formatted: String) {
    if !state.config.persist_to_disk {
        return;
    }

    // Check if rotation is needed
    let line_len = formatted.len() + 1; // +1 for newline
    if state.current_log_size + line_len > state.config.max_log_size {
        rotate_logs(state);
    }

    state.pending_flush.push(formatted);
    state.current_log_size += line_len;

    // Auto-flush when batch reaches 64 lines
    if state.pending_flush.len() >= 64 {
        flush_to_disk(state);
    }
}

/// Rotate log files: audit.log → audit.log.1 → audit.log.2 → ...
fn rotate_logs(state: &mut AuditState) {
    let base = state.config.log_file_path.clone();
    let max = state.config.log_rotate_count;

    // Rotate existing files
    for i in (1..max).rev() {
        let old = alloc::format!("{}.{}", base, i);
        let new = alloc::format!("{}.{}", base, i + 1);
        let _ = crate::file_manager::rename(&old, &new);
    }

    // Rename current log to .1
    let rotated = alloc::format!("{}.1", base);
    let _ = crate::file_manager::rename(&base, &rotated);

    state.current_log_size = 0;
    serial_println!("[audit] Log rotated: {}", base);
}

/// Flush pending audit lines to VFS
fn flush_to_disk(state: &mut AuditState) {
    if state.pending_flush.is_empty() {
        return;
    }

    // Build the batch payload
    let mut payload = String::new();
    for line in state.pending_flush.drain(..) {
        payload.push_str(&line);
        payload.push('\n');
    }

    // Append to log file
    let path = &state.config.log_file_path;
    let _ = crate::file_manager::append_file(path, payload.as_bytes());
}

/// Flush any buffered audit events to disk immediately
pub fn flush() {
    let mut state = AUDIT.lock();
    flush_to_disk(&mut state);
}

/// Initialize audit subsystem
pub fn init() {
    // Log initial boot event
    log_event(
        AuditEventType::ConfigChange,
        AuditSeverity::Info,
        0,
        0,
        true,
        "audit: initialized, config_change op=set audit=1",
    );
    serial_println!("[KnoxOS] Audit logging subsystem initialized");
}
