/// SELinux — Security-Enhanced Linux Mandatory Access Control
///
/// Implements a Type Enforcement (TE) and Role-Based Access Control (RBAC)
/// security model compatible with Linux SELinux policies.
///
/// Features:
/// - Security contexts (user:role:type:level)
/// - Type Enforcement rules (allow, deny, auditallow, dontaudit)
/// - Role-Based Access Control
/// - Multi-Level Security (MLS) with sensitivity levels
/// - Object class and permission definitions
/// - Per-process security contexts
/// - File labeling and transition rules
/// - Enforcing / Permissive / Disabled modes
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── SELinux Modes ──────────────────────────────────────────────────

/// SELinux enforcement mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelinuxMode {
    /// Not enforcing, not logging
    Disabled,
    /// Logging violations but not enforcing
    Permissive,
    /// Fully enforcing policy
    Enforcing,
}

// ─── Security Context ───────────────────────────────────────────────

/// SELinux security context: user:role:type:level
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityContext {
    pub user: String,
    pub role: String,
    pub stype: String, // "type" is reserved
    pub level: String, // MLS level (e.g., "s0" or "s0-s0:c0.c1023")
}

impl SecurityContext {
    pub fn new(user: &str, role: &str, stype: &str, level: &str) -> Self {
        Self {
            user: String::from(user),
            role: String::from(role),
            stype: String::from(stype),
            level: String::from(level),
        }
    }

    /// Parse a context string "user:role:type:level"
    pub fn from_str(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(4, ':').collect();
        if parts.len() >= 3 {
            Some(Self {
                user: String::from(parts[0]),
                role: String::from(parts[1]),
                stype: String::from(parts[2]),
                level: if parts.len() >= 4 {
                    String::from(parts[3])
                } else {
                    String::from("s0")
                },
            })
        } else {
            None
        }
    }

    /// Format as "user:role:type:level"
    pub fn to_context_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.user);
        s.push(':');
        s.push_str(&self.role);
        s.push(':');
        s.push_str(&self.stype);
        s.push(':');
        s.push_str(&self.level);
        s
    }
}

// ─── Object Classes ─────────────────────────────────────────────────

/// SELinux object classes (subset of standard Linux classes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ObjectClass {
    File = 1,
    Dir = 2,
    Process = 3,
    Socket = 4,
    Filesystem = 5,
    Capability = 6,
    TcpSocket = 7,
    UdpSocket = 8,
    UnixStreamSocket = 9,
    UnixDgramSocket = 10,
    NetlinkSocket = 11,
    Ipc = 12,
    Sem = 13,
    Shm = 14,
    Msg = 15,
    Msgq = 16,
    Fd = 17,
    Node = 18,
    Netif = 19,
    LnkFile = 20,
    ChrFile = 21,
    BlkFile = 22,
    FifoFile = 23,
    SockFile = 24,
    Key = 25,
    System = 26,
    Security = 27,
    KernelService = 28,
    Bpf = 29,
    PerfEvent = 30,
}

// ─── Permissions ────────────────────────────────────────────────────

/// Permission bitmask for access vector
pub type AccessVector = u32;

/// Common file permissions
pub const PERM_READ: AccessVector = 1 << 0;
pub const PERM_WRITE: AccessVector = 1 << 1;
pub const PERM_EXECUTE: AccessVector = 1 << 2;
pub const PERM_CREATE: AccessVector = 1 << 3;
pub const PERM_GETATTR: AccessVector = 1 << 4;
pub const PERM_SETATTR: AccessVector = 1 << 5;
pub const PERM_UNLINK: AccessVector = 1 << 6;
pub const PERM_RENAME: AccessVector = 1 << 7;
pub const PERM_APPEND: AccessVector = 1 << 8;
pub const PERM_LOCK: AccessVector = 1 << 9;
pub const PERM_IOCTL: AccessVector = 1 << 10;
pub const PERM_LINK: AccessVector = 1 << 11;
pub const PERM_OPEN: AccessVector = 1 << 12;
pub const PERM_MAP: AccessVector = 1 << 13;

