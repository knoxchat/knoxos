// kcov.rs — Kernel code coverage for fuzzing
// Supports coverage-guided fuzzing (like syzkaller)

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

/// KCOV mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KcovMode {
    Disabled,
    TracePC,  // Instruction coverage
    TraceCmp, // Comparison coverage
}

/// KCOV ioctl commands
pub const KCOV_INIT_TRACE: u64 = 0xC0086301;
pub const KCOV_ENABLE: u64 = 0x6364;
pub const KCOV_DISABLE: u64 = 0x6365;

/// KCOV trace modes
pub const KCOV_TRACE_PC: u64 = 0;
pub const KCOV_TRACE_CMP: u64 = 1;

/// Maximum coverage buffer size
const MAX_COVER_SIZE: usize = 1 << 20; // 1M entries

/// A KCOV instance (one per thread)
#[derive(Debug)]
pub struct KcovInstance {
    pub id: u64,
    pub pid: u64,
    pub mode: KcovMode,
    pub cover_size: usize,
    /// Coverage buffer: first entry is count, rest are PCs
    pub cover: Vec<u64>,
    /// Comparison operands
    pub comparisons: Vec<KcovCmp>,
    pub enabled: bool,
    /// Total PCs recorded across all sessions
    pub total_pcs: u64,
}

/// Comparison coverage entry
#[derive(Debug, Clone)]
pub struct KcovCmp {
    pub pc: u64,
    pub arg1: u64,
    pub arg2: u64,
    pub size: u8, // 1, 2, 4, or 8 bytes
    pub is_const: bool,
}

impl KcovInstance {
    pub fn new(id: u64, cover_size: usize) -> Self {
        let size = core::cmp::min(cover_size, MAX_COVER_SIZE);
        KcovInstance {
            id,
            pid: 0,
            mode: KcovMode::Disabled,
            cover_size: size,
            cover: alloc::vec![0u64; size + 1], // +1 for count at index 0
            comparisons: Vec::new(),
            enabled: false,
            total_pcs: 0,
        }
    }

    /// Enable coverage collection
    pub fn enable(&mut self, mode: u64, pid: u64) -> Result<(), KcovError> {
        if self.enabled {
            return Err(KcovError::AlreadyEnabled);
        }

        self.mode = match mode {
            KCOV_TRACE_PC => KcovMode::TracePC,
            KCOV_TRACE_CMP => KcovMode::TraceCmp,
            _ => return Err(KcovError::InvalidMode),
        };

        self.pid = pid;
        self.enabled = true;
        self.cover[0] = 0; // Reset count
        self.comparisons.clear();

        Ok(())
    }

    /// Disable coverage collection
    pub fn disable(&mut self) {
        self.enabled = false;
        self.mode = KcovMode::Disabled;
    }

    /// Record a PC (instruction pointer)
    pub fn trace_pc(&mut self, pc: u64) {
        if !self.enabled || self.mode != KcovMode::TracePC {
            return;
        }

        let count = self.cover[0] as usize;
        if count < self.cover_size {
            self.cover[count + 1] = pc;
            self.cover[0] = (count + 1) as u64;
            self.total_pcs += 1;
        }
    }

    /// Record a comparison
    pub fn trace_cmp(&mut self, pc: u64, arg1: u64, arg2: u64, size: u8, is_const: bool) {
        if !self.enabled || self.mode != KcovMode::TraceCmp {
            return;
        }

        self.comparisons.push(KcovCmp {
            pc,
            arg1,
            arg2,
            size,
            is_const,
        });
    }

    /// Get current coverage count
    pub fn count(&self) -> usize {
        self.cover[0] as usize
    }

    /// Reset coverage buffer
    pub fn reset(&mut self) {
        self.cover[0] = 0;
        self.comparisons.clear();
    }

    /// Get coverage PCs
    pub fn get_pcs(&self) -> &[u64] {
        let count = self.cover[0] as usize;
        &self.cover[1..=count]
    }

