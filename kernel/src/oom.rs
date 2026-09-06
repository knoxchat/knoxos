/// Out-Of-Memory Killer — Linux-compatible OOM management
///
/// Implements the kernel's last-resort memory reclamation:
///   - OOM score calculation for each process
///   - oom_score_adj per-process tuning (-1000 to 1000)
///   - Process selection heuristic (largest RSS, lowest priority)
///   - Kill selected process and reclaim memory
///   - /proc/[pid]/oom_score and /proc/[pid]/oom_score_adj
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::process::Pid;
use crate::serial_println;

/// OOM score adjustment range
pub const OOM_SCORE_ADJ_MIN: i32 = -1000;
pub const OOM_SCORE_ADJ_MAX: i32 = 1000;
/// Special value: this process is never killed
pub const OOM_SCORE_ADJ_DISABLE: i32 = -1000;

/// Per-process OOM state
#[derive(Debug, Clone)]
pub struct OomState {
    /// OOM score adjustment (-1000 to 1000)
    pub oom_score_adj: i32,
    /// Estimated memory usage in pages
    pub memory_pages: u64,
    /// Whether this process is unkillable (init, kernel threads)
    pub unkillable: bool,
}

impl Default for OomState {
    fn default() -> Self {
        Self::new()
    }
}

impl OomState {
    pub fn new() -> Self {
        OomState {
            oom_score_adj: 0,
            memory_pages: 0,
            unkillable: false,
        }
    }
}

/// OOM killer state
struct OomKiller {
    /// Per-process OOM state
    process_state: BTreeMap<Pid, OomState>,
    /// Total OOM kills
    total_kills: u64,
    /// Last killed PID
    last_killed_pid: Option<Pid>,
    /// Whether OOM killer is enabled
    enabled: bool,
    /// Panic on OOM instead of killing
    panic_on_oom: bool,
}

lazy_static::lazy_static! {
    static ref OOM: Mutex<OomKiller> = Mutex::new(OomKiller {
        process_state: BTreeMap::new(),
        total_kills: 0,
        last_killed_pid: None,
        enabled: true,
        panic_on_oom: false,
    });
}

/// Register a process with the OOM killer
pub fn register_process(pid: Pid) {
    let mut oom = OOM.lock();
    let mut state = OomState::new();
    // PID 1 (init) is unkillable
    if pid <= 1 {
        state.unkillable = true;
        state.oom_score_adj = OOM_SCORE_ADJ_DISABLE;
    }
    oom.process_state.insert(pid, state);
}

/// Unregister a process
pub fn unregister_process(pid: Pid) {
    let mut oom = OOM.lock();
    oom.process_state.remove(&pid);
}

/// Update memory usage for a process
pub fn update_memory(pid: Pid, pages: u64) {
    let mut oom = OOM.lock();
    if let Some(state) = oom.process_state.get_mut(&pid) {
        state.memory_pages = pages;
    }
}

/// Set OOM score adjustment for a process
pub fn set_oom_score_adj(pid: Pid, adj: i32) -> Result<(), i32> {
    if !(OOM_SCORE_ADJ_MIN..=OOM_SCORE_ADJ_MAX).contains(&adj) {
        return Err(-22); // EINVAL
    }
    let mut oom = OOM.lock();
    if let Some(state) = oom.process_state.get_mut(&pid) {
        state.oom_score_adj = adj;
        Ok(())
    } else {
        Err(-3) // ESRCH
    }
}

/// Get OOM score adjustment for a process
pub fn get_oom_score_adj(pid: Pid) -> Result<i32, i32> {
    let oom = OOM.lock();
    oom.process_state
        .get(&pid)
        .map(|s| s.oom_score_adj)
        .ok_or(-3) // ESRCH
}

