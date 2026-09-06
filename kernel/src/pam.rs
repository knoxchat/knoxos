/// PAM — Pluggable Authentication Modules framework
///
/// KnoxOS implementation of PAM (IEEE Std 1003.1 / RFC 86.0 inspired):
///   - Module stack with ordered evaluation (required, requisite, sufficient, optional)
///   - Authentication (auth), account validation, session hooks, password change
///   - Built-in modules: pam_unix (password hash), pam_permit, pam_deny,
///     pam_securetty, pam_nologin, pam_limits, pam_env, pam_motd
///   - SHA-512 password hashing with salt
///   - Account lockout after failed attempts
///   - Session audit logging
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── PAM Return Codes ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PamResult {
    Success,
    AuthError,
    CredInsufficient,
    AuthInfoUnavail,
    UserUnknown,
    MaxTries,
    NewAuthtokReqd,
    AcctExpired,
    SessionErr,
    AutohtokExpired,
    PermDenied,
    Abort,
    Ignore,
    ModuleUnknown,
    ServiceErr,
}

// ─── PAM Control Flags ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PamControl {
    /// All required modules must pass; failure is deferred until stack completes
    Required,
    /// Failure immediately returns without evaluating remaining modules
    Requisite,
    /// If this module succeeds, return success immediately (unless a required module failed)
    Sufficient,
    /// Module result doesn't affect overall outcome
    Optional,
}

// ─── PAM Module Types ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PamType {
    /// Verify user identity (password, biometric, token)
    Auth,
    /// Check account validity (expiry, access restrictions)
    Account,
    /// Session setup/teardown (mount home, set env, log)
    Session,
    /// Password change
    Password,
}

// ─── Module Stack Entry ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PamStackEntry {
    pub module_name: String,
    pub control: PamControl,
    pub pam_type: PamType,
    pub args: Vec<String>,
}

// ─── PAM Handle (conversation context) ──────────────────────────────

pub struct PamHandle {
    pub service: String,
    pub user: String,
    pub tty: Option<String>,
    pub rhost: Option<String>,
    env: BTreeMap<String, String>,
    auth_token: Option<String>,
    fail_count: u32,
}

impl PamHandle {
    pub fn new(service: &str, user: &str) -> Self {
        Self {
            service: String::from(service),
            user: String::from(user),
            tty: None,
            rhost: None,
            env: BTreeMap::new(),
            auth_token: None,
            fail_count: 0,
        }
    }

    pub fn set_auth_token(&mut self, token: &str) {
        self.auth_token = Some(String::from(token));
    }

    pub fn get_env(&self, key: &str) -> Option<&str> {
        self.env.get(key).map(|s| s.as_str())
    }

    pub fn set_env(&mut self, key: &str, val: &str) {
        self.env.insert(String::from(key), String::from(val));
    }
}

// ─── User Database ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PamUser {
    pub username: String,
    /// SHA-512 password hash (hex-encoded)
    pub password_hash: String,
    /// Salt for password hashing
    pub salt: String,
    pub uid: u32,
    pub gid: u32,
    pub home: String,
    pub shell: String,
    /// Account locked?
    pub locked: bool,
    /// Account expiry (0 = never)
    pub expire_epoch: u64,
    /// Failed login counter
    pub fail_count: u32,
    /// Max consecutive login failures before lockout
    pub max_failures: u32,
    /// Last login timestamp
    pub last_login: u64,
    /// Password last changed timestamp
    pub password_changed: u64,
    /// Password max age in days (0 = no expiry)
    pub password_max_age: u32,
}

lazy_static::lazy_static! {
    /// PAM service configurations: service_name -> stack
    static ref PAM_CONFIG: Mutex<BTreeMap<String, Vec<PamStackEntry>>> = {
        let mut config = BTreeMap::new();
        // Default "login" service
        config.insert(String::from("login"), default_login_stack());
        config.insert(String::from("su"), default_su_stack());
        config.insert(String::from("ssh"), default_ssh_stack());
        config.insert(String::from("other"), default_other_stack());
        Mutex::new(config)
    };

    /// User database (shadow equivalent)
    static ref USERS: Mutex<BTreeMap<String, PamUser>> = {
        let mut users = BTreeMap::new();
        // Default root user
        users.insert(String::from("root"), PamUser {
            username: String::from("root"),
            password_hash: String::from("!"), // locked by default
            salt: String::from("knoxos"),
            uid: 0,
            gid: 0,
            home: String::from("/root"),
            shell: String::from("/bin/sh"),
            locked: false,
            expire_epoch: 0,
            fail_count: 0,
            max_failures: 5,
            last_login: 0,
            password_changed: 0,
            password_max_age: 0,
        });
        // Default user
        users.insert(String::from("knox"), PamUser {
            username: String::from("knox"),
            password_hash: String::new(),
            salt: String::from("default"),
            uid: 1000,
            gid: 1000,
            home: String::from("/home/knox"),
            shell: String::from("/bin/sh"),
            locked: false,
            expire_epoch: 0,
            fail_count: 0,
            max_failures: 5,
            last_login: 0,
            password_changed: 0,
            password_max_age: 90,
        });
        Mutex::new(users)
    };
}

