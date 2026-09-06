/// membarrier — Memory barrier expediting
///
/// Implements the Linux membarrier() syscall which provides a way
/// to execute memory barriers across CPUs without expensive
/// inter-processor interrupts in the fast path.
///
/// Features:
/// - MEMBARRIER_CMD_GLOBAL — global memory barrier on all CPUs
/// - MEMBARRIER_CMD_GLOBAL_EXPEDITED — expedited global barrier
/// - MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED
/// - MEMBARRIER_CMD_PRIVATE_EXPEDITED — per-process expedited
/// - MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED
/// - MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE — for JIT/self-modifying code
/// - MEMBARRIER_CMD_PRIVATE_EXPEDITED_RSEQ — restartable sequence barrier
/// - Used by RCU, JIT compilers, userspace sequence locks
use alloc::collections::BTreeSet;
use spin::Mutex;

use crate::serial_println;

// ─── Command constants ──────────────────────────────────────────────

pub const MEMBARRIER_CMD_QUERY: u32 = 0;
pub const MEMBARRIER_CMD_GLOBAL: u32 = 1 << 0;
pub const MEMBARRIER_CMD_GLOBAL_EXPEDITED: u32 = 1 << 1;
pub const MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED: u32 = 1 << 2;
pub const MEMBARRIER_CMD_PRIVATE_EXPEDITED: u32 = 1 << 3;
pub const MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED: u32 = 1 << 4;
pub const MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE: u32 = 1 << 5;
pub const MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_SYNC_CORE: u32 = 1 << 6;
pub const MEMBARRIER_CMD_PRIVATE_EXPEDITED_RSEQ: u32 = 1 << 7;
pub const MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_RSEQ: u32 = 1 << 8;
pub const MEMBARRIER_CMD_GET_REGISTRATIONS: u32 = 1 << 9;

/// All supported commands bitmask
pub const MEMBARRIER_CMD_SUPPORTED: u32 = MEMBARRIER_CMD_GLOBAL
    | MEMBARRIER_CMD_GLOBAL_EXPEDITED
    | MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED
    | MEMBARRIER_CMD_PRIVATE_EXPEDITED
    | MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED
    | MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE
    | MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_SYNC_CORE
    | MEMBARRIER_CMD_PRIVATE_EXPEDITED_RSEQ
    | MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_RSEQ
    | MEMBARRIER_CMD_GET_REGISTRATIONS;

// ─── Registration State ─────────────────────────────────────────────

/// Per-process membarrier registrations
#[derive(Debug, Clone, Default)]
pub struct ProcessRegistration {
    /// Registered for global expedited
    pub global_expedited: bool,
    /// Registered for private expedited
    pub private_expedited: bool,
    /// Registered for private expedited sync core
    pub private_expedited_sync_core: bool,
    /// Registered for private expedited rseq
    pub private_expedited_rseq: bool,
}

impl ProcessRegistration {
    /// Return bitmask of all registrations
    pub fn as_mask(&self) -> u32 {
        let mut mask = 0u32;
        if self.global_expedited {
            mask |= MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED;
        }
        if self.private_expedited {
            mask |= MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED;
        }
        if self.private_expedited_sync_core {
            mask |= MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_SYNC_CORE;
        }
        if self.private_expedited_rseq {
            mask |= MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_RSEQ;
        }
        mask
    }
}

// ─── Global State ───────────────────────────────────────────────────

pub struct MembarrierState {
    /// Per-PID registrations
    pub registrations: alloc::collections::BTreeMap<u32, ProcessRegistration>,
    /// PIDs registered for global expedited (for fast lookup)
    pub global_expedited_pids: BTreeSet<u32>,
    /// Statistics
    pub stats: MembarrierStats,
}

#[derive(Debug, Clone, Default)]
pub struct MembarrierStats {
    pub query_calls: u64,
    pub global_barriers: u64,
    pub global_expedited_barriers: u64,
    pub private_expedited_barriers: u64,
    pub sync_core_barriers: u64,
    pub rseq_barriers: u64,
    pub registrations: u64,
}

lazy_static::lazy_static! {
    pub static ref MEMBARRIER: Mutex<MembarrierState> = Mutex::new(MembarrierState::new());
}

impl Default for MembarrierState {
    fn default() -> Self {
        Self::new()
    }
}

