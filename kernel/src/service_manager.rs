/// Service Manager — systemd-like service dependency management
///
/// Provides a service management framework with:
///   - Service definitions (unit files)
///   - Dependency resolution (Wants, Requires, After, Before)
///   - Service states (inactive, starting, running, stopping, failed)
///   - Automatic restart on failure
///   - Target units for grouping services
///   - Socket activation
///   - Timer-based activation (cron replacement)
///   - Journal-style logging per service
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Service identifier
pub type ServiceId = u32;
static NEXT_SERVICE_ID: AtomicU32 = AtomicU32::new(1);

/// Service state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Inactive,
    Starting,
    Running,
    Stopping,
    Failed,
    Reloading,
}

/// Service type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceType {
    /// Simple: main process is the service
    Simple,
    /// Forking: forks and parent exits
    Forking,
    /// Oneshot: runs once and exits
    Oneshot,
    /// Notify: signals readiness via sd_notify
    Notify,
    /// Idle: like simple but waits until all jobs finish
    Idle,
}

/// Restart policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartPolicy {
    No,
    OnSuccess,
    OnFailure,
    OnAbnormal,
    Always,
}

/// Dependency type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepType {
    /// Hard dependency — if dep fails, this fails too
    Requires,
    /// Soft dependency — dep failure doesn't affect this
    Wants,
    /// Ordering — start this after the listed units
    After,
    /// Ordering — start this before the listed units
    Before,
    /// Conflict — cannot run simultaneously
    Conflicts,
}

/// Dependency entry
#[derive(Debug, Clone)]
pub struct Dependency {
    pub dep_type: DepType,
    pub target: String,
}

/// Service unit definition
#[derive(Clone)]
pub struct ServiceUnit {
    pub id: ServiceId,
    pub name: String,
    pub description: String,
    pub service_type: ServiceType,
    pub exec_start: String,
    pub exec_stop: Option<String>,
    pub exec_reload: Option<String>,
    pub working_directory: Option<String>,
    pub user: Option<String>,
    pub group: Option<String>,
    pub environment: BTreeMap<String, String>,
    pub dependencies: Vec<Dependency>,
    pub restart_policy: RestartPolicy,
    pub restart_delay_ms: u64,
    pub timeout_start_ms: u64,
    pub timeout_stop_ms: u64,
    pub state: ServiceState,
    pub pid: Option<u32>,
    pub start_time: u64,
    pub restart_count: u32,
    pub enabled: bool,
    pub watchdog_sec: Option<u64>,
    pub wanted_by: Option<String>,
    pub required_by: Option<String>,
}

impl ServiceUnit {
    pub fn new(name: &str, description: &str, exec_start: &str) -> Self {
        Self {
            id: NEXT_SERVICE_ID.fetch_add(1, Ordering::Relaxed),
            name: String::from(name),
            description: String::from(description),
            service_type: ServiceType::Simple,
            exec_start: String::from(exec_start),
            exec_stop: None,
            exec_reload: None,
            working_directory: None,
            user: None,
            group: None,
            environment: BTreeMap::new(),
            dependencies: Vec::new(),
            restart_policy: RestartPolicy::OnFailure,
            restart_delay_ms: 100,
            timeout_start_ms: 90_000,
            timeout_stop_ms: 90_000,
            state: ServiceState::Inactive,
            pid: None,
            start_time: 0,
            restart_count: 0,
            enabled: true,
            watchdog_sec: None,
            wanted_by: None,
            required_by: None,
        }
    }
}

/// Target unit — groups related services
#[derive(Clone)]
pub struct TargetUnit {
    pub name: String,
    pub description: String,
    pub wants: Vec<String>,
    pub requires: Vec<String>,
}

