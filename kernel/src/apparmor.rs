/// AppArmor-style Mandatory Access Control
///
/// Provides path-based mandatory access control (MAC) profiles
/// for process confinement. Each profile defines allowed file access,
/// network operations, capability usage, and signal delivery.
///
/// Implements enforcement, complain, and unconfined modes.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── Profile Mode ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProfileMode {
    Enforce,
    Complain,
    Unconfined,
    Kill,
}

impl ProfileMode {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Enforce => "enforce",
            Self::Complain => "complain",
            Self::Unconfined => "unconfined",
            Self::Kill => "kill",
        }
    }
}

// ─── Access Permissions ─────────────────────────────────────────────

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct FilePermission: u32 {
        const READ       = 0x0001;
        const WRITE      = 0x0002;
        const APPEND     = 0x0004;
        const EXEC       = 0x0008;
        const MMAP_EXEC  = 0x0010;
        const LINK       = 0x0020;
        const LOCK       = 0x0040;
        const CREATE     = 0x0080;
        const DELETE     = 0x0100;
        const RENAME_SRC = 0x0200;
        const RENAME_DST = 0x0400;
        const CHOWN      = 0x0800;
        const CHMOD      = 0x1000;
        const SETATTR    = 0x2000;
        const GETATTR    = 0x4000;
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct NetPermission: u32 {
        const TCP_CONNECT = 0x01;
        const TCP_ACCEPT  = 0x02;
        const UDP_SEND    = 0x04;
        const UDP_RECV    = 0x08;
        const RAW         = 0x10;
        const UNIX_STREAM = 0x20;
        const UNIX_DGRAM  = 0x40;
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct CapPermission: u64 {
        const CHOWN            = 1 << 0;
        const DAC_OVERRIDE     = 1 << 1;
        const DAC_READ_SEARCH  = 1 << 2;
        const FOWNER           = 1 << 3;
        const FSETID           = 1 << 4;
        const KILL             = 1 << 5;
        const SETGID           = 1 << 6;
        const SETUID           = 1 << 7;
        const NET_BIND_SERVICE = 1 << 10;
        const NET_ADMIN        = 1 << 12;
        const NET_RAW          = 1 << 13;
        const SYS_CHROOT       = 1 << 18;
        const SYS_PTRACE       = 1 << 19;
        const SYS_ADMIN        = 1 << 21;
        const SYS_RESOURCE     = 1 << 24;
        const MKNOD            = 1 << 27;
    }
}

// ─── Access Rules ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FileRule {
    pub path: String,
    pub permissions: FilePermission,
    pub owner_conditional: bool,
}

#[derive(Debug, Clone)]
pub struct NetRule {
    pub permissions: NetPermission,
    pub allowed_ports: Vec<u16>,
    pub allowed_addresses: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SignalRule {
    pub signals: Vec<i32>,
    pub targets: Vec<String>, // target profile names
}

#[derive(Debug, Clone)]
pub struct MountRule {
    pub fstype: String,
    pub options: Vec<String>,
    pub src: String,
    pub dst: String,
}

#[derive(Debug, Clone)]
pub struct PtraceRule {
    pub read: bool,
    pub trace: bool,
    pub targets: Vec<String>,
}

// ─── Profile ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AppArmorProfile {
    pub name: String,
    pub mode: ProfileMode,
    pub file_rules: Vec<FileRule>,
    pub net_rules: Vec<NetRule>,
    pub capabilities: CapPermission,
    pub signal_rules: Vec<SignalRule>,
    pub mount_rules: Vec<MountRule>,
    pub ptrace_rules: Vec<PtraceRule>,
    pub child_profiles: Vec<String>,
    pub hat_profiles: BTreeMap<String, AppArmorProfile>,
    pub deny_rules: Vec<FileRule>,
    pub audit_rules: Vec<FileRule>,
    pub rlimits: BTreeMap<String, u64>,
    pub transitions: BTreeMap<String, String>, // path → target profile
}

impl AppArmorProfile {
    pub fn new(name: &str, mode: ProfileMode) -> Self {
        Self {
            name: String::from(name),
            mode,
            file_rules: Vec::new(),
            net_rules: Vec::new(),
            capabilities: CapPermission::empty(),
            signal_rules: Vec::new(),
            mount_rules: Vec::new(),
            ptrace_rules: Vec::new(),
            child_profiles: Vec::new(),
            hat_profiles: BTreeMap::new(),
            deny_rules: Vec::new(),
            audit_rules: Vec::new(),
            rlimits: BTreeMap::new(),
            transitions: BTreeMap::new(),
        }
    }

    /// Add a file access rule
    pub fn allow_file(&mut self, path: &str, perms: FilePermission) {
        self.file_rules.push(FileRule {
            path: String::from(path),
            permissions: perms,
            owner_conditional: false,
        });
    }

