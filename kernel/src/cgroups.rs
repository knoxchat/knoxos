use alloc::collections::BTreeMap;
/// cgroups - Control Groups for resource management
/// Compatible with Linux cgroups v1 interface
/// Controls CPU, memory, and I/O resource allocation
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// cgroup resource controller types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Controller {
    Cpu,
    Memory,
    Io,
    Pids,
    Cpuacct,
    Freezer,
}

/// CPU controller settings
#[derive(Debug, Clone)]
pub struct CpuController {
    /// CPU shares (relative weight, default 1024)
    pub shares: u32,
    /// CPU quota in microseconds per period
    pub quota_us: i64,
    /// CPU period in microseconds (default 100000 = 100ms)
    pub period_us: u64,
}

/// Memory controller settings
#[derive(Debug, Clone)]
pub struct MemoryController {
    /// Memory limit in bytes (-1 = unlimited)
    pub limit_bytes: i64,
    /// Soft limit in bytes
    pub soft_limit_bytes: i64,
    /// Current memory usage
    pub usage_bytes: u64,
    /// Maximum memory usage observed
    pub max_usage_bytes: u64,
    /// OOM kill enabled
    pub oom_kill_enabled: bool,
}

/// I/O controller settings
#[derive(Debug, Clone)]
pub struct IoController {
    /// Read bytes per second limit (0 = unlimited)
    pub read_bps_limit: u64,
    /// Write bytes per second limit
    pub write_bps_limit: u64,
    /// Read IOPS limit
    pub read_iops_limit: u64,
    /// Write IOPS limit
    pub write_iops_limit: u64,
}

/// PIDs controller
#[derive(Debug, Clone)]
pub struct PidsController {
    /// Maximum number of PIDs (-1 = unlimited)
    pub max: i64,
    /// Current number of PIDs
    pub current: u64,
}

/// Freezer state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreezerState {
    Thawed,
    Freezing,
    Frozen,
}

/// A control group
#[derive(Debug, Clone)]
pub struct CGroup {
    pub name: String,
    pub path: String,
    pub pids: Vec<u32>,
    pub cpu: CpuController,
    pub memory: MemoryController,
    pub io: IoController,
    pub pids_ctrl: PidsController,
    pub freezer: FreezerState,
}

impl CGroup {
    pub fn new(name: &str, path: &str) -> Self {
        Self {
            name: String::from(name),
            path: String::from(path),
            pids: Vec::new(),
            cpu: CpuController {
                shares: 1024,
                quota_us: -1,
                period_us: 100_000,
            },
            memory: MemoryController {
                limit_bytes: -1,
                soft_limit_bytes: -1,
                usage_bytes: 0,
                max_usage_bytes: 0,
                oom_kill_enabled: true,
            },
            io: IoController {
                read_bps_limit: 0,
                write_bps_limit: 0,
                read_iops_limit: 0,
                write_iops_limit: 0,
            },
            pids_ctrl: PidsController {
                max: -1,
                current: 0,
            },
            freezer: FreezerState::Thawed,
        }
    }

    /// Add a PID to this cgroup
    pub fn add_pid(&mut self, pid: u32) -> Result<(), &'static str> {
        if self.pids_ctrl.max >= 0 && self.pids_ctrl.current as i64 >= self.pids_ctrl.max {
            return Err("PID limit reached");
        }
        if !self.pids.contains(&pid) {
            self.pids.push(pid);
            self.pids_ctrl.current = self.pids.len() as u64;
        }
        Ok(())
    }

    /// Remove a PID from this cgroup
    pub fn remove_pid(&mut self, pid: u32) {
        self.pids.retain(|&p| p != pid);
        self.pids_ctrl.current = self.pids.len() as u64;
    }

    /// Check if a PID can allocate memory
    pub fn can_allocate(&self, bytes: u64) -> bool {
        if self.memory.limit_bytes < 0 {
            return true;
        }
        self.memory.usage_bytes + bytes <= self.memory.limit_bytes as u64
    }

    /// Record memory allocation
    pub fn record_alloc(&mut self, bytes: u64) {
        self.memory.usage_bytes += bytes;
        if self.memory.usage_bytes > self.memory.max_usage_bytes {
            self.memory.max_usage_bytes = self.memory.usage_bytes;
        }
    }

    /// Record memory deallocation
    pub fn record_free(&mut self, bytes: u64) {
        self.memory.usage_bytes = self.memory.usage_bytes.saturating_sub(bytes);
    }
}

/// Global cgroup hierarchy
lazy_static::lazy_static! {
    static ref CGROUPS: Mutex<BTreeMap<String, CGroup>> = Mutex::new(BTreeMap::new());
}

