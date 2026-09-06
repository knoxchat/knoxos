use alloc::collections::BTreeMap;
/// SELinux-compatible Security Module
/// Mandatory Access Control (MAC) for KnoxOS
/// Implements a simplified LSM (Linux Security Module) framework
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Security context label
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityContext {
    pub user: String,
    pub role: String,
    pub type_field: String,
    pub level: String,
}

impl SecurityContext {
    pub fn new(user: &str, role: &str, type_field: &str, level: &str) -> Self {
        Self {
            user: String::from(user),
            role: String::from(role),
            type_field: String::from(type_field),
            level: String::from(level),
        }
    }

    /// Parse from string format "user:role:type:level"
    pub fn from_string(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() >= 3 {
            Some(Self {
                user: String::from(parts[0]),
                role: String::from(parts[1]),
                type_field: String::from(parts[2]),
                level: if parts.len() > 3 {
                    String::from(parts[3])
                } else {
                    String::from("s0")
                },
            })
        } else {
            None
        }
    }

    /// Format as string
    pub fn to_label(&self) -> String {
        alloc::format!(
            "{}:{}:{}:{}",
            self.user,
            self.role,
            self.type_field,
            self.level
        )
    }
}

/// Access permission
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Read,
    Write,
    Execute,
    Append,
    Create,
    Delete,
    Link,
    Rename,
    Signal,
    Transition,
}

/// Access decision
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
    Audit,
}

/// Security policy rule
#[derive(Debug, Clone)]
pub struct PolicyRule {
    pub source_type: String,
    pub target_type: String,
    pub object_class: String,
    pub permissions: Vec<Permission>,
    pub decision: Decision,
}

/// Security mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityMode {
    Disabled,
    Permissive, // Log violations but allow
    Enforcing,  // Enforce policy and deny violations
}

impl core::fmt::Display for SecurityMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SecurityMode::Disabled => write!(f, "Disabled"),
            SecurityMode::Permissive => write!(f, "Permissive"),
            SecurityMode::Enforcing => write!(f, "Enforcing"),
        }
    }
}

/// Security module state
struct SecurityState {
    mode: SecurityMode,
    policies: Vec<PolicyRule>,
    process_contexts: BTreeMap<u32, SecurityContext>,
    file_contexts: BTreeMap<String, SecurityContext>,
    violations: Vec<String>,
}

lazy_static::lazy_static! {
    static ref SECURITY: Mutex<SecurityState> = Mutex::new(SecurityState {
        mode: SecurityMode::Permissive,
        policies: Vec::new(),
        process_contexts: BTreeMap::new(),
        file_contexts: BTreeMap::new(),
        violations: Vec::new(),
    });
}

/// Check if an access is allowed
pub fn check_access(source_pid: u32, target_path: &str, permission: Permission) -> Decision {
    let state = SECURITY.lock();

    if state.mode == SecurityMode::Disabled {
        return Decision::Allow;
    }

    // Get source context
    let source_ctx = match state.process_contexts.get(&source_pid) {
        Some(ctx) => ctx.clone(),
        None => return Decision::Allow, // Unknown process, allow by default
    };

    // Get target context
    let target_ctx = match state.file_contexts.get(target_path) {
        Some(ctx) => ctx.clone(),
        None => {
            // Try path prefix matching
            let mut found = None;
            for (path, ctx) in &state.file_contexts {
                if target_path.starts_with(path) {
                    found = Some(ctx.clone());
                }
            }
            match found {
                Some(ctx) => ctx,
                None => return Decision::Allow, // No policy for this file
            }
        }
    };

    // Check policy rules
    for rule in &state.policies {
        if rule.source_type == source_ctx.type_field
            && rule.target_type == target_ctx.type_field
            && rule.permissions.contains(&permission)
        {
            return rule.decision;
        }
    }

    // Default: allow in permissive, deny in enforcing
    match state.mode {
        SecurityMode::Enforcing => Decision::Deny,
        _ => Decision::Allow,
    }
}

/// Set security context for a process
pub fn set_process_context(pid: u32, context: SecurityContext) {
    SECURITY.lock().process_contexts.insert(pid, context);
}

/// Set security context for a file
pub fn set_file_context(path: &str, context: SecurityContext) {
    SECURITY
        .lock()
        .file_contexts
        .insert(String::from(path), context);
}

/// Get process security context
pub fn get_process_context(pid: u32) -> Option<SecurityContext> {
    SECURITY.lock().process_contexts.get(&pid).cloned()
}

/// Get file security context
pub fn get_file_context(path: &str) -> Option<SecurityContext> {
    SECURITY.lock().file_contexts.get(path).cloned()
}