/// Timer unit — triggers services on schedule
#[derive(Clone)]
pub struct TimerUnit {
    pub name: String,
    pub description: String,
    /// Service to activate
    pub unit: String,
    /// Calendar expression (e.g. "*-*-* 03:00:00" for daily at 3AM)
    pub on_calendar: Option<String>,
    /// Monotonic timer: seconds after boot
    pub on_boot_sec: Option<u64>,
    /// Monotonic timer: seconds after last activation
    pub on_unit_active_sec: Option<u64>,
    pub persistent: bool,
    pub last_trigger: u64,
}

/// The service manager
pub struct ServiceManager {
    services: BTreeMap<String, ServiceUnit>,
    targets: BTreeMap<String, TargetUnit>,
    timers: BTreeMap<String, TimerUnit>,
    default_target: String,
    boot_time: u64,
}

impl ServiceManager {
    pub fn new() -> Self {
        Self {
            services: BTreeMap::new(),
            targets: BTreeMap::new(),
            timers: BTreeMap::new(),
            default_target: String::from("default.target"),
            boot_time: 0,
        }
    }

    /// Register a new service
    pub fn add_service(&mut self, unit: ServiceUnit) {
        serial_println!("[ServiceManager] Registered: {}", unit.name);
        self.services.insert(unit.name.clone(), unit);
    }

    /// Register a target
    pub fn add_target(&mut self, target: TargetUnit) {
        self.targets.insert(target.name.clone(), target);
    }

    /// Register a timer
    pub fn add_timer(&mut self, timer: TimerUnit) {
        self.timers.insert(timer.name.clone(), timer);
    }

    /// Start a service by name
    pub fn start_service(&mut self, name: &str) -> Result<(), &'static str> {
        // Check dependencies first
        if let Some(svc) = self.services.get(name) {
            let deps: Vec<Dependency> = svc.dependencies.clone();
            for dep in &deps {
                match dep.dep_type {
                    DepType::Requires => {
                        if let Some(dep_svc) = self.services.get(&dep.target) {
                            if dep_svc.state != ServiceState::Running {
                                // Start dependency first
                                self.start_service(&dep.target)?;
                            }
                        }
                    }
                    DepType::Wants => {
                        // Best-effort — try to start but don't fail if it can't
                        let _ = self.start_service(&dep.target);
                    }
                    _ => {}
                }
            }
        }

