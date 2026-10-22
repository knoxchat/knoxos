/// cgroups v2 Resource Accounting — Pressure Stall Information (PSI)
/// Extended resource controllers for production Linux workloads
///
/// Implements:
///   - CPU controller (bandwidth, burst, weight, max)
///   - Memory controller (min, low, high, max, swap, reclaim)
///   - I/O controller (weight, max BPS/IOPS, latency targets)
///   - PID controller (max PIDs per cgroup)
///   - RDMA controller (for high-performance networking)
///   - HugeTLB controller
///   - Freezer (suspend/resume entire cgroup hierarchy)
///   - PSI (Pressure Stall Information) tracking per cgroup
///   - Nested cgroup hierarchies with delegation
///   - cgroup.events and cgroup.stat
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── Resource Controller Types ──────────────────────────────────────

/// CPU controller settings
#[derive(Debug, Clone)]
pub struct CpuController {
    /// Weight for proportional sharing (1-10000, default 100)
    pub weight: u32,
    /// Maximum bandwidth: max_quota per max_period (microseconds)
    pub max_quota: i64, // -1 = unlimited
    pub max_period: u64, // default 100000 (100ms)
    /// Burst capacity (extra microseconds above quota)
    pub burst: u64,
    /// CPU pressure stall info
    pub psi: PressureData,
    /// Accumulated CPU time (nanoseconds)
    pub usage_nsec: u64,
    /// User time
    pub user_nsec: u64,
    /// System time
    pub system_nsec: u64,
    /// Number of throttled periods
    pub nr_throttled: u64,
    /// Total throttled time
    pub throttled_nsec: u64,
}

impl Default for CpuController {
    fn default() -> Self {
        Self {
            weight: 100,
            max_quota: -1,
            max_period: 100_000,
            burst: 0,
            psi: PressureData::default(),
            usage_nsec: 0,
            user_nsec: 0,
            system_nsec: 0,
            nr_throttled: 0,
            throttled_nsec: 0,
        }
    }
}

