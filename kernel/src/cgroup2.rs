// cgroup2.rs — Cgroup v2 unified hierarchy
// Modern cgroup interface with single hierarchy, delegation, pressure stall info

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

/// Cgroup v2 controller types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cgroup2Controller {
    Cpu,
    Memory,
    Io,
    Pids,
    Cpuset,
    Rdma,
    HugeTlb,
    Misc,
}

impl Cgroup2Controller {
    pub fn name(&self) -> &'static str {
        match self {
            Cgroup2Controller::Cpu => "cpu",
            Cgroup2Controller::Memory => "memory",
            Cgroup2Controller::Io => "io",
            Cgroup2Controller::Pids => "pids",
            Cgroup2Controller::Cpuset => "cpuset",
            Cgroup2Controller::Rdma => "rdma",
            Cgroup2Controller::HugeTlb => "hugetlb",
            Cgroup2Controller::Misc => "misc",
        }
    }
}

/// Memory controller settings
#[derive(Debug, Clone)]
pub struct MemoryController {
    pub max: i64,          // memory.max (-1 = max)
    pub min: u64,          // memory.min (protection)
    pub low: u64,          // memory.low (soft protection)
    pub high: u64,         // memory.high (throttle threshold)
    pub swap_max: i64,     // memory.swap.max (-1 = max)
    pub current: u64,      // memory.current
    pub swap_current: u64, // memory.swap.current
    pub oom_group: bool,   // memory.oom.group
}

impl Default for MemoryController {
    fn default() -> Self {
        MemoryController {
            max: -1,
            min: 0,
            low: 0,
            high: u64::MAX,
            swap_max: -1,
            current: 0,
            swap_current: 0,
            oom_group: false,
        }
    }
}

/// CPU controller settings
#[derive(Debug, Clone)]
pub struct CpuController {
    pub weight: u32,      // cpu.weight (1-10000, default 100)
    pub weight_nice: i32, // cpu.weight.nice (-20 to 19)
    pub max: (u64, u64),  // cpu.max (quota, period) in microseconds
    pub burst: u64,       // cpu.max.burst
    pub uclamp_min: u32,  // cpu.uclamp.min (0-1024)
    pub uclamp_max: u32,  // cpu.uclamp.max (0-1024)
    pub stat_usage: u64,  // cpu.stat usage_usec
    pub stat_user: u64,   // cpu.stat user_usec
    pub stat_system: u64, // cpu.stat system_usec
    pub nr_periods: u64,
    pub nr_throttled: u64,
    pub throttled_usec: u64,
}

impl Default for CpuController {
    fn default() -> Self {
        CpuController {
            weight: 100,
            weight_nice: 0,
            max: (u64::MAX, 100000), // no limit, 100ms period
            burst: 0,
            uclamp_min: 0,
            uclamp_max: 1024,
            stat_usage: 0,
            stat_user: 0,
            stat_system: 0,
            nr_periods: 0,
            nr_throttled: 0,
            throttled_usec: 0,
        }
    }
}

/// IO controller settings
#[derive(Debug, Clone)]
pub struct IoController {
    pub weight: u32, // io.weight (1-10000, default 100)
    /// Per-device limits: (major:minor) -> IoMax
    pub max: BTreeMap<(u32, u32), IoMax>,
    pub stat: Vec<IoStat>,
    pub pressure: PressureStallInfo,
}

impl Default for IoController {
    fn default() -> Self {
        IoController {
            weight: 100,
            max: BTreeMap::new(),
            stat: Vec::new(),
            pressure: PressureStallInfo::default(),
        }
    }
}

/// IO max limits per device
#[derive(Debug, Clone)]
pub struct IoMax {
    pub rbps: Option<u64>,  // Read bytes per second
    pub wbps: Option<u64>,  // Write bytes per second
    pub riops: Option<u64>, // Read IOPS
    pub wiops: Option<u64>, // Write IOPS
}

/// IO statistics per device
#[derive(Debug, Clone)]
pub struct IoStat {
    pub major: u32,
    pub minor: u32,
    pub rbytes: u64,
    pub wbytes: u64,
    pub rios: u64,
    pub wios: u64,
    pub dbytes: u64,
    pub dios: u64,
}

/// PIDs controller settings
#[derive(Debug, Clone)]
pub struct PidsController {
    pub max: i64,        // pids.max (-1 = max)
    pub current: u64,    // pids.current
    pub events_max: u64, // pids.events max limit hit count
}