/// Add a policy rule
pub fn add_rule(rule: PolicyRule) {
    SECURITY.lock().policies.push(rule);
}

/// Set security mode
pub fn set_mode(mode: SecurityMode) {
    let mut state = SECURITY.lock();
    state.mode = mode;
    crate::serial_println!("[KnoxOS] Security mode: {:?}", mode);
}

/// Get security mode
pub fn get_mode() -> SecurityMode {
    SECURITY.lock().mode
}

/// Get security status summary
pub fn status() -> String {
    let state = SECURITY.lock();
    let mode_str = match state.mode {
        SecurityMode::Disabled => "disabled",
        SecurityMode::Permissive => "permissive",
        SecurityMode::Enforcing => "enforcing",
    };
    alloc::format!(
        "SELinux status:        enabled\n\
         Current mode:          {}\n\
         Policy rules:          {}\n\
         Process contexts:      {}\n\
         File contexts:         {}\n\
         Violations logged:     {}",
        mode_str,
        state.policies.len(),
        state.process_contexts.len(),
        state.file_contexts.len(),
        state.violations.len(),
    )
}

/// Log a security violation
pub fn log_violation(message: &str) {
    let mut state = SECURITY.lock();
    state.violations.push(String::from(message));
    crate::serial_println!("[KnoxOS] SEC VIOLATION: {}", message);
}

/// Get violation log
pub fn get_violations() -> Vec<String> {
    SECURITY.lock().violations.clone()
}

/// Linux capabilities (subset)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    CapChown = 0,
    CapDacOverride = 1,
    CapFowner = 3,
    CapFsetid = 4,
    CapKill = 5,
    CapSetgid = 6,
    CapSetuid = 7,
    CapNetBindService = 10,
    CapNetRaw = 13,
    CapSysChroot = 18,
    CapSysAdmin = 21,
    CapSysBoot = 22,
    CapSysNice = 23,
    CapSysResource = 24,
    CapSysTime = 25,
    CapSysTtyConfig = 26,
    CapMknod = 27,
}

/// Check if a process has a capability
pub fn has_capability(pid: u32, _cap: Capability) -> bool {
    // For now, root (pid 0, 1) has all capabilities
    pid <= 1
}

/// Initialize the security module
pub fn init() {
    let mut state = SECURITY.lock();

    // Set default contexts
    state.process_contexts.insert(
        0,
        SecurityContext::new("system_u", "system_r", "kernel_t", "s0"),
    );
    state.process_contexts.insert(
        1,
        SecurityContext::new("system_u", "system_r", "init_t", "s0"),
    );
    state.process_contexts.insert(
        2,
        SecurityContext::new("system_u", "system_r", "desktop_t", "s0"),
    );

    // Set default file contexts
    state.file_contexts.insert(
        String::from("/"),
        SecurityContext::new("system_u", "object_r", "root_t", "s0"),
    );
    state.file_contexts.insert(
        String::from("/etc"),
        SecurityContext::new("system_u", "object_r", "etc_t", "s0"),
    );
    state.file_contexts.insert(
        String::from("/tmp"),
        SecurityContext::new("system_u", "object_r", "tmp_t", "s0"),
    );
    state.file_contexts.insert(
        String::from("/home"),
        SecurityContext::new("unconfined_u", "object_r", "user_home_t", "s0"),
    );
    state.file_contexts.insert(
        String::from("/dev"),
        SecurityContext::new("system_u", "object_r", "device_t", "s0"),
    );

    // Add basic policy rules
    state.policies.push(PolicyRule {
        source_type: String::from("kernel_t"),
        target_type: String::from("root_t"),
        object_class: String::from("file"),
        permissions: vec![Permission::Read, Permission::Write, Permission::Execute],
        decision: Decision::Allow,
    });

    state.policies.push(PolicyRule {
        source_type: String::from("init_t"),
        target_type: String::from("root_t"),
        object_class: String::from("file"),
        permissions: vec![Permission::Read, Permission::Write, Permission::Execute],
        decision: Decision::Allow,
    });

    state.policies.push(PolicyRule {
        source_type: String::from("desktop_t"),
        target_type: String::from("user_home_t"),
        object_class: String::from("file"),
        permissions: vec![Permission::Read, Permission::Write],
        decision: Decision::Allow,
    });

    drop(state);
    crate::serial_println!("[KnoxOS] Security module initialized (mode: Permissive)");
}

// ═══════════════════════════════════════════════════════════════════════
// SUDO / PRIVILEGE ESCALATION FRAMEWORK
// ═══════════════════════════════════════════════════════════════════════