    /// Add a deny rule
    pub fn deny_file(&mut self, path: &str, perms: FilePermission) {
        self.deny_rules.push(FileRule {
            path: String::from(path),
            permissions: perms,
            owner_conditional: false,
        });
    }

    /// Add network access rule
    pub fn allow_network(&mut self, perms: NetPermission) {
        self.net_rules.push(NetRule {
            permissions: perms,
            allowed_ports: Vec::new(),
            allowed_addresses: Vec::new(),
        });
    }

    /// Set allowed capabilities
    pub fn allow_capabilities(&mut self, caps: CapPermission) {
        self.capabilities |= caps;
    }

    /// Add a profile transition rule
    pub fn add_transition(&mut self, path: &str, target: &str) {
        self.transitions
            .insert(String::from(path), String::from(target));
    }
}

// ─── Access Decision ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AccessDecision {
    Allow,
    Deny,
    Audit,    // Allow but log
    Complain, // Allow in complain mode but log
}

// ─── Global State ───────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref PROFILES: Mutex<BTreeMap<String, AppArmorProfile>> = Mutex::new(BTreeMap::new());
    static ref PROCESS_PROFILES: Mutex<BTreeMap<u32, String>> = Mutex::new(BTreeMap::new());
    static ref AUDIT_LOG: Mutex<Vec<AuditEntry>> = Mutex::new(Vec::new());
    static ref STATS: Mutex<MacStats> = Mutex::new(MacStats::default());
}

#[derive(Debug, Default)]
pub struct MacStats {
    pub allowed: u64,
    pub denied: u64,
    pub audited: u64,
    pub complained: u64,
    pub profiles_loaded: u32,
    pub processes_confined: u32,
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub pid: u32,
    pub profile: String,
    pub operation: String,
    pub target: String,
    pub decision: AccessDecision,
    pub timestamp: u64,
}

// ─── Path Matching ──────────────────────────────────────────────────

fn path_matches(pattern: &str, path: &str) -> bool {
    if pattern == path {
        return true;
    }

    // Handle glob patterns
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path.starts_with(prefix);
    }

    if let Some(prefix) = pattern.strip_suffix("/*") {
        if !path.starts_with(prefix) {
            return false;
        }
        let rest = &path[prefix.len()..];
        return rest.starts_with('/') && !rest[1..].contains('/');
    }

    if pattern.contains('*') {
        // Simple wildcard matching
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            return path.starts_with(parts[0]) && path.ends_with(parts[1]);
        }
    }

    if pattern.contains('{') && pattern.contains('}') {
        // Brace expansion (simplified)
        let start = pattern.find('{').unwrap();
        let end = pattern.find('}').unwrap();
        let prefix = &pattern[..start];
        let suffix = &pattern[end + 1..];
        let alternatives = &pattern[start + 1..end];

        for alt in alternatives.split(',') {
            let expanded = alloc::format!("{}{}{}", prefix, alt.trim(), suffix);
            if path_matches(&expanded, path) {
                return true;
            }
        }
        return false;
    }

    false
}

// ─── Access Checking ────────────────────────────────────────────────

/// Check file access permission
pub fn check_file_access(pid: u32, path: &str, requested: FilePermission) -> AccessDecision {
    let profiles = PROFILES.lock();
    let process_profiles = PROCESS_PROFILES.lock();

    let profile_name = match process_profiles.get(&pid) {
        Some(name) => name.clone(),
        None => return AccessDecision::Allow, // Unconfined
    };

    let profile = match profiles.get(&profile_name) {
        Some(p) => p,
        None => return AccessDecision::Allow,
    };

    match profile.mode {
        ProfileMode::Unconfined => return AccessDecision::Allow,
        ProfileMode::Kill => {
            // Check deny rules first
            for rule in &profile.deny_rules {
                if path_matches(&rule.path, path) && rule.permissions.intersects(requested) {
                    log_audit(pid, &profile_name, "file", path, AccessDecision::Deny);
                    STATS.lock().denied += 1;
                    return AccessDecision::Deny;
                }
            }
        }
        _ => {}
    }

    // Check explicit deny rules
    for rule in &profile.deny_rules {
        if path_matches(&rule.path, path) && rule.permissions.intersects(requested) {
            log_audit(pid, &profile_name, "file", path, AccessDecision::Deny);
            STATS.lock().denied += 1;
            return AccessDecision::Deny;
        }
    }

    // Check allow rules
    for rule in &profile.file_rules {
        if path_matches(&rule.path, path) && rule.permissions.contains(requested) {
            // Check audit rules
            for audit_rule in &profile.audit_rules {
                if path_matches(&audit_rule.path, path) {
                    log_audit(pid, &profile_name, "file", path, AccessDecision::Audit);
                    STATS.lock().audited += 1;
                    return AccessDecision::Audit;
                }
            }
            STATS.lock().allowed += 1;
            return AccessDecision::Allow;
        }
    }

    // Not explicitly allowed
    match profile.mode {
        ProfileMode::Enforce | ProfileMode::Kill => {
            log_audit(
                pid,
                &profile_name,
                "file_denied",
                path,
                AccessDecision::Deny,
            );
            STATS.lock().denied += 1;
            AccessDecision::Deny
        }
        ProfileMode::Complain => {
            log_audit(
                pid,
                &profile_name,
                "file_complain",
                path,
                AccessDecision::Complain,
            );
            STATS.lock().complained += 1;
            AccessDecision::Complain
        }
        ProfileMode::Unconfined => AccessDecision::Allow,
    }
}

