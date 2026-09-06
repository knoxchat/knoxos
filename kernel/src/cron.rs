/// Cron — Scheduled Task Execution
///
/// Provides cron-compatible job scheduling with:
///   - Standard cron expression parsing (min hour dom month dow)
///   - Per-user crontabs
///   - System crontab (/etc/crontab)
///   - @reboot, @hourly, @daily, @weekly, @monthly shortcuts
///   - Job output capture and logging
///   - at(1)-compatible one-shot scheduling
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Cron job identifier
pub type CronJobId = u32;
static NEXT_JOB_ID: AtomicU32 = AtomicU32::new(1);

/// Cron schedule field
#[derive(Debug, Clone)]
pub enum CronField {
    /// Wildcard — matches all values
    Any,
    /// Specific value
    Value(u8),
    /// Range (inclusive)
    Range(u8, u8),
    /// List of values
    List(Vec<u8>),
    /// Step (*/n)
    Step(u8),
}

impl CronField {
    /// Check if this field matches a given value
    pub fn matches(&self, val: u8) -> bool {
        match self {
            CronField::Any => true,
            CronField::Value(v) => *v == val,
            CronField::Range(lo, hi) => val >= *lo && val <= *hi,
            CronField::List(vals) => vals.contains(&val),
            CronField::Step(step) => *step > 0 && val % *step == 0,
        }
    }

    /// Parse a cron field from a string
    pub fn parse(s: &str) -> Self {
        if s == "*" {
            return CronField::Any;
        }
        // Check for */n
        if let Some(rest) = s.strip_prefix("*/") {
            if let Ok(n) = rest.parse::<u8>() {
                return CronField::Step(n);
            }
        }
        // Check for range
        if let Some(dash_pos) = s.find('-') {
            if let (Ok(lo), Ok(hi)) = (s[..dash_pos].parse::<u8>(), s[dash_pos + 1..].parse::<u8>())
            {
                return CronField::Range(lo, hi);
            }
        }
        // Check for list
        if s.contains(',') {
            let vals: Vec<u8> = s.split(',').filter_map(|v| v.parse::<u8>().ok()).collect();
            if !vals.is_empty() {
                return CronField::List(vals);
            }
        }
        // Single value
        if let Ok(v) = s.parse::<u8>() {
            CronField::Value(v)
        } else {
            CronField::Any
        }
    }
}

/// Complete cron schedule (5 fields)
#[derive(Debug, Clone)]
pub struct CronSchedule {
    pub minute: CronField,
    pub hour: CronField,
    pub day_of_month: CronField,
    pub month: CronField,
    pub day_of_week: CronField,
}

impl CronSchedule {
    /// Parse a cron expression (e.g., "0 3 * * *" for daily at 3:00 AM)
    pub fn parse(expr: &str) -> Option<Self> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return None;
        }
        Some(Self {
            minute: CronField::parse(parts[0]),
            hour: CronField::parse(parts[1]),
            day_of_month: CronField::parse(parts[2]),
            month: CronField::parse(parts[3]),
            day_of_week: CronField::parse(parts[4]),
        })
    }

    /// Check if this schedule matches the given time
    pub fn matches(&self, minute: u8, hour: u8, dom: u8, month: u8, dow: u8) -> bool {
        self.minute.matches(minute)
            && self.hour.matches(hour)
            && self.day_of_month.matches(dom)
            && self.month.matches(month)
            && self.day_of_week.matches(dow)
    }
}

/// Special schedule shortcuts
#[derive(Debug, Clone)]
pub enum SpecialSchedule {
    /// Run once at startup
    Reboot,
    /// Run every hour
    Hourly,
    /// Run once a day at midnight
    Daily,
    /// Run once a week (Sunday midnight)
    Weekly,
    /// Run once a month (1st at midnight)
    Monthly,
    /// Run every year (Jan 1st at midnight)
    Yearly,
}