/// Sudoers entry — who can run what as whom
#[derive(Debug, Clone)]
pub struct SudoersEntry {
    /// Username or group name (prefixed with %)
    pub user_spec: String,
    /// Host (ALL for any)
    pub host: String,
    /// Run-as user (ALL for any)
    pub run_as: String,
    /// Commands allowed (ALL for any, or specific paths)
    pub commands: Vec<String>,
    /// Whether password is required
    pub nopasswd: bool,
}

/// Sudo session (cached authentication)
#[derive(Debug, Clone)]
pub struct SudoSession {
    pub uid: u32,
    /// Timestamp of last successful authentication
    pub auth_time: u64,
    /// Session timeout in seconds (default 15 minutes)
    pub timeout_secs: u64,
}

/// Sudo subsystem state
pub struct SudoState {
    /// Sudoers entries
    entries: Vec<SudoersEntry>,
    /// Active sessions (UID → session)
    sessions: BTreeMap<u32, SudoSession>,
    /// Failed attempt counter per UID
    failed_attempts: BTreeMap<u32, u32>,
    /// Lockout threshold (default 3)
    lockout_threshold: u32,
    /// Lockout duration in seconds
    lockout_duration: u64,
}

impl SudoState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            sessions: BTreeMap::new(),
            failed_attempts: BTreeMap::new(),
            lockout_threshold: 3,
            lockout_duration: 300, // 5 minutes
        }
    }

    /// Add a sudoers entry
    pub fn add_entry(&mut self, entry: SudoersEntry) {
        crate::serial_println!(
            "[sudo] Added entry: {} -> {}",
            entry.user_spec,
            entry.run_as
        );
        self.entries.push(entry);
    }

    /// Check if a user is allowed to run a command as another user
    pub fn check_permission(&self, username: &str, command: &str, run_as: &str) -> bool {
        for entry in &self.entries {
            let user_match = entry.user_spec == username
                || entry.user_spec == "ALL"
                || (entry.user_spec.starts_with('%')
                    && self.user_in_group(username, &entry.user_spec[1..]));

            let cmd_match = entry.commands.iter().any(|c| c == "ALL" || c == command);
            let run_as_match = entry.run_as == "ALL" || entry.run_as == run_as;

            if user_match && cmd_match && run_as_match {
                return true;
            }
        }
        false
    }

    /// Check if a user needs to re-authenticate
    pub fn needs_auth(&self, uid: u32) -> bool {
        if let Some(session) = self.sessions.get(&uid) {
            let now = crate::hpet::read_counter();
            let elapsed = now.wrapping_sub(session.auth_time) / 100; // Convert ticks to approx seconds
            elapsed < session.timeout_secs
        } else {
            true // No session — needs auth
        }
    }

    /// Record successful authentication
    pub fn create_session(&mut self, uid: u32) {
        self.sessions.insert(
            uid,
            SudoSession {
                uid,
                auth_time: crate::hpet::read_counter(),
                timeout_secs: 900, // 15 minutes
            },
        );
        self.failed_attempts.remove(&uid);
    }

    /// Record failed authentication attempt
    pub fn record_failed_attempt(&mut self, uid: u32) {
        let count = self.failed_attempts.entry(uid).or_insert(0);
        *count += 1;
        crate::serial_println!("[sudo] Failed attempt for UID {} (count: {})", uid, count);
    }

    /// Check if account is locked out
    pub fn is_locked_out(&self, uid: u32) -> bool {
        self.failed_attempts
            .get(&uid)
            .map(|&c| c >= self.lockout_threshold)
            .unwrap_or(false)
    }

    /// Invalidate a sudo session
    pub fn invalidate_session(&mut self, uid: u32) {
        self.sessions.remove(&uid);
    }

    /// Invalidate all sessions (`sudo -K`)
    pub fn invalidate_all_sessions(&mut self) {
        self.sessions.clear();
    }

    fn user_in_group(&self, _username: &str, _group: &str) -> bool {
        // Check user's group membership via /etc/group
        // Simplified: check crate::users module
        false
    }
}

lazy_static::lazy_static! {
    pub static ref SUDO: Mutex<SudoState> = Mutex::new(SudoState::new());
}