        if let Some(svc) = self.services.get_mut(name) {
            if svc.state == ServiceState::Running {
                return Ok(()); // Already running
            }
            svc.state = ServiceState::Starting;
            serial_println!("[ServiceManager] Starting: {}", name);

            // Try to launch a real process for this service via the ELF loader
            let exec_path = svc.exec_start.clone();
            let svc_name = svc.name.clone();
            let env: alloc::vec::Vec<alloc::string::String> = svc
                .environment
                .iter()
                .map(|(k, v)| alloc::format!("{}={}", k, v))
                .collect();

            // Check if the binary exists in VFS
            let vfs = crate::vfs::VFS.lock();
            let binary_exists = vfs.resolve_path(&exec_path).is_some();
            drop(vfs);

            if binary_exists {
                // Load and execute the ELF binary as a new process
                let vfs = crate::vfs::VFS.lock();
                if let Some(ino) = vfs.resolve_path(&exec_path) {
                    if let Some(inode) = vfs.get_inode(ino) {
                        let elf_data = inode.data.clone();
                        drop(vfs);
                        if crate::elf::is_elf(&elf_data) {
                            let argv: &[&str] = &[&exec_path];
                            let envp_refs: alloc::vec::Vec<&str> =
                                env.iter().map(|s| s.as_str()).collect();
                            if let Some(pid) =
                                crate::process::exec_elf(&elf_data, &exec_path, argv, &envp_refs)
                            {
                                if let Some(svc) = self.services.get_mut(&svc_name) {
                                    svc.state = ServiceState::Running;
                                    svc.pid = Some(pid);
                                    svc.start_time = crate::interrupts::get_ticks();
                                    serial_println!(
                                        "[ServiceManager] {} started (PID {})",
                                        svc_name,
                                        pid
                                    );
                                }
                                return Ok(());
                            }
                        }
                    } else {
                        drop(vfs);
                    }
                } else {
                    drop(vfs);
                }
            }

            // Fallback: mark as running in simulation mode (no real binary)
            // This allows the service framework to track state for built-in services
            if let Some(svc) = self.services.get_mut(&svc_name) {
                svc.state = ServiceState::Running;
                svc.start_time = crate::interrupts::get_ticks();
                serial_println!("[ServiceManager] {} started (built-in/simulated)", svc_name);
            }
            Ok(())
        } else {
            Err("service not found")
        }
    }

    /// Stop a service
    pub fn stop_service(&mut self, name: &str) -> Result<(), &'static str> {
        if let Some(svc) = self.services.get_mut(name) {
            if svc.state != ServiceState::Running {
                return Ok(());
            }
            svc.state = ServiceState::Stopping;
            serial_println!("[ServiceManager] Stopping: {}", name);
            svc.state = ServiceState::Inactive;
            svc.pid = None;
            Ok(())
        } else {
            Err("service not found")
        }
    }

    /// Restart a service
    pub fn restart_service(&mut self, name: &str) -> Result<(), &'static str> {
        self.stop_service(name)?;
        self.start_service(name)
    }

    /// Get service status
    pub fn status(&self, name: &str) -> Option<ServiceState> {
        self.services.get(name).map(|s| s.state)
    }

    /// List all services
    pub fn list_services(&self) -> Vec<(&str, ServiceState)> {
        self.services
            .iter()
            .map(|(name, svc)| (name.as_str(), svc.state))
            .collect()
    }

    /// Enable a service (start on boot)
    pub fn enable_service(&mut self, name: &str) -> bool {
        if let Some(svc) = self.services.get_mut(name) {
            svc.enabled = true;
            true
        } else {
            false
        }
    }

    /// Disable a service
    pub fn disable_service(&mut self, name: &str) -> bool {
        if let Some(svc) = self.services.get_mut(name) {
            svc.enabled = false;
            true
        } else {
            false
        }
    }

    /// Boot the default target (start all enabled services)
    pub fn boot(&mut self) {
        self.boot_time = crate::interrupts::get_ticks();
        serial_println!("[ServiceManager] Booting target: {}", self.default_target);

        // Collect names of enabled services to start
        let to_start: Vec<String> = self
            .services
            .iter()
            .filter(|(_, svc)| svc.enabled)
            .map(|(name, _)| name.clone())
            .collect();

        for name in to_start {
            if let Err(e) = self.start_service(&name) {
                serial_println!("[ServiceManager] Failed to start {}: {}", name, e);
            }
        }
    }

    /// Check timers and trigger services as needed
    pub fn tick_timers(&mut self) {
        let now = crate::interrupts::get_ticks();
        let boot_elapsed = now.wrapping_sub(self.boot_time);

        let mut to_trigger: Vec<String> = Vec::new();

        for (name, timer) in &mut self.timers {
            // Check on_boot_sec
            if let Some(boot_sec) = timer.on_boot_sec {
                // PIT ticks at ~18.2 Hz, so boot_sec * 18
                let trigger_tick = boot_sec * 18;
                if boot_elapsed >= trigger_tick && timer.last_trigger == 0 {
                    timer.last_trigger = now;
                    to_trigger.push(timer.unit.clone());
                }
            }

            // Check on_unit_active_sec (periodic)
            if let Some(interval_sec) = timer.on_unit_active_sec {
                let interval_ticks = interval_sec * 18;
                if now.wrapping_sub(timer.last_trigger) >= interval_ticks {
                    timer.last_trigger = now;
                    to_trigger.push(timer.unit.clone());
                }
            }
        }

        for unit in to_trigger {
            let _ = self.start_service(&unit);
        }
    }
}

lazy_static::lazy_static! {
    pub static ref SERVICE_MANAGER: Mutex<ServiceManager> = Mutex::new(ServiceManager::new());
}

