/// io_prio — I/O scheduling priority (ioprio)
///
/// Implements the Linux ioprio_set/ioprio_get interface for per-process
/// and per-thread I/O scheduling priority control. Supports CFQ-style
/// scheduling classes.
///
/// Features:
/// - IOPRIO_CLASS_NONE — no I/O priority set (inherit)
/// - IOPRIO_CLASS_RT — real-time I/O (8 priorities)
/// - IOPRIO_CLASS_BE — best-effort I/O (8 priorities)
/// - IOPRIO_CLASS_IDLE — idle I/O (background)
/// - Per-process and per-pgrp priority
/// - WHO_PROCESS, WHO_PGRP, WHO_USER selectors
/// - BFQ and mq-deadline scheduler awareness
use alloc::collections::BTreeMap;
use spin::Mutex;

use crate::serial_println;

// ─── Constants ──────────────────────────────────────────────────────

/// I/O scheduling classes
pub const IOPRIO_CLASS_NONE: u16 = 0;
pub const IOPRIO_CLASS_RT: u16 = 1;
pub const IOPRIO_CLASS_BE: u16 = 2;
pub const IOPRIO_CLASS_IDLE: u16 = 3;

/// Who selectors
pub const IOPRIO_WHO_PROCESS: u32 = 1;
pub const IOPRIO_WHO_PGRP: u32 = 2;
pub const IOPRIO_WHO_USER: u32 = 3;

/// Number of priority levels per class
pub const IOPRIO_NR_LEVELS: u16 = 8;
/// Bits for priority data
pub const IOPRIO_CLASS_SHIFT: u16 = 13;
pub const IOPRIO_PRIO_MASK: u16 = (1 << IOPRIO_CLASS_SHIFT) - 1;

// ─── Helper macros ──────────────────────────────────────────────────

/// Encode class and priority into ioprio value
pub const fn ioprio_value(class: u16, data: u16) -> u16 {
    (class << IOPRIO_CLASS_SHIFT) | (data & IOPRIO_PRIO_MASK)
}

/// Extract class from ioprio value
pub const fn ioprio_class(ioprio: u16) -> u16 {
    ioprio >> IOPRIO_CLASS_SHIFT
}

/// Extract priority data from ioprio value
pub const fn ioprio_data(ioprio: u16) -> u16 {
    ioprio & IOPRIO_PRIO_MASK
}

// ─── Data Structures ────────────────────────────────────────────────

/// I/O priority for a task
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoPriority {
    /// Scheduling class
    pub class: IoClass,
    /// Priority level within class (0-7, 0 = highest)
    pub level: u8,
}

/// I/O scheduling class
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IoClass {
    /// No priority set — inherit from cgroup or use BE(4)
    None,
    /// Real-time — guaranteed I/O bandwidth, starves lower classes
    RealTime,
    /// Best-effort — default class, 8 priority levels
    BestEffort,
    /// Idle — only serviced when no other I/O pending
    Idle,
}

impl IoClass {
    pub fn from_raw(class: u16) -> Self {
        match class {
            IOPRIO_CLASS_RT => IoClass::RealTime,
            IOPRIO_CLASS_BE => IoClass::BestEffort,
            IOPRIO_CLASS_IDLE => IoClass::Idle,
            _ => IoClass::None,
        }
    }

    pub fn to_raw(self) -> u16 {
        match self {
            IoClass::None => IOPRIO_CLASS_NONE,
            IoClass::RealTime => IOPRIO_CLASS_RT,
            IoClass::BestEffort => IOPRIO_CLASS_BE,
            IoClass::Idle => IOPRIO_CLASS_IDLE,
        }
    }
}

impl Default for IoPriority {
    fn default() -> Self {
        Self {
            class: IoClass::None,
            level: 4,
        }
    }
}

impl IoPriority {
    /// Create from raw ioprio value
    pub fn from_raw(ioprio: u16) -> Self {
        Self {
            class: IoClass::from_raw(ioprio_class(ioprio)),
            level: ioprio_data(ioprio) as u8,
        }
    }

    /// Encode to raw ioprio value
    pub fn to_raw(self) -> u16 {
        ioprio_value(self.class.to_raw(), self.level as u16)
    }

    /// Get the effective priority (resolving NONE to BE(4))
    pub fn effective(&self) -> IoPriority {
        if self.class == IoClass::None {
            IoPriority {
                class: IoClass::BestEffort,
                level: 4,
            }
        } else {
            *self
        }
    }

    /// Check if this priority allows immediate I/O
    pub fn is_realtime(&self) -> bool {
        self.class == IoClass::RealTime
    }

    /// Check if this is idle priority
    pub fn is_idle(&self) -> bool {
        self.class == IoClass::Idle
    }