impl Default for PidsController {
    fn default() -> Self {
        PidsController {
            max: -1,
            current: 0,
            events_max: 0,
        }
    }
}

/// Pressure Stall Information (PSI)
#[derive(Debug, Clone, Default)]
pub struct PressureStallInfo {
    pub some_avg10: f32,
    pub some_avg60: f32,
    pub some_avg300: f32,
    pub some_total: u64,
    pub full_avg10: f32,
    pub full_avg60: f32,
    pub full_avg300: f32,
    pub full_total: u64,
}

/// A cgroup v2 node
#[derive(Debug)]
pub struct Cgroup2 {
    pub id: u64,
    pub path: String,
    pub parent_id: Option<u64>,
    pub children: Vec<u64>,
    pub processes: Vec<u64>,
    pub threads: Vec<u64>,
    /// Controllers enabled for children (cgroup.subtree_control)
    pub subtree_control: Vec<Cgroup2Controller>,
    /// Controllers available (cgroup.controllers)
    pub controllers: Vec<Cgroup2Controller>,
    pub frozen: bool,

    pub memory: MemoryController,
    pub cpu: CpuController,
    pub io: IoController,
    pub pids: PidsController,

    pub pressure_cpu: PressureStallInfo,
    pub pressure_memory: PressureStallInfo,
    pub pressure_io: PressureStallInfo,
}