/// Check network access permission
pub fn check_net_access(pid: u32, requested: NetPermission) -> AccessDecision {
    let profiles = PROFILES.lock();
    let process_profiles = PROCESS_PROFILES.lock();

    let profile_name = match process_profiles.get(&pid) {
        Some(name) => name.clone(),
        None => return AccessDecision::Allow,
    };

    let profile = match profiles.get(&profile_name) {
        Some(p) => p,
        None => return AccessDecision::Allow,
    };

    if profile.mode == ProfileMode::Unconfined {
        return AccessDecision::Allow;
    }

    for rule in &profile.net_rules {
        if rule.permissions.contains(requested) {
            STATS.lock().allowed += 1;
            return AccessDecision::Allow;
        }
    }

    match profile.mode {
        ProfileMode::Enforce | ProfileMode::Kill => {
            STATS.lock().denied += 1;
            AccessDecision::Deny
        }
        ProfileMode::Complain => {
            STATS.lock().complained += 1;
            AccessDecision::Complain
        }
        _ => AccessDecision::Allow,
    }
}

/// Check capability usage
pub fn check_capability(pid: u32, cap: CapPermission) -> AccessDecision {
    let profiles = PROFILES.lock();
    let process_profiles = PROCESS_PROFILES.lock();

    let profile_name = match process_profiles.get(&pid) {
        Some(name) => name.clone(),
        None => return AccessDecision::Allow,
    };

    let profile = match profiles.get(&profile_name) {
        Some(p) => p,
        None => return AccessDecision::Allow,
    };

    if profile.mode == ProfileMode::Unconfined {
        return AccessDecision::Allow;
    }

    if profile.capabilities.contains(cap) {
        STATS.lock().allowed += 1;
        return AccessDecision::Allow;
    }

    match profile.mode {
        ProfileMode::Enforce | ProfileMode::Kill => {
            STATS.lock().denied += 1;
            AccessDecision::Deny
        }
        ProfileMode::Complain => {
            STATS.lock().complained += 1;
            AccessDecision::Complain
        }
        _ => AccessDecision::Allow,
    }
}

fn log_audit(pid: u32, profile: &str, operation: &str, target: &str, decision: AccessDecision) {
    let entry = AuditEntry {
        pid,
        profile: String::from(profile),
        operation: String::from(operation),
        target: String::from(target),
        decision,
        timestamp: 0,
    };

    serial_println!(
        "[AppArmor] {:?} pid={} profile={} op={} target={}",
        decision,
        pid,
        profile,
        operation,
        target
    );

    let mut log = AUDIT_LOG.lock();
    if log.len() > 10000 {
        log.drain(0..5000);
    }
    log.push(entry);
}

// ─── Profile Management ────────────────────────────────────────────

/// Load a profile
pub fn load_profile(profile: AppArmorProfile) {
    serial_println!(
        "[AppArmor] Loading profile '{}' ({})",
        profile.name,
        profile.mode.name()
    );
    let name = profile.name.clone();
    PROFILES.lock().insert(name, profile);
    STATS.lock().profiles_loaded += 1;
}

/// Remove a profile
pub fn remove_profile(name: &str) -> bool {
    PROFILES.lock().remove(name).is_some()
}

/// Assign a profile to a process
pub fn confine_process(pid: u32, profile_name: &str) -> bool {
    if !PROFILES.lock().contains_key(profile_name) {
        return false;
    }
    PROCESS_PROFILES
        .lock()
        .insert(pid, String::from(profile_name));
    STATS.lock().processes_confined += 1;
    serial_println!(
        "[AppArmor] Process {} confined with profile '{}'",
        pid,
        profile_name
    );
    true
}

/// Release a process from confinement
pub fn release_process(pid: u32) {
    PROCESS_PROFILES.lock().remove(&pid);
}

/// Get the profile name for a process
pub fn process_profile(pid: u32) -> Option<String> {
    PROCESS_PROFILES.lock().get(&pid).cloned()
}