/// Process permissions
pub const PERM_FORK: AccessVector = 1 << 14;
pub const PERM_SIGNAL: AccessVector = 1 << 15;
pub const PERM_PTRACE: AccessVector = 1 << 16;
pub const PERM_TRANSITION: AccessVector = 1 << 17;
pub const PERM_SIGCHLD: AccessVector = 1 << 18;
pub const PERM_GETSCHED: AccessVector = 1 << 19;
pub const PERM_SETSCHED: AccessVector = 1 << 20;
pub const PERM_GETSESSION: AccessVector = 1 << 21;
pub const PERM_GETCAP: AccessVector = 1 << 22;
pub const PERM_SETCAP: AccessVector = 1 << 23;

/// Dir permissions
pub const PERM_ADD_NAME: AccessVector = 1 << 24;
pub const PERM_REMOVE_NAME: AccessVector = 1 << 25;
pub const PERM_SEARCH: AccessVector = 1 << 26;
pub const PERM_REPARENT: AccessVector = 1 << 27;
pub const PERM_RMDIR: AccessVector = 1 << 28;

/// Socket permissions
pub const PERM_BIND: AccessVector = 1 << 14;
pub const PERM_LISTEN: AccessVector = 1 << 15;
pub const PERM_ACCEPT: AccessVector = 1 << 16;
pub const PERM_CONNECT: AccessVector = 1 << 17;
pub const PERM_SENDTO: AccessVector = 1 << 18;
pub const PERM_RECVFROM: AccessVector = 1 << 19;
pub const PERM_SHUTDOWN: AccessVector = 1 << 20;

// ─── Policy Rules ───────────────────────────────────────────────────

/// Type of TE (Type Enforcement) rule
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleType {
    /// allow source target : class { perms };
    Allow,
    /// auditallow source target : class { perms };
    AuditAllow,
    /// dontaudit source target : class { perms };
    DontAudit,
    /// neverallow source target : class { perms };
    NeverAllow,
}

/// A Type Enforcement rule
#[derive(Debug, Clone)]
pub struct TeRule {
    pub rule_type: RuleType,
    pub source_type: String,
    pub target_type: String,
    pub class: ObjectClass,
    pub permissions: AccessVector,
}

/// A type transition rule: type_transition source target : class new_type;
#[derive(Debug, Clone)]
pub struct TypeTransition {
    pub source_type: String,
    pub target_type: String,
    pub class: ObjectClass,
    pub new_type: String,
}

/// Role allow rule: allow role1 role2;
#[derive(Debug, Clone)]
pub struct RoleAllow {
    pub source_role: String,
    pub target_role: String,
}

/// Role transition: role_transition source target : class new_role;
#[derive(Debug, Clone)]
pub struct RoleTransition {
    pub source_role: String,
    pub target_type: String,
    pub class: ObjectClass,
    pub new_role: String,
}

// ─── AVC (Access Vector Cache) ──────────────────────────────────────

/// AVC cache entry key
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct AvcKey {
    source: String,
    target: String,
    class: u32,
}

/// AVC cache entry
#[derive(Debug, Clone)]
struct AvcEntry {
    allowed: AccessVector,
    decided: AccessVector,
    audit_allow: AccessVector,
    audit_deny: AccessVector,
}

/// AVC denial record for audit
#[derive(Debug, Clone)]
pub struct AvcDenial {
    pub source_context: String,
    pub target_context: String,
    pub class: ObjectClass,
    pub requested: AccessVector,
    pub pid: u32,
    pub comm: String,
}

// ─── File Context ───────────────────────────────────────────────────

/// File context specification for labeling
#[derive(Debug, Clone)]
pub struct FileContext {
    pub path_regex: String,
    pub file_type: Option<ObjectClass>,
    pub context: SecurityContext,
}

// ─── SELinux Policy ─────────────────────────────────────────────────

