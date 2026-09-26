use alloc::string::String;

use crate::serial_println;

use super::hw::pm_timer_read;
use super::manager::PM;
use super::types::WakeupSource;

// ── Wakeup source management ────────────────────────────────────────────

/// Register a wakeup source
pub fn register_wakeup_source(name: &str) {
    let mut pm = PM.lock();
    pm.wakeup_sources.push(WakeupSource {
        name: String::from(name),
        enabled: true,
        active_count: 0,
        wakeup_count: 0,
        last_time_us: 0,
        total_time_us: 0,
    });
}

/// Enable/disable a wakeup source
pub fn set_wakeup_source_enabled(name: &str, enabled: bool) -> Result<(), i32> {
    let mut pm = PM.lock();
    for ws in pm.wakeup_sources.iter_mut() {
        if ws.name == name {
            ws.enabled = enabled;
            serial_println!(
                "[PM] Wakeup source '{}' {}",
                name,
                if enabled { "enabled" } else { "disabled" }
            );
            return Ok(());
        }
    }
    Err(-2) // ENOENT
}

/// Record a wakeup event from a source
pub fn wakeup_event(name: &str) {
    let mut pm = PM.lock();
    for ws in pm.wakeup_sources.iter_mut() {
        if ws.name == name {
            ws.active_count += 1;
            ws.wakeup_count += 1;
            ws.last_time_us = pm_timer_read() as u64;
            return;
        }
    }
}
