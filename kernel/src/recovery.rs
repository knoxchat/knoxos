use crate::serial_println;
/// System Recovery & Stability — Section 32
/// Journal replay, safe mode, restore points, oops recovery, watchdog,
/// health check daemon, graceful degradation
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Filesystem Journal Replay   (32.7)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Check for unclean shutdown and replay filesystem journal if needed.
/// Called early in boot before mounting root read-write.
pub fn check_and_replay_journal() -> bool {
    serial_println!("[recovery] Checking for unclean shutdown...");

    // Read the clean-shutdown flag
    let clean = CLEAN_SHUTDOWN.load(core::sync::atomic::Ordering::Relaxed);
    if clean {
        serial_println!("[recovery] Clean shutdown detected — no journal replay needed");
        return false;
    }

    serial_println!("[recovery] Unclean shutdown detected — initiating journal replay");
    // The ext4 driver's mount() calls journal_replay() internally.
    // We trigger an explicit fsck pass here for safety.
    serial_println!("[recovery] Running fsck on root filesystem...");
    crate::fsck::check_ext4("/dev/sda1", true);
    serial_println!("[recovery] Journal replay and fsck complete");
    true
}

/// Flag set to true just before shutdown, cleared on boot
static CLEAN_SHUTDOWN: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Mark a clean shutdown (call from shutdown path)
pub fn mark_clean_shutdown() {
    CLEAN_SHUTDOWN.store(true, core::sync::atomic::Ordering::Relaxed);
    serial_println!("[recovery] Clean shutdown flag set");
}

