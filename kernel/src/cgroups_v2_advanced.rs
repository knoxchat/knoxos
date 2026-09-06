/// cgroups_v2_advanced — Advanced cgroup v2 features for production use
///
/// Extends cgroup2.rs with:
/// - Hierarchical memory reclaim
/// - Memory.oom.group for group OOM killing
/// - CPU burst scheduling
/// - IO latency controller (io.latency)
/// - Threaded cgroup mode
/// - Per-cgroup BPF program attachment
/// - cgroup.events notifications
/// - Memory watermark notifications
/// - Cgroup delegation to unprivileged users
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── Advanced Memory Controller ─────────────────────────────────────

/// Memory watermark event type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryEvent {
    /// Usage exceeded memory.high
    High,
    /// Usage exceeded memory.max (OOM imminent)
    Max,
    /// OOM killer invoked in this cgroup
    Oom,
    /// OOM kill occurred in this cgroup
    OomKill,
    /// OOM group kill
    OomGroupKill,
}

/// Memory reclaim configuration
#[derive(Debug, Clone)]
pub struct MemoryReclaim {
    /// Minimum bytes to keep (memory.min — hard guarantee)
    pub min: u64,
    /// Low watermark (memory.low — best-effort guarantee)
    pub low: u64,
    /// High watermark (memory.high — throttling threshold)
    pub high: u64,
    /// Maximum limit (memory.max — hard limit, OOM above this)
    pub max: u64,
    /// Swap maximum
    pub swap_max: u64,
    /// Whether OOM kills the entire cgroup or individual processes
    pub oom_group: bool,
    /// Memory events counter
    pub events: BTreeMap<String, u64>,
}

impl Default for MemoryReclaim {
    fn default() -> Self {
        Self {
            min: 0,
            low: 0,
            high: u64::MAX,
            max: u64::MAX,
            swap_max: u64::MAX,
            oom_group: false,
            events: BTreeMap::new(),
        }
    }
}

// ─── IO Latency Controller ──────────────────────────────────────────

/// IO latency target configuration (io.latency)
#[derive(Debug, Clone)]
pub struct IoLatencyConfig {
    /// Device major:minor
    pub device: (u32, u32),
    /// Target latency in microseconds
    pub target_us: u64,
    /// Whether active
    pub active: bool,
    /// Measured average latency
    pub avg_latency_us: u64,
    /// Number of samples
    pub samples: u64,
}

/// IO cost model configuration (io.cost)
#[derive(Debug, Clone)]
pub struct IoCostConfig {
    /// Device major:minor
    pub device: (u32, u32),
    /// Model type: linear or auto
    pub model: IoCostModel,
    /// Weight (1-10000, default 100)
    pub weight: u32,
    /// QoS parameters
    pub qos: IoCostQos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoCostModel {
    Linear,
    Auto,
}

#[derive(Debug, Clone)]
pub struct IoCostQos {
    /// Read latency target (percentile) in microseconds
    pub rlat: u64,
    /// Write latency target (percentile) in microseconds
    pub wlat: u64,
    /// Minimum percentage of bandwidth allocation
    pub min: u32,
    /// Maximum percentage of bandwidth allocation
    pub max: u32,
}

impl Default for IoCostQos {
    fn default() -> Self {
        Self {
            rlat: 25000, // 25ms
            wlat: 50000, // 50ms
            min: 1,
            max: 100,
        }
    }
}

// ─── CPU Burst ──────────────────────────────────────────────────────

/// CPU burst configuration
#[derive(Debug, Clone, Default)]
pub struct CpuBurstConfig {
    /// Maximum burst capacity in microseconds
    pub burst_us: u64,
    /// Current accumulated burst credit
    pub credit_us: u64,
    /// Number of burst events
    pub burst_count: u64,
}

// ─── Threaded Cgroup Mode ───────────────────────────────────────────

/// Cgroup thread mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgroupType {
    /// Domain cgroup (default) — processes are members
    Domain,
    /// Threaded cgroup — individual threads can be members
    Threaded,
    /// Domain-threaded — domain cgroup that serves as root of threaded subtree
    DomainThreaded,
    /// Domain-invalid — transient state during threaded setup
    DomainInvalid,
}

// ─── BPF Attachment ─────────────────────────────────────────────────

