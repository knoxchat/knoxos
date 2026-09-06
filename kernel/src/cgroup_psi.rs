/// Cgroup v2 Pressure Stall Information (PSI) & Resource Controllers
/// Extends cgroup2 with memory/CPU/IO pressure monitoring
///
/// Features:
/// - PSI (Pressure Stall Information) for CPU, memory, IO
/// - CPU bandwidth controller (cpu.max)
/// - Memory high/max limits (memory.high, memory.max)
/// - IO weight-based bandwidth control
/// - Freezer controller (cgroup.freeze)
/// - PID limits (pids.max)
/// - Pressure notifications via eventfd
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── PSI (Pressure Stall Information) ───────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct PsiStats {
    /// Percentage of time at least SOME tasks stalled (avg10, avg60, avg300)
    pub some_avg10: f32,
    pub some_avg60: f32,
    pub some_avg300: f32,
    pub some_total_us: u64,

    /// Percentage of time ALL tasks stalled
    pub full_avg10: f32,
    pub full_avg60: f32,
    pub full_avg300: f32,
    pub full_total_us: u64,
}

impl PsiStats {
    pub fn update(&mut self, some_stall_us: u64, full_stall_us: u64, elapsed_us: u64) {
        self.some_total_us += some_stall_us;
        self.full_total_us += full_stall_us;

        if elapsed_us > 0 {
            let some_pct = (some_stall_us as f32 / elapsed_us as f32) * 100.0;
            let full_pct = (full_stall_us as f32 / elapsed_us as f32) * 100.0;

            // Exponential moving average
            self.some_avg10 = self.some_avg10 * 0.9 + some_pct * 0.1;
            self.some_avg60 = self.some_avg60 * 0.95 + some_pct * 0.05;
            self.some_avg300 = self.some_avg300 * 0.99 + some_pct * 0.01;

            self.full_avg10 = self.full_avg10 * 0.9 + full_pct * 0.1;
            self.full_avg60 = self.full_avg60 * 0.95 + full_pct * 0.05;
            self.full_avg300 = self.full_avg300 * 0.99 + full_pct * 0.01;
        }
    }
}

// ─── Resource Controller Limits ─────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CpuController {
    pub weight: u32,    // cpu.weight (1-10000, default 100)
    pub max_us: i64,    // cpu.max (microseconds per period, -1 = max)
    pub period_us: u64, // cpu.max period (default 100000 = 100ms)
    pub used_us: u64,   // CPU usage in current period
    pub throttled_count: u64,
    pub throttled_us: u64,
    pub nr_periods: u64,
}

impl Default for CpuController {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuController {
    pub fn new() -> Self {
        Self {
            weight: 100,
            max_us: -1, // No limit
            period_us: 100_000,
            used_us: 0,
            throttled_count: 0,
            throttled_us: 0,
            nr_periods: 0,
        }
    }

    pub fn is_throttled(&self) -> bool {
        self.max_us >= 0 && self.used_us >= self.max_us as u64
    }
}

#[derive(Debug, Clone)]
pub struct MemoryController {
    pub current: u64,      // memory.current (bytes)
    pub min: u64,          // memory.min (hard minimum guarantee)
    pub low: u64,          // memory.low (soft minimum guarantee)
    pub high: u64,         // memory.high (throttle threshold)
    pub max: u64,          // memory.max (hard limit, OOM)
    pub swap_max: u64,     // memory.swap.max
    pub swap_current: u64, // memory.swap.current
    pub oom_kills: u64,    // memory.oom.group kills
    pub events_high: u64,
    pub events_max: u64,
    pub events_oom: u64,
}

impl Default for MemoryController {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryController {
    pub fn new() -> Self {
        Self {
            current: 0,
            min: 0,
            low: 0,
            high: u64::MAX,
            max: u64::MAX,
            swap_max: u64::MAX,
            swap_current: 0,
            oom_kills: 0,
            events_high: 0,
            events_max: 0,
            events_oom: 0,
        }
    }

    pub fn can_charge(&self, bytes: u64) -> bool {
        self.current + bytes <= self.max
    }

    pub fn charge(&mut self, bytes: u64) -> bool {
        if self.current + bytes > self.max {
            self.events_max += 1;
            return false;
        }
        self.current += bytes;
        if self.current > self.high {
            self.events_high += 1;
        }
        true
    }

