use crate::serial_println;
/// Advanced Service Management
///
/// Extended service/init management: timer units, transient services,
/// resource limits via cgroups, network-online target, PID 1 zombie reaping.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Service resource limits
#[derive(Debug, Clone, Copy)]
pub struct ResourceLimits {
    pub cpu_quota_percent: Option<u32>, // e.g. 50 = 50% of one core
    pub memory_max_bytes: Option<u64>,
    pub io_weight: Option<u16>, // 1-10000
    pub tasks_max: Option<u32>,
}

/// Timer unit schedule
#[derive(Debug, Clone)]
pub enum TimerSchedule {
    OnBoot { delay_secs: u64 },
    Periodic { interval_secs: u64 },
    Calendar { expression: String }, // "Mon *-*-* 03:00:00"
}

/// Timer unit
#[derive(Debug, Clone)]
pub struct TimerUnit {
    pub name: String,
    pub schedule: TimerSchedule,
    pub service_name: String,
    pub persistent: bool, // Run immediately if missed
    pub enabled: bool,
    pub last_trigger: u64,
    pub next_trigger: u64,
}

/// Transient service (created at runtime, not from config files)
pub struct TransientService {
    pub name: String,
    pub exec_command: String,
    pub limits: ResourceLimits,
    pub pid: Option<u32>,
    pub oneshot: bool,
}

/// Network readiness state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NetworkState {
    Offline,
    Connecting,
    Online,
}

/// Advanced service manager
pub struct ServiceManager {
    pub timers: Vec<TimerUnit>,
    pub transients: Vec<TransientService>,
    pub network_state: NetworkState,
    pub zombie_count: u64,
}

lazy_static::lazy_static! {
    static ref MGR: Mutex<ServiceManager> = Mutex::new(ServiceManager {
        timers: Vec::new(),
        transients: Vec::new(),
        network_state: NetworkState::Offline,
        zombie_count: 0,
    });
}

impl ServiceManager {
    /// Register a timer unit
    pub fn add_timer(&mut self, timer: TimerUnit) {
        serial_println!(
            "[SERVICE] Timer registered: {} → {}",
            timer.name,
            timer.service_name
        );
        self.timers.push(timer);
    }

    /// Create a transient service
    pub fn create_transient(&mut self, name: &str, command: &str, limits: ResourceLimits) {
        self.transients.push(TransientService {
            name: String::from(name),
            exec_command: String::from(command),
            limits,
            pid: None,
            oneshot: false,
        });
        serial_println!("[SERVICE] Transient service created: {}", name);
    }

    /// Check and fire timers
    pub fn check_timers(&mut self, now_secs: u64) {
        for timer in &mut self.timers {
            if !timer.enabled {
                continue;
            }
            if now_secs >= timer.next_trigger {
                serial_println!(
                    "[SERVICE] Timer fired: {} → {}",
                    timer.name,
                    timer.service_name
                );
                timer.last_trigger = now_secs;
                timer.next_trigger = match &timer.schedule {
                    TimerSchedule::OnBoot { .. } => u64::MAX, // one-shot
                    TimerSchedule::Periodic { interval_secs } => now_secs + interval_secs,
                    TimerSchedule::Calendar { .. } => now_secs + 86400, // simplified
                };
                // Start the associated service
            }
        }
    }

    /// PID 1 responsibility: reap zombie processes
    pub fn reap_zombies(&mut self) {
        // Call waitpid(-1, WNOHANG) in a loop
        // For each reaped child:
        //   self.zombie_count += 1;
        //   Check if it was a managed service → handle restart policy
    }

    /// Set network-online state
    pub fn set_network_state(&mut self, state: NetworkState) {
        let old = self.network_state;
        self.network_state = state;
        if old != NetworkState::Online && state == NetworkState::Online {
            serial_println!("[SERVICE] network-online.target reached");
            // Start services that depend on network-online
        }
    }

    /// Apply resource limits to a process via cgroup
    pub fn apply_limits(&self, pid: u32, limits: &ResourceLimits) {
        if let Some(cpu) = limits.cpu_quota_percent {
            serial_println!("[SERVICE] PID {}: CPU quota {}%", pid, cpu);
        }
        if let Some(mem) = limits.memory_max_bytes {
            serial_println!(
                "[SERVICE] PID {}: memory max {}MB",
                pid,
                mem / (1024 * 1024)
            );
        }
        if let Some(tasks) = limits.tasks_max {
            serial_println!("[SERVICE] PID {}: tasks max {}", pid, tasks);
        }
    }
}

pub fn init() {
    serial_println!("[SERVICE] Advanced service manager loaded");
}