/// Complete SELinux policy
pub struct SelinuxPolicy {
    /// TE rules
    pub te_rules: Vec<TeRule>,
    /// Type transitions
    pub type_transitions: Vec<TypeTransition>,
    /// Role allow rules
    pub role_allows: Vec<RoleAllow>,
    /// Role transitions
    pub role_transitions: Vec<RoleTransition>,
    /// File contexts for labeling
    pub file_contexts: Vec<FileContext>,
    /// Type definitions (type name → attributes)
    pub types: BTreeMap<String, Vec<String>>,
    /// Role definitions (role name → allowed types)
    pub roles: BTreeMap<String, Vec<String>>,
    /// User definitions (user name → allowed roles)
    pub users: BTreeMap<String, Vec<String>>,
    /// Boolean conditionals
    pub booleans: BTreeMap<String, bool>,
    /// MLS sensitivity levels
    pub sensitivities: Vec<String>,
    /// MLS categories
    pub categories: Vec<String>,
}

// ─── SELinux State ──────────────────────────────────────────────────

/// Global SELinux state
pub struct SelinuxState {
    pub mode: SelinuxMode,
    pub policy: SelinuxPolicy,
    /// Per-process security contexts (pid → context)
    pub process_contexts: BTreeMap<u32, SecurityContext>,
    /// File label cache (inode → context)
    pub file_labels: BTreeMap<u64, SecurityContext>,
    /// AVC cache
    avc_cache: BTreeMap<AvcKey, AvcEntry>,
    /// AVC statistics
    pub avc_lookups: u64,
    pub avc_hits: u64,
    pub avc_misses: u64,
    /// Denial log
    pub denials: Vec<AvcDenial>,
    /// Total denials count
    pub denial_count: u64,
}

lazy_static::lazy_static! {
    pub static ref SELINUX: Mutex<SelinuxState> = Mutex::new(SelinuxState::new());
}

impl SelinuxState {
    pub fn new() -> Self {
        Self {
            mode: SelinuxMode::Permissive,
            policy: SelinuxPolicy::default_policy(),
            process_contexts: BTreeMap::new(),
            file_labels: BTreeMap::new(),
            avc_cache: BTreeMap::new(),
            avc_lookups: 0,
            avc_hits: 0,
            avc_misses: 0,
            denials: Vec::new(),
            denial_count: 0,
        }
    }

    /// Get the current mode
    pub fn get_mode(&self) -> SelinuxMode {
        self.mode
    }

    /// Set the enforcement mode
    pub fn set_mode(&mut self, mode: SelinuxMode) {
        serial_println!("[SELinux] Mode changed: {:?} → {:?}", self.mode, mode);
        self.mode = mode;
        // Flush AVC on mode change
        self.avc_cache.clear();
    }

    /// Set security context for a process
    pub fn set_process_context(&mut self, pid: u32, ctx: SecurityContext) {
        self.process_contexts.insert(pid, ctx);
    }

    /// Get security context for a process
    pub fn get_process_context(&self, pid: u32) -> Option<&SecurityContext> {
        self.process_contexts.get(&pid)
    }

    /// Set security label for a file inode
    pub fn set_file_label(&mut self, ino: u64, ctx: SecurityContext) {
        self.file_labels.insert(ino, ctx);
    }

    /// Get security label for a file inode
    pub fn get_file_label(&self, ino: u64) -> Option<&SecurityContext> {
        self.file_labels.get(&ino)
    }