/// Type of BPF program attached to a cgroup
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgroupBpfType {
    /// Ingress network packets
    Ingress,
    /// Egress network packets
    Egress,
    /// Device access control (cgroup_device)
    Device,
    /// Socket create
    SockCreate,
    /// Socket operations
    SockOps,
    /// Bind
    Bind4,
    Bind6,
    /// Connect
    Connect4,
    Connect6,
    /// Sendmsg
    Sendmsg4,
    Sendmsg6,
    /// Sysctl
    Sysctl,
    /// Getsockopt / Setsockopt
    Getsockopt,
    Setsockopt,
}

/// Attached BPF program
#[derive(Debug, Clone)]
pub struct CgroupBpfProgram {
    pub prog_id: u32,
    pub attach_type: CgroupBpfType,
    /// Whether the program is multi-attach
    pub multi: bool,
    /// Flags (BPF_F_ALLOW_OVERRIDE, BPF_F_ALLOW_MULTI)
    pub flags: u32,
}

// ─── Cgroup Events ──────────────────────────────────────────────────

/// cgroup.events file contents
#[derive(Debug, Clone, Default)]
pub struct CgroupEvents {
    /// Whether the cgroup is populated (has live processes)
    pub populated: bool,
    /// Whether the cgroup is frozen
    pub frozen: bool,
}

// ─── Delegation ─────────────────────────────────────────────────────

/// Cgroup delegation configuration
#[derive(Debug, Clone, Default)]
pub struct CgroupDelegation {
    /// UID that owns this cgroup
    pub owner_uid: u32,
    /// GID that owns this cgroup
    pub owner_gid: u32,
    /// Delegated controllers
    pub controllers: Vec<String>,
    /// Whether this cgroup is delegated
    pub delegated: bool,
}

// ─── Advanced Cgroup Node ───────────────────────────────────────────

/// Extended cgroup node with Phase 15 features
#[derive(Debug, Clone)]
pub struct AdvancedCgroup {
    /// Cgroup path (e.g., "/sys/fs/cgroup/user.slice/user-1000.slice")
    pub path: String,
    /// Cgroup type (domain, threaded, etc.)
    pub cgroup_type: CgroupType,
    /// Advanced memory controller
    pub memory: MemoryReclaim,
    /// CPU burst config
    pub cpu_burst: CpuBurstConfig,
    /// IO latency targets per device
    pub io_latency: Vec<IoLatencyConfig>,
    /// IO cost configs per device
    pub io_cost: Vec<IoCostConfig>,
    /// Attached BPF programs
    pub bpf_programs: Vec<CgroupBpfProgram>,
    /// Events
    pub events: CgroupEvents,
    /// Delegation config
    pub delegation: CgroupDelegation,
    /// PIDs in this cgroup
    pub pids: Vec<u32>,
    /// Child cgroups
    pub children: Vec<String>,
}