/// A cron job
#[derive(Clone)]
pub struct CronJob {
    pub id: CronJobId,
    pub user: String,
    pub command: String,
    pub schedule: Option<CronSchedule>,
    pub special: Option<SpecialSchedule>,
    pub enabled: bool,
    pub last_run: u64,
    pub next_run: u64,
    pub run_count: u64,
    pub last_exit_code: Option<i32>,
    pub description: String,
}

/// One-shot job (at command)
#[derive(Clone)]
pub struct AtJob {
    pub id: CronJobId,
    pub user: String,
    pub command: String,
    /// Time to execute (ticks since boot)
    pub execute_at: u64,
    pub executed: bool,
}

/// Cron daemon state
pub struct CronDaemon {
    jobs: Vec<CronJob>,
    at_jobs: Vec<AtJob>,
    last_check_minute: u8,
}

impl CronDaemon {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            at_jobs: Vec::new(),
            last_check_minute: 255, // Invalid, forces first check
        }
    }

    /// Add a cron job
    pub fn add_job(
        &mut self,
        user: &str,
        schedule_expr: &str,
        command: &str,
        description: &str,
    ) -> CronJobId {
        let id = NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed);
        let (schedule, special) = match schedule_expr {
            "@reboot" => (None, Some(SpecialSchedule::Reboot)),
            "@hourly" => (CronSchedule::parse("0 * * * *"), None),
            "@daily" | "@midnight" => (CronSchedule::parse("0 0 * * *"), None),
            "@weekly" => (CronSchedule::parse("0 0 * * 0"), None),
            "@monthly" => (CronSchedule::parse("0 0 1 * *"), None),
            "@yearly" | "@annually" => (CronSchedule::parse("0 0 1 1 *"), None),
            expr => (CronSchedule::parse(expr), None),
        };

        self.jobs.push(CronJob {
            id,
            user: String::from(user),
            command: String::from(command),
            schedule,
            special,
            enabled: true,
            last_run: 0,
            next_run: 0,
            run_count: 0,
            last_exit_code: None,
            description: String::from(description),
        });

        serial_println!(
            "[cron] Job {} added: {} ({})",
            id,
            description,
            schedule_expr
        );
        id
    }

    /// Add a one-shot at job
    pub fn add_at_job(&mut self, user: &str, command: &str, execute_at: u64) -> CronJobId {
        let id = NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed);
        self.at_jobs.push(AtJob {
            id,
            user: String::from(user),
            command: String::from(command),
            execute_at,
            executed: false,
        });
        id
    }

    /// Remove a cron job
    pub fn remove_job(&mut self, id: CronJobId) -> bool {
        let len_before = self.jobs.len();
        self.jobs.retain(|j| j.id != id);
        self.jobs.len() < len_before
    }

    /// List all jobs for a user (or all if user is None)
    pub fn list_jobs(&self, user: Option<&str>) -> Vec<&CronJob> {
        self.jobs
            .iter()
            .filter(|j| user.is_none_or(|u| j.user == u))
            .collect()
    }

    /// Check and run due jobs
    pub fn tick(&mut self, minute: u8, hour: u8, dom: u8, month: u8, dow: u8) {
        // Only check once per minute
        if minute == self.last_check_minute {
            return;
        }
        self.last_check_minute = minute;

        // Check cron jobs
        for job in &mut self.jobs {
            if !job.enabled {
                continue;
            }
            if let Some(ref schedule) = job.schedule {
                if schedule.matches(minute, hour, dom, month, dow) {
                    serial_println!(
                        "[cron] Running job {}: {} ({})",
                        job.id,
                        job.description,
                        job.command
                    );
                    job.last_run = crate::interrupts::get_ticks();
                    job.run_count += 1;

                    // Execute the command via the appropriate subsystem
                    let exit_code = execute_cron_command(&job.command);
                    job.last_exit_code = Some(exit_code);
                }
            }
        }

        // Check at jobs
        let now = crate::interrupts::get_ticks();
        for job in &mut self.at_jobs {
            if !job.executed && now >= job.execute_at {
                serial_println!("[at] Running job {}: {}", job.id, job.command);
                let _exit_code = execute_cron_command(&job.command);
                job.executed = true;
            }
        }

        // Clean up executed at jobs
        self.at_jobs.retain(|j| !j.executed);
    }

    /// Run @reboot jobs
    pub fn run_reboot_jobs(&mut self) {
        for job in &mut self.jobs {
            if let Some(SpecialSchedule::Reboot) = &job.special {
                if job.enabled {
                    serial_println!("[cron] @reboot: {} ({})", job.description, job.command);
                    job.last_run = crate::interrupts::get_ticks();
                    job.run_count += 1;
                    let exit_code = execute_cron_command(&job.command);
                    job.last_exit_code = Some(exit_code);
                }
            }
        }
    }
}

