/// sched_ext — Extended scheduler policies and features
/// Linux-compatible scheduling classes (CFS, RT, Deadline, Idle)
/// CPU affinity (sched_setaffinity/sched_getaffinity)
/// Priority management (nice, setpriority/getpriority)
use alloc::collections::BTreeMap;
use spin::Mutex;

use crate::serial_println;

/// Scheduling policies (matching Linux SCHED_* constants)
pub const SCHED_NORMAL: i32 = 0;
pub const SCHED_FIFO: i32 = 1;
pub const SCHED_RR: i32 = 2;
pub const SCHED_BATCH: i32 = 3;
pub const SCHED_ISO: i32 = 4;
pub const SCHED_IDLE: i32 = 5;
pub const SCHED_DEADLINE: i32 = 6;

/// Scheduling parameters
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SchedParam {
    pub sched_priority: i32,
}

/// Scheduling attributes (for sched_setattr/sched_getattr)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SchedAttr {
    pub size: u32,
    pub sched_policy: u32,
    pub sched_flags: u64,
    pub sched_nice: i32,
    pub sched_priority: u32,
    pub sched_runtime: u64,  // For SCHED_DEADLINE (ns)
    pub sched_deadline: u64, // For SCHED_DEADLINE (ns)
    pub sched_period: u64,   // For SCHED_DEADLINE (ns)
}

impl Default for SchedAttr {
    fn default() -> Self {
        SchedAttr {
            size: core::mem::size_of::<SchedAttr>() as u32,
            sched_policy: SCHED_NORMAL as u32,
            sched_flags: 0,
            sched_nice: 0,
            sched_priority: 0,
            sched_runtime: 0,
            sched_deadline: 0,
            sched_period: 0,
        }
    }
}

/// CPU affinity mask (supports up to 64 CPUs)
#[derive(Debug, Clone, Copy)]
pub struct CpuSet {
    pub bits: u64,
}

impl Default for CpuSet {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuSet {
    pub fn new() -> Self {
        CpuSet { bits: !0u64 } // All CPUs
    }

    pub fn set(&mut self, cpu: usize) {
        if cpu < 64 {
            self.bits |= 1u64 << cpu;
        }
    }

    pub fn clear(&mut self, cpu: usize) {
        if cpu < 64 {
            self.bits &= !(1u64 << cpu);
        }
    }

    pub fn is_set(&self, cpu: usize) -> bool {
        if cpu < 64 {
            (self.bits & (1u64 << cpu)) != 0
        } else {
            false
        }
    }