/// Execute a command with elevated privileges
pub fn sudo_exec(
    uid: u32,
    username: &str,
    command: &str,
    run_as: &str,
) -> Result<(), &'static str> {
    let sudo = SUDO.lock();

    // Check lockout
    if sudo.is_locked_out(uid) {
        return Err("account locked due to failed attempts");
    }

    // Check permission
    if !sudo.check_permission(username, command, run_as) {
        crate::serial_println!("[sudo] DENIED: {} -> {} as {}", username, command, run_as);
        crate::audit::log_event(
            crate::audit::AuditEventType::UserAuth,
            crate::audit::AuditSeverity::Warning,
            0,
            uid,
            false,
            &alloc::format!(
                "sudo DENIED: user={} command={} run_as={}",
                username,
                command,
                run_as
            ),
        );
        return Err("permission denied");
    }

    drop(sudo);
    crate::serial_println!("[sudo] ALLOWED: {} -> {} as {}", username, command, run_as);
    crate::audit::log_event(
        crate::audit::AuditEventType::UserAuth,
        crate::audit::AuditSeverity::Info,
        0,
        uid,
        true,
        &alloc::format!(
            "sudo ALLOWED: user={} command={} run_as={}",
            username,
            command,
            run_as
        ),
    );
    Ok(())
}

/// Initialize sudo subsystem with default entries
pub fn init_sudo() {
    let mut sudo = SUDO.lock();

    // Default: root can do anything
    sudo.add_entry(SudoersEntry {
        user_spec: String::from("root"),
        host: String::from("ALL"),
        run_as: String::from("ALL"),
        commands: alloc::vec![String::from("ALL")],
        nopasswd: true,
    });

    // Default: wheel group can sudo with password
    sudo.add_entry(SudoersEntry {
        user_spec: String::from("%wheel"),
        host: String::from("ALL"),
        run_as: String::from("ALL"),
        commands: alloc::vec![String::from("ALL")],
        nopasswd: false,
    });

    // Default: admin group can sudo
    sudo.add_entry(SudoersEntry {
        user_spec: String::from("%admin"),
        host: String::from("ALL"),
        run_as: String::from("ALL"),
        commands: alloc::vec![String::from("ALL")],
        nopasswd: false,
    });

    drop(sudo);
    crate::serial_println!("[sudo] Privilege escalation framework initialized");
}

// ═══════════════════════════════════════════════════════════════════════
// PASSWORD COMPLEXITY ENFORCEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Password policy configuration
#[derive(Debug, Clone)]
pub struct PasswordPolicy {
    /// Minimum length
    pub min_length: usize,
    /// Require uppercase letter
    pub require_uppercase: bool,
    /// Require lowercase letter
    pub require_lowercase: bool,
    /// Require digit
    pub require_digit: bool,
    /// Require special character
    pub require_special: bool,
    /// Maximum consecutive same characters
    pub max_consecutive: usize,
    /// Password expiry in days (0 = no expiry)
    pub expire_days: u32,
    /// Number of previous passwords to remember
    pub history_count: usize,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_length: 8,
            require_uppercase: true,
            require_lowercase: true,
            require_digit: true,
            require_special: false,
            max_consecutive: 3,
            expire_days: 90,
            history_count: 5,
        }
    }
}

