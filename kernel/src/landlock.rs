// landlock.rs — Landlock LSM for unprivileged sandboxing
// Linux 5.13+ access control for filesystem and networking

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

/// Landlock ABI version
pub const LANDLOCK_ABI_VERSION: u32 = 4;

/// Landlock rule types
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum LandlockRuleType {
    PathBeneath = 1,
    NetPort = 2,
}

/// Filesystem access rights (handled_access_fs)
pub const LANDLOCK_ACCESS_FS_EXECUTE: u64 = 1 << 0;
pub const LANDLOCK_ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
pub const LANDLOCK_ACCESS_FS_READ_FILE: u64 = 1 << 2;
pub const LANDLOCK_ACCESS_FS_READ_DIR: u64 = 1 << 3;
pub const LANDLOCK_ACCESS_FS_REMOVE_DIR: u64 = 1 << 4;
pub const LANDLOCK_ACCESS_FS_REMOVE_FILE: u64 = 1 << 5;
pub const LANDLOCK_ACCESS_FS_MAKE_CHAR: u64 = 1 << 6;
pub const LANDLOCK_ACCESS_FS_MAKE_DIR: u64 = 1 << 7;
pub const LANDLOCK_ACCESS_FS_MAKE_REG: u64 = 1 << 8;
pub const LANDLOCK_ACCESS_FS_MAKE_SOCK: u64 = 1 << 9;
pub const LANDLOCK_ACCESS_FS_MAKE_FIFO: u64 = 1 << 10;
pub const LANDLOCK_ACCESS_FS_MAKE_BLOCK: u64 = 1 << 11;
pub const LANDLOCK_ACCESS_FS_MAKE_SYM: u64 = 1 << 12;
pub const LANDLOCK_ACCESS_FS_REFER: u64 = 1 << 13;
pub const LANDLOCK_ACCESS_FS_TRUNCATE: u64 = 1 << 14;

/// Network access rights (handled_access_net)
pub const LANDLOCK_ACCESS_NET_BIND_TCP: u64 = 1 << 0;
pub const LANDLOCK_ACCESS_NET_CONNECT_TCP: u64 = 1 << 1;

/// All filesystem access rights
pub const LANDLOCK_ACCESS_FS_ALL: u64 = (1 << 15) - 1;

/// All network access rights
pub const LANDLOCK_ACCESS_NET_ALL: u64 = (1 << 2) - 1;

/// Ruleset creation flags
pub const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1 << 0;

/// Restrict self flags
pub const LANDLOCK_RESTRICT_SELF_FLAGS: u32 = 0;

/// Landlock ruleset attributes
#[derive(Debug, Clone)]
pub struct LandlockRulesetAttr {
    pub handled_access_fs: u64,
    pub handled_access_net: u64,
}

/// A path-beneath rule
#[derive(Debug, Clone)]
pub struct PathBeneathRule {
    pub allowed_access: u64,
    pub parent_path: String,
}

/// A network port rule
#[derive(Debug, Clone)]
pub struct NetPortRule {
    pub allowed_access: u64,
    pub port: u16,
}

/// A landlock rule
#[derive(Debug, Clone)]
pub enum LandlockRule {
    PathBeneath(PathBeneathRule),
    NetPort(NetPortRule),
}

/// A landlock ruleset
#[derive(Debug)]
pub struct LandlockRuleset {
    pub id: u64,
    pub handled_access_fs: u64,
    pub handled_access_net: u64,
    pub rules: Vec<LandlockRule>,
    pub enforced: bool,
    pub owner_pid: u64,
}

impl LandlockRuleset {
    pub fn new(id: u64, attr: &LandlockRulesetAttr) -> Self {
        LandlockRuleset {
            id,
            handled_access_fs: attr.handled_access_fs,
            handled_access_net: attr.handled_access_net,
            rules: Vec::new(),
            enforced: false,
            owner_pid: 0,
        }
    }

    /// Check if filesystem access is allowed
    pub fn check_fs_access(&self, path: &str, access: u64) -> bool {
        // Only check access types we handle
        let relevant = access & self.handled_access_fs;
        if relevant == 0 {
            return true; // Not handling this access type
        }

        // Check if any rule allows this access
        for rule in &self.rules {
            if let LandlockRule::PathBeneath(path_rule) = rule {
                if path.starts_with(&path_rule.parent_path)
                    && (path_rule.allowed_access & relevant) == relevant
                {
                    return true;
                }
            }
        }

        false // Denied by default if handled
    }

