use crate::serial_println;
/// Health Check Daemon
///
/// Monitor critical services, restart failed services, check disk/memory/CPU
/// thresholds, send alerts.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct HealthCheck {
    pub name: String,
    pub status: HealthStatus,
    pub message: String,
    pub last_check: u64,
    pub check_interval_secs: u64,
}

pub struct HealthDaemon {
    pub checks: Vec<HealthCheck>,
    pub overall: HealthStatus,
    pub disk_warn_percent: u8,
    pub mem_warn_percent: u8,
    pub cpu_warn_percent: u8,
}

lazy_static::lazy_static! {
    static ref HEALTH: Mutex<HealthDaemon> = Mutex::new(HealthDaemon {
        checks: Vec::new(),
        overall: HealthStatus::Unknown,
        disk_warn_percent: 90,
        mem_warn_percent: 90,
        cpu_warn_percent: 95,
    });
}

impl HealthDaemon {
    pub fn add_check(&mut self, name: &str, interval: u64) {
        self.checks.push(HealthCheck {
            name: String::from(name),
            status: HealthStatus::Unknown,
            message: String::new(),
            last_check: 0,
            check_interval_secs: interval,
        });
    }

    pub fn update_check(&mut self, name: &str, status: HealthStatus, msg: &str) {
        if let Some(c) = self.checks.iter_mut().find(|c| c.name == name) {
            c.status = status;
            c.message = String::from(msg);
            if status == HealthStatus::Critical {
                serial_println!("[HEALTH] CRITICAL: {} — {}", name, msg);
            }
        }
        self.recompute_overall();
    }

    fn recompute_overall(&mut self) {
        self.overall = HealthStatus::Healthy;
        for c in &self.checks {
            match c.status {
                HealthStatus::Critical => {
                    self.overall = HealthStatus::Critical;
                    return;
                }
                HealthStatus::Warning if self.overall == HealthStatus::Healthy => {
                    self.overall = HealthStatus::Warning;
                }
                _ => {}
            }
        }
    }

    pub fn run_checks(&mut self, now: u64) {
        for check in &mut self.checks {
            if now - check.last_check >= check.check_interval_secs {
                check.last_check = now;
                // Would run actual check logic
            }
        }
    }

    pub fn overall_status(&self) -> HealthStatus {
        self.overall
    }
}

pub fn init() {
    let mut h = HEALTH.lock();
    h.add_check("disk_usage", 60);
    h.add_check("memory_usage", 30);
    h.add_check("critical_services", 10);
    serial_println!(
        "[HEALTH] Health check daemon initialized ({} checks)",
        h.checks.len()
    );
}