/// Calculate OOM score for a process (0-1000, higher = more likely to be killed)
pub fn calculate_oom_score(pid: Pid) -> u32 {
    let oom = OOM.lock();
    let state = match oom.process_state.get(&pid) {
        Some(s) => s,
        None => return 0,
    };

    if state.unkillable || state.oom_score_adj == OOM_SCORE_ADJ_DISABLE {
        return 0;
    }

    // Base score from memory usage (normalized to 0-1000)
    let total_memory_pages = {
        let total: u64 = oom.process_state.values().map(|s| s.memory_pages).sum();
        if total == 0 { 1 } else { total }
    };

    let base_score = ((state.memory_pages * 1000) / total_memory_pages) as i32;

    // Apply adjustment
    let adjusted = base_score + state.oom_score_adj;

    // Clamp to 0-1000
    adjusted.clamp(0, 1000) as u32
}

/// Select the best victim process to kill
fn select_victim(oom: &OomKiller) -> Option<Pid> {
    let mut best_pid: Option<Pid> = None;
    let mut best_score: u32 = 0;

    let total_pages: u64 = oom.process_state.values().map(|s| s.memory_pages).sum();
    let total_pages = if total_pages == 0 { 1 } else { total_pages };

    for (&pid, state) in &oom.process_state {
        if state.unkillable || state.oom_score_adj == OOM_SCORE_ADJ_DISABLE {
            continue;
        }

        let base = ((state.memory_pages * 1000) / total_pages) as i32;
        let score = (base + state.oom_score_adj).clamp(0, 1000) as u32;

        if score > best_score {
            best_score = score;
            best_pid = Some(pid);
        }
    }

    best_pid
}

/// Trigger the OOM killer — called when memory allocation fails
pub fn trigger_oom() {
    let mut oom = OOM.lock();

    if !oom.enabled {
        serial_println!("[OOM] OOM killer disabled, allocation will fail");
        return;
    }

    if oom.panic_on_oom {
        panic!("[OOM] Out of memory! panic_on_oom is set");
    }

    serial_println!("[OOM] Out of memory! Selecting victim...");

    // Select victim
    let victim = match select_victim(&oom) {
        Some(pid) => pid,
        None => {
            serial_println!("[OOM] No eligible victim found!");
            return;
        }
    };

    let memory = oom
        .process_state
        .get(&victim)
        .map(|s| s.memory_pages)
        .unwrap_or(0);

    serial_println!(
        "[OOM] Killing process {} (score={}, memory={} pages)",
        victim,
        calculate_oom_score_internal(&oom, victim),
        memory
    );

    oom.total_kills += 1;
    oom.last_killed_pid = Some(victim);

    // Remove from our tracking
    oom.process_state.remove(&victim);

    // Drop lock before killing
    drop(oom);

    // Send SIGKILL to the victim process
    crate::process::kill(victim);
}

/// Internal score calculation (caller holds lock)
fn calculate_oom_score_internal(oom: &OomKiller, pid: Pid) -> u32 {
    let state = match oom.process_state.get(&pid) {
        Some(s) => s,
        None => return 0,
    };

    if state.unkillable {
        return 0;
    }

    let total: u64 = oom.process_state.values().map(|s| s.memory_pages).sum();
    let total = if total == 0 { 1 } else { total };
    let base = ((state.memory_pages * 1000) / total) as i32;
    (base + state.oom_score_adj).clamp(0, 1000) as u32
}

/// Get OOM statistics
pub fn stats() -> (u64, Option<Pid>, bool) {
    let oom = OOM.lock();
    (oom.total_kills, oom.last_killed_pid, oom.enabled)
}

/// Enable/disable OOM killer
pub fn set_enabled(enabled: bool) {
    let mut oom = OOM.lock();
    oom.enabled = enabled;
}

/// Set panic_on_oom
pub fn set_panic_on_oom(panic: bool) {
    let mut oom = OOM.lock();
    oom.panic_on_oom = panic;
}

/// Generate /proc/[pid]/oom_score content
pub fn proc_oom_score(pid: Pid) -> String {
    alloc::format!("{}\n", calculate_oom_score(pid))
}

/// Generate /proc/[pid]/oom_score_adj content
pub fn proc_oom_score_adj(pid: Pid) -> String {
    let adj = get_oom_score_adj(pid).unwrap_or(0);
    alloc::format!("{}\n", adj)
}

/// Initialize OOM killer
pub fn init() {
    // Register init process
    register_process(1);
    serial_println!("[KnoxOS] OOM killer initialized");
}
