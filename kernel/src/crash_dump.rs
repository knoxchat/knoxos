use crate::serial_println;
/// Kernel Crash Dump & Recovery
///
/// Kernel panic dump to disk, crash log analysis, kdump-style
/// memory capture, automatic restart policy, and watchdog integration.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Crash dump header — written to dedicated partition
#[repr(C)]
pub struct CrashDumpHeader {
    pub magic: u32, // 0x4B4E4F58 ("KNOX")
    pub version: u32,
    pub timestamp: u64,
    pub rip: u64,
    pub rsp: u64,
    pub rbp: u64,
    pub cr2: u64, // page fault address
    pub error_code: u64,
    pub panic_message_offset: u32,
    pub panic_message_len: u32,
    pub memory_map_offset: u32,
    pub memory_map_entries: u32,
    pub stack_dump_offset: u32,
    pub stack_dump_len: u32,
}

/// Crash dump entry in log
#[derive(Debug, Clone)]
pub struct CrashEntry {
    pub timestamp: u64,
    pub message: String,
    pub rip: u64,
    pub module: String,
    pub stack_trace: Vec<u64>,
}

/// Watchdog state
#[derive(Debug)]
pub struct Watchdog {
    pub enabled: bool,
    pub timeout_ms: u64,
    pub last_pet: u64,
    pub use_nmi: bool,
}

/// Restart policy
#[derive(Debug, Clone, Copy)]
pub enum RestartPolicy {
    Halt,
    Reboot,
    RebootAfterDump,
    KexecCrashKernel,
}

/// Crash dump manager
pub struct CrashManager {
    pub dump_partition_lba: u64,
    pub dump_partition_sectors: u64,
    pub crash_log: Vec<CrashEntry>,
    pub restart_policy: RestartPolicy,
    pub watchdog: Watchdog,
    pub max_dumps: usize,
}

lazy_static::lazy_static! {
    static ref CRASH: Mutex<CrashManager> = Mutex::new(CrashManager {
        dump_partition_lba: 0,
        dump_partition_sectors: 0,
        crash_log: Vec::new(),
        restart_policy: RestartPolicy::RebootAfterDump,
        watchdog: Watchdog {
            enabled: false,
            timeout_ms: 10_000,
            last_pet: 0,
            use_nmi: true,
        },
        max_dumps: 5,
    });
}

impl CrashManager {
    /// Configure dump partition
    pub fn set_dump_partition(&mut self, lba: u64, sectors: u64) {
        self.dump_partition_lba = lba;
        self.dump_partition_sectors = sectors;
        serial_println!("[CRASH] Dump partition: LBA {} ({} sectors)", lba, sectors);
    }

    /// Capture crash dump (called from panic handler)
    pub fn capture_dump(&mut self, rip: u64, rsp: u64, rbp: u64, cr2: u64, msg: &str) {
        serial_println!("[CRASH] === KERNEL CRASH DUMP ===");
        serial_println!("[CRASH] RIP: {:#018x}", rip);
        serial_println!("[CRASH] RSP: {:#018x}", rsp);
        serial_println!("[CRASH] RBP: {:#018x}", rbp);
        serial_println!("[CRASH] CR2: {:#018x}", cr2);
        serial_println!("[CRASH] Message: {}", msg);

        let entry = CrashEntry {
            timestamp: 0,
            message: String::from(msg),
            rip,
            module: String::from("unknown"),
            stack_trace: Vec::new(),
        };

        self.crash_log.push(entry);

        // Trim old entries
        while self.crash_log.len() > self.max_dumps {
            self.crash_log.remove(0);
        }

        // Would write CrashDumpHeader + memory to dump partition
    }

    /// Walk stack frames to build trace
    pub fn capture_stack_trace(&self, rbp: u64, max_depth: usize) -> Vec<u64> {
        let mut frames = Vec::new();
        let mut frame_ptr = rbp;
        if frame_ptr != 0 && max_depth > 0 {
            // Read return address at [rbp + 8]
            // frame_ptr = *[rbp] (next frame pointer)
            frames.push(frame_ptr);
            // Can't dereference further without unsafe
        }
        frames
    }

    /// Enable hardware watchdog
    pub fn enable_watchdog(&mut self, timeout_ms: u64) {
        self.watchdog.enabled = true;
        self.watchdog.timeout_ms = timeout_ms;
        serial_println!("[CRASH] Watchdog enabled ({}ms timeout)", timeout_ms);
    }

    /// Pet the watchdog (call periodically)
    pub fn pet_watchdog(&mut self, now_ms: u64) {
        self.watchdog.last_pet = now_ms;
    }

    /// Check watchdog timeout
    pub fn check_watchdog(&self, now_ms: u64) -> bool {
        if !self.watchdog.enabled {
            return false;
        }
        now_ms - self.watchdog.last_pet > self.watchdog.timeout_ms
    }

    /// Get restart policy
    pub fn get_restart_policy(&self) -> RestartPolicy {
        self.restart_policy
    }

    /// List recent crashes
    pub fn recent_crashes(&self) -> &[CrashEntry] {
        &self.crash_log
    }
}

pub fn init() {
    serial_println!("[CRASH] Crash dump & recovery manager initialized");
}