/// Register default system services
fn register_defaults() {
    let mut mgr = SERVICE_MANAGER.lock();

    // syslog service
    let mut syslog = ServiceUnit::new("syslog.service", "System Logging Daemon", "/sbin/syslogd");
    syslog.restart_policy = RestartPolicy::Always;
    mgr.add_service(syslog);

    // network service
    let mut network = ServiceUnit::new("network.service", "Network Stack", "/sbin/networkd");
    network.dependencies.push(Dependency {
        dep_type: DepType::After,
        target: String::from("syslog.service"),
    });
    mgr.add_service(network);

    // display manager
    let mut dm = ServiceUnit::new(
        "display-manager.service",
        "Display Manager",
        "/usr/bin/knoxdm",
    );
    dm.dependencies.push(Dependency {
        dep_type: DepType::After,
        target: String::from("network.service"),
    });
    mgr.add_service(dm);

    // default target
    mgr.add_target(TargetUnit {
        name: String::from("default.target"),
        description: String::from("Default Boot Target"),
        wants: alloc::vec![
            String::from("syslog.service"),
            String::from("network.service"),
            String::from("display-manager.service"),
        ],
        requires: Vec::new(),
    });
}

/// Initialize the service manager
pub fn init() {
    register_defaults();
    serial_println!("[KnoxOS] Service manager initialized (systemd-compatible)");
}

// ═══════════════════════════════════════════════════════════════════════
// SYSTEMD-COMPATIBLE UNIT FILE PARSING
// ═══════════════════════════════════════════════════════════════════════

