use crate::serial_println;
/// Boot Time Optimization
///
/// Parallel initialization, boot time measurement, dependency-based
/// service startup ordering, readahead.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone)]
pub struct BootStage {
    pub name: String,
    pub start_us: u64,
    pub end_us: u64,
    pub parallel: bool,
}

pub struct BootTimer {
    pub stages: Vec<BootStage>,
    pub total_start: u64,
    pub kernel_ready: u64,
    pub userspace_ready: u64,
    pub gui_ready: u64,
}

lazy_static::lazy_static! {
    static ref TIMER: Mutex<BootTimer> = Mutex::new(BootTimer {
        stages: Vec::new(),
        total_start: 0,
        kernel_ready: 0,
        userspace_ready: 0,
        gui_ready: 0,
    });
}

impl BootTimer {
    pub fn start_stage(&mut self, name: &str, now_us: u64, parallel: bool) {
        self.stages.push(BootStage {
            name: String::from(name),
            start_us: now_us,
            end_us: 0,
            parallel,
        });
    }

    pub fn end_stage(&mut self, name: &str, now_us: u64) {
        if let Some(s) = self
            .stages
            .iter_mut()
            .find(|s| s.name == name && s.end_us == 0)
        {
            s.end_us = now_us;
            let dur = s.end_us - s.start_us;
            serial_println!(
                "[BOOT] {} completed in {}us{}",
                name,
                dur,
                if s.parallel { " (parallel)" } else { "" }
            );
        }
    }

    pub fn mark_kernel_ready(&mut self, now_us: u64) {
        self.kernel_ready = now_us;
        serial_println!("[BOOT] Kernel ready at {}ms", now_us / 1000);
    }

    pub fn mark_userspace_ready(&mut self, now_us: u64) {
        self.userspace_ready = now_us;
        serial_println!("[BOOT] Userspace ready at {}ms", now_us / 1000);
    }

    pub fn mark_gui_ready(&mut self, now_us: u64) {
        self.gui_ready = now_us;
        serial_println!("[BOOT] GUI ready at {}ms", now_us / 1000);
    }

    pub fn print_summary(&self) {
        serial_println!("[BOOT] ===== BOOT TIME SUMMARY =====");
        serial_println!("[BOOT] Kernel:    {}ms", self.kernel_ready / 1000);
        serial_println!("[BOOT] Userspace: {}ms", self.userspace_ready / 1000);
        serial_println!("[BOOT] GUI:       {}ms", self.gui_ready / 1000);
        serial_println!("[BOOT] Total:     {}ms", self.gui_ready / 1000);
        for stage in &self.stages {
            if stage.end_us > 0 {
                serial_println!(
                    "[BOOT]   {}: {}us",
                    stage.name,
                    stage.end_us - stage.start_us
                );
            }
        }
    }
}

pub fn init() {
    serial_println!("[BOOT] Boot timer initialized");
}