    /// Get unique PCs (deduplicated)
    pub fn get_unique_pcs(&self) -> Vec<u64> {
        let count = self.cover[0] as usize;
        let mut pcs: Vec<u64> = self.cover[1..=count].to_vec();
        pcs.sort();
        pcs.dedup();
        pcs
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KcovError {
    NotFound,
    AlreadyEnabled,
    InvalidMode,
    InvalidSize,
    TooMany,
}

lazy_static! {
    static ref INSTANCES: Mutex<BTreeMap<u64, KcovInstance>> = Mutex::new(BTreeMap::new());
    static ref NEXT_ID: Mutex<u64> = Mutex::new(1);
    /// Map thread IDs to active kcov instance
    static ref THREAD_KCOV: Mutex<BTreeMap<u64, u64>> = Mutex::new(BTreeMap::new());
}

/// Create a KCOV device instance
pub fn kcov_open() -> u64 {
    let mut next = NEXT_ID.lock();
    let id = *next;
    *next += 1;
    drop(next);

    // Default size, will be set by KCOV_INIT_TRACE
    let instance = KcovInstance::new(id, 0);
    INSTANCES.lock().insert(id, instance);

    id
}

/// Initialize trace buffer (KCOV_INIT_TRACE ioctl)
pub fn kcov_init_trace(id: u64, size: usize) -> Result<(), KcovError> {
    let mut instances = INSTANCES.lock();
    let instance = instances.get_mut(&id).ok_or(KcovError::NotFound)?;

    if size == 0 || size > MAX_COVER_SIZE {
        return Err(KcovError::InvalidSize);
    }

    instance.cover_size = size;
    instance.cover = alloc::vec![0u64; size + 1];

    Ok(())
}

/// Enable coverage (KCOV_ENABLE ioctl)
pub fn kcov_enable(id: u64, mode: u64, pid: u64) -> Result<(), KcovError> {
    let mut instances = INSTANCES.lock();
    let instance = instances.get_mut(&id).ok_or(KcovError::NotFound)?;
    instance.enable(mode, pid)?;
    drop(instances);

    THREAD_KCOV.lock().insert(pid, id);

    Ok(())
}

/// Disable coverage (KCOV_DISABLE ioctl)
pub fn kcov_disable(id: u64) -> Result<(), KcovError> {
    let mut instances = INSTANCES.lock();
    let instance = instances.get_mut(&id).ok_or(KcovError::NotFound)?;

    let pid = instance.pid;
    instance.disable();
    drop(instances);

    THREAD_KCOV.lock().remove(&pid);

    Ok(())
}

/// Close a KCOV instance
pub fn kcov_close(id: u64) {
    let mut instances = INSTANCES.lock();
    if let Some(instance) = instances.get(&id) {
        let pid = instance.pid;
        THREAD_KCOV.lock().remove(&pid);
    }
    instances.remove(&id);
}

/// Record a PC for the current thread (called from instrumented code)
pub fn kcov_trace_pc_for_thread(tid: u64, pc: u64) {
    let thread_kcov = THREAD_KCOV.lock();
    if let Some(&kcov_id) = thread_kcov.get(&tid) {
        drop(thread_kcov);
        let mut instances = INSTANCES.lock();
        if let Some(instance) = instances.get_mut(&kcov_id) {
            instance.trace_pc(pc);
        }
    }
}

/// Record a comparison for the current thread
pub fn kcov_trace_cmp_for_thread(tid: u64, pc: u64, arg1: u64, arg2: u64, size: u8) {
    let thread_kcov = THREAD_KCOV.lock();
    if let Some(&kcov_id) = thread_kcov.get(&tid) {
        drop(thread_kcov);
        let mut instances = INSTANCES.lock();
        if let Some(instance) = instances.get_mut(&kcov_id) {
            instance.trace_cmp(pc, arg1, arg2, size, false);
        }
    }
}

/// Get coverage statistics for an instance
pub fn kcov_stats(id: u64) -> Option<KcovStats> {
    let instances = INSTANCES.lock();
    instances.get(&id).map(|inst| {
        let unique_count = inst.get_unique_pcs().len();
        KcovStats {
            id: inst.id,
            mode: inst.mode,
            cover_size: inst.cover_size,
            current_count: inst.count(),
            unique_pcs: unique_count,
            total_pcs: inst.total_pcs,
            comparisons: inst.comparisons.len(),
            enabled: inst.enabled,
        }
    })
}

#[derive(Debug, Clone)]
pub struct KcovStats {
    pub id: u64,
    pub mode: KcovMode,
    pub cover_size: usize,
    pub current_count: usize,
    pub unique_pcs: usize,
    pub total_pcs: u64,
    pub comparisons: usize,
    pub enabled: bool,
}

/// Initialize kcov subsystem
pub fn init() {
    crate::serial_println!(
        "  kcov subsystem initialized (TRACE_PC, TRACE_CMP, coverage-guided fuzzing)"
    );
}