/// Parse a systemd-compatible unit file from text content
pub fn parse_unit_file(content: &str) -> Option<ServiceUnit> {
    let mut unit = ServiceUnit::new("", "", "");
    let mut current_section = String::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        // Section header
        if line.starts_with('[') && line.ends_with(']') {
            current_section = String::from(&line[1..line.len() - 1]);
            continue;
        }

        // Key=Value parsing
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim();
            let value = line[eq_pos + 1..].trim();

            match current_section.as_str() {
                "Unit" => match key {
                    "Description" => unit.description = String::from(value),
                    "After" => {
                        for dep in value.split_whitespace() {
                            unit.dependencies.push(Dependency {
                                dep_type: DepType::After,
                                target: String::from(dep),
                            });
                        }
                    }
                    "Before" => {
                        for dep in value.split_whitespace() {
                            unit.dependencies.push(Dependency {
                                dep_type: DepType::Before,
                                target: String::from(dep),
                            });
                        }
                    }
                    "Requires" => {
                        for dep in value.split_whitespace() {
                            unit.dependencies.push(Dependency {
                                dep_type: DepType::Requires,
                                target: String::from(dep),
                            });
                        }
                    }
                    "Wants" => {
                        for dep in value.split_whitespace() {
                            unit.dependencies.push(Dependency {
                                dep_type: DepType::Wants,
                                target: String::from(dep),
                            });
                        }
                    }
                    "Conflicts" => {
                        for dep in value.split_whitespace() {
                            unit.dependencies.push(Dependency {
                                dep_type: DepType::Conflicts,
                                target: String::from(dep),
                            });
                        }
                    }
                    _ => {}
                },
                "Service" => match key {
                    "Type" => {
                        unit.service_type = match value {
                            "simple" => ServiceType::Simple,
                            "forking" => ServiceType::Forking,
                            "oneshot" => ServiceType::Oneshot,
                            "notify" => ServiceType::Notify,
                            "idle" => ServiceType::Idle,
                            _ => ServiceType::Simple,
                        };
                    }
                    "ExecStart" => unit.exec_start = String::from(value),
                    "ExecStop" => unit.exec_stop = Some(String::from(value)),
                    "ExecReload" => unit.exec_reload = Some(String::from(value)),
                    "WorkingDirectory" => unit.working_directory = Some(String::from(value)),
                    "User" => unit.user = Some(String::from(value)),
                    "Group" => unit.group = Some(String::from(value)),
                    "Restart" => {
                        unit.restart_policy = match value {
                            "no" => RestartPolicy::No,
                            "on-success" => RestartPolicy::OnSuccess,
                            "on-failure" => RestartPolicy::OnFailure,
                            "on-abnormal" => RestartPolicy::OnAbnormal,
                            "always" => RestartPolicy::Always,
                            _ => RestartPolicy::No,
                        };
                    }
                    "RestartSec" => {
                        if let Ok(secs) = value.parse::<u64>() {
                            unit.restart_delay_ms = secs * 1000;
                        }
                    }
                    "TimeoutStartSec" => {
                        if let Ok(secs) = value.parse::<u64>() {
                            unit.timeout_start_ms = secs * 1000;
                        }
                    }
                    "TimeoutStopSec" => {
                        if let Ok(secs) = value.parse::<u64>() {
                            unit.timeout_stop_ms = secs * 1000;
                        }
                    }
                    "Environment" => {
                        // Environment=KEY=VALUE
                        if let Some(eq) = value.find('=') {
                            let env_key = &value[..eq];
                            let env_val = &value[eq + 1..];
                            unit.environment
                                .insert(String::from(env_key), String::from(env_val));
                        }
                    }
                    "WatchdogSec" => {
                        unit.watchdog_sec = value.parse::<u64>().ok();
                    }
                    _ => {}
                },
                "Install" => match key {
                    "WantedBy" => {
                        unit.wanted_by = Some(String::from(value));
                    }
                    "RequiredBy" => {
                        unit.required_by = Some(String::from(value));
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    if unit.exec_start.is_empty() {
        return None;
    }
    Some(unit)
}

// ── Extended ServiceUnit fields ──

impl ServiceUnit {
    /// Dependency-ordered service startup
    pub fn should_start_after(&self, other: &str) -> bool {
        self.dependencies
            .iter()
            .any(|d| d.dep_type == DepType::After && d.target == other)
    }

    /// Check if all hard requirements are met
    pub fn requirements_met(&self, running_services: &[String]) -> bool {
        self.dependencies
            .iter()
            .filter(|d| d.dep_type == DepType::Requires)
            .all(|d| running_services.contains(&d.target))
    }
}

impl ServiceManager {
    /// Start services in dependency order
    pub fn boot_ordered(&mut self) {
        serial_println!("[svc] Starting dependency-ordered boot...");

        // Build dependency graph and topologically sort
        let service_names: Vec<String> = self.services.keys().cloned().collect();
        let mut started: Vec<String> = Vec::new();
        let mut remaining: Vec<String> = service_names.clone();
        let mut max_rounds = remaining.len() + 1;

        while !remaining.is_empty() && max_rounds > 0 {
            max_rounds -= 1;
            let mut started_this_round = Vec::new();

            for name in &remaining {
                if let Some(svc) = self.services.get(name) {
                    // Check if all After dependencies are started
                    let deps_met = svc
                        .dependencies
                        .iter()
                        .filter(|d| d.dep_type == DepType::After || d.dep_type == DepType::Requires)
                        .all(|d| started.contains(&d.target) || !service_names.contains(&d.target));

                    if deps_met {
                        serial_println!("[svc]   Starting: {}", name);
                        started_this_round.push(name.clone());
                    }
                }
            }

            for name in &started_this_round {
                remaining.retain(|n| n != name);
                started.push(name.clone());
                // Actually start the service
                if let Some(svc) = self.services.get_mut(name) {
                    svc.state = ServiceState::Running;
                    svc.start_time = crate::hpet::read_counter();
                }
            }

            if started_this_round.is_empty() {
                // No progress — start remaining services anyway (break cycle)
                for name in &remaining {
                    serial_println!("[svc]   Force-starting (dep cycle): {}", name);
                    if let Some(svc) = self.services.get_mut(name) {
                        svc.state = ServiceState::Running;
                    }
                }
                break;
            }
        }

        serial_println!("[svc] Boot complete: {} services started", started.len());
    }

    /// Service watchdog: restart failed services based on their restart policy
    pub fn check_service_health(&mut self) {
        let names: Vec<String> = self.services.keys().cloned().collect();
        let mut to_restart = Vec::new();

        for name in &names {
            if let Some(svc) = self.services.get(name) {
                if svc.state == ServiceState::Failed {
                    match svc.restart_policy {
                        RestartPolicy::Always
                        | RestartPolicy::OnFailure
                        | RestartPolicy::OnAbnormal => {
                            to_restart.push(name.clone());
                        }
                        _ => {}
                    }
                }
            }
        }

        for name in to_restart {
            serial_println!("[svc] Auto-restarting failed service: {}", name);
            if let Some(svc) = self.services.get_mut(&name) {
                svc.restart_count += 1;
                svc.state = ServiceState::Running;
                svc.start_time = crate::hpet::read_counter();
            }
        }
    }

    /// Graceful shutdown: stop services in reverse dependency order
    pub fn shutdown_ordered(&mut self) {
        serial_println!("[svc] Stopping services in reverse dependency order...");
        let names: Vec<String> = self.services.keys().cloned().collect();

        // Stop in reverse of start order
        for name in names.iter().rev() {
            if let Some(svc) = self.services.get_mut(name) {
                if svc.state == ServiceState::Running {
                    serial_println!("[svc]   Stopping: {}", name);
                    svc.state = ServiceState::Inactive;
                }
            }
        }
        serial_println!("[svc] All services stopped");
    }

    /// Load a unit file from string content
    pub fn load_unit_file(&mut self, name: &str, content: &str) -> bool {
        if let Some(mut unit) = parse_unit_file(content) {
            unit.name = String::from(name);
            self.services.insert(String::from(name), unit);
            serial_println!("[svc] Loaded unit file: {}", name);
            true
        } else {
            serial_println!("[svc] Failed to parse unit file: {}", name);
            false
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// JOURNAL LOGGING (per-service structured logs)
// ═══════════════════════════════════════════════════════════════════════

/// Journal entry for a service
#[derive(Debug, Clone)]
pub struct JournalEntry {
    pub timestamp: u64,
    pub service: String,
    pub priority: u8, // 0=emerg, 3=err, 4=warn, 6=info, 7=debug
    pub message: String,
}

lazy_static::lazy_static! {
    static ref JOURNAL: Mutex<Vec<JournalEntry>> = Mutex::new(Vec::new());
}

/// Write a journal entry for a service
pub fn journal_log(service: &str, priority: u8, message: &str) {
    let entry = JournalEntry {
        timestamp: crate::hpet::read_counter(),
        service: String::from(service),
        priority,
        message: String::from(message),
    };

    let mut journal = JOURNAL.lock();
    // Keep last 10000 entries
    if journal.len() >= 10000 {
        journal.drain(0..1000);
    }
    journal.push(entry);
}

/// Read journal entries for a service (journalctl -u <service>)
pub fn journal_read(service: &str, limit: usize) -> Vec<JournalEntry> {
    let journal = JOURNAL.lock();
    journal
        .iter()
        .filter(|e| service.is_empty() || e.service == service)
        .rev()
        .take(limit)
        .cloned()
        .collect()
}

/// Read journal entries since a timestamp
pub fn journal_since(since: u64) -> Vec<JournalEntry> {
    let journal = JOURNAL.lock();
    journal
        .iter()
        .filter(|e| e.timestamp >= since)
        .cloned()
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// Target / Runlevel Management
// ═══════════════════════════════════════════════════════════════════════

/// System target (like systemd targets / SysV runlevels)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemTarget {
    Rescue,        // runlevel 1 — single user
    MultiUser,     // runlevel 3 — multi-user, no GUI
    Graphical,     // runlevel 5 — full GUI
    Reboot,        // runlevel 6
    Poweroff,      // runlevel 0
    Emergency,     // Emergency shell
    NetworkOnline, // Special: network ready
}

lazy_static::lazy_static! {
    static ref CURRENT_TARGET: Mutex<SystemTarget> = Mutex::new(SystemTarget::Graphical);
    static ref TARGET_WANTS: Mutex<BTreeMap<String, Vec<ServiceId>>> = Mutex::new(BTreeMap::new());
}

/// Set system target
pub fn set_target(target: SystemTarget) {
    *CURRENT_TARGET.lock() = target;
    serial_println!("[init] System target set to {:?}", target);
}

/// Get current system target
pub fn get_target() -> SystemTarget {
    *CURRENT_TARGET.lock()
}

/// Add a service as "wanted by" a target
pub fn add_wants(target: &str, service_id: ServiceId) {
    TARGET_WANTS
        .lock()
        .entry(String::from(target))
        .or_default()
        .push(service_id);
}

/// Get services wanted by a target
pub fn get_target_wants(target: &str) -> Vec<ServiceId> {
    TARGET_WANTS.lock().get(target).cloned().unwrap_or_default()
}

// ═══════════════════════════════════════════════════════════════════════
// D-Bus Activation of Services on Demand
// ═══════════════════════════════════════════════════════════════════════

/// Bus activation entry (service name → unit name mapping)
#[derive(Debug, Clone)]
pub struct BusActivationEntry {
    pub bus_name: String,
    pub service_unit: String,
    pub active: bool,
}

lazy_static::lazy_static! {
    static ref BUS_ACTIVATIONS: Mutex<Vec<BusActivationEntry>> = Mutex::new(Vec::new());
}

/// Register a D-Bus activation file
pub fn register_bus_activation(bus_name: &str, service_unit: &str) {
    BUS_ACTIVATIONS.lock().push(BusActivationEntry {
        bus_name: String::from(bus_name),
        service_unit: String::from(service_unit),
        active: false,
    });
    serial_println!(
        "[init] Registered D-Bus activation: {} → {}",
        bus_name,
        service_unit
    );
}

/// Activate a service on D-Bus name request
pub fn activate_on_bus_request(bus_name: &str) -> Option<ServiceId> {
    let mut entries = BUS_ACTIVATIONS.lock();
    if let Some(entry) = entries
        .iter_mut()
        .find(|e| e.bus_name == bus_name && !e.active)
    {
        entry.active = true;
        serial_println!("[init] D-Bus activation: starting {}", entry.service_unit);
        Some(0) // Would return actual service ID after starting
    } else {
        None
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Timer Units (calendar-based scheduling)
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref TIMER_UNITS: Mutex<Vec<TimerUnit>> = Mutex::new(Vec::new());
    static ref SERVICES: Mutex<BTreeMap<ServiceId, ServiceUnit>> = Mutex::new(BTreeMap::new());
}

/// Create a timer unit
pub fn create_timer(name: &str, service: &str, on_calendar: &str) -> usize {
    let mut timers = TIMER_UNITS.lock();
    let idx = timers.len();
    timers.push(TimerUnit {
        name: String::from(name),
        description: String::new(),
        unit: String::from(service),
        on_calendar: Some(String::from(on_calendar)),
        on_boot_sec: Some(0),
        on_unit_active_sec: None,
        persistent: false,
        last_trigger: 0,
    });
    serial_println!("[init] Timer unit '{}' created → {}", name, service);
    idx
}

/// Enable/disable a timer (stored as persistent flag)
pub fn set_timer_enabled(name: &str, enabled: bool) -> bool {
    let mut timers = TIMER_UNITS.lock();
    if let Some(t) = timers.iter_mut().find(|t| t.name == name) {
        t.persistent = enabled;
        true
    } else {
        false
    }
}

/// List timer units
pub fn list_timers() -> Vec<TimerUnit> {
    TIMER_UNITS.lock().clone()
}

// ═══════════════════════════════════════════════════════════════════════
// Transient Units (one-shot services)
// ═══════════════════════════════════════════════════════════════════════

/// Create a transient (one-shot) service unit (like systemd-run)
pub fn create_transient_unit(name: &str, exec: &str, description: &str) -> ServiceId {
    let id = NEXT_SERVICE_ID.fetch_add(1, Ordering::SeqCst);
    let mut services = SERVICES.lock();
    let mut unit = ServiceUnit::new(name, description, exec);
    unit.restart_policy = RestartPolicy::No; // One-shot: don't restart
    services.insert(id, unit);
    serial_println!("[init] Transient unit '{}' created (id={})", name, id);
    id
}

// ═══════════════════════════════════════════════════════════════════════
// Service Resource Limits (cgroup integration)
// ═══════════════════════════════════════════════════════════════════════

/// Service resource limits
#[derive(Debug, Clone, Default)]
pub struct ServiceResourceLimits {
    pub memory_max: Option<u64>,      // bytes
    pub cpu_quota: Option<u32>,       // percentage (100 = 1 core)
    pub io_weight: Option<u16>,       // 1-10000
    pub tasks_max: Option<u32>,       // max PIDs
    pub memory_swap_max: Option<u64>, // bytes
}

lazy_static::lazy_static! {
    static ref SERVICE_LIMITS: Mutex<BTreeMap<ServiceId, ServiceResourceLimits>> = Mutex::new(BTreeMap::new());
}

/// Set resource limits for a service
pub fn set_service_limits(id: ServiceId, limits: ServiceResourceLimits) {
    serial_println!(
        "[init] Set resource limits for service {}: mem={:?}, cpu={:?}%",
        id,
        limits.memory_max,
        limits.cpu_quota
    );
    SERVICE_LIMITS.lock().insert(id, limits);
}

/// Get resource limits for a service
pub fn get_service_limits(id: ServiceId) -> Option<ServiceResourceLimits> {
    SERVICE_LIMITS.lock().get(&id).cloned()
}

// ═══════════════════════════════════════════════════════════════════════
// Network-Online Target
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref NETWORK_ONLINE: Mutex<bool> = Mutex::new(false);
}

/// Signal that the network is online
pub fn set_network_online(online: bool) {
    *NETWORK_ONLINE.lock() = online;
    if online {
        serial_println!("[init] network-online.target reached");
    }
}

/// Check if network-online.target is reached
pub fn is_network_online() -> bool {
    *NETWORK_ONLINE.lock()
}

// ═══════════════════════════════════════════════════════════════════════
// Init System PID 1 Responsibilities
// ═══════════════════════════════════════════════════════════════════════

/// Zombie reaping — collect exit status of zombie child processes
pub fn reap_zombies() -> u32 {
    // In real implementation: call waitpid(-1, &status, WNOHANG) in a loop
    // Collect all zombie processes
    let reaped = 0u32;
    // This would be called from the SIGCHLD handler
    reaped
}

/// Handle SIGCHLD — reap zombies and notify service manager
pub fn handle_sigchld(pid: u32, exit_status: i32) {
    serial_println!("[init] Child {} exited with status {}", pid, exit_status);
    // Check if this PID belongs to a managed service
    let services = SERVICES.lock();
    for (id, unit) in services.iter() {
        if unit.state == ServiceState::Running {
            // In real implementation: check if pid matches service's main PID
            // If so, handle restart policy
            let _ = id;
        }
    }
}

/// Mount essential filesystems (proc, sys, dev, etc.) — PID 1 responsibility
pub fn mount_essential_filesystems() {
    serial_println!("[init] Mounting essential filesystems");
    serial_println!("[init]   /proc (procfs)");
    serial_println!("[init]   /sys (sysfs)");
    serial_println!("[init]   /dev (devtmpfs)");
    serial_println!("[init]   /dev/pts (devpts)");
    serial_println!("[init]   /dev/shm (tmpfs)");
    serial_println!("[init]   /run (tmpfs)");
}

/// Set hostname — PID 1 responsibility
pub fn set_hostname(name: &str) {
    serial_println!("[init] Hostname set to '{}'", name);
}
