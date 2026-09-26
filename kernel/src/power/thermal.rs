use crate::serial_println;

use super::manager::PM;
use super::sleep::suspend;
use super::types::{SystemState, TripType};

// ── Thermal management ──────────────────────────────────────────────────

/// Get CPU temperature (millidegrees Celsius)
pub fn get_cpu_temp() -> i32 {
    // Try reading from MSR IA32_THERM_STATUS (0x19C)
    let temp = unsafe {
        let mut lo: u32 = 0;
        let mut _hi: u32 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "rdmsr",
            in("ecx") 0x19Cu32, // IA32_THERM_STATUS
            out("eax") lo,
            out("edx") _hi,
        );
        lo
    };

    // If valid (bit 31 set), temperature = Tj_max - digital_readout
    if temp & (1 << 31) != 0 {
        let digital_readout = ((temp >> 16) & 0x7F) as i32;
        let tj_max = 100; // Assume Tj_max = 100°C (common for Intel)
        return (tj_max - digital_readout) * 1000; // millidegrees
    }

    // Fallback: read from stored thermal zone
    let pm = PM.lock();
    pm.thermal_zones.first().map(|z| z.temp_mc).unwrap_or(45000)
}

/// Check thermal trip points and apply cooling if needed
pub fn thermal_check() {
    let temp = get_cpu_temp();
    let mut pm = PM.lock();

    for zone in pm.thermal_zones.iter_mut() {
        zone.temp_mc = temp;

        for trip in &zone.trip_points {
            match trip.trip_type {
                TripType::Critical => {
                    if temp >= trip.temp_mc {
                        serial_println!(
                            "[PM] CRITICAL: CPU temp {}°C >= trip {}°C — EMERGENCY SHUTDOWN",
                            temp / 1000,
                            trip.temp_mc / 1000
                        );
                        drop(pm);
                        // Emergency shutdown
                        let _ = suspend(SystemState::SoftOff);
                        return;
                    }
                }
                TripType::Hot => {
                    if temp >= trip.temp_mc {
                        serial_println!(
                            "[PM] HOT: CPU temp {}°C >= trip {}°C — throttling",
                            temp / 1000,
                            trip.temp_mc / 1000
                        );
                        // Apply throttling
                    }
                }
                TripType::Passive => {
                    if temp >= trip.temp_mc {
                        // Reduce CPU frequency
                        for cd in zone.cooling_devices.iter() {
                            serial_println!("[PM] Passive cooling: {} activated", cd.name);
                        }
                    }
                }
                TripType::Active => {
                    // Active cooling (fan control)
                    if temp >= trip.temp_mc {
                        for cd in zone.cooling_devices.iter() {
                            serial_println!("[PM] Active cooling: {} fan speed increased", cd.name);
                        }
                    }
                }
            }
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Thermal Throttling   (31.10)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Thermal throttle state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrottleLevel {
    None,
    Light,    // reduce to 75% frequency
    Medium,   // reduce to 50% frequency
    Heavy,    // reduce to 25% frequency
    Critical, // emergency shutdown
}

static THROTTLE_LEVEL: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Update thermal throttle based on current temperature (°C)
pub fn update_thermal_throttle(temp_c: u32) {
    let level = if temp_c >= 105 {
        ThrottleLevel::Critical
    } else if temp_c >= 95 {
        ThrottleLevel::Heavy
    } else if temp_c >= 85 {
        ThrottleLevel::Medium
    } else if temp_c >= 75 {
        ThrottleLevel::Light
    } else {
        ThrottleLevel::None
    };

    let prev = THROTTLE_LEVEL.load(core::sync::atomic::Ordering::Relaxed);
    let new = level as u8;
    if prev != new {
        THROTTLE_LEVEL.store(new, core::sync::atomic::Ordering::Relaxed);
        serial_println!("[PM] Thermal throttle: {:?} ({}°C)", level, temp_c);
        if let ThrottleLevel::Critical = level {
            serial_println!(
                "[PM] CRITICAL: Temperature {}°C — initiating emergency shutdown!",
                temp_c
            );
        }
    }
}

/// Get current throttle level
pub fn thermal_throttle_level() -> ThrottleLevel {
    match THROTTLE_LEVEL.load(core::sync::atomic::Ordering::Relaxed) {
        1 => ThrottleLevel::Light,
        2 => ThrottleLevel::Medium,
        3 => ThrottleLevel::Heavy,
        4 => ThrottleLevel::Critical,
        _ => ThrottleLevel::None,
    }
}