    pub fn uncharge(&mut self, bytes: u64) {
        self.current = self.current.saturating_sub(bytes);
    }
}

#[derive(Debug, Clone)]
pub struct IoController {
    pub weight: u32,    // io.weight (1-10000, default 100)
    pub rbytes: u64,    // io.stat read bytes
    pub wbytes: u64,    // io.stat write bytes
    pub rios: u64,      // io.stat read IOs
    pub wios: u64,      // io.stat write IOs
    pub rbps_max: u64,  // io.max read bytes/sec (0 = unlimited)
    pub wbps_max: u64,  // io.max write bytes/sec
    pub riops_max: u64, // io.max read IOPS
    pub wiops_max: u64, // io.max write IOPS
}

impl Default for IoController {
    fn default() -> Self {
        Self::new()
    }
}

impl IoController {
    pub fn new() -> Self {
        Self {
            weight: 100,
            rbytes: 0,
            wbytes: 0,
            rios: 0,
            wios: 0,
            rbps_max: 0,
            wbps_max: 0,
            riops_max: 0,
            wiops_max: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PidsController {
    pub current: u32,
    pub max: u32, // pids.max (0 = unlimited from u32 perspective)
    pub events_max: u64,
}

impl Default for PidsController {
    fn default() -> Self {
        Self::new()
    }
}

impl PidsController {
    pub fn new() -> Self {
        Self {
            current: 0,
            max: u32::MAX,
            events_max: 0,
        }
    }

    pub fn can_fork(&self) -> bool {
        self.current < self.max
    }
}

// ─── Enhanced Cgroup Node ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CgroupV2Node {
    pub id: u32,
    pub name: String,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    pub pids: Vec<u32>,
    pub frozen: bool,

    // Controllers
    pub cpu: CpuController,
    pub memory: MemoryController,
    pub io: IoController,
    pub pids_ctrl: PidsController,

    // PSI
    pub psi_cpu: PsiStats,
    pub psi_memory: PsiStats,
    pub psi_io: PsiStats,

    // Enabled controllers
    pub controllers_enabled: u32, // Bitmask: 1=cpu, 2=memory, 4=io, 8=pids
}

impl CgroupV2Node {
    pub fn new(id: u32, name: String, parent: Option<u32>) -> Self {
        Self {
            id,
            name,
            parent,
            children: Vec::new(),
            pids: Vec::new(),
            frozen: false,
            cpu: CpuController::new(),
            memory: MemoryController::new(),
            io: IoController::new(),
            pids_ctrl: PidsController::new(),
            psi_cpu: PsiStats::default(),
            psi_memory: PsiStats::default(),
            psi_io: PsiStats::default(),
            controllers_enabled: 0xF, // All enabled by default
        }
    }
}

// ─── Global State ───────────────────────────────────────────────────

static CGROUPS_V2: Mutex<BTreeMap<u32, CgroupV2Node>> = Mutex::new(BTreeMap::new());
static NEXT_CG_ID: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(1);

/// Create a cgroup v2 node
pub fn create_cgroup(name: &str, parent: Option<u32>) -> u32 {
    let id = NEXT_CG_ID.fetch_add(1, Ordering::Relaxed);
    let node = CgroupV2Node::new(id, String::from(name), parent);

    let mut cgroups = CGROUPS_V2.lock();
    if let Some(parent_id) = parent {
        if let Some(p) = cgroups.get_mut(&parent_id) {
            p.children.push(id);
        }
    }
    cgroups.insert(id, node);
    id
}

/// Attach a PID to a cgroup
pub fn attach_pid(cgroup_id: u32, pid: u32) -> Result<(), &'static str> {
    let mut cgroups = CGROUPS_V2.lock();
    // Remove from any existing cgroup
    for (_, cg) in cgroups.iter_mut() {
        cg.pids.retain(|&p| p != pid);
    }
    let cg = cgroups.get_mut(&cgroup_id).ok_or("Cgroup not found")?;
    if !cg.pids_ctrl.can_fork() {
        return Err("PIDs limit exceeded");
    }
    cg.pids.push(pid);
    cg.pids_ctrl.current += 1;
    Ok(())
}

/// Set CPU bandwidth limit
pub fn set_cpu_max(cgroup_id: u32, max_us: i64, period_us: u64) -> Result<(), &'static str> {
    let mut cgroups = CGROUPS_V2.lock();
    let cg = cgroups.get_mut(&cgroup_id).ok_or("Cgroup not found")?;
    cg.cpu.max_us = max_us;
    cg.cpu.period_us = period_us;
    Ok(())
}

/// Set memory limits
pub fn set_memory_max(cgroup_id: u32, max_bytes: u64) -> Result<(), &'static str> {
    let mut cgroups = CGROUPS_V2.lock();
    let cg = cgroups.get_mut(&cgroup_id).ok_or("Cgroup not found")?;
    cg.memory.max = max_bytes;
    Ok(())
}

/// Freeze/thaw a cgroup
pub fn freeze(cgroup_id: u32, frozen: bool) -> Result<(), &'static str> {
    let mut cgroups = CGROUPS_V2.lock();
    let cg = cgroups.get_mut(&cgroup_id).ok_or("Cgroup not found")?;
    cg.frozen = frozen;
    serial_println!(
        "[CGv2] Cgroup '{}' {}",
        cg.name,
        if frozen { "frozen" } else { "thawed" }
    );
    Ok(())
}

/// Get PSI stats for a cgroup
pub fn get_psi(cgroup_id: u32) -> Result<(PsiStats, PsiStats, PsiStats), &'static str> {
    let cgroups = CGROUPS_V2.lock();
    let cg = cgroups.get(&cgroup_id).ok_or("Cgroup not found")?;
    Ok((cg.psi_cpu.clone(), cg.psi_memory.clone(), cg.psi_io.clone()))
}

pub fn init() {
    // Create root cgroup
    let root = create_cgroup("root", None);

    // Create system slice
    let _system = create_cgroup("system.slice", Some(root));
    let _user = create_cgroup("user.slice", Some(root));

    serial_println!("[CGv2] Cgroup v2 PSI & resource controllers initialized");
    serial_println!("[CGv2]   Controllers: cpu, memory, io, pids, freezer");
    serial_println!("[CGv2]   PSI monitoring: cpu, memory, io pressure");
}