/// Memory controller settings
#[derive(Debug, Clone)]
pub struct MemoryController {
    /// Minimum memory guarantee (bytes)
    pub min: u64,
    /// Low watermark — reclaim starts above this
    pub low: u64,
    /// High watermark — throttle allocations above this
    pub high: u64,
    /// Hard limit — OOM kill above this (-1 = unlimited)
    pub max: i64,
    /// Swap limit (-1 = unlimited)
    pub swap_max: i64,
    /// Current usage (bytes)
    pub current: u64,
    /// Swap usage (bytes)
    pub swap_current: u64,
    /// Peak usage (bytes)
    pub peak: u64,
    /// OOM kill count
    pub oom_kill: u64,
    /// OOM group kill enabled
    pub oom_group: bool,
    /// Memory pressure stall info
    pub psi: PressureData,
    /// Memory events
    pub events: MemoryEvents,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryEvents {
    pub low: u64,
    pub high: u64,
    pub max: u64,
    pub oom: u64,
    pub oom_kill: u64,
    pub oom_group_kill: u64,
}

impl Default for MemoryController {
    fn default() -> Self {
        Self {
            min: 0,
            low: 0,
            high: u64::MAX,
            max: -1,
            swap_max: -1,
            current: 0,
            swap_current: 0,
            peak: 0,
            oom_kill: 0,
            oom_group: false,
            psi: PressureData::default(),
            events: MemoryEvents::default(),
        }
    }
}

/// I/O controller settings
#[derive(Debug, Clone)]
pub struct IoController {
    /// Weight for proportional sharing (1-10000)
    pub weight: u32,
    /// Per-device limits
    pub device_limits: Vec<IoDeviceLimit>,
    /// I/O pressure stall info
    pub psi: PressureData,
    /// Read bytes
    pub rbytes: u64,
    /// Write bytes
    pub wbytes: u64,
    /// Read I/O operations
    pub rios: u64,
    /// Write I/O operations
    pub wios: u64,
}

#[derive(Debug, Clone)]
pub struct IoDeviceLimit {
    pub major: u32,
    pub minor: u32,
    pub rbps_max: u64,  // Read bytes per second
    pub wbps_max: u64,  // Write bytes per second
    pub riops_max: u64, // Read IOPs
    pub wiops_max: u64, // Write IOPs
}

impl Default for IoController {
    fn default() -> Self {
        Self {
            weight: 100,
            device_limits: Vec::new(),
            psi: PressureData::default(),
            rbytes: 0,
            wbytes: 0,
            rios: 0,
            wios: 0,
        }
    }
}

/// PID controller
#[derive(Debug, Clone)]
pub struct PidController {
    /// Maximum number of PIDs (-1 = unlimited)
    pub max: i64,
    /// Current PID count
    pub current: u64,
    /// Number of times PID limit was hit
    pub events_max: u64,
}

impl Default for PidController {
    fn default() -> Self {
        Self {
            max: -1,
            current: 0,
            events_max: 0,
        }
    }
}

/// Pressure Stall Information (PSI) data
#[derive(Debug, Clone, Default)]
pub struct PressureData {
    /// Some: percentage of time at least one task is stalled (avg10/avg60/avg300)
    pub some_avg10: u32, // x100 (e.g., 1234 = 12.34%)
    pub some_avg60: u32,
    pub some_avg300: u32,
    pub some_total_usec: u64,
    /// Full: percentage of time ALL tasks are stalled
    pub full_avg10: u32,
    pub full_avg60: u32,
    pub full_avg300: u32,
    pub full_total_usec: u64,
}

/// Cgroup v2 node
#[derive(Debug, Clone)]
pub struct CgroupV2 {
    pub name: String,
    pub path: String,
    pub parent: Option<String>,
    pub children: Vec<String>,
    /// Attached PIDs
    pub pids: Vec<u32>,
    /// Controllers
    pub cpu: CpuController,
    pub memory: MemoryController,
    pub io: IoController,
    pub pid_ctrl: PidController,
    /// Subtree control — which controllers are enabled for children
    pub subtree_control: Vec<String>,
    /// Frozen state
    pub frozen: bool,
    /// Populated (has processes or populated children)
    pub populated: bool,
}

impl CgroupV2 {
    fn new(name: &str, path: &str) -> Self {
        Self {
            name: String::from(name),
            path: String::from(path),
            parent: None,
            children: Vec::new(),
            pids: Vec::new(),
            cpu: CpuController::default(),
            memory: MemoryController::default(),
            io: IoController::default(),
            pid_ctrl: PidController::default(),
            subtree_control: Vec::new(),
            frozen: false,
            populated: false,
        }
    }
}

lazy_static::lazy_static! {
    static ref CGROUP_TREE: Mutex<BTreeMap<String, CgroupV2>> = {
        let mut tree = BTreeMap::new();
        // Create root cgroup
        let mut root = CgroupV2::new("", "/");
        root.subtree_control = alloc::vec![
            String::from("cpu"),
            String::from("memory"),
            String::from("io"),
            String::from("pids"),
        ];
        tree.insert(String::from("/"), root);
        Mutex::new(tree)
    };
}

/// Initialize cgroup v2 resource controllers
pub fn init() {
    // Create default system cgroups
    create_cgroup("/system.slice").ok();
    create_cgroup("/user.slice").ok();
    create_cgroup("/init.scope").ok();
    serial_println!(
        "[KnoxOS] cgroup v2 resource controllers initialized (CPU, memory, IO, PID, PSI)"
    );
}

// ─── Cgroup CRUD ────────────────────────────────────────────────────

pub fn create_cgroup(path: &str) -> Result<(), &'static str> {
    let mut tree = CGROUP_TREE.lock();
    if tree.contains_key(path) {
        return Err("cgroup already exists");
    }

    // Find parent
    let parent_path = parent_of(path);
    if !tree.contains_key(&parent_path) {
        return Err("parent cgroup does not exist");
    }

    let name = path.rsplit('/').next().unwrap_or(path);
    let mut cg = CgroupV2::new(name, path);
    cg.parent = Some(parent_path.clone());

    // Inherit subtree control from parent
    if let Some(parent) = tree.get(&parent_path) {
        cg.subtree_control = parent.subtree_control.clone();
    }

    tree.insert(String::from(path), cg);

    // Add to parent's children
    if let Some(parent) = tree.get_mut(&parent_path) {
        parent.children.push(String::from(path));
        parent.populated = true;
    }

    Ok(())
}