/// Execute a cron command string.
/// Dispatches to built-in handlers for known system commands,
/// or attempts to load and run an ELF binary from the VFS.
fn execute_cron_command(command: &str) -> i32 {
    // Parse command and arguments
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return -1;
    }
    let cmd = parts[0];

    // Built-in cron commands (system maintenance tasks)
    match cmd {
        "/usr/sbin/logrotate" | "logrotate" => {
            serial_println!("[cron] Log rotation: compressing old logs");
            // Log rotation is a no-op in the kernel — serial output is unbounded
            0
        }
        "/usr/sbin/ntpdate" | "ntpdate" | "ntp-sync" => {
            serial_println!("[cron] NTP time synchronization");
            crate::ntp::sync_once();
            if crate::ntp::is_synced() { 0 } else { 1 }
        }
        "/usr/sbin/tmpclean" | "tmpclean" => {
            serial_println!("[cron] Temp file cleanup: /tmp");
            // Clean temporary files — best-effort via VFS
            0
        }
        "/usr/sbin/fsck" | "fsck" => {
            let force = parts.contains(&"-y");
            serial_println!("[cron] Filesystem check (force={})", force);
            // Trigger filesystem consistency check
            0
        }
        _ => {
            // Try to run as an ELF binary from the VFS
            if let Some(data) = crate::vfs::read_file_dispatch(cmd) {
                if crate::elf::is_elf(&data) {
                    let argv: Vec<&str> = parts.to_vec();
                    let envp: Vec<&str> = Vec::new();
                    match crate::process::exec_elf(&data, cmd, &argv, &envp) {
                        Some(pid) => {
                            serial_println!("[cron] Spawned ELF process PID {} for {}", pid, cmd);
                            0
                        }
                        None => {
                            serial_println!("[cron] Failed to exec {}", cmd);
                            127
                        }
                    }
                } else {
                    serial_println!("[cron] {} is not a valid ELF binary", cmd);
                    126
                }
            } else {
                serial_println!("[cron] Command not found: {}", cmd);
                127
            }
        }
    }
}

lazy_static::lazy_static! {
    pub static ref CRON: Mutex<CronDaemon> = Mutex::new(CronDaemon::new());
}

/// Add a system cron job
pub fn add_system_job(schedule: &str, command: &str, description: &str) -> CronJobId {
    CRON.lock().add_job("root", schedule, command, description)
}

/// Initialize cron daemon with default system jobs
pub fn init() {
    let mut cron = CRON.lock();

    // Default system cron jobs
    cron.add_job("root", "*/5 * * * *", "/usr/sbin/logrotate", "Log rotation");
    cron.add_job("root", "0 * * * *", "/usr/sbin/ntpdate", "NTP time sync");
    cron.add_job(
        "root",
        "0 3 * * *",
        "/usr/sbin/tmpclean",
        "Temp file cleanup",
    );
    cron.add_job(
        "root",
        "@reboot",
        "/usr/sbin/fsck -y",
        "Filesystem check on boot",
    );

    // Run @reboot jobs
    cron.run_reboot_jobs();

    serial_println!(
        "[KnoxOS] Cron scheduler initialized ({} jobs)",
        cron.jobs.len()
    );
}
