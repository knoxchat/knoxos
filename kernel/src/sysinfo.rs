/// System Information — /proc/sysinfo and sysinfo() syscall backing
/// Provides comprehensive system statistics for Linux compatibility
///
/// Implements the sysinfo struct returned by the sysinfo(2) syscall
use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::serial_println;

/// Detect total physical RAM from multiboot memory map, fallback to 256 MiB
fn detect_total_ram() -> u64 {
    let mb = crate::multiboot::total_memory();
    if mb > 0 {
        mb
    } else {
        256 * 1024 * 1024 // Fallback for non-multiboot environments
    }
}

/// Total swap (no swap partition configured by default)
const TOTAL_SWAP: u64 = 0;

/// Memory unit (1 byte for exact accounting)
const MEM_UNIT: u32 = 1;

static SHARED_RAM: AtomicU64 = AtomicU64::new(0);
static BUFFER_RAM: AtomicU64 = AtomicU64::new(1024 * 1024); // 1 MiB buffer cache

/// Linux-compatible sysinfo struct (matches <sys/sysinfo.h>)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SysInfo {
    /// Seconds since boot
    pub uptime: i64,
    /// 1, 5, and 15 minute load averages (scaled by 65536)
    pub loads: [u64; 3],
    /// Total usable main memory size
    pub totalram: u64,
    /// Available memory size
    pub freeram: u64,
    /// Amount of shared memory
    pub sharedram: u64,
    /// Memory used by buffers
    pub bufferram: u64,
    /// Total swap space size
    pub totalswap: u64,
    /// Swap space still available
    pub freeswap: u64,
    /// Number of current processes
    pub procs: u16,
    /// Padding
    pub _pad: u16,
    /// Total high memory size
    pub totalhigh: u64,
    /// Available high memory size
    pub freehigh: u64,
    /// Memory unit size in bytes
    pub mem_unit: u32,
}

impl Default for SysInfo {
    fn default() -> Self {
        Self {
            uptime: 0,
            loads: [0; 3],
            totalram: 0,
            freeram: 0,
            sharedram: 0,
            bufferram: 0,
            totalswap: 0,
            freeswap: 0,
            procs: 0,
            _pad: 0,
            totalhigh: 0,
            freehigh: 0,
            mem_unit: 1,
        }
    }
}

/// Initialize sysinfo subsystem
pub fn init() {
    serial_println!("[KnoxOS] System information subsystem initialized");
}

/// Get current system info (for sysinfo() syscall)
pub fn get_sysinfo() -> SysInfo {
    let now = crate::rtc::unix_time() as u64;
    let boot = now.saturating_sub(crate::interrupts::get_ticks() / 18); // ~18 ticks/sec

    let heap_size = crate::allocator::HEAP_SIZE as u64;
    let used_estimate = heap_size / 3; // Rough estimate

    let pt = crate::process::PROCESS_TABLE.lock();
    let procs = pt.processes.len() as u16;
    let running = pt
        .processes
        .iter()
        .filter(|p| p.state == crate::process::ProcessState::Running)
        .count() as u64;

    // Load averages scaled by 65536 (SI_LOAD_SHIFT = 16)
    let load_scale = 65536u64;
    let load1 = running * load_scale;
    let load5 = (running * load_scale * 80) / 100;
    let load15 = (running * load_scale * 60) / 100;

    let total_ram = detect_total_ram();

    SysInfo {
        uptime: (crate::interrupts::get_ticks() / 18) as i64,
        loads: [load1, load5, load15],
        totalram: total_ram,
        freeram: total_ram.saturating_sub(used_estimate),
        sharedram: SHARED_RAM.load(Ordering::Relaxed),
        bufferram: BUFFER_RAM.load(Ordering::Relaxed),
        totalswap: TOTAL_SWAP,
        freeswap: TOTAL_SWAP,
        procs,
        _pad: 0,
        totalhigh: 0,
        freehigh: 0,
        mem_unit: MEM_UNIT,
    }
}

/// Update shared memory tracking
pub fn record_shared_memory(bytes: u64) {
    SHARED_RAM.fetch_add(bytes, Ordering::Relaxed);
}

/// Write sysinfo to a user-space pointer (for sysinfo() syscall)
pub fn write_sysinfo_to_user(ptr: u64) -> Result<(), ()> {
    if ptr == 0 {
        return Err(());
    }
    let info = get_sysinfo();
    let dest = ptr as *mut SysInfo;
    unsafe {
        core::ptr::write(dest, info);
    }
    Ok(())
}

/// Format system info as a human-readable string (for `sysinfo` shell command)
pub fn format_sysinfo() -> String {
    let info = get_sysinfo();
    let uptime_h = info.uptime / 3600;
    let uptime_m = (info.uptime % 3600) / 60;
    let uptime_s = info.uptime % 60;

    format!(
        "KnoxOS System Information\n\
         ═════════════════════════\n\
         Uptime:       {:02}:{:02}:{:02}\n\
         Processes:    {}\n\
         Load Average: {:.2} {:.2} {:.2}\n\
         Total RAM:    {} MiB\n\
         Free RAM:     {} MiB\n\
         Shared:       {} KiB\n\
         Buffers:      {} KiB\n\
         Total Swap:   {} MiB\n\
         Free Swap:    {} MiB\n\
         Mem Unit:     {} bytes\n",
        uptime_h,
        uptime_m,
        uptime_s,
        info.procs,
        info.loads[0] as f64 / 65536.0,
        info.loads[1] as f64 / 65536.0,
        info.loads[2] as f64 / 65536.0,
        info.totalram / (1024 * 1024),
        info.freeram / (1024 * 1024),
        info.sharedram / 1024,
        info.bufferram / 1024,
        info.totalswap / (1024 * 1024),
        info.freeswap / (1024 * 1024),
        info.mem_unit,
    )
}