/// Change a profile's mode
pub fn set_profile_mode(name: &str, mode: ProfileMode) -> bool {
    if let Some(profile) = PROFILES.lock().get_mut(name) {
        profile.mode = mode;
        serial_println!(
            "[AppArmor] Profile '{}' mode changed to {}",
            name,
            mode.name()
        );
        true
    } else {
        false
    }
}

/// Get profile count
pub fn profile_count() -> usize {
    PROFILES.lock().len()
}

/// Get confined process count
pub fn confined_count() -> usize {
    PROCESS_PROFILES.lock().len()
}

/// Get audit log entries
pub fn audit_entries(limit: usize) -> Vec<AuditEntry> {
    let log = AUDIT_LOG.lock();
    let start = if log.len() > limit {
        log.len() - limit
    } else {
        0
    };
    log[start..].to_vec()
}

// ─── Default Profiles ───────────────────────────────────────────────

fn create_default_profiles() {
    // Default profile for shells
    let mut shell_profile = AppArmorProfile::new("/bin/sh", ProfileMode::Enforce);
    shell_profile.allow_file("/bin/**", FilePermission::READ | FilePermission::EXEC);
    shell_profile.allow_file("/usr/bin/**", FilePermission::READ | FilePermission::EXEC);
    shell_profile.allow_file("/lib/**", FilePermission::READ | FilePermission::MMAP_EXEC);
    shell_profile.allow_file(
        "/usr/lib/**",
        FilePermission::READ | FilePermission::MMAP_EXEC,
    );
    shell_profile.allow_file("/etc/**", FilePermission::READ);
    shell_profile.allow_file(
        "/tmp/**",
        FilePermission::READ | FilePermission::WRITE | FilePermission::CREATE,
    );
    shell_profile.allow_file("/dev/null", FilePermission::READ | FilePermission::WRITE);
    shell_profile.allow_file("/dev/zero", FilePermission::READ);
    shell_profile.allow_file("/dev/urandom", FilePermission::READ);
    shell_profile.allow_file("/proc/**", FilePermission::READ);
    shell_profile.deny_file("/etc/shadow", FilePermission::READ | FilePermission::WRITE);
    shell_profile.allow_network(
        NetPermission::TCP_CONNECT | NetPermission::UDP_SEND | NetPermission::UDP_RECV,
    );
    load_profile(shell_profile);

    // Minimal network daemon profile
    let mut daemon_profile = AppArmorProfile::new("network-daemon", ProfileMode::Enforce);
    daemon_profile.allow_file("/etc/**", FilePermission::READ);
    daemon_profile.allow_file(
        "/var/log/**",
        FilePermission::WRITE | FilePermission::APPEND | FilePermission::CREATE,
    );
    daemon_profile.allow_file(
        "/run/**",
        FilePermission::READ | FilePermission::WRITE | FilePermission::CREATE,
    );
    daemon_profile.allow_file(
        "/tmp/**",
        FilePermission::READ | FilePermission::WRITE | FilePermission::CREATE,
    );
    daemon_profile.allow_network(
        NetPermission::TCP_CONNECT
            | NetPermission::TCP_ACCEPT
            | NetPermission::UDP_SEND
            | NetPermission::UDP_RECV,
    );
    daemon_profile.allow_capabilities(CapPermission::NET_BIND_SERVICE);
    load_profile(daemon_profile);

    // Container profile
    let mut container_profile = AppArmorProfile::new("container-default", ProfileMode::Enforce);
    container_profile.allow_file(
        "/**",
        FilePermission::READ
            | FilePermission::WRITE
            | FilePermission::EXEC
            | FilePermission::CREATE,
    );
    container_profile.deny_file("/proc/sys/**", FilePermission::WRITE);
    container_profile.deny_file("/sys/**", FilePermission::WRITE);
    container_profile.allow_network(
        NetPermission::TCP_CONNECT
            | NetPermission::TCP_ACCEPT
            | NetPermission::UDP_SEND
            | NetPermission::UDP_RECV
            | NetPermission::UNIX_STREAM
            | NetPermission::UNIX_DGRAM,
    );
    container_profile.allow_capabilities(
        CapPermission::CHOWN
            | CapPermission::DAC_OVERRIDE
            | CapPermission::FOWNER
            | CapPermission::FSETID
            | CapPermission::KILL
            | CapPermission::SETGID
            | CapPermission::SETUID
            | CapPermission::NET_BIND_SERVICE
            | CapPermission::MKNOD,
    );
    load_profile(container_profile);
}

// ─── Init ───────────────────────────────────────────────────────────

pub fn init() {
    serial_println!("[KnoxOS] AppArmor MAC subsystem initialized");
    create_default_profiles();
    serial_println!("[KnoxOS]   {} default profiles loaded", profile_count());
    serial_println!("[KnoxOS]   Modes: enforce, complain, unconfined, kill");
}