impl Cgroup2 {
    pub fn new(id: u64, path: &str, parent_id: Option<u64>) -> Self {
        Cgroup2 {
            id,
            path: String::from(path),
            parent_id,
            children: Vec::new(),
            processes: Vec::new(),
            threads: Vec::new(),
            subtree_control: Vec::new(),
            controllers: alloc::vec![
                Cgroup2Controller::Cpu,
                Cgroup2Controller::Memory,
                Cgroup2Controller::Io,
                Cgroup2Controller::Pids,
                Cgroup2Controller::Cpuset,
            ],
            frozen: false,
            memory: MemoryController::default(),
            cpu: CpuController::default(),
            io: IoController::default(),
            pids: PidsController::default(),
            pressure_cpu: PressureStallInfo::default(),
            pressure_memory: PressureStallInfo::default(),
            pressure_io: PressureStallInfo::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cgroup2Error {
    NotFound,
    AlreadyExists,
    NotEmpty,
    InvalidPath,
    ControllerNotAvailable,
    LimitExceeded,
    PermDenied,
}

lazy_static! {
    static ref CGROUPS: Mutex<Cgroup2Table> = Mutex::new(Cgroup2Table::new());
}

struct Cgroup2Table {
    cgroups: BTreeMap<u64, Cgroup2>,
    path_to_id: BTreeMap<String, u64>,
    next_id: u64,
}

impl Cgroup2Table {
    fn new() -> Self {
        let mut table = Cgroup2Table {
            cgroups: BTreeMap::new(),
            path_to_id: BTreeMap::new(),
            next_id: 1,
        };

        // Create root cgroup
        let root = Cgroup2::new(0, "/", None);
        table.cgroups.insert(0, root);
        table.path_to_id.insert(String::from("/"), 0);

        table
    }
}

/// Create a cgroup
pub fn mkdir(path: &str) -> Result<u64, Cgroup2Error> {
    let mut table = CGROUPS.lock();

    if table.path_to_id.contains_key(path) {
        return Err(Cgroup2Error::AlreadyExists);
    }

    // Find parent
    let parent_path = parent_of(path);
    let parent_id = *table
        .path_to_id
        .get(&parent_path)
        .ok_or(Cgroup2Error::NotFound)?;

    let id = table.next_id;
    table.next_id += 1;

    // Inherit available controllers from parent's subtree_control
    let parent_controllers = table
        .cgroups
        .get(&parent_id)
        .map(|p| p.subtree_control.clone())
        .unwrap_or_default();

    let mut cgroup = Cgroup2::new(id, path, Some(parent_id));
    cgroup.controllers = parent_controllers;

    table.cgroups.insert(id, cgroup);
    table.path_to_id.insert(String::from(path), id);

    // Add to parent's children
    if let Some(parent) = table.cgroups.get_mut(&parent_id) {
        parent.children.push(id);
    }

    Ok(id)
}

/// Remove a cgroup
pub fn rmdir(path: &str) -> Result<(), Cgroup2Error> {
    let mut table = CGROUPS.lock();

    let id = *table.path_to_id.get(path).ok_or(Cgroup2Error::NotFound)?;

    // Check empty (no processes, no children)
    if let Some(cg) = table.cgroups.get(&id) {
        if !cg.processes.is_empty() || !cg.children.is_empty() {
            return Err(Cgroup2Error::NotEmpty);
        }
    }

    // Remove from parent's children
    if let Some(cg) = table.cgroups.get(&id) {
        if let Some(parent_id) = cg.parent_id {
            if let Some(parent) = table.cgroups.get_mut(&parent_id) {
                parent.children.retain(|&c| c != id);
            }
        }
    }

    table.cgroups.remove(&id);
    table.path_to_id.remove(path);

    Ok(())
}

/// Add a process to a cgroup (write to cgroup.procs)
pub fn attach_process(path: &str, pid: u64) -> Result<(), Cgroup2Error> {
    let mut table = CGROUPS.lock();
    let id = *table.path_to_id.get(path).ok_or(Cgroup2Error::NotFound)?;

    // Remove from any existing cgroup first
    for cg in table.cgroups.values_mut() {
        cg.processes.retain(|&p| p != pid);
    }

    let cg = table.cgroups.get_mut(&id).ok_or(Cgroup2Error::NotFound)?;

    // Check pids limit
    if cg.pids.max >= 0 && cg.pids.current >= cg.pids.max as u64 {
        cg.pids.events_max += 1;
        return Err(Cgroup2Error::LimitExceeded);
    }

    cg.processes.push(pid);
    cg.pids.current += 1;

    Ok(())
}

/// Enable/disable controllers for children (write to cgroup.subtree_control)
pub fn set_subtree_control(
    path: &str,
    enable: &[Cgroup2Controller],
    disable: &[Cgroup2Controller],
) -> Result<(), Cgroup2Error> {
    let mut table = CGROUPS.lock();
    let id = *table.path_to_id.get(path).ok_or(Cgroup2Error::NotFound)?;

    let cg = table.cgroups.get_mut(&id).ok_or(Cgroup2Error::NotFound)?;

    for ctrl in enable {
        if !cg.controllers.contains(ctrl) {
            return Err(Cgroup2Error::ControllerNotAvailable);
        }
        if !cg.subtree_control.contains(ctrl) {
            cg.subtree_control.push(*ctrl);
        }
    }

    for ctrl in disable {
        cg.subtree_control.retain(|c| c != ctrl);
    }

    Ok(())
}

/// Set memory.max
pub fn set_memory_max(path: &str, max: i64) -> Result<(), Cgroup2Error> {
    let mut table = CGROUPS.lock();
    let id = *table.path_to_id.get(path).ok_or(Cgroup2Error::NotFound)?;
    let cg = table.cgroups.get_mut(&id).ok_or(Cgroup2Error::NotFound)?;
    cg.memory.max = max;
    Ok(())
}

/// Set cpu.weight
pub fn set_cpu_weight(path: &str, weight: u32) -> Result<(), Cgroup2Error> {
    let mut table = CGROUPS.lock();
    let id = *table.path_to_id.get(path).ok_or(Cgroup2Error::NotFound)?;
    let cg = table.cgroups.get_mut(&id).ok_or(Cgroup2Error::NotFound)?;
    cg.cpu.weight = weight.clamp(1, 10000);
    Ok(())
}

/// Set cpu.max (quota period)
pub fn set_cpu_max(path: &str, quota: u64, period: u64) -> Result<(), Cgroup2Error> {
    let mut table = CGROUPS.lock();
    let id = *table.path_to_id.get(path).ok_or(Cgroup2Error::NotFound)?;
    let cg = table.cgroups.get_mut(&id).ok_or(Cgroup2Error::NotFound)?;
    cg.cpu.max = (quota, period);
    Ok(())
}

/// Set pids.max
pub fn set_pids_max(path: &str, max: i64) -> Result<(), Cgroup2Error> {
    let mut table = CGROUPS.lock();
    let id = *table.path_to_id.get(path).ok_or(Cgroup2Error::NotFound)?;
    let cg = table.cgroups.get_mut(&id).ok_or(Cgroup2Error::NotFound)?;
    cg.pids.max = max;
    Ok(())
}

/// Freeze/unfreeze cgroup
pub fn set_frozen(path: &str, frozen: bool) -> Result<(), Cgroup2Error> {
    let mut table = CGROUPS.lock();
    let id = *table.path_to_id.get(path).ok_or(Cgroup2Error::NotFound)?;
    let cg = table.cgroups.get_mut(&id).ok_or(Cgroup2Error::NotFound)?;
    cg.frozen = frozen;
    Ok(())
}

/// Generate cgroup.stat content
pub fn read_stat(path: &str) -> Result<String, Cgroup2Error> {
    let table = CGROUPS.lock();
    let id = *table.path_to_id.get(path).ok_or(Cgroup2Error::NotFound)?;
    let cg = table.cgroups.get(&id).ok_or(Cgroup2Error::NotFound)?;

    Ok(alloc::format!(
        "nr_descendants {}\nnr_dying_descendants 0\n",
        count_descendants(&table, id),
    ))
}

fn count_descendants(table: &Cgroup2Table, id: u64) -> usize {
    let cg = match table.cgroups.get(&id) {
        Some(cg) => cg,
        None => return 0,
    };
    let mut count = cg.children.len();
    for &child_id in &cg.children {
        count += count_descendants(table, child_id);
    }
    count
}

/// Generate memory.stat content
pub fn read_memory_stat(path: &str) -> Result<String, Cgroup2Error> {
    let table = CGROUPS.lock();
    let id = *table.path_to_id.get(path).ok_or(Cgroup2Error::NotFound)?;
    let cg = table.cgroups.get(&id).ok_or(Cgroup2Error::NotFound)?;

    Ok("anon 0\nfile 0\nkernel 0\nkernel_stack 0\npagetables 0\nslab_reclaimable 0\nslab_unreclaimable 0\n\
         pgfault 0\npgmajfault 0\nworkingset_refault_anon 0\nworkingset_refault_file 0\n".into())
}

/// Generate cpu.stat content
pub fn read_cpu_stat(path: &str) -> Result<String, Cgroup2Error> {
    let table = CGROUPS.lock();
    let id = *table.path_to_id.get(path).ok_or(Cgroup2Error::NotFound)?;
    let cg = table.cgroups.get(&id).ok_or(Cgroup2Error::NotFound)?;

    Ok(alloc::format!(
        "usage_usec {}\nuser_usec {}\nsystem_usec {}\nnr_periods {}\nnr_throttled {}\nthrottled_usec {}\n",
        cg.cpu.stat_usage,
        cg.cpu.stat_user,
        cg.cpu.stat_system,
        cg.cpu.nr_periods,
        cg.cpu.nr_throttled,
        cg.cpu.throttled_usec,
    ))
}

/// Generate PSI content (pressure stall information)
pub fn read_pressure(path: &str, resource: &str) -> Result<String, Cgroup2Error> {
    let table = CGROUPS.lock();
    let id = *table.path_to_id.get(path).ok_or(Cgroup2Error::NotFound)?;
    let cg = table.cgroups.get(&id).ok_or(Cgroup2Error::NotFound)?;

    let psi = match resource {
        "cpu" => &cg.pressure_cpu,
        "memory" => &cg.pressure_memory,
        "io" => &cg.pressure_io,
        _ => return Err(Cgroup2Error::NotFound),
    };

    Ok(alloc::format!(
        "some avg10={:.2} avg60={:.2} avg300={:.2} total={}\nfull avg10={:.2} avg60={:.2} avg300={:.2} total={}\n",
        psi.some_avg10,
        psi.some_avg60,
        psi.some_avg300,
        psi.some_total,
        psi.full_avg10,
        psi.full_avg60,
        psi.full_avg300,
        psi.full_total,
    ))
}

/// List all cgroups
pub fn list_cgroups() -> Vec<String> {
    let table = CGROUPS.lock();
    table.path_to_id.keys().cloned().collect()
}

fn parent_of(path: &str) -> String {
    if path == "/" {
        return String::from("/");
    }
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) => String::from("/"),
        Some(pos) => String::from(&trimmed[..pos]),
        None => String::from("/"),
    }
}

/// Initialize cgroup v2 subsystem
pub fn init() {
    // Create default system cgroups
    let _ = mkdir("/system.slice");
    let _ = mkdir("/user.slice");
    let _ = mkdir("/init.scope");

    // Enable all controllers at root
    let _ = set_subtree_control(
        "/",
        &[
            Cgroup2Controller::Cpu,
            Cgroup2Controller::Memory,
            Cgroup2Controller::Io,
            Cgroup2Controller::Pids,
        ],
        &[],
    );

    crate::serial_println!(
        "  cgroup v2 initialized (unified hierarchy, PSI, cpu/memory/io/pids controllers)"
    );
}