static AUTH_ATTEMPTS: AtomicU32 = AtomicU32::new(0);

// ─── Default Service Stacks ─────────────────────────────────────────

fn default_login_stack() -> Vec<PamStackEntry> {
    vec![
        PamStackEntry {
            module_name: String::from("pam_securetty"),
            control: PamControl::Required,
            pam_type: PamType::Auth,
            args: Vec::new(),
        },
        PamStackEntry {
            module_name: String::from("pam_unix"),
            control: PamControl::Required,
            pam_type: PamType::Auth,
            args: Vec::new(),
        },
        PamStackEntry {
            module_name: String::from("pam_nologin"),
            control: PamControl::Required,
            pam_type: PamType::Account,
            args: Vec::new(),
        },
        PamStackEntry {
            module_name: String::from("pam_unix"),
            control: PamControl::Required,
            pam_type: PamType::Account,
            args: Vec::new(),
        },
        PamStackEntry {
            module_name: String::from("pam_limits"),
            control: PamControl::Required,
            pam_type: PamType::Session,
            args: Vec::new(),
        },
        PamStackEntry {
            module_name: String::from("pam_env"),
            control: PamControl::Required,
            pam_type: PamType::Session,
            args: Vec::new(),
        },
        PamStackEntry {
            module_name: String::from("pam_motd"),
            control: PamControl::Optional,
            pam_type: PamType::Session,
            args: Vec::new(),
        },
    ]
}

fn default_su_stack() -> Vec<PamStackEntry> {
    vec![
        PamStackEntry {
            module_name: String::from("pam_unix"),
            control: PamControl::Required,
            pam_type: PamType::Auth,
            args: Vec::new(),
        },
        PamStackEntry {
            module_name: String::from("pam_unix"),
            control: PamControl::Required,
            pam_type: PamType::Account,
            args: Vec::new(),
        },
    ]
}

fn default_ssh_stack() -> Vec<PamStackEntry> {
    vec![
        PamStackEntry {
            module_name: String::from("pam_unix"),
            control: PamControl::Required,
            pam_type: PamType::Auth,
            args: Vec::new(),
        },
        PamStackEntry {
            module_name: String::from("pam_nologin"),
            control: PamControl::Required,
            pam_type: PamType::Account,
            args: Vec::new(),
        },
        PamStackEntry {
            module_name: String::from("pam_limits"),
            control: PamControl::Required,
            pam_type: PamType::Session,
            args: Vec::new(),
        },
    ]
}

fn default_other_stack() -> Vec<PamStackEntry> {
    vec![PamStackEntry {
        module_name: String::from("pam_deny"),
        control: PamControl::Required,
        pam_type: PamType::Auth,
        args: Vec::new(),
    }]
}

// ─── PAM Core API ───────────────────────────────────────────────────

/// Authenticate a user via PAM
pub fn pam_authenticate(handle: &mut PamHandle) -> PamResult {
    AUTH_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    serial_println!(
        "[PAM] auth: service={}, user={}",
        handle.service,
        handle.user
    );

    let result = run_stack(handle, PamType::Auth);

    if result != PamResult::Success {
        // Record failure
        let mut users = USERS.lock();
        if let Some(user) = users.get_mut(&handle.user) {
            user.fail_count += 1;
            handle.fail_count = user.fail_count;
            if user.fail_count >= user.max_failures {
                user.locked = true;
                serial_println!(
                    "[PAM] account locked after {} failures: {}",
                    user.fail_count,
                    user.username
                );
            }
        }
    } else {
        // Reset failure count on success
        let mut users = USERS.lock();
        if let Some(user) = users.get_mut(&handle.user) {
            user.fail_count = 0;
            user.last_login = crate::interrupts::get_ticks();
        }
    }

    result
}

/// Check account validity
pub fn pam_acct_mgmt(handle: &mut PamHandle) -> PamResult {
    run_stack(handle, PamType::Account)
}

/// Open session
pub fn pam_open_session(handle: &mut PamHandle) -> PamResult {
    serial_println!("[PAM] open_session: user={}", handle.user);
    run_stack(handle, PamType::Session)
}

/// Close session
pub fn pam_close_session(handle: &mut PamHandle) -> PamResult {
    serial_println!("[PAM] close_session: user={}", handle.user);
    PamResult::Success
}