    /// Check access from source to target for the given class and permissions.
    /// Returns true if access is allowed.
    pub fn check_access(
        &mut self,
        source: &SecurityContext,
        target: &SecurityContext,
        class: ObjectClass,
        requested: AccessVector,
    ) -> bool {
        if self.mode == SelinuxMode::Disabled {
            return true;
        }

        self.avc_lookups += 1;

        // Check AVC cache first
        let key = AvcKey {
            source: source.stype.clone(),
            target: target.stype.clone(),
            class: class as u32,
        };

        if let Some(entry) = self.avc_cache.get(&key) {
            self.avc_hits += 1;
            let allowed = (entry.allowed & requested) == requested;
            if !allowed && self.mode == SelinuxMode::Enforcing {
                self.log_denial(source, target, class, requested);
                return false;
            }
            return allowed || self.mode == SelinuxMode::Permissive;
        }

        self.avc_misses += 1;

        // Compute access decision from policy
        let mut allowed: AccessVector = 0;
        let mut audit_allow: AccessVector = 0;
        let mut audit_deny: AccessVector = !0; // default: audit all denials

        for rule in &self.policy.te_rules {
            if rule.source_type == source.stype
                && rule.target_type == target.stype
                && rule.class == class
            {
                match rule.rule_type {
                    RuleType::Allow => {
                        allowed |= rule.permissions;
                    }
                    RuleType::AuditAllow => {
                        audit_allow |= rule.permissions;
                    }
                    RuleType::DontAudit => {
                        audit_deny &= !rule.permissions;
                    }
                    RuleType::NeverAllow => {
                        // Compile-time check; runtime we just don't grant
                    }
                }
            }
        }

        // Self-access: types always have basic access to themselves
        if source.stype == target.stype {
            allowed |= PERM_READ | PERM_GETATTR;
        }

        // Cache the result
        let entry = AvcEntry {
            allowed,
            decided: !0, // all bits decided
            audit_allow,
            audit_deny,
        };
        self.avc_cache.insert(key, entry);

        let granted = (allowed & requested) == requested;
        if !granted {
            if (audit_deny & requested) != 0 {
                self.log_denial(source, target, class, requested);
            }
            if self.mode == SelinuxMode::Enforcing {
                return false;
            }
        }

        // In permissive mode, always allow but log
        true
    }

    /// Log an AVC denial
    fn log_denial(
        &mut self,
        source: &SecurityContext,
        target: &SecurityContext,
        class: ObjectClass,
        requested: AccessVector,
    ) {
        self.denial_count += 1;

        let denial = AvcDenial {
            source_context: source.to_context_string(),
            target_context: target.to_context_string(),
            class,
            requested,
            pid: 0, // filled in by caller
            comm: String::from("unknown"),
        };

        serial_println!(
            "[SELinux] AVC denied {{ {} -> {} : {:?} perm=0x{:x} }}",
            denial.source_context,
            denial.target_context,
            class,
            requested
        );

        // Keep last 256 denials
        if self.denials.len() >= 256 {
            self.denials.remove(0);
        }
        self.denials.push(denial);
    }

    /// Compute type transition: what type should a new object have
    /// when created by source_type in target_type directory
    pub fn compute_transition(
        &self,
        source_type: &str,
        target_type: &str,
        class: ObjectClass,
    ) -> Option<String> {
        for tt in &self.policy.type_transitions {
            if tt.source_type == source_type && tt.target_type == target_type && tt.class == class {
                return Some(tt.new_type.clone());
            }
        }
        None
    }

    /// Get file context for a given path
    pub fn lookup_file_context(&self, path: &str) -> Option<&SecurityContext> {
        // Simple prefix matching (real SELinux uses regex)
        let mut best_match: Option<&FileContext> = None;
        let mut best_len = 0;

        for fc in &self.policy.file_contexts {
            if path.starts_with(&fc.path_regex) && fc.path_regex.len() > best_len {
                best_match = Some(fc);
                best_len = fc.path_regex.len();
            }
        }

        best_match.map(|fc| &fc.context)
    }

    /// Inherit security context on fork
    pub fn fork_context(&mut self, parent_pid: u32, child_pid: u32) {
        if let Some(ctx) = self.process_contexts.get(&parent_pid).cloned() {
            self.process_contexts.insert(child_pid, ctx);
        }
    }

    /// Remove context on process exit
    pub fn exit_context(&mut self, pid: u32) {
        self.process_contexts.remove(&pid);
    }

    /// Get a boolean value from policy
    pub fn get_bool(&self, name: &str) -> bool {
        self.policy.booleans.get(name).copied().unwrap_or(false)
    }