/// Create a new cgroup
pub fn create(path: &str) -> Result<(), &'static str> {
    let mut cgroups = CGROUPS.lock();

    if cgroups.contains_key(path) {
        return Err("cgroup already exists");
    }

    let name = path.rsplit('/').next().unwrap_or(path);
    cgroups.insert(String::from(path), CGroup::new(name, path));

    crate::serial_println!("[KnoxOS] cgroup: created {}", path);
    Ok(())
}

/// Delete a cgroup (must have no processes)
pub fn delete(path: &str) -> Result<(), &'static str> {
    let mut cgroups = CGROUPS.lock();

    let cg = cgroups.get(path).ok_or("cgroup not found")?;
    if !cg.pids.is_empty() {
        return Err("cgroup has running processes");
    }

    cgroups.remove(path);
    Ok(())
}

/// Move a process into a cgroup
pub fn attach_pid(cgroup_path: &str, pid: u32) -> Result<(), &'static str> {
    let mut cgroups = CGROUPS.lock();

    // Remove from current cgroup if in one
    for cg in cgroups.values_mut() {
        cg.remove_pid(pid);
    }

    let cg = cgroups.get_mut(cgroup_path).ok_or("cgroup not found")?;
    cg.add_pid(pid)
}

/// Set CPU shares for a cgroup
pub fn set_cpu_shares(path: &str, shares: u32) -> Result<(), &'static str> {
    let mut cgroups = CGROUPS.lock();
    let cg = cgroups.get_mut(path).ok_or("cgroup not found")?;
    cg.cpu.shares = shares;
    Ok(())
}

/// Set memory limit for a cgroup
pub fn set_memory_limit(path: &str, limit_bytes: i64) -> Result<(), &'static str> {
    let mut cgroups = CGROUPS.lock();
    let cg = cgroups.get_mut(path).ok_or("cgroup not found")?;
    cg.memory.limit_bytes = limit_bytes;
    Ok(())
}

/// Set PID limit for a cgroup
pub fn set_pids_max(path: &str, max: i64) -> Result<(), &'static str> {
    let mut cgroups = CGROUPS.lock();
    let cg = cgroups.get_mut(path).ok_or("cgroup not found")?;
    cg.pids_ctrl.max = max;
    Ok(())
}

/// Freeze all processes in a cgroup
pub fn freeze(path: &str) -> Result<(), &'static str> {
    let mut cgroups = CGROUPS.lock();
    let cg = cgroups.get_mut(path).ok_or("cgroup not found")?;
    cg.freezer = FreezerState::Frozen;
    crate::serial_println!("[KnoxOS] cgroup: frozen {}", path);
    Ok(())
}

/// Thaw all processes in a cgroup
pub fn thaw(path: &str) -> Result<(), &'static str> {
    let mut cgroups = CGROUPS.lock();
    let cg = cgroups.get_mut(path).ok_or("cgroup not found")?;
    cg.freezer = FreezerState::Thawed;
    Ok(())
}

/// Get cgroup info
pub fn info(path: &str) -> Option<CGroup> {
    CGROUPS.lock().get(path).cloned()
}

/// List all cgroups
pub fn list() -> Vec<CGroup> {
    CGROUPS.lock().values().cloned().collect()
}

/// List all cgroup paths with info (for shell display)
pub fn list_all() -> Vec<String> {
    let cgroups = CGROUPS.lock();
    let mut result = Vec::new();
    for (path, cg) in cgroups.iter() {
        result.push(alloc::format!(
            "{:<20} pids={:<3} cpu_shares={:<5} mem_limit={}",
            path,
            cg.pids.len(),
            cg.cpu.shares,
            if cg.memory.limit_bytes < 0 {
                alloc::string::String::from("unlimited")
            } else {
                alloc::format!("{}MB", cg.memory.limit_bytes / (1024 * 1024))
            },
        ));
    }
    result.sort();
    result
}

/// Initialize the cgroup subsystem
pub fn init() {
    let mut cgroups = CGROUPS.lock();

    // Create root cgroup hierarchy
    cgroups.insert(String::from("/"), CGroup::new("root", "/"));

    // Create system slice (for system services)
    cgroups.insert(
        String::from("/system.slice"),
        CGroup::new("system.slice", "/system.slice"),
    );

    // Create user slice
    cgroups.insert(
        String::from("/user.slice"),
        CGroup::new("user.slice", "/user.slice"),
    );

    // Add init process to root cgroup
    if let Some(root) = cgroups.get_mut("/") {
        let _ = root.add_pid(0);
        let _ = root.add_pid(1);
    }

    drop(cgroups);
    crate::serial_println!("[KnoxOS] cgroups subsystem initialized");
}