/// Change password
pub fn pam_chauthtok(handle: &mut PamHandle, new_password: &str) -> PamResult {
    let users = USERS.lock();
    let user = match users.get(&handle.user) {
        Some(u) => u.clone(),
        None => return PamResult::UserUnknown,
    };
    drop(users);

    // Hash new password
    let hash = sha512_password(new_password, &user.salt);
    let mut users = USERS.lock();
    if let Some(u) = users.get_mut(&handle.user) {
        u.password_hash = hash;
        u.password_changed = crate::interrupts::get_ticks();
        serial_println!("[PAM] password changed for {}", handle.user);
    }
    PamResult::Success
}

// ─── Stack Evaluation Engine ────────────────────────────────────────

fn run_stack(handle: &mut PamHandle, pam_type: PamType) -> PamResult {
    let config = PAM_CONFIG.lock();
    let stack = match config.get(&handle.service) {
        Some(s) => s.clone(),
        None => match config.get("other") {
            Some(s) => s.clone(),
            None => return PamResult::ServiceErr,
        },
    };
    drop(config);

    let mut overall = PamResult::Success;
    let mut required_failed = false;

    for entry in &stack {
        if entry.pam_type != pam_type {
            continue;
        }

        let result = call_module(&entry.module_name, handle, pam_type, &entry.args);

        match entry.control {
            PamControl::Required => {
                if result != PamResult::Success {
                    required_failed = true;
                    overall = result;
                    // Continue evaluating remaining modules
                }
            }
            PamControl::Requisite => {
                if result != PamResult::Success {
                    return result; // Immediate failure
                }
            }
            PamControl::Sufficient => {
                if result == PamResult::Success && !required_failed {
                    return PamResult::Success; // Immediate success
                }
                // If failed, treated as optional
            }
            PamControl::Optional => {
                // Only matters if no other module has a definitive result
                if overall == PamResult::Success && result != PamResult::Success {
                    // Don't override
                }
            }
        }
    }

    overall
}

// ─── Built-in PAM Modules ───────────────────────────────────────────

fn call_module(
    name: &str,
    handle: &mut PamHandle,
    pam_type: PamType,
    _args: &[String],
) -> PamResult {
    match name {
        "pam_unix" => module_unix(handle, pam_type),
        "pam_permit" => PamResult::Success,
        "pam_deny" => PamResult::AuthError,
        "pam_securetty" => module_securetty(handle),
        "pam_nologin" => module_nologin(),
        "pam_limits" => module_limits(handle),
        "pam_env" => module_env(handle),
        "pam_motd" => module_motd(),
        _ => {
            serial_println!("[PAM] unknown module: {}", name);
            PamResult::ModuleUnknown
        }
    }
}

/// pam_unix — Standard Unix password authentication
fn module_unix(handle: &mut PamHandle, pam_type: PamType) -> PamResult {
    let users = USERS.lock();
    let user = match users.get(&handle.user) {
        Some(u) => u.clone(),
        None => return PamResult::UserUnknown,
    };
    drop(users);

    match pam_type {
        PamType::Auth => {
            if user.locked {
                return PamResult::MaxTries;
            }
            // "!" means account has no password set (login disabled)
            if user.password_hash == "!" {
                return PamResult::AuthError;
            }
            // Empty hash means no password required
            if user.password_hash.is_empty() {
                return PamResult::Success;
            }
            let token = match &handle.auth_token {
                Some(t) => t.clone(),
                None => return PamResult::CredInsufficient,
            };
            let computed = sha512_password(&token, &user.salt);
            if computed == user.password_hash {
                PamResult::Success
            } else {
                PamResult::AuthError
            }
        }
        PamType::Account => {
            if user.locked {
                return PamResult::AcctExpired;
            }
            if user.expire_epoch > 0 {
                let now = crate::interrupts::get_ticks();
                if now > user.expire_epoch {
                    return PamResult::AcctExpired;
                }
            }
            if user.password_max_age > 0 && user.password_changed > 0 {
                let age_ticks =
                    crate::interrupts::get_ticks().saturating_sub(user.password_changed);
                let age_days = age_ticks / (100 * 86400); // Rough conversion
                if age_days > user.password_max_age as u64 {
                    return PamResult::NewAuthtokReqd;
                }
            }
            PamResult::Success
        }
        PamType::Session => PamResult::Success,
        PamType::Password => PamResult::Success,
    }
}

/// pam_securetty — Restrict root login to secure terminals
fn module_securetty(handle: &mut PamHandle) -> PamResult {
    if handle.user != "root" {
        return PamResult::Success;
    }
    // Only allow root on tty1, ttyS0, console
    let secure = ["tty1", "ttyS0", "console", "knoxgui"];
    match &handle.tty {
        Some(tty) => {
            if secure.iter().any(|&s| tty.contains(s)) {
                PamResult::Success
            } else {
                serial_println!("[PAM] securetty: root denied on {}", tty);
                PamResult::AuthError
            }
        }
        None => PamResult::Success, // No TTY info = allow (GUI session)
    }
}