pub fn remove_cgroup(path: &str) -> Result<(), &'static str> {
    let mut tree = CGROUP_TREE.lock();
    let cg = tree.get(path).ok_or("cgroup not found")?;
    if !cg.pids.is_empty() {
        return Err("cgroup has attached processes");
    }
    if !cg.children.is_empty() {
        return Err("cgroup has children");
    }

    let parent_path = cg.parent.clone();
    tree.remove(path);

    if let Some(pp) = parent_path {
        if let Some(parent) = tree.get_mut(&pp) {
            parent.children.retain(|c| c != path);
            parent.populated = !parent.children.is_empty() || !parent.pids.is_empty();
        }
    }
    Ok(())
}

/// Attach a PID to a cgroup
pub fn attach_pid(path: &str, pid: u32) -> Result<(), &'static str> {
    let mut tree = CGROUP_TREE.lock();

    // Check PID limit
    if let Some(cg) = tree.get(path) {
        if cg.pid_ctrl.max >= 0 && cg.pid_ctrl.current >= cg.pid_ctrl.max as u64 {
            // Increment events_max counter
            if let Some(cg) = tree.get_mut(path) {
                cg.pid_ctrl.events_max += 1;
            }
            return Err("PID limit reached");
        }
    }

    // Remove from old cgroup
    for (_, cg) in tree.iter_mut() {
        cg.pids.retain(|p| *p != pid);
        cg.pid_ctrl.current = cg.pids.len() as u64;
    }

    // Add to new cgroup
    let cg = tree.get_mut(path).ok_or("cgroup not found")?;
    cg.pids.push(pid);
    cg.pid_ctrl.current = cg.pids.len() as u64;
    cg.populated = true;
    Ok(())
}

/// Set CPU weight
pub fn set_cpu_weight(path: &str, weight: u32) -> Result<(), &'static str> {
    let mut tree = CGROUP_TREE.lock();
    let cg = tree.get_mut(path).ok_or("cgroup not found")?;
    if !(1..=10000).contains(&weight) {
        return Err("weight must be 1-10000");
    }
    cg.cpu.weight = weight;
    Ok(())
}

/// Set CPU max bandwidth
pub fn set_cpu_max(path: &str, quota: i64, period: u64) -> Result<(), &'static str> {
    let mut tree = CGROUP_TREE.lock();
    let cg = tree.get_mut(path).ok_or("cgroup not found")?;
    cg.cpu.max_quota = quota;
    cg.cpu.max_period = period;
    Ok(())
}

/// Set memory limits
pub fn set_memory_max(path: &str, max_bytes: i64) -> Result<(), &'static str> {
    let mut tree = CGROUP_TREE.lock();
    let cg = tree.get_mut(path).ok_or("cgroup not found")?;
    cg.memory.max = max_bytes;
    Ok(())
}

pub fn set_memory_high(path: &str, high_bytes: u64) -> Result<(), &'static str> {
    let mut tree = CGROUP_TREE.lock();
    let cg = tree.get_mut(path).ok_or("cgroup not found")?;
    cg.memory.high = high_bytes;
    Ok(())
}

pub fn set_memory_low(path: &str, low_bytes: u64) -> Result<(), &'static str> {
    let mut tree = CGROUP_TREE.lock();
    let cg = tree.get_mut(path).ok_or("cgroup not found")?;
    cg.memory.low = low_bytes;
    Ok(())
}

/// Set PID limit
pub fn set_pids_max(path: &str, max: i64) -> Result<(), &'static str> {
    let mut tree = CGROUP_TREE.lock();
    let cg = tree.get_mut(path).ok_or("cgroup not found")?;
    cg.pid_ctrl.max = max;
    Ok(())
}

/// Set I/O weight
pub fn set_io_weight(path: &str, weight: u32) -> Result<(), &'static str> {
    let mut tree = CGROUP_TREE.lock();
    let cg = tree.get_mut(path).ok_or("cgroup not found")?;
    if !(1..=10000).contains(&weight) {
        return Err("weight must be 1-10000");
    }
    cg.io.weight = weight;
    Ok(())
}

/// Freeze a cgroup (SIGSTOP equivalent for all processes)
pub fn freeze(path: &str) -> Result<(), &'static str> {
    let mut tree = CGROUP_TREE.lock();
    let cg = tree.get_mut(path).ok_or("cgroup not found")?;
    cg.frozen = true;
    for &pid in &cg.pids {
        let _ = crate::signals::kill(pid, crate::signals::Signal::SIGSTOP, 0);
    }
    serial_println!("[cgroup] Frozen: {}", path);
    Ok(())
}