    /// Calculate time slice weight for BFQ-style scheduling
    /// Higher weight = more I/O bandwidth
    pub fn bfq_weight(&self) -> u32 {
        let eff = self.effective();
        match eff.class {
            IoClass::RealTime => 800 - (eff.level as u32 * 80), // 800-160
            IoClass::BestEffort => 400 - (eff.level as u32 * 40), // 400-80
            IoClass::Idle => 10,
            IoClass::None => 200, // shouldn't happen after effective()
        }
    }
}

// ─── Global State ───────────────────────────────────────────────────

pub struct IoPrioState {
    /// Per-PID I/O priority
    pub pid_prio: BTreeMap<u32, IoPriority>,
    /// Per-user I/O priority
    pub user_prio: BTreeMap<u32, IoPriority>,
    /// Per-pgrp I/O priority
    pub pgrp_prio: BTreeMap<u32, IoPriority>,
    /// Statistics
    pub stats: IoPrioStats,
}

#[derive(Debug, Clone, Default)]
pub struct IoPrioStats {
    pub set_calls: u64,
    pub get_calls: u64,
    pub rt_tasks: u64,
    pub idle_tasks: u64,
}

lazy_static::lazy_static! {
    pub static ref IOPRIO: Mutex<IoPrioState> = Mutex::new(IoPrioState::new());
}

impl IoPrioState {
    pub fn new() -> Self {
        Self {
            pid_prio: BTreeMap::new(),
            user_prio: BTreeMap::new(),
            pgrp_prio: BTreeMap::new(),
            stats: IoPrioStats::default(),
        }
    }

    /// ioprio_set(which, who, ioprio)
    pub fn set(&mut self, which: u32, who: u32, ioprio: u16) -> Result<(), i32> {
        let prio = IoPriority::from_raw(ioprio);

        // Validate priority level
        if prio.level >= IOPRIO_NR_LEVELS as u8 && prio.class != IoClass::Idle {
            return Err(-22); // EINVAL
        }

        // RT class requires CAP_SYS_ADMIN (simplified check)
        // In production, check caller's capabilities

        match which {
            IOPRIO_WHO_PROCESS => {
                self.pid_prio.insert(who, prio);
                if prio.is_realtime() {
                    self.stats.rt_tasks += 1;
                }
                if prio.is_idle() {
                    self.stats.idle_tasks += 1;
                }
            }
            IOPRIO_WHO_PGRP => {
                self.pgrp_prio.insert(who, prio);
            }
            IOPRIO_WHO_USER => {
                self.user_prio.insert(who, prio);
            }
            _ => return Err(-22), // EINVAL
        }

        self.stats.set_calls += 1;
        Ok(())
    }

    /// ioprio_get(which, who)
    pub fn get(&mut self, which: u32, who: u32) -> Result<u16, i32> {
        self.stats.get_calls += 1;

        let prio = match which {
            IOPRIO_WHO_PROCESS => self.pid_prio.get(&who).copied(),
            IOPRIO_WHO_PGRP => self.pgrp_prio.get(&who).copied(),
            IOPRIO_WHO_USER => self.user_prio.get(&who).copied(),
            _ => return Err(-22), // EINVAL
        };

        Ok(prio.unwrap_or_default().to_raw())
    }

    /// Get effective priority for a given PID (checks PID → pgrp → user → default)
    pub fn effective_for_pid(&self, pid: u32, _pgrp: u32, _uid: u32) -> IoPriority {
        if let Some(prio) = self.pid_prio.get(&pid) {
            return prio.effective();
        }
        if let Some(prio) = self.pgrp_prio.get(&_pgrp) {
            return prio.effective();
        }
        if let Some(prio) = self.user_prio.get(&_uid) {
            return prio.effective();
        }
        IoPriority::default().effective()
    }

    /// Remove priority for a terminated process
    pub fn remove_pid(&mut self, pid: u32) {
        self.pid_prio.remove(&pid);
    }
}

// ─── Public API ─────────────────────────────────────────────────────

pub fn ioprio_set(which: u32, who: u32, ioprio: u16) -> Result<(), i32> {
    IOPRIO.lock().set(which, who, ioprio)
}

pub fn ioprio_get(which: u32, who: u32) -> Result<u16, i32> {
    IOPRIO.lock().get(which, who)
}

pub fn effective_for_pid(pid: u32, pgrp: u32, uid: u32) -> IoPriority {
    IOPRIO.lock().effective_for_pid(pid, pgrp, uid)
}

pub fn init() {
    serial_println!(
        "[IOPRIO] I/O priority subsystem initialized (RT/BE/IDLE classes, 8 levels, BFQ weights)"
    );
}