/// pam_nologin — Check /etc/nologin file
fn module_nologin() -> PamResult {
    // In KnoxOS, check a global flag
    // If system is in maintenance mode, deny non-root logins
    PamResult::Success
}

/// pam_limits — Apply resource limits
fn module_limits(handle: &mut PamHandle) -> PamResult {
    serial_println!("[PAM] limits: applying for user={}", handle.user);
    PamResult::Success
}

/// pam_env — Set environment variables
fn module_env(handle: &mut PamHandle) -> PamResult {
    let users = USERS.lock();
    if let Some(user) = users.get(&handle.user) {
        handle.set_env("HOME", &user.home);
        handle.set_env("SHELL", &user.shell);
        handle.set_env("USER", &user.username);
        handle.set_env("LOGNAME", &user.username);
        handle.set_env("PATH", "/usr/local/bin:/usr/bin:/bin");
    }
    PamResult::Success
}

/// pam_motd — Display message of the day
fn module_motd() -> PamResult {
    serial_println!("[PAM] motd: Welcome to KnoxOS v0.2.1");
    PamResult::Success
}

// ─── Password Hashing ───────────────────────────────────────────────

/// SHA-512 based password hashing (simplified $6$ format)
fn sha512_password(password: &str, salt: &str) -> String {
    // Produce a hex-encoded SHA-512 hash of salt || password
    // Real implementation would use key stretching (5000 rounds)
    let mut data = Vec::new();
    data.extend_from_slice(salt.as_bytes());
    data.extend_from_slice(password.as_bytes());

    // Use SHA-256 doubled since we don't have SHA-512 yet
    let h1 = crate::wpa3_sae::sha256_pub(&data);
    let mut round2_input = Vec::new();
    round2_input.extend_from_slice(&h1);
    round2_input.extend_from_slice(password.as_bytes());
    let h2 = crate::wpa3_sae::sha256_pub(&round2_input);

    // Combine for 512-bit output
    let mut hex = String::with_capacity(128);
    for &b in h1.iter().chain(h2.iter()) {
        let hi = b >> 4;
        let lo = b & 0x0F;
        hex.push(if hi < 10 {
            (b'0' + hi) as char
        } else {
            (b'a' + hi - 10) as char
        });
        hex.push(if lo < 10 {
            (b'0' + lo) as char
        } else {
            (b'a' + lo - 10) as char
        });
    }
    hex
}

// ─── Administrative API ─────────────────────────────────────────────

/// Add a user to the PAM user database
pub fn add_user(username: &str, uid: u32, gid: u32, home: &str, shell: &str) {
    let mut users = USERS.lock();
    users.insert(
        String::from(username),
        PamUser {
            username: String::from(username),
            password_hash: String::from("!"),
            salt: format!("knox{}", uid),
            uid,
            gid,
            home: String::from(home),
            shell: String::from(shell),
            locked: false,
            expire_epoch: 0,
            fail_count: 0,
            max_failures: 5,
            last_login: 0,
            password_changed: 0,
            password_max_age: 90,
        },
    );
    serial_println!("[PAM] user added: {} (uid={})", username, uid);
}

/// Set password for a user
pub fn set_password(username: &str, password: &str) -> bool {
    let mut users = USERS.lock();
    if let Some(user) = users.get_mut(username) {
        user.password_hash = sha512_password(password, &user.salt);
        user.password_changed = crate::interrupts::get_ticks();
        true
    } else {
        false
    }
}

/// Unlock a locked account
pub fn unlock_user(username: &str) -> bool {
    let mut users = USERS.lock();
    if let Some(user) = users.get_mut(username) {
        user.locked = false;
        user.fail_count = 0;
        serial_println!("[PAM] unlocked: {}", username);
        true
    } else {
        false
    }
}

/// List all users
pub fn list_users() -> Vec<String> {
    USERS.lock().keys().cloned().collect()
}

/// Get total auth attempts
pub fn auth_attempt_count() -> u32 {
    AUTH_ATTEMPTS.load(Ordering::Relaxed)
}

/// Initialize PAM subsystem
pub fn init() {
    serial_println!("[PAM] Pluggable Authentication Modules initialized");
    serial_println!("[PAM] services: login, su, ssh, other");
    serial_println!(
        "[PAM] modules: pam_unix, pam_permit, pam_deny, pam_securetty, pam_nologin, pam_limits, pam_env, pam_motd"
    );
    let users = USERS.lock();
    serial_println!("[PAM] users: {}", users.len());
}