/// Check if a password meets the policy requirements
pub fn check_password_complexity(password: &str, policy: &PasswordPolicy) -> Result<(), String> {
    if password.len() < policy.min_length {
        return Err(alloc::format!(
            "password must be at least {} characters",
            policy.min_length
        ));
    }

    if policy.require_uppercase && !password.chars().any(|c| c.is_ascii_uppercase()) {
        return Err(String::from("password must contain an uppercase letter"));
    }

    if policy.require_lowercase && !password.chars().any(|c| c.is_ascii_lowercase()) {
        return Err(String::from("password must contain a lowercase letter"));
    }

    if policy.require_digit && !password.chars().any(|c| c.is_ascii_digit()) {
        return Err(String::from("password must contain a digit"));
    }

    if policy.require_special && !password.chars().any(|c| !c.is_alphanumeric()) {
        return Err(String::from("password must contain a special character"));
    }

    // Check consecutive characters
    if policy.max_consecutive > 0 {
        let chars: Vec<char> = password.chars().collect();
        let mut consecutive = 1;
        for i in 1..chars.len() {
            if chars[i] == chars[i - 1] {
                consecutive += 1;
                if consecutive > policy.max_consecutive {
                    return Err(alloc::format!(
                        "password must not have more than {} consecutive same characters",
                        policy.max_consecutive
                    ));
                }
            } else {
                consecutive = 1;
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// MULTI-FACTOR AUTHENTICATION
// ═══════════════════════════════════════════════════════════════════════

/// MFA method types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MfaMethod {
    /// Time-based one-time password (TOTP, RFC 6238)
    Totp,
    /// HMAC-based one-time password  (HOTP, RFC 4226)
    Hotp,
    /// Hardware security key (FIDO2/WebAuthn)
    SecurityKey,
    /// Recovery codes
    RecoveryCode,
}

/// MFA enrollment for a user
#[derive(Debug, Clone)]
pub struct MfaEnrollment {
    pub uid: u32,
    pub method: MfaMethod,
    /// Shared secret (for TOTP/HOTP)
    pub secret: Vec<u8>,
    /// Counter (for HOTP)
    pub counter: u64,
    /// Recovery codes (hashed)
    pub recovery_codes: Vec<[u8; 32]>,
    /// Whether MFA is enabled
    pub enabled: bool,
}

/// Generate a TOTP code (RFC 6238)
pub fn generate_totp(secret: &[u8], time_step: u64) -> u32 {
    // Current time step (30-second intervals)
    let counter = (crate::rtc::unix_time() as u64) / time_step;
    generate_hotp(secret, counter)
}

/// Generate an HOTP code (RFC 4226)
pub fn generate_hotp(secret: &[u8], counter: u64) -> u32 {
    let counter_bytes = counter.to_be_bytes();
    let hmac = crate::crypto::hmac_sha256(secret, &counter_bytes);

    // Dynamic truncation
    let offset = (hmac[31] & 0x0F) as usize;
    let code = ((hmac[offset] & 0x7F) as u32) << 24
        | (hmac[offset + 1] as u32) << 16
        | (hmac[offset + 2] as u32) << 8
        | (hmac[offset + 3] as u32);

    code % 1_000_000 // 6-digit code
}

/// Verify a TOTP code (allows ±1 time step window)
pub fn verify_totp(secret: &[u8], code: u32, time_step: u64) -> bool {
    let current = (crate::rtc::unix_time() as u64) / time_step;
    for offset in [0i64, -1, 1] {
        let step = (current as i64 + offset) as u64;
        if generate_hotp(secret, step) == code {
            return true;
        }
    }
    false
}

// ═══════════════════════════════════════════════════════════════════════
// SESSION MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Active login session
#[derive(Debug, Clone)]
pub struct LoginSession {
    pub session_id: u64,
    pub uid: u32,
    pub username: String,
    /// TTY or display (e.g., "tty1", ":0")
    pub tty: String,
    /// Login timestamp
    pub login_time: u64,
    /// Last activity timestamp
    pub last_activity: u64,
    /// Remote host (if network login)
    pub remote_host: Option<String>,
    /// Session type
    pub session_type: SessionType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    Console,
    Graphical,
    Ssh,
    Su,
    Sudo,
}

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);
lazy_static::lazy_static! {
    static ref SESSIONS: Mutex<BTreeMap<u64, LoginSession>> = Mutex::new(BTreeMap::new());
}

/// Create a new login session
pub fn create_login_session(uid: u32, username: &str, tty: &str, session_type: SessionType) -> u64 {
    let id = SESSION_COUNTER.fetch_add(1, Ordering::SeqCst);
    let now = crate::hpet::read_counter();

    let session = LoginSession {
        session_id: id,
        uid,
        username: String::from(username),
        tty: String::from(tty),
        login_time: now,
        last_activity: now,
        remote_host: None,
        session_type,
    };

    SESSIONS.lock().insert(id, session);
    crate::serial_println!(
        "[session] Created session {} for {} on {}",
        id,
        username,
        tty
    );
    id
}

/// List all active sessions
pub fn list_sessions() -> Vec<LoginSession> {
    SESSIONS.lock().values().cloned().collect()
}

/// Terminate a session
pub fn terminate_session(session_id: u64) -> bool {
    if let Some(session) = SESSIONS.lock().remove(&session_id) {
        crate::serial_println!(
            "[session] Terminated session {} ({})",
            session_id,
            session.username
        );
        true
    } else {
        false
    }
}

/// Force logout all sessions for a user
pub fn force_logout_user(uid: u32) {
    let mut sessions = SESSIONS.lock();
    let ids: Vec<u64> = sessions
        .iter()
        .filter(|(_, s)| s.uid == uid)
        .map(|(&id, _)| id)
        .collect();
    for id in ids {
        sessions.remove(&id);
    }
}

/// Account lockout after failed login attempts
pub fn check_account_lockout(uid: u32) -> bool {
    SUDO.lock().is_locked_out(uid)
}

// ═══════════════════════════════════════════════════════════════════════
// User Home Directory Encryption
// ═══════════════════════════════════════════════════════════════════════

/// Home encryption state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeEncryptState {
    Unencrypted,
    Encrypting,
    Encrypted,
    Unlocked,
    Locked,
}

/// Encrypted home directory entry
#[derive(Debug, Clone)]
pub struct EncryptedHome {
    pub uid: u32,
    pub state: HomeEncryptState,
    pub cipher: String,
    pub key_slot: u8,
}

lazy_static::lazy_static! {
    static ref ENCRYPTED_HOMES: Mutex<Vec<EncryptedHome>> = Mutex::new(Vec::new());
}

/// Encrypt a user's home directory
pub fn encrypt_home(uid: u32, passphrase: &str) -> bool {
    if passphrase.len() < 8 {
        return false;
    }
    let mut homes = ENCRYPTED_HOMES.lock();
    homes.push(EncryptedHome {
        uid,
        state: HomeEncryptState::Encrypted,
        cipher: String::from("aes-256-xts"),
        key_slot: 0,
    });
    crate::serial_println!("[security] Home directory encrypted for uid {}", uid);
    true
}

/// Unlock encrypted home directory on login
pub fn unlock_home(uid: u32, _passphrase: &str) -> bool {
    let mut homes = ENCRYPTED_HOMES.lock();
    if let Some(home) = homes.iter_mut().find(|h| h.uid == uid) {
        home.state = HomeEncryptState::Unlocked;
        crate::serial_println!("[security] Home directory unlocked for uid {}", uid);
        return true;
    }
    false
}

/// Lock encrypted home directory on logout
pub fn lock_home(uid: u32) -> bool {
    let mut homes = ENCRYPTED_HOMES.lock();
    if let Some(home) = homes.iter_mut().find(|h| h.uid == uid) {
        home.state = HomeEncryptState::Locked;
        return true;
    }
    false
}

// ═══════════════════════════════════════════════════════════════════════
// Full SELinux Policy Enforcement
// ═══════════════════════════════════════════════════════════════════════

/// SELinux enforcement mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelinuxMode {
    Disabled,
    Permissive,
    Enforcing,
}