    /// Check if network access is allowed
    pub fn check_net_access(&self, port: u16, access: u64) -> bool {
        let relevant = access & self.handled_access_net;
        if relevant == 0 {
            return true;
        }

        for rule in &self.rules {
            if let LandlockRule::NetPort(net_rule) = rule {
                if net_rule.port == port && (net_rule.allowed_access & relevant) == relevant {
                    return true;
                }
            }
        }

        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LandlockError {
    NotFound,
    InvalidAttr,
    InvalidRule,
    AlreadyEnforced,
    PermDenied,
    NotSupported,
}

lazy_static! {
    static ref RULESETS: Mutex<BTreeMap<u64, LandlockRuleset>> = Mutex::new(BTreeMap::new());
    static ref NEXT_ID: Mutex<u64> = Mutex::new(1);
    /// Per-process active rulesets
    static ref PROCESS_RULESETS: Mutex<BTreeMap<u64, Vec<u64>>> = Mutex::new(BTreeMap::new());
}

/// landlock_create_ruleset — create a new ruleset
pub fn sys_landlock_create_ruleset(
    attr: &LandlockRulesetAttr,
    _size: usize,
    flags: u32,
) -> Result<u64, LandlockError> {
    // If LANDLOCK_CREATE_RULESET_VERSION flag is set, return ABI version
    if flags & LANDLOCK_CREATE_RULESET_VERSION != 0 {
        return Ok(LANDLOCK_ABI_VERSION as u64);
    }

    // Validate access masks
    if attr.handled_access_fs & !LANDLOCK_ACCESS_FS_ALL != 0 {
        return Err(LandlockError::InvalidAttr);
    }
    if attr.handled_access_net & !LANDLOCK_ACCESS_NET_ALL != 0 {
        return Err(LandlockError::InvalidAttr);
    }

    let mut next = NEXT_ID.lock();
    let id = *next;
    *next += 1;
    drop(next);

    let ruleset = LandlockRuleset::new(id, attr);
    RULESETS.lock().insert(id, ruleset);

    Ok(id)
}

/// landlock_add_rule — add a rule to a ruleset
pub fn sys_landlock_add_rule(
    ruleset_fd: u64,
    rule_type: LandlockRuleType,
    rule: LandlockRule,
    _flags: u32,
) -> Result<(), LandlockError> {
    let mut rulesets = RULESETS.lock();
    let ruleset = rulesets
        .get_mut(&ruleset_fd)
        .ok_or(LandlockError::NotFound)?;

    if ruleset.enforced {
        return Err(LandlockError::AlreadyEnforced);
    }

    // Validate rule type matches
    match (&rule, rule_type) {
        (LandlockRule::PathBeneath(_), LandlockRuleType::PathBeneath) => {}
        (LandlockRule::NetPort(_), LandlockRuleType::NetPort) => {}
        _ => return Err(LandlockError::InvalidRule),
    }

    ruleset.rules.push(rule);
    Ok(())
}

/// landlock_restrict_self — enforce a ruleset on the calling process
pub fn sys_landlock_restrict_self(
    ruleset_fd: u64,
    _flags: u32,
    pid: u64,
) -> Result<(), LandlockError> {
    let mut rulesets = RULESETS.lock();
    let ruleset = rulesets
        .get_mut(&ruleset_fd)
        .ok_or(LandlockError::NotFound)?;

    ruleset.enforced = true;
    ruleset.owner_pid = pid;
    drop(rulesets);

    // Add to process's active rulesets
    let mut proc_rulesets = PROCESS_RULESETS.lock();
    proc_rulesets.entry(pid).or_default().push(ruleset_fd);

    Ok(())
}

/// Check if a filesystem access is allowed for a process
pub fn check_process_fs_access(pid: u64, path: &str, access: u64) -> bool {
    let proc_rulesets = PROCESS_RULESETS.lock();
    let ruleset_ids = match proc_rulesets.get(&pid) {
        Some(ids) => ids.clone(),
        None => return true, // No restrictions
    };
    drop(proc_rulesets);

    let rulesets = RULESETS.lock();
    for id in &ruleset_ids {
        if let Some(ruleset) = rulesets.get(id) {
            if !ruleset.check_fs_access(path, access) {
                return false;
            }
        }
    }

    true
}

/// Check if a network access is allowed for a process
pub fn check_process_net_access(pid: u64, port: u16, access: u64) -> bool {
    let proc_rulesets = PROCESS_RULESETS.lock();
    let ruleset_ids = match proc_rulesets.get(&pid) {
        Some(ids) => ids.clone(),
        None => return true,
    };
    drop(proc_rulesets);

    let rulesets = RULESETS.lock();
    for id in &ruleset_ids {
        if let Some(ruleset) = rulesets.get(id) {
            if !ruleset.check_net_access(port, access) {
                return false;
            }
        }
    }

    true
}

/// Clean up rulesets when process exits
pub fn process_exit(pid: u64) {
    PROCESS_RULESETS.lock().remove(&pid);
}

/// List active rulesets for a process
pub fn list_process_rulesets(pid: u64) -> Vec<u64> {
    PROCESS_RULESETS
        .lock()
        .get(&pid)
        .cloned()
        .unwrap_or_default()
}

/// Initialize landlock subsystem
pub fn init() {
    crate::serial_println!(
        "  Landlock LSM initialized (ABI v{}, fs+net access control)",
        LANDLOCK_ABI_VERSION
    );
}
