use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Boot Optimization   (31.11)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Boot stage timing for parallel initialization
#[derive(Clone)]
pub struct BootStage {
    pub name: &'static str,
    pub start_tick: u64,
    pub end_tick: u64,
    pub parallel: bool,
}

static BOOT_STAGES: Mutex<Vec<BootStage>> = Mutex::new(Vec::new());

/// Record the start of a boot stage
pub fn boot_stage_start(name: &'static str, parallel: bool) -> usize {
    let mut stages = BOOT_STAGES.lock();
    let idx = stages.len();
    stages.push(BootStage {
        name,
        start_tick: crate::hpet::read_counter(),
        end_tick: 0,
        parallel,
    });
    idx
}

/// Record the end of a boot stage
pub fn boot_stage_end(idx: usize) {
    let mut stages = BOOT_STAGES.lock();
    if let Some(stage) = stages.get_mut(idx) {
        stage.end_tick = crate::hpet::read_counter();
    }
}

/// Get boot timing report
pub fn boot_timing_report() -> Vec<BootStage> {
    BOOT_STAGES.lock().clone()
}

/// Print boot timing summary to serial
pub fn print_boot_timing() {
    let stages = BOOT_STAGES.lock();
    serial_println!("[BOOT] === Boot Timing Report ===");
    for stage in stages.iter() {
        let duration = stage.end_tick.saturating_sub(stage.start_tick);
        let par = if stage.parallel { " (parallel)" } else { "" };
        serial_println!("[BOOT]   {}: {} ticks{}", stage.name, duration, par);
    }
}
