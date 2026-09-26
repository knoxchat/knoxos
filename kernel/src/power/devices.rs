use alloc::string::String;

use crate::serial_println;

use super::manager::PM;
use super::types::{DevicePmOps, RuntimePmState};

// ── Device suspend/resume ───────────────────────────────────────────────

/// Register a device for power management
pub fn register_device(name: &str) {
    let mut pm = PM.lock();
    pm.devices.push(DevicePmOps {
        name: String::from(name),
        state: RuntimePmState::Active,
        suspend_ok: false,
        resume_ok: false,
        autosuspend_delay_ms: 2000,
        autosuspend_enabled: false,
        usage_count: 0,
    });
}

/// Suspend all registered devices (called before entering sleep)
pub(super) fn suspend_devices() -> Result<(), &'static str> {
    let mut pm = PM.lock();
    serial_println!("[PM] Suspending {} devices...", pm.devices.len());
    for dev in pm.devices.iter_mut() {
        if dev.state == RuntimePmState::Active {
            serial_println!("[PM]   Suspending device: {}", dev.name);
            dev.state = RuntimePmState::Suspending;
            // In a real system: call device-specific suspend callback
            // e.g., virtio_blk_suspend(), hda_suspend(), e1000_suspend()
            dev.state = RuntimePmState::Suspended;
            dev.suspend_ok = true;
        }
    }
    serial_println!("[PM] All devices suspended");
    Ok(())
}

/// Resume all registered devices (called after waking from sleep)
pub(super) fn resume_devices() {
    let mut pm = PM.lock();
    serial_println!("[PM] Resuming {} devices...", pm.devices.len());
    for dev in pm.devices.iter_mut().rev() {
        if dev.state == RuntimePmState::Suspended {
            serial_println!("[PM]   Resuming device: {}", dev.name);
            dev.state = RuntimePmState::Resuming;
            // In a real system: call device-specific resume callback
            dev.state = RuntimePmState::Active;
            dev.resume_ok = true;
        }
    }
    serial_println!("[PM] All devices resumed");
}

/// Runtime suspend a single device
pub fn runtime_suspend_device(name: &str) -> Result<(), i32> {
    let mut pm = PM.lock();
    for dev in pm.devices.iter_mut() {
        if dev.name == name {
            if dev.usage_count > 0 {
                return Err(-16); // EBUSY
            }
            dev.state = RuntimePmState::Suspended;
            serial_println!("[PM] Runtime suspended: {}", name);
            return Ok(());
        }
    }
    Err(-19) // ENODEV
}

/// Runtime resume a single device
pub fn runtime_resume_device(name: &str) -> Result<(), i32> {
    let mut pm = PM.lock();
    for dev in pm.devices.iter_mut() {
        if dev.name == name {
            dev.state = RuntimePmState::Active;
            serial_println!("[PM] Runtime resumed: {}", name);
            return Ok(());
        }
    }
    Err(-19) // ENODEV
}