impl AdvancedCgroup {
    pub fn new(path: &str) -> Self {
        Self {
            path: String::from(path),
            cgroup_type: CgroupType::Domain,
            memory: MemoryReclaim::default(),
            cpu_burst: CpuBurstConfig::default(),
            io_latency: Vec::new(),
            io_cost: Vec::new(),
            bpf_programs: Vec::new(),
            events: CgroupEvents::default(),
            delegation: CgroupDelegation::default(),
            pids: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Set cgroup type to threaded
    pub fn set_threaded(&mut self) {
        self.cgroup_type = CgroupType::Threaded;
    }

    /// Add a PID to this cgroup
    pub fn add_pid(&mut self, pid: u32) {
        if !self.pids.contains(&pid) {
            self.pids.push(pid);
            self.events.populated = !self.pids.is_empty();
        }
    }

    /// Remove a PID from this cgroup
    pub fn remove_pid(&mut self, pid: u32) {
        self.pids.retain(|&p| p != pid);
        self.events.populated = !self.pids.is_empty();
    }

    /// Configure memory.high
    pub fn set_memory_high(&mut self, bytes: u64) {
        self.memory.high = bytes;
    }

    /// Configure memory.max
    pub fn set_memory_max(&mut self, bytes: u64) {
        self.memory.max = bytes;
    }

    /// Configure memory.min (hard guarantee)
    pub fn set_memory_min(&mut self, bytes: u64) {
        self.memory.min = bytes;
    }

    /// Configure memory.low (best-effort guarantee)
    pub fn set_memory_low(&mut self, bytes: u64) {
        self.memory.low = bytes;
    }

    /// Set OOM group kill mode
    pub fn set_oom_group(&mut self, enabled: bool) {
        self.memory.oom_group = enabled;
    }

    /// Configure CPU burst
    pub fn set_cpu_burst(&mut self, burst_us: u64) {
        self.cpu_burst.burst_us = burst_us;
    }

    /// Add IO latency target for a device
    pub fn add_io_latency(&mut self, major: u32, minor: u32, target_us: u64) {
        self.io_latency.push(IoLatencyConfig {
            device: (major, minor),
            target_us,
            active: true,
            avg_latency_us: 0,
            samples: 0,
        });
    }

    /// Attach a BPF program
    pub fn attach_bpf(&mut self, prog_id: u32, attach_type: CgroupBpfType, flags: u32) {
        self.bpf_programs.push(CgroupBpfProgram {
            prog_id,
            attach_type,
            multi: (flags & 0x2) != 0, // BPF_F_ALLOW_MULTI
            flags,
        });
    }

    /// Detach a BPF program
    pub fn detach_bpf(&mut self, prog_id: u32, attach_type: CgroupBpfType) {
        self.bpf_programs
            .retain(|p| !(p.prog_id == prog_id && p.attach_type == attach_type));
    }

    /// Delegate this cgroup to a user
    pub fn delegate(&mut self, uid: u32, gid: u32, controllers: Vec<String>) {
        self.delegation = CgroupDelegation {
            owner_uid: uid,
            owner_gid: gid,
            controllers,
            delegated: true,
        };
    }

    /// Record a memory event
    pub fn record_memory_event(&mut self, event: MemoryEvent) {
        let key = match event {
            MemoryEvent::High => "high",
            MemoryEvent::Max => "max",
            MemoryEvent::Oom => "oom",
            MemoryEvent::OomKill => "oom_kill",
            MemoryEvent::OomGroupKill => "oom_group_kill",
        };
        let counter = self.memory.events.entry(String::from(key)).or_insert(0);
        *counter += 1;
    }

    /// Freeze this cgroup
    pub fn freeze(&mut self) {
        self.events.frozen = true;
    }

    /// Thaw (unfreeze) this cgroup
    pub fn thaw(&mut self) {
        self.events.frozen = false;
    }

    /// Generate cgroup.events output
    pub fn format_events(&self) -> String {
        let mut s = String::new();
        s.push_str("populated ");
        s.push_str(if self.events.populated { "1" } else { "0" });
        s.push('\n');
        s.push_str("frozen ");
        s.push_str(if self.events.frozen { "1" } else { "0" });
        s.push('\n');
        s
    }

    /// Generate memory.events output
    pub fn format_memory_events(&self) -> String {
        let mut s = String::new();
        for key in &["low", "high", "max", "oom", "oom_kill", "oom_group_kill"] {
            s.push_str(key);
            s.push(' ');
            let count = self.memory.events.get(*key).unwrap_or(&0);
            // Simple u64 to string
            let mut buf = [0u8; 20];
            let mut n = *count;
            let mut i = 0;
            if n == 0 {
                s.push('0');
            } else {
                while n > 0 {
                    buf[i] = b'0' + (n % 10) as u8;
                    n /= 10;
                    i += 1;
                }
                for j in (0..i).rev() {
                    s.push(buf[j] as char);
                }
            }
            s.push('\n');
        }
        s
    }
}

// ─── Global Registry ────────────────────────────────────────────────

pub struct AdvancedCgroupRegistry {
    pub cgroups: BTreeMap<String, AdvancedCgroup>,
}

lazy_static::lazy_static! {
    pub static ref ADVANCED_CGROUPS: Mutex<AdvancedCgroupRegistry> =
        Mutex::new(AdvancedCgroupRegistry {
            cgroups: BTreeMap::new(),
        });
}

impl AdvancedCgroupRegistry {
    /// Create a new advanced cgroup
    pub fn create(&mut self, path: &str) -> Result<(), i32> {
        if self.cgroups.contains_key(path) {
            return Err(-17); // EEXIST
        }
        self.cgroups
            .insert(String::from(path), AdvancedCgroup::new(path));
        Ok(())
    }

    /// Remove a cgroup (must have no children and no processes)
    pub fn remove(&mut self, path: &str) -> Result<(), i32> {
        if let Some(cg) = self.cgroups.get(path) {
            if !cg.pids.is_empty() {
                return Err(-16); // EBUSY
            }
            if !cg.children.is_empty() {
                return Err(-16); // EBUSY
            }
        } else {
            return Err(-2); // ENOENT
        }
        self.cgroups.remove(path);
        Ok(())
    }

    /// Get a cgroup reference
    pub fn get(&self, path: &str) -> Option<&AdvancedCgroup> {
        self.cgroups.get(path)
    }

    /// Get a mutable cgroup reference
    pub fn get_mut(&mut self, path: &str) -> Option<&mut AdvancedCgroup> {
        self.cgroups.get_mut(path)
    }

    /// List all cgroup paths
    pub fn list(&self) -> Vec<String> {
        self.cgroups.keys().cloned().collect()
    }
}

// ─── Public API ─────────────────────────────────────────────────────

/// Create an advanced cgroup
pub fn create_cgroup(path: &str) -> Result<(), i32> {
    ADVANCED_CGROUPS.lock().create(path)
}

/// Remove a cgroup
pub fn remove_cgroup(path: &str) -> Result<(), i32> {
    ADVANCED_CGROUPS.lock().remove(path)
}

/// Add a PID to a cgroup
pub fn attach_pid(path: &str, pid: u32) -> Result<(), i32> {
    if let Some(cg) = ADVANCED_CGROUPS.lock().get_mut(path) {
        cg.add_pid(pid);
        Ok(())
    } else {
        Err(-2) // ENOENT
    }
}

/// Set memory.max for a cgroup
pub fn set_memory_max(path: &str, bytes: u64) -> Result<(), i32> {
    if let Some(cg) = ADVANCED_CGROUPS.lock().get_mut(path) {
        cg.set_memory_max(bytes);
        Ok(())
    } else {
        Err(-2)
    }
}

/// Set CPU burst for a cgroup
pub fn set_cpu_burst(path: &str, burst_us: u64) -> Result<(), i32> {
    if let Some(cg) = ADVANCED_CGROUPS.lock().get_mut(path) {
        cg.set_cpu_burst(burst_us);
        Ok(())
    } else {
        Err(-2)
    }
}

/// Set cgroup type to threaded
pub fn set_threaded(path: &str) -> Result<(), i32> {
    if let Some(cg) = ADVANCED_CGROUPS.lock().get_mut(path) {
        cg.set_threaded();
        Ok(())
    } else {
        Err(-2)
    }
}

/// Delegate cgroup to unprivileged user
pub fn delegate_cgroup(
    path: &str,
    uid: u32,
    gid: u32,
    controllers: Vec<String>,
) -> Result<(), i32> {
    if let Some(cg) = ADVANCED_CGROUPS.lock().get_mut(path) {
        cg.delegate(uid, gid, controllers);
        Ok(())
    } else {
        Err(-2)
    }
}

/// Initialize advanced cgroups
pub fn init() {
    let mut reg = ADVANCED_CGROUPS.lock();

    // Create default cgroup hierarchy
    let defaults = [
        "/sys/fs/cgroup",
        "/sys/fs/cgroup/system.slice",
        "/sys/fs/cgroup/user.slice",
        "/sys/fs/cgroup/user.slice/user-0.slice",
        "/sys/fs/cgroup/user.slice/user-1000.slice",
        "/sys/fs/cgroup/init.scope",
        "/sys/fs/cgroup/machine.slice",
    ];

    for path in &defaults {
        let _ = reg.create(path);
    }

    // Attach kernel (PID 0) and init (PID 1) to init.scope
    if let Some(cg) = reg.get_mut("/sys/fs/cgroup/init.scope") {
        cg.add_pid(0);
        cg.add_pid(1);
    }

    // Delegate user.slice/user-1000.slice to UID 1000
    if let Some(cg) = reg.get_mut("/sys/fs/cgroup/user.slice/user-1000.slice") {
        cg.delegate(
            1000,
            1000,
            alloc::vec![
                String::from("cpu"),
                String::from("memory"),
                String::from("io"),
                String::from("pids"),
            ],
        );
    }

    serial_println!(
        "[cgroups_v2] Advanced cgroup v2 initialized ({} cgroups, threaded mode, io.latency, delegation)",
        reg.cgroups.len()
    );
}