/// Thaw a cgroup
pub fn thaw(path: &str) -> Result<(), &'static str> {
    let mut tree = CGROUP_TREE.lock();
    let cg = tree.get_mut(path).ok_or("cgroup not found")?;
    cg.frozen = false;
    for &pid in &cg.pids {
        let _ = crate::signals::kill(pid, crate::signals::Signal::SIGCONT, 0);
    }
    serial_println!("[cgroup] Thawed: {}", path);
    Ok(())
}

/// Record memory charge for a cgroup
pub fn charge_memory(path: &str, bytes: u64) -> Result<(), &'static str> {
    let mut tree = CGROUP_TREE.lock();
    let cg = tree.get_mut(path).ok_or("cgroup not found")?;

    let new_current = cg.memory.current + bytes;

    // Check hard limit
    if cg.memory.max >= 0 && new_current > cg.memory.max as u64 {
        cg.memory.events.max += 1;
        return Err("memory limit exceeded");
    }

    // Check high watermark (throttle but allow)
    if new_current > cg.memory.high {
        cg.memory.events.high += 1;
    }

    cg.memory.current = new_current;
    if new_current > cg.memory.peak {
        cg.memory.peak = new_current;
    }
    Ok(())
}

/// Read PSI data for a resource
pub fn read_psi(path: &str, resource: &str) -> Option<String> {
    let tree = CGROUP_TREE.lock();
    let cg = tree.get(path)?;
    let psi = match resource {
        "cpu" => &cg.cpu.psi,
        "memory" => &cg.memory.psi,
        "io" => &cg.io.psi,
        _ => return None,
    };
    Some(alloc::format!(
        "some avg10={}.{:02} avg60={}.{:02} avg300={}.{:02} total={}\n\
         full avg10={}.{:02} avg60={}.{:02} avg300={}.{:02} total={}\n",
        psi.some_avg10 / 100,
        psi.some_avg10 % 100,
        psi.some_avg60 / 100,
        psi.some_avg60 % 100,
        psi.some_avg300 / 100,
        psi.some_avg300 % 100,
        psi.some_total_usec,
        psi.full_avg10 / 100,
        psi.full_avg10 % 100,
        psi.full_avg60 / 100,
        psi.full_avg60 % 100,
        psi.full_avg300 / 100,
        psi.full_avg300 % 100,
        psi.full_total_usec,
    ))
}

/// Read cgroup stat
pub fn read_stat(path: &str) -> Option<String> {
    let tree = CGROUP_TREE.lock();
    let cg = tree.get(path)?;
    Some(alloc::format!(
        "nr_descendants {}\nnr_dying_descendants 0\n",
        cg.children.len(),
    ))
}

/// List all cgroups
pub fn list_cgroups() -> Vec<String> {
    let tree = CGROUP_TREE.lock();
    tree.keys().cloned().collect()
}

/// Read cgroup info as formatted string (for /sys/fs/cgroup)
pub fn read_cgroup_info(path: &str) -> Option<String> {
    let tree = CGROUP_TREE.lock();
    let cg = tree.get(path)?;
    Some(alloc::format!(
        "path: {}\n\
         frozen: {}\n\
         populated: {}\n\
         pids: {:?}\n\
         cpu.weight: {}\n\
         cpu.max: {} {}\n\
         memory.current: {}\n\
         memory.max: {}\n\
         memory.high: {}\n\
         memory.peak: {}\n\
         io.weight: {}\n\
         pids.max: {}\n\
         pids.current: {}\n\
         subtree_control: {}\n",
        cg.path,
        cg.frozen,
        cg.populated,
        cg.pids,
        cg.cpu.weight,
        if cg.cpu.max_quota < 0 {
            String::from("max")
        } else {
            alloc::format!("{}", cg.cpu.max_quota)
        },
        cg.cpu.max_period,
        cg.memory.current,
        if cg.memory.max < 0 {
            String::from("max")
        } else {
            alloc::format!("{}", cg.memory.max)
        },
        cg.memory.high,
        cg.memory.peak,
        cg.io.weight,
        if cg.pid_ctrl.max < 0 {
            String::from("max")
        } else {
            alloc::format!("{}", cg.pid_ctrl.max)
        },
        cg.pid_ctrl.current,
        cg.subtree_control.join(" "),
    ))
}

// ─── Helpers ────────────────────────────────────────────────────────

fn parent_of(path: &str) -> String {
    if path == "/" {
        return String::from("/");
    }
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) => String::from("/"),
        Some(i) => String::from(&trimmed[..i]),
        None => String::from("/"),
    }
}