impl MembarrierState {
    pub fn new() -> Self {
        Self {
            registrations: alloc::collections::BTreeMap::new(),
            global_expedited_pids: BTreeSet::new(),
            stats: MembarrierStats::default(),
        }
    }

    /// Main membarrier() syscall implementation
    pub fn sys_membarrier(&mut self, cmd: u32, _flags: u32, pid: u32) -> Result<u32, i32> {
        match cmd {
            MEMBARRIER_CMD_QUERY => {
                self.stats.query_calls += 1;
                Ok(MEMBARRIER_CMD_SUPPORTED)
            }

            MEMBARRIER_CMD_GLOBAL => {
                // Issue a full memory barrier on all CPUs
                // In hardware, this sends IPIs to all CPUs
                core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
                self.stats.global_barriers += 1;
                Ok(0)
            }

            MEMBARRIER_CMD_GLOBAL_EXPEDITED => {
                // Expedited global barrier — only target CPUs running registered threads
                let reg = self.registrations.get(&pid);
                if reg.is_some_and(|r| r.global_expedited)
                    || self.global_expedited_pids.contains(&pid)
                {
                    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
                    self.stats.global_expedited_barriers += 1;
                    Ok(0)
                } else {
                    // Not registered — still works, just not expedited
                    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
                    self.stats.global_expedited_barriers += 1;
                    Ok(0)
                }
            }

            MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED => {
                let reg = self.registrations.entry(pid).or_default();
                reg.global_expedited = true;
                self.global_expedited_pids.insert(pid);
                self.stats.registrations += 1;
                Ok(0)
            }

            MEMBARRIER_CMD_PRIVATE_EXPEDITED => {
                // Private expedited — barrier only on CPUs running threads of this process
                let reg = self.registrations.get(&pid);
                if !reg.is_some_and(|r| r.private_expedited) {
                    return Err(-1); // EPERM — not registered
                }
                core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
                self.stats.private_expedited_barriers += 1;
                Ok(0)
            }

            MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED => {
                let reg = self.registrations.entry(pid).or_default();
                reg.private_expedited = true;
                self.stats.registrations += 1;
                Ok(0)
            }

            MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE => {
                // Sync-core: ensures instruction pipeline is flushed
                // Critical for JIT compilers that modify code
                let reg = self.registrations.get(&pid);
                if !reg.is_some_and(|r| r.private_expedited_sync_core) {
                    return Err(-1); // EPERM
                }
                core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
                // On real hardware, this would also do CPUID or serialize instruction
                self.stats.sync_core_barriers += 1;
                Ok(0)
            }

            MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_SYNC_CORE => {
                let reg = self.registrations.entry(pid).or_default();
                reg.private_expedited_sync_core = true;
                self.stats.registrations += 1;
                Ok(0)
            }

            MEMBARRIER_CMD_PRIVATE_EXPEDITED_RSEQ => {
                // RSEQ barrier: abort restartable sequences on target CPUs
                let reg = self.registrations.get(&pid);
                if !reg.is_some_and(|r| r.private_expedited_rseq) {
                    return Err(-1);
                }
                core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
                self.stats.rseq_barriers += 1;
                Ok(0)
            }

            MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_RSEQ => {
                let reg = self.registrations.entry(pid).or_default();
                reg.private_expedited_rseq = true;
                self.stats.registrations += 1;
                Ok(0)
            }

            MEMBARRIER_CMD_GET_REGISTRATIONS => {
                let mask = self.registrations.get(&pid).map_or(0, |r| r.as_mask());
                Ok(mask)
            }

            _ => Err(-22), // EINVAL
        }
    }

    /// Cleanup when process exits
    pub fn process_exit(&mut self, pid: u32) {
        self.registrations.remove(&pid);
        self.global_expedited_pids.remove(&pid);
    }
}

// ─── Public API ─────────────────────────────────────────────────────

pub fn sys_membarrier(cmd: u32, flags: u32, pid: u32) -> Result<u32, i32> {
    MEMBARRIER.lock().sys_membarrier(cmd, flags, pid)
}

pub fn process_exit(pid: u32) {
    MEMBARRIER.lock().process_exit(pid);
}

pub fn init() {
    serial_println!(
        "[MEMBARRIER] Memory barrier subsystem initialized (global, expedited, sync-core, rseq)"
    );
}