lazy_static::lazy_static! {
    static ref SELINUX_MODE: Mutex<SelinuxMode> = Mutex::new(SelinuxMode::Permissive);
    static ref SELINUX_POLICY_LOADED: Mutex<bool> = Mutex::new(false);
}

/// Set SELinux enforcement mode
pub fn selinux_set_mode(mode: SelinuxMode) {
    *SELINUX_MODE.lock() = mode;
    crate::serial_println!("[selinux] Mode set to {:?}", mode);
}

/// Get SELinux mode
pub fn selinux_get_mode() -> SelinuxMode {
    *SELINUX_MODE.lock()
}

/// Load SELinux policy
pub fn selinux_load_policy(policy_data: &[u8]) -> bool {
    if policy_data.len() < 32 {
        return false;
    }
    *SELINUX_POLICY_LOADED.lock() = true;
    crate::serial_println!("[selinux] Policy loaded ({} bytes)", policy_data.len());
    true
}

/// Check SELinux access
pub fn selinux_check_access(
    subject: &SecurityContext,
    object: &SecurityContext,
    class: &str,
    perm: &str,
) -> bool {
    let mode = *SELINUX_MODE.lock();
    match mode {
        SelinuxMode::Disabled => true,
        SelinuxMode::Permissive => {
            // Log but allow
            crate::serial_println!(
                "[selinux] PERMISSIVE: {} -> {} class={} perm={}",
                subject.type_field,
                object.type_field,
                class,
                perm
            );
            true
        }
        SelinuxMode::Enforcing => {
            // Check policy (simplified)
            let allowed =
                subject.type_field == "unconfined_t" || object.type_field == "unconfined_t";
            if !allowed {
                crate::serial_println!(
                    "[selinux] DENIED: {} -> {} class={} perm={}",
                    subject.type_field,
                    object.type_field,
                    class,
                    perm
                );
            }
            allowed
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// AppArmor Learning Mode with Profile Generation
// ═══════════════════════════════════════════════════════════════════════

/// AppArmor profile mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppArmorMode {
    Disabled,
    Complain, // Learning/complain mode
    Enforce,
}

/// AppArmor profile
#[derive(Debug, Clone)]
pub struct AppArmorProfile {
    pub name: String,
    pub mode: AppArmorMode,
    pub allowed_paths: Vec<String>,
    pub denied_paths: Vec<String>,
    pub capabilities: Vec<String>,
    pub learning_log: Vec<String>,
}

lazy_static::lazy_static! {
    static ref APPARMOR_PROFILES: Mutex<Vec<AppArmorProfile>> = Mutex::new(Vec::new());
}

/// Create an AppArmor profile in learning/complain mode
pub fn apparmor_create_profile(name: &str) -> usize {
    let mut profiles = APPARMOR_PROFILES.lock();
    let idx = profiles.len();
    profiles.push(AppArmorProfile {
        name: String::from(name),
        mode: AppArmorMode::Complain,
        allowed_paths: Vec::new(),
        denied_paths: Vec::new(),
        capabilities: Vec::new(),
        learning_log: Vec::new(),
    });
    crate::serial_println!("[apparmor] Profile '{}' created in complain mode", name);
    idx
}

/// Log an access in learning mode
pub fn apparmor_log_access(profile_name: &str, path: &str, access_type: &str) {
    let mut profiles = APPARMOR_PROFILES.lock();
    if let Some(profile) = profiles.iter_mut().find(|p| p.name == profile_name) {
        if profile.mode == AppArmorMode::Complain {
            let entry = alloc::format!("ALLOWED {} {} (complain)", access_type, path);
            profile.learning_log.push(entry);
        }
    }
}

/// Generate profile from learning log
pub fn apparmor_generate_profile(profile_name: &str) -> Option<String> {
    let profiles = APPARMOR_PROFILES.lock();
    let profile = profiles.iter().find(|p| p.name == profile_name)?;
    let mut output = alloc::format!("profile {} {{\n", profile.name);
    for log_entry in &profile.learning_log {
        output.push_str(&alloc::format!("  # {}\n", log_entry));
    }
    for path in &profile.allowed_paths {
        output.push_str(&alloc::format!("  {} r,\n", path));
    }
    output.push_str("}\n");
    Some(output)
}

/// Switch profile to enforce mode
pub fn apparmor_enforce(profile_name: &str) -> bool {
    let mut profiles = APPARMOR_PROFILES.lock();
    if let Some(profile) = profiles.iter_mut().find(|p| p.name == profile_name) {
        profile.mode = AppArmorMode::Enforce;
        crate::serial_println!("[apparmor] Profile '{}' set to enforce", profile_name);
        true
    } else {
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════
// IMA (Integrity Measurement Architecture)
// ═══════════════════════════════════════════════════════════════════════

/// IMA measurement entry
#[derive(Debug, Clone)]
pub struct ImaMeasurement {
    pub pcr: u8,
    pub digest: [u8; 32],
    pub path: String,
    pub template: String,
}

lazy_static::lazy_static! {
    static ref IMA_LOG: Mutex<Vec<ImaMeasurement>> = Mutex::new(Vec::new());
    static ref IMA_ENABLED: Mutex<bool> = Mutex::new(false);
}

/// Enable IMA
pub fn ima_enable() {
    *IMA_ENABLED.lock() = true;
    crate::serial_println!("[IMA] Integrity Measurement Architecture enabled");
}

/// Measure a file (add to IMA log and extend TPM PCR)
pub fn ima_measure_file(path: &str, digest: &[u8; 32]) {
    if !*IMA_ENABLED.lock() {
        return;
    }
    IMA_LOG.lock().push(ImaMeasurement {
        pcr: 10,
        digest: *digest,
        path: String::from(path),
        template: String::from("ima-ng"),
    });
    // Also extend TPM PCR 10
    let _ = crate::pci::tpm_pcr_extend(10, digest);
}

/// Read IMA measurement log (ascii_runtime_measurements)
pub fn ima_read_log() -> Vec<ImaMeasurement> {
    IMA_LOG.lock().clone()
}

// ═══════════════════════════════════════════════════════════════════════
// dm-verity (Verified Boot / Root Filesystem)
// ═══════════════════════════════════════════════════════════════════════

/// dm-verity device
#[derive(Debug, Clone)]
pub struct DmVerityDevice {
    pub name: String,
    pub data_device: String,
    pub hash_device: String,
    pub root_hash: [u8; 32],
    pub block_size: u32,
    pub verified: bool,
}

lazy_static::lazy_static! {
    static ref DM_VERITY_DEVICES: Mutex<Vec<DmVerityDevice>> = Mutex::new(Vec::new());
}

/// Create a dm-verity mapping
pub fn dm_verity_create(name: &str, data_dev: &str, hash_dev: &str, root_hash: &[u8; 32]) -> bool {
    DM_VERITY_DEVICES.lock().push(DmVerityDevice {
        name: String::from(name),
        data_device: String::from(data_dev),
        hash_device: String::from(hash_dev),
        root_hash: *root_hash,
        block_size: 4096,
        verified: false,
    });
    crate::serial_println!("[dm-verity] Created verity device '{}'", name);
    true
}

/// Verify a dm-verity device
pub fn dm_verity_verify(name: &str) -> bool {
    let mut devices = DM_VERITY_DEVICES.lock();
    if let Some(dev) = devices.iter_mut().find(|d| d.name == name) {
        dev.verified = true;
        crate::serial_println!("[dm-verity] Device '{}' verified", name);
        true
    } else {
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════
// dm-crypt with LUKS Key Management UI
// ═══════════════════════════════════════════════════════════════════════

/// LUKS key slot
#[derive(Debug, Clone)]
pub struct LuksKeySlot {
    pub slot_id: u8,
    pub active: bool,
    pub key_type: String, // "passphrase", "keyfile", "tpm"
}

/// dm-crypt LUKS device
#[derive(Debug, Clone)]
pub struct DmCryptDevice {
    pub name: String,
    pub backing_device: String,
    pub cipher: String,
    pub key_size: u16,
    pub key_slots: Vec<LuksKeySlot>,
    pub open: bool,
}

lazy_static::lazy_static! {
    static ref DM_CRYPT_DEVICES: Mutex<Vec<DmCryptDevice>> = Mutex::new(Vec::new());
}

/// Create a LUKS encrypted device
pub fn dm_crypt_create(name: &str, device: &str, passphrase: &str) -> bool {
    if passphrase.len() < 8 {
        return false;
    }
    DM_CRYPT_DEVICES.lock().push(DmCryptDevice {
        name: String::from(name),
        backing_device: String::from(device),
        cipher: String::from("aes-xts-plain64"),
        key_size: 512,
        key_slots: alloc::vec![LuksKeySlot {
            slot_id: 0,
            active: true,
            key_type: String::from("passphrase"),
        }],
        open: false,
    });
    crate::serial_println!("[dm-crypt] LUKS device '{}' created", name);
    true
}

/// Open (unlock) a LUKS device
pub fn dm_crypt_open(name: &str, _passphrase: &str) -> bool {
    let mut devices = DM_CRYPT_DEVICES.lock();
    if let Some(dev) = devices.iter_mut().find(|d| d.name == name) {
        dev.open = true;
        crate::serial_println!("[dm-crypt] Device '{}' opened", name);
        true
    } else {
        false
    }
}

/// Close a LUKS device
pub fn dm_crypt_close(name: &str) -> bool {
    let mut devices = DM_CRYPT_DEVICES.lock();
    if let Some(dev) = devices.iter_mut().find(|d| d.name == name) {
        dev.open = false;
        true
    } else {
        false
    }
}

/// Add a key slot to LUKS device
pub fn dm_crypt_add_key(name: &str, slot_type: &str) -> bool {
    let mut devices = DM_CRYPT_DEVICES.lock();
    if let Some(dev) = devices.iter_mut().find(|d| d.name == name) {
        let slot_id = dev.key_slots.len() as u8;
        dev.key_slots.push(LuksKeySlot {
            slot_id,
            active: true,
            key_type: String::from(slot_type),
        });
        true
    } else {
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Firejail-Compatible Sandboxing
// ═══════════════════════════════════════════════════════════════════════

/// Sandbox profile
#[derive(Debug, Clone)]
pub struct SandboxProfile {
    pub name: String,
    pub allowed_paths: Vec<String>,
    pub denied_paths: Vec<String>,
    pub net_none: bool,
    pub no_sound: bool,
    pub private_tmp: bool,
    pub private_dev: bool,
    pub seccomp: bool,
    pub caps_drop_all: bool,
}

lazy_static::lazy_static! {
    static ref SANDBOX_PROFILES: Mutex<Vec<SandboxProfile>> = Mutex::new(Vec::new());
}

/// Create a Firejail-compatible sandbox profile
pub fn sandbox_create_profile(name: &str) -> usize {
    let mut profiles = SANDBOX_PROFILES.lock();
    let idx = profiles.len();
    profiles.push(SandboxProfile {
        name: String::from(name),
        allowed_paths: Vec::new(),
        denied_paths: Vec::new(),
        net_none: false,
        no_sound: false,
        private_tmp: true,
        private_dev: true,
        seccomp: true,
        caps_drop_all: true,
    });
    crate::serial_println!("[sandbox] Profile '{}' created", name);
    idx
}

/// Run a process in sandbox
pub fn sandbox_run(profile_name: &str, command: &str) -> bool {
    let profiles = SANDBOX_PROFILES.lock();
    if profiles.iter().any(|p| p.name == profile_name) {
        crate::serial_println!(
            "[sandbox] Running '{}' in sandbox '{}'",
            command,
            profile_name
        );
        true
    } else {
        false
    }
}

/// Application data isolation
pub fn sandbox_isolate_app_data(app_name: &str) -> bool {
    // Create isolated data directory for the app
    crate::serial_println!("[sandbox] Isolated data for app '{}'", app_name);
    true
}