    /// Set a boolean value in policy (flushes AVC)
    pub fn set_bool(&mut self, name: &str, value: bool) {
        self.policy.booleans.insert(String::from(name), value);
        self.avc_cache.clear(); // booleans may affect rules
    }

    /// Get AVC statistics
    pub fn avc_stats(&self) -> (u64, u64, u64) {
        (self.avc_lookups, self.avc_hits, self.avc_misses)
    }

    /// Reset AVC cache
    pub fn avc_reset(&mut self) {
        self.avc_cache.clear();
        self.avc_lookups = 0;
        self.avc_hits = 0;
        self.avc_misses = 0;
    }
}

impl SelinuxPolicy {
    /// Create a default reference policy (minimal policy for KnoxOS)
    pub fn default_policy() -> Self {
        let mut policy = Self {
            te_rules: Vec::new(),
            type_transitions: Vec::new(),
            role_allows: Vec::new(),
            role_transitions: Vec::new(),
            file_contexts: Vec::new(),
            types: BTreeMap::new(),
            roles: BTreeMap::new(),
            users: BTreeMap::new(),
            booleans: BTreeMap::new(),
            sensitivities: Vec::new(),
            categories: Vec::new(),
        };

        // ─── Define types ──────────────────────────────────────────
        let type_names = [
            "kernel_t",
            "init_t",
            "unconfined_t",
            "user_t",
            "staff_t",
            "sysadm_t",
            "bin_t",
            "lib_t",
            "etc_t",
            "var_t",
            "tmp_t",
            "home_t",
            "proc_t",
            "sysfs_t",
            "devpts_t",
            "device_t",
            "port_t",
            "node_t",
            "netif_t",
            "unlabeled_t",
            "default_t",
            "shell_t",
            "passwd_t",
            "shadow_t",
            "sshd_t",
            "httpd_t",
            "container_t",
            "svirt_t",
            "docker_t",
            "systemd_t",
        ];
        for t in &type_names {
            policy.types.insert(String::from(*t), Vec::new());
        }

        // ─── Define roles ──────────────────────────────────────────
        policy.roles.insert(
            String::from("system_r"),
            alloc::vec![
                String::from("kernel_t"),
                String::from("init_t"),
                String::from("systemd_t"),
            ],
        );
        policy.roles.insert(
            String::from("unconfined_r"),
            alloc::vec![String::from("unconfined_t")],
        );
        policy
            .roles
            .insert(String::from("user_r"), alloc::vec![String::from("user_t")]);
        policy.roles.insert(
            String::from("staff_r"),
            alloc::vec![String::from("staff_t"), String::from("sysadm_t"),],
        );
        policy.roles.insert(
            String::from("sysadm_r"),
            alloc::vec![String::from("sysadm_t")],
        );
        policy.roles.insert(
            String::from("object_r"),
            alloc::vec![
                String::from("bin_t"),
                String::from("lib_t"),
                String::from("etc_t"),
                String::from("var_t"),
                String::from("tmp_t"),
                String::from("home_t"),
            ],
        );

        // ─── Define users ──────────────────────────────────────────
        policy.users.insert(
            String::from("system_u"),
            alloc::vec![String::from("system_r"), String::from("unconfined_r")],
        );
        policy.users.insert(
            String::from("root"),
            alloc::vec![
                String::from("system_r"),
                String::from("sysadm_r"),
                String::from("unconfined_r"),
            ],
        );
        policy
            .users
            .insert(String::from("user_u"), alloc::vec![String::from("user_r")]);
        policy.users.insert(
            String::from("staff_u"),
            alloc::vec![String::from("staff_r"), String::from("sysadm_r")],
        );
        policy.users.insert(
            String::from("unconfined_u"),
            alloc::vec![String::from("unconfined_r")],
        );

        // ─── MLS sensitivities and categories ─────────────────────
        policy.sensitivities = alloc::vec![
            String::from("s0"),
            String::from("s1"),
            String::from("s2"),
            String::from("s3"),
        ];
        for i in 0..1024 {
            let mut cat = String::from("c");
            // Simple int-to-string
            if i >= 100 {
                cat.push((b'0' + (i / 100) as u8) as char);
            }
            if i >= 10 {
                cat.push((b'0' + ((i / 10) % 10) as u8) as char);
            }
            cat.push((b'0' + (i % 10) as u8) as char);
            policy.categories.push(cat);
        }

        // ─── TE rules ─────────────────────────────────────────────

        // Kernel can do everything
        for class in [
            ObjectClass::File,
            ObjectClass::Dir,
            ObjectClass::Process,
            ObjectClass::Socket,
            ObjectClass::Capability,
        ] {
            policy.te_rules.push(TeRule {
                rule_type: RuleType::Allow,
                source_type: String::from("kernel_t"),
                target_type: String::from("kernel_t"),
                class,
                permissions: !0, // all permissions
            });
        }

        // init_t can manage processes and files
        policy.te_rules.push(TeRule {
            rule_type: RuleType::Allow,
            source_type: String::from("init_t"),
            target_type: String::from("init_t"),
            class: ObjectClass::Process,
            permissions: PERM_FORK | PERM_SIGNAL | PERM_TRANSITION | PERM_GETSCHED | PERM_SETSCHED,
        });
        policy.te_rules.push(TeRule {
            rule_type: RuleType::Allow,
            source_type: String::from("init_t"),
            target_type: String::from("bin_t"),
            class: ObjectClass::File,
            permissions: PERM_READ | PERM_EXECUTE | PERM_GETATTR | PERM_OPEN | PERM_MAP,
        });
        policy.te_rules.push(TeRule {
            rule_type: RuleType::Allow,
            source_type: String::from("init_t"),
            target_type: String::from("lib_t"),
            class: ObjectClass::File,
            permissions: PERM_READ | PERM_EXECUTE | PERM_GETATTR | PERM_OPEN | PERM_MAP,
        });
        policy.te_rules.push(TeRule {
            rule_type: RuleType::Allow,
            source_type: String::from("init_t"),
            target_type: String::from("etc_t"),
            class: ObjectClass::File,
            permissions: PERM_READ | PERM_GETATTR | PERM_OPEN,
        });

        // unconfined_t has full access (like permissive domain)
        for target in &type_names {
            for class in [
                ObjectClass::File,
                ObjectClass::Dir,
                ObjectClass::Process,
                ObjectClass::Socket,
                ObjectClass::Capability,
            ] {
                policy.te_rules.push(TeRule {
                    rule_type: RuleType::Allow,
                    source_type: String::from("unconfined_t"),
                    target_type: String::from(*target),
                    class,
                    permissions: !0,
                });
            }
        }

        // user_t basic access
        policy.te_rules.push(TeRule {
            rule_type: RuleType::Allow,
            source_type: String::from("user_t"),
            target_type: String::from("home_t"),
            class: ObjectClass::File,
            permissions: PERM_READ
                | PERM_WRITE
                | PERM_CREATE
                | PERM_GETATTR
                | PERM_SETATTR
                | PERM_UNLINK
                | PERM_RENAME
                | PERM_OPEN,
        });
        policy.te_rules.push(TeRule {
            rule_type: RuleType::Allow,
            source_type: String::from("user_t"),
            target_type: String::from("home_t"),
            class: ObjectClass::Dir,
            permissions: PERM_READ
                | PERM_WRITE
                | PERM_CREATE
                | PERM_GETATTR
                | PERM_SEARCH
                | PERM_ADD_NAME
                | PERM_REMOVE_NAME,
        });
        policy.te_rules.push(TeRule {
            rule_type: RuleType::Allow,
            source_type: String::from("user_t"),
            target_type: String::from("bin_t"),
            class: ObjectClass::File,
            permissions: PERM_READ | PERM_EXECUTE | PERM_GETATTR | PERM_OPEN | PERM_MAP,
        });
        policy.te_rules.push(TeRule {
            rule_type: RuleType::Allow,
            source_type: String::from("user_t"),
            target_type: String::from("lib_t"),
            class: ObjectClass::File,
            permissions: PERM_READ | PERM_EXECUTE | PERM_GETATTR | PERM_OPEN | PERM_MAP,
        });
        policy.te_rules.push(TeRule {
            rule_type: RuleType::Allow,
            source_type: String::from("user_t"),
            target_type: String::from("tmp_t"),
            class: ObjectClass::File,
            permissions: PERM_READ | PERM_WRITE | PERM_CREATE | PERM_UNLINK | PERM_OPEN,
        });
        policy.te_rules.push(TeRule {
            rule_type: RuleType::Allow,
            source_type: String::from("user_t"),
            target_type: String::from("user_t"),
            class: ObjectClass::Process,
            permissions: PERM_FORK | PERM_SIGNAL | PERM_SIGCHLD | PERM_GETSCHED,
        });

        // container_t: restricted access
        policy.te_rules.push(TeRule {
            rule_type: RuleType::Allow,
            source_type: String::from("container_t"),
            target_type: String::from("container_t"),
            class: ObjectClass::Process,
            permissions: PERM_FORK | PERM_SIGNAL | PERM_SIGCHLD,
        });
        policy.te_rules.push(TeRule {
            rule_type: RuleType::Allow,
            source_type: String::from("container_t"),
            target_type: String::from("container_t"),
            class: ObjectClass::File,
            permissions: PERM_READ | PERM_WRITE | PERM_CREATE | PERM_GETATTR | PERM_OPEN,
        });

        // httpd_t: web server access
        policy.te_rules.push(TeRule {
            rule_type: RuleType::Allow,
            source_type: String::from("httpd_t"),
            target_type: String::from("httpd_t"),
            class: ObjectClass::TcpSocket,
            permissions: PERM_READ | PERM_WRITE | PERM_BIND | PERM_LISTEN | PERM_ACCEPT,
        });

        // ─── Type transitions ──────────────────────────────────────
        // When init_t creates a process executing bin_t, transition to user_t
        policy.type_transitions.push(TypeTransition {
            source_type: String::from("init_t"),
            target_type: String::from("bin_t"),
            class: ObjectClass::Process,
            new_type: String::from("user_t"),
        });

        // ─── Role allows ───────────────────────────────────────────
        policy.role_allows.push(RoleAllow {
            source_role: String::from("sysadm_r"),
            target_role: String::from("user_r"),
        });
        policy.role_allows.push(RoleAllow {
            source_role: String::from("staff_r"),
            target_role: String::from("user_r"),
        });

        // ─── File contexts ─────────────────────────────────────────
        let default_file_contexts = [
            ("/", "system_u:object_r:default_t:s0"),
            ("/bin", "system_u:object_r:bin_t:s0"),
            ("/sbin", "system_u:object_r:bin_t:s0"),
            ("/usr/bin", "system_u:object_r:bin_t:s0"),
            ("/usr/sbin", "system_u:object_r:bin_t:s0"),
            ("/lib", "system_u:object_r:lib_t:s0"),
            ("/usr/lib", "system_u:object_r:lib_t:s0"),
            ("/etc", "system_u:object_r:etc_t:s0"),
            ("/var", "system_u:object_r:var_t:s0"),
            ("/tmp", "system_u:object_r:tmp_t:s0"),
            ("/home", "unconfined_u:object_r:home_t:s0"),
            ("/proc", "system_u:object_r:proc_t:s0"),
            ("/sys", "system_u:object_r:sysfs_t:s0"),
            ("/dev", "system_u:object_r:device_t:s0"),
            ("/dev/pts", "system_u:object_r:devpts_t:s0"),
        ];

        for (path, ctx_str) in &default_file_contexts {
            if let Some(ctx) = SecurityContext::from_str(ctx_str) {
                policy.file_contexts.push(FileContext {
                    path_regex: String::from(*path),
                    file_type: None,
                    context: ctx,
                });
            }
        }

        // ─── Booleans ──────────────────────────────────────────────
        policy
            .booleans
            .insert(String::from("httpd_can_network_connect"), false);
        policy.booleans.insert(String::from("httpd_use_nfs"), false);
        policy.booleans.insert(String::from("allow_ptrace"), false);
        policy.booleans.insert(String::from("secure_mode"), false);
        policy
            .booleans
            .insert(String::from("user_exec_content"), true);
        policy.booleans.insert(String::from("allow_execmem"), false);
        policy.booleans.insert(String::from("deny_ptrace"), false);
        policy
            .booleans
            .insert(String::from("container_manage_cgroup"), true);

        policy
    }
}