    pub fn count(&self) -> u32 {
        self.bits.count_ones()
    }
}

/// Per-process scheduler extension data
#[derive(Debug, Clone)]
struct ProcessSchedData {
    policy: i32,
    priority: i32,
    nice: i32,
    cpu_affinity: CpuSet,
    attr: SchedAttr,
}

lazy_static::lazy_static! {
    static ref SCHED_DATA: Mutex<BTreeMap<u32, ProcessSchedData>> = Mutex::new(BTreeMap::new());
}

/// Set scheduling policy and parameters
pub fn sched_setscheduler(pid: u32, policy: i32, param: &SchedParam) -> Result<(), i32> {
    let caller_uid = crate::users::get_current_uid();

    // Only root can set RT priorities
    if (policy == SCHED_FIFO || policy == SCHED_RR || policy == SCHED_DEADLINE) && caller_uid != 0 {
        return Err(-1); // EPERM
    }

    // Validate priority range
    let (min_prio, max_prio) = sched_get_priority_range(policy);
    if param.sched_priority < min_prio || param.sched_priority > max_prio {
        return Err(-22); // EINVAL
    }

    let mut data = SCHED_DATA.lock();
    let entry = data.entry(pid).or_insert_with(|| ProcessSchedData {
        policy: SCHED_NORMAL,
        priority: 0,
        nice: 0,
        cpu_affinity: CpuSet::new(),
        attr: SchedAttr::default(),
    });

    entry.policy = policy;
    entry.priority = param.sched_priority;

    // Update the actual scheduler
    let sched_policy = match policy {
        SCHED_FIFO => crate::scheduler::SchedPolicy::Fifo,
        SCHED_RR => crate::scheduler::SchedPolicy::RoundRobin,
        SCHED_BATCH => crate::scheduler::SchedPolicy::Batch,
        SCHED_IDLE => crate::scheduler::SchedPolicy::Idle,
        _ => crate::scheduler::SchedPolicy::Normal,
    };
    crate::scheduler::set_policy(pid, sched_policy);

    Ok(())
}

/// Get scheduling policy
pub fn sched_getscheduler(pid: u32) -> Result<i32, i32> {
    let data = SCHED_DATA.lock();
    Ok(data.get(&pid).map(|d| d.policy).unwrap_or(SCHED_NORMAL))
}

/// Get scheduling parameters
pub fn sched_getparam(pid: u32) -> Result<SchedParam, i32> {
    let data = SCHED_DATA.lock();
    let priority = data.get(&pid).map(|d| d.priority).unwrap_or(0);
    Ok(SchedParam {
        sched_priority: priority,
    })
}

/// Get priority range for a policy
pub fn sched_get_priority_range(policy: i32) -> (i32, i32) {
    match policy {
        SCHED_FIFO | SCHED_RR => (1, 99),
        SCHED_DEADLINE => (0, 0),
        _ => (0, 0),
    }
}

/// Set CPU affinity
pub fn sched_setaffinity(pid: u32, cpuset: &CpuSet) -> Result<(), i32> {
    if cpuset.count() == 0 {
        return Err(-22); // EINVAL
    }

    let mut data = SCHED_DATA.lock();
    let entry = data.entry(pid).or_insert_with(|| ProcessSchedData {
        policy: SCHED_NORMAL,
        priority: 0,
        nice: 0,
        cpu_affinity: CpuSet::new(),
        attr: SchedAttr::default(),
    });

    entry.cpu_affinity = *cpuset;
    Ok(())
}

/// Get CPU affinity
pub fn sched_getaffinity(pid: u32) -> Result<CpuSet, i32> {
    let data = SCHED_DATA.lock();
    Ok(data.get(&pid).map(|d| d.cpu_affinity).unwrap_or_default())
}

/// Set nice value
pub fn setpriority(which: i32, who: u32, prio: i32) -> Result<(), i32> {
    let nice = prio.clamp(-20, 19);

    let target_pid = match which {
        0 => {
            // PRIO_PROCESS
            if who == 0 {
                crate::scheduler::current_pid().unwrap_or(0)
            } else {
                who
            }
        }
        _ => return Err(-22),
    };

    let mut data = SCHED_DATA.lock();
    let entry = data.entry(target_pid).or_insert_with(|| ProcessSchedData {
        policy: SCHED_NORMAL,
        priority: 0,
        nice: 0,
        cpu_affinity: CpuSet::new(),
        attr: SchedAttr::default(),
    });
    entry.nice = nice;

    // Update the actual scheduler
    crate::scheduler::set_priority(target_pid, nice);

    Ok(())
}

/// Get nice value
pub fn getpriority(which: i32, who: u32) -> Result<i32, i32> {
    let target_pid = match which {
        0 => {
            if who == 0 {
                crate::scheduler::current_pid().unwrap_or(0)
            } else {
                who
            }
        }
        _ => return Err(-22),
    };

    let data = SCHED_DATA.lock();
    Ok(data.get(&target_pid).map(|d| d.nice).unwrap_or(0))
}

/// sched_yield — voluntarily give up the CPU
pub fn sched_yield() -> Result<(), i32> {
    crate::scheduler::yield_current();
    Ok(())
}

/// Set extended scheduling attributes
pub fn sched_setattr(pid: u32, attr: &SchedAttr) -> Result<(), i32> {
    let param = SchedParam {
        sched_priority: attr.sched_priority as i32,
    };
    sched_setscheduler(pid, attr.sched_policy as i32, &param)?;

    let mut data = SCHED_DATA.lock();
    if let Some(entry) = data.get_mut(&pid) {
        entry.nice = attr.sched_nice;
        entry.attr = *attr;
    }

    Ok(())
}

/// Get extended scheduling attributes
pub fn sched_getattr(pid: u32) -> Result<SchedAttr, i32> {
    let data = SCHED_DATA.lock();
    Ok(data.get(&pid).map(|d| d.attr).unwrap_or_default())
}

/// Clean up scheduler data for a process
pub fn cleanup_process(pid: u32) {
    SCHED_DATA.lock().remove(&pid);
}

pub fn init() {
    serial_println!("[KnoxOS] Extended scheduler (CFS/RT/DEADLINE/affinity) initialized");
}
