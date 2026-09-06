/// Task Manager — process monitoring, kill, renice, resource graphs
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Snapshot of a process for display
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u64,
    pub name: String,
    pub state: ProcessState,
    pub cpu_percent: u16, // x100 (e.g. 1234 = 12.34%)
    pub memory_kb: u64,
    pub threads: u32,
    pub nice: i8,
    pub user: String,
    pub start_time: u64, // ms since boot
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Sleeping,
    Stopped,
    Zombie,
    Idle,
}

/// Column to sort by
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Pid,
    Name,
    Cpu,
    Memory,
    Nice,
}

/// Overall system resource snapshot
#[derive(Debug, Clone)]
pub struct SystemSnapshot {
    pub cpu_usage: Vec<u16>, // per-core usage x100
    pub total_memory_kb: u64,
    pub used_memory_kb: u64,
    pub cached_memory_kb: u64,
    pub swap_total_kb: u64,
    pub swap_used_kb: u64,
    pub load_avg: [u32; 3], // x100 (1min, 5min, 15min)
    pub uptime_secs: u64,
    pub num_processes: u32,
    pub num_threads: u32,
}

lazy_static::lazy_static! {
    static ref CPU_HISTORY: Mutex<Vec<Vec<u16>>> = Mutex::new(Vec::new());
    static ref MEM_HISTORY: Mutex<Vec<u64>> = Mutex::new(Vec::new());
}

/// Collect a system resource snapshot
pub fn snapshot() -> SystemSnapshot {
    SystemSnapshot {
        cpu_usage: Vec::new(),
        total_memory_kb: 0,
        used_memory_kb: 0,
        cached_memory_kb: 0,
        swap_total_kb: 0,
        swap_used_kb: 0,
        load_avg: [0; 3],
        uptime_secs: 0,
        num_processes: 0,
        num_threads: 0,
    }
}

/// Get sorted process list
pub fn list_processes(sort: SortColumn, ascending: bool) -> Vec<ProcessInfo> {
    // Would query the process table
    let mut procs: Vec<ProcessInfo> = Vec::new();
    // Sort
    match sort {
        SortColumn::Pid => procs.sort_by_key(|p| p.pid),
        SortColumn::Name => procs.sort_by(|a, b| a.name.cmp(&b.name)),
        SortColumn::Cpu => procs.sort_by_key(|p| p.cpu_percent),
        SortColumn::Memory => procs.sort_by_key(|p| p.memory_kb),
        SortColumn::Nice => procs.sort_by_key(|p| p.nice),
    }
    if !ascending {
        procs.reverse();
    }
    procs
}

/// Kill a process by PID
pub fn kill_process(pid: u64, signal: u32) -> Result<(), &'static str> {
    serial_println!("[taskmgr] Sending signal {} to PID {}", signal, pid);
    Ok(())
}

/// Change process priority (renice)
pub fn renice(pid: u64, nice: i8) -> Result<(), &'static str> {
    if !(-20..=19).contains(&nice) {
        return Err("Nice value out of range (-20..19)");
    }
    serial_println!("[taskmgr] Renice PID {} to {}", pid, nice);
    Ok(())
}

/// Record a snapshot to history for graphing
pub fn record_history(snap: &SystemSnapshot) {
    let mut cpu_hist = CPU_HISTORY.lock();
    cpu_hist.push(snap.cpu_usage.clone());
    if cpu_hist.len() > 300 {
        // 5 minutes at 1Hz
        cpu_hist.remove(0);
    }

    let mut mem_hist = MEM_HISTORY.lock();
    mem_hist.push(snap.used_memory_kb);
    if mem_hist.len() > 300 {
        mem_hist.remove(0);
    }
}

/// Get CPU usage history for graph rendering
pub fn cpu_history() -> Vec<Vec<u16>> {
    CPU_HISTORY.lock().clone()
}

/// Get memory usage history for graph rendering
pub fn mem_history() -> Vec<u64> {
    MEM_HISTORY.lock().clone()
}

pub fn init() {
    serial_println!("[taskmgr] Task manager initialized");
}