// ─── Public API ─────────────────────────────────────────────────────

/// Check if SELinux would allow an access
pub fn check_access(
    source_pid: u32,
    target_ino: u64,
    class: ObjectClass,
    requested: AccessVector,
) -> bool {
    let mut state = SELINUX.lock();
    if state.mode == SelinuxMode::Disabled {
        return true;
    }

    let source_ctx = state
        .process_contexts
        .get(&source_pid)
        .cloned()
        .unwrap_or_else(|| SecurityContext::new("system_u", "system_r", "unconfined_t", "s0"));

    let target_ctx = state
        .file_labels
        .get(&target_ino)
        .cloned()
        .unwrap_or_else(|| SecurityContext::new("system_u", "object_r", "default_t", "s0"));

    state.check_access(&source_ctx, &target_ctx, class, requested)
}

/// Get current enforcement mode
pub fn getenforce() -> SelinuxMode {
    SELINUX.lock().get_mode()
}

/// Set enforcement mode (setenforce)
pub fn setenforce(mode: SelinuxMode) {
    SELINUX.lock().set_mode(mode);
}

/// Get a process security context
pub fn getcon(pid: u32) -> Option<String> {
    SELINUX
        .lock()
        .get_process_context(pid)
        .map(|ctx| ctx.to_context_string())
}