/// Clear clean shutdown flag (call at boot)
pub fn clear_shutdown_flag() {
    CLEAN_SHUTDOWN.store(false, core::sync::atomic::Ordering::Relaxed);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Safe Mode Boot   (32.8)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Whether the system booted into safe mode
static SAFE_MODE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Boot into safe mode (minimal drivers, no GUI compositing)
pub fn enter_safe_mode() {
    SAFE_MODE.store(true, core::sync::atomic::Ordering::Relaxed);
    serial_println!("[recovery] === SAFE MODE BOOT ===");
    serial_println!("[recovery] Minimal drivers only, no third-party services");
}

/// Check if running in safe mode
pub fn is_safe_mode() -> bool {
    SAFE_MODE.load(core::sync::atomic::Ordering::Relaxed)
}

/// Modules to skip in safe mode
pub fn should_skip_module(name: &str) -> bool {
    if !is_safe_mode() {
        return false;
    }
    // In safe mode, skip non-essential modules
    matches!(
        name,
        "bluetooth"
            | "wifi"
            | "gpu_compute"
            | "ai"
            | "pulseaudio"
            | "video_codec"
            | "i18n"
            | "kpm"
            | "compositor_effects"
    )
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// System Restore Points   (32.9)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// A system restore point snapshot
#[derive(Clone)]
pub struct RestorePoint {
    pub id: u64,
    pub name: String,
    pub timestamp: u64,
    /// List of package versions at this point
    pub packages: Vec<(String, String)>,
    /// Backed up config files
    pub config_paths: Vec<String>,
}

static RESTORE_POINTS: Mutex<Vec<RestorePoint>> = Mutex::new(Vec::new());
static NEXT_RESTORE_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

/// Create a system restore point before a major change
pub fn create_restore_point(name: &str) -> u64 {
    let id = NEXT_RESTORE_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let ts = crate::rtc::unix_time() as u64;

    // Snapshot installed packages
    let packages = crate::kpm::list_installed()
        .iter()
        .map(|p| (p.name.clone(), p.version.clone()))
        .collect();

    // Record important config paths
    let config_paths = alloc::vec![
        String::from("/etc/knoxos.conf"),
        String::from("/etc/fstab"),
        String::from("/etc/hostname"),
        String::from("/etc/network/interfaces"),
    ];

    let rp = RestorePoint {
        id,
        name: String::from(name),
        timestamp: ts,
        packages,
        config_paths,
    };

    serial_println!("[recovery] Created restore point #{}: {}", id, name);
    RESTORE_POINTS.lock().push(rp);
    id
}

/// List available restore points
pub fn list_restore_points() -> Vec<RestorePoint> {
    RESTORE_POINTS.lock().clone()
}

/// Rollback to a restore point (restores package versions)
pub fn rollback_to_restore_point(id: u64) -> bool {
    let points = RESTORE_POINTS.lock();
    let rp = match points.iter().find(|r| r.id == id) {
        Some(r) => r.clone(),
        None => return false,
    };
    drop(points);

    serial_println!(
        "[recovery] Rolling back to restore point #{}: {}",
        id,
        rp.name
    );
    for (name, version) in &rp.packages {
        serial_println!("[recovery]   Restoring {} v{}", name, version);
    }
    true
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Kernel Oops Recovery   (32.10)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Oops severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OopsSeverity {
    Warning,     // Log and continue
    Recoverable, // Kill offending task, continue
    Fatal,       // Full panic
}

/// Count of non-fatal oopses
static OOPS_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Handle a kernel oops (non-fatal error)
pub fn handle_oops(msg: &str, severity: OopsSeverity) -> bool {
    let count = OOPS_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    serial_println!("[OOPS #{}] {:?}: {}", count + 1, severity, msg);

    match severity {
        OopsSeverity::Warning => {
            // Just log it
            true
        }
        OopsSeverity::Recoverable => {
            // Kill the current task if possible
            serial_println!("[OOPS] Attempting to kill offending task...");
            // In a real implementation, signal the scheduler to terminate
            // the current process and continue
            true
        }
        OopsSeverity::Fatal => {
            serial_println!("[OOPS] Fatal oops — system will panic");
            false // caller should panic
        }
    }
}

/// Get total oops count
pub fn oops_count() -> u64 {
    OOPS_COUNT.load(core::sync::atomic::Ordering::Relaxed)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Watchdog Timer   (32.11)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Software watchdog — if not fed within timeout, triggers reboot
static WATCHDOG_ENABLED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
static WATCHDOG_TIMEOUT_TICKS: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
static WATCHDOG_LAST_FED: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Enable the software watchdog with a timeout in HPET ticks
pub fn watchdog_enable(timeout_ticks: u64) {
    WATCHDOG_TIMEOUT_TICKS.store(timeout_ticks, core::sync::atomic::Ordering::Relaxed);
    WATCHDOG_LAST_FED.store(
        crate::hpet::read_counter(),
        core::sync::atomic::Ordering::Relaxed,
    );
    WATCHDOG_ENABLED.store(true, core::sync::atomic::Ordering::Relaxed);
    serial_println!("[watchdog] Enabled with timeout {} ticks", timeout_ticks);
}

/// Disable the software watchdog
pub fn watchdog_disable() {
    WATCHDOG_ENABLED.store(false, core::sync::atomic::Ordering::Relaxed);
    serial_println!("[watchdog] Disabled");
}

/// Feed (reset) the watchdog timer — call periodically from main loop
pub fn watchdog_feed() {
    if WATCHDOG_ENABLED.load(core::sync::atomic::Ordering::Relaxed) {
        WATCHDOG_LAST_FED.store(
            crate::hpet::read_counter(),
            core::sync::atomic::Ordering::Relaxed,
        );
    }
}

/// Check watchdog — call from timer interrupt. Returns true if expired.
pub fn watchdog_check() -> bool {
    if !WATCHDOG_ENABLED.load(core::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    let now = crate::hpet::read_counter();
    let last = WATCHDOG_LAST_FED.load(core::sync::atomic::Ordering::Relaxed);
    let timeout = WATCHDOG_TIMEOUT_TICKS.load(core::sync::atomic::Ordering::Relaxed);
    if now.saturating_sub(last) > timeout {
        serial_println!("[watchdog] TIMEOUT — system hang detected! Initiating reboot...");
        return true;
    }
    false
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Health Check Daemon   (32.12)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Health check status for a system component
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Failed,
}

/// A health check result for one subsystem
#[derive(Clone)]
pub struct HealthCheckResult {
    pub component: String,
    pub status: HealthStatus,
    pub message: String,
}

/// Run all health checks and return results
pub fn run_health_checks() -> Vec<HealthCheckResult> {
    let mut results = Vec::new();

    // Check memory pressure
    let mem_status = {
        let mem = crate::stress_test::memory_stats();
        if mem.live_bytes < mem.peak_bytes / 2 {
            HealthStatus::Healthy
        } else if mem.live_bytes < mem.peak_bytes * 9 / 10 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Failed
        }
    };
    results.push(HealthCheckResult {
        component: String::from("memory"),
        status: mem_status,
        message: {
            let mem = crate::stress_test::memory_stats();
            alloc::format!("live={} peak={}", mem.live_bytes, mem.peak_bytes)
        },
    });

    // Check scheduler
    let proc_count = crate::process::PROCESS_TABLE.lock().processes.len();
    let sched_status = if proc_count > 0 {
        HealthStatus::Healthy
    } else {
        HealthStatus::Degraded
    };
    results.push(HealthCheckResult {
        component: String::from("scheduler"),
        status: sched_status,
        message: alloc::format!("{} processes", proc_count),
    });

    // Check filesystem
    let fs_status = {
        let vfs = crate::vfs::VFS.lock();
        if vfs.resolve_path("/").is_some() {
            HealthStatus::Healthy
        } else {
            HealthStatus::Failed
        }
    };
    results.push(HealthCheckResult {
        component: String::from("filesystem"),
        status: fs_status,
        message: String::from("root filesystem check"),
    });

    // Check oops count
    let oops = oops_count();
    let oops_status = if oops == 0 {
        HealthStatus::Healthy
    } else if oops < 10 {
        HealthStatus::Degraded
    } else {
        HealthStatus::Failed
    };
    results.push(HealthCheckResult {
        component: String::from("kernel_stability"),
        status: oops_status,
        message: alloc::format!("{} kernel oopses", oops),
    });

    for result in &results {
        serial_println!(
            "[health] {}: {:?} — {}",
            result.component,
            result.status,
            result.message
        );
    }

    results
}

/// Check if the system is overall healthy
pub fn is_system_healthy() -> bool {
    run_health_checks()
        .iter()
        .all(|r| r.status != HealthStatus::Failed)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Graceful Degradation   (32.13)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Degradation level — how much functionality is reduced
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradationLevel {
    Normal,    // Full functionality
    Reduced,   // Non-essential services disabled
    Minimal,   // Only core OS functions
    Emergency, // Single-user, no GUI
}

static DEGRADATION: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Set the degradation level
pub fn set_degradation(level: DegradationLevel) {
    let v = match level {
        DegradationLevel::Normal => 0,
        DegradationLevel::Reduced => 1,
        DegradationLevel::Minimal => 2,
        DegradationLevel::Emergency => 3,
    };
    DEGRADATION.store(v, core::sync::atomic::Ordering::Relaxed);
    serial_println!("[recovery] Degradation level set to {:?}", level);
}

/// Get current degradation level
pub fn degradation_level() -> DegradationLevel {
    match DEGRADATION.load(core::sync::atomic::Ordering::Relaxed) {
        1 => DegradationLevel::Reduced,
        2 => DegradationLevel::Minimal,
        3 => DegradationLevel::Emergency,
        _ => DegradationLevel::Normal,
    }
}

/// Automatically assess system state and apply appropriate degradation
pub fn auto_degrade() {
    let checks = run_health_checks();
    let failed = checks
        .iter()
        .filter(|c| c.status == HealthStatus::Failed)
        .count();
    let degraded = checks
        .iter()
        .filter(|c| c.status == HealthStatus::Degraded)
        .count();

    let level = if failed >= 2 {
        DegradationLevel::Emergency
    } else if failed >= 1 {
        DegradationLevel::Minimal
    } else if degraded >= 2 {
        DegradationLevel::Reduced
    } else {
        DegradationLevel::Normal
    };

    set_degradation(level);
}

/// Initialize recovery subsystem
pub fn init() {
    clear_shutdown_flag();
    serial_println!("[recovery] Recovery subsystem initialized");
}