/// Set a process security context
pub fn setcon(pid: u32, context: &str) -> Result<(), i32> {
    if let Some(ctx) = SecurityContext::from_str(context) {
        SELINUX.lock().set_process_context(pid, ctx);
        Ok(())
    } else {
        Err(-22) // EINVAL
    }
}

/// Get file security context
pub fn getfilecon(ino: u64) -> Option<String> {
    SELINUX
        .lock()
        .get_file_label(ino)
        .map(|ctx| ctx.to_context_string())
}

/// Set file security context
pub fn setfilecon(ino: u64, context: &str) -> Result<(), i32> {
    if let Some(ctx) = SecurityContext::from_str(context) {
        SELINUX.lock().set_file_label(ino, ctx);
        Ok(())
    } else {
        Err(-22) // EINVAL
    }
}

/// Get a policy boolean value
pub fn getsebool(name: &str) -> Option<bool> {
    let state = SELINUX.lock();
    state.policy.booleans.get(name).copied()
}

/// Set a policy boolean value
pub fn setsebool(name: &str, value: bool) {
    SELINUX.lock().set_bool(name, value);
}

/// Initialize SELinux subsystem
pub fn init() {
    let mut state = SELINUX.lock();

    // Set default process contexts
    state.set_process_context(
        0,
        SecurityContext::new("system_u", "system_r", "kernel_t", "s0"),
    );
    state.set_process_context(
        1,
        SecurityContext::new("system_u", "system_r", "init_t", "s0"),
    );

    // Set to permissive by default (can be toggled to enforcing)
    state.mode = SelinuxMode::Permissive;

    let num_rules = state.policy.te_rules.len();
    let num_types = state.policy.types.len();
    let num_roles = state.policy.roles.len();
    let num_contexts = state.policy.file_contexts.len();
    let num_bools = state.policy.booleans.len();

    serial_println!(
        "[SELinux] Initialized (mode={:?}, {} TE rules, {} types, {} roles, {} file contexts, {} booleans)",
        state.mode,
        num_rules,
        num_types,
        num_roles,
        num_contexts,
        num_bools
    );
}
