/// Battery Monitor — ACPI battery status reading
///
/// Provides:
///   - Battery state (charging/discharging/full/not present)
///   - Charge percentage
///   - Time remaining estimation
///   - AC adapter status
///   - Battery health/cycle count
///   - Low battery warnings
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU16, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// BATTERY STATE
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryState {
    NotPresent,
    Charging,
    Discharging,
    Full,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcAdapterState {
    Online,
    Offline,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct BatteryInfo {
    pub state: BatteryState,
    pub charge_percent: u8,
    pub design_capacity_mwh: u32,
    pub full_charge_capacity_mwh: u32,
    pub remaining_capacity_mwh: u32,
    pub present_rate_mw: u32, // Current power draw
    pub voltage_mv: u32,
    pub temperature_c: i8,
    pub cycle_count: u32,
    pub time_remaining_minutes: Option<u32>,
    pub ac_adapter: AcAdapterState,
}

impl BatteryInfo {
    pub fn not_present() -> Self {
        Self {
            state: BatteryState::NotPresent,
            charge_percent: 0,
            design_capacity_mwh: 0,
            full_charge_capacity_mwh: 0,
            remaining_capacity_mwh: 0,
            present_rate_mw: 0,
            voltage_mv: 0,
            temperature_c: 0,
            cycle_count: 0,
            time_remaining_minutes: None,
            ac_adapter: AcAdapterState::Unknown,
        }
    }

    /// Calculate health percentage (full_charge / design * 100)
    pub fn health_percent(&self) -> u8 {
        if self.design_capacity_mwh == 0 {
            return 100;
        }
        ((self.full_charge_capacity_mwh as u64 * 100) / self.design_capacity_mwh as u64).min(100)
            as u8
    }

    /// Estimate time remaining
    pub fn estimate_time_remaining(&mut self) {
        if self.present_rate_mw == 0 {
            self.time_remaining_minutes = None;
            return;
        }

        match self.state {
            BatteryState::Discharging => {
                let minutes =
                    (self.remaining_capacity_mwh as u64 * 60) / self.present_rate_mw as u64;
                self.time_remaining_minutes = Some(minutes as u32);
            }
            BatteryState::Charging => {
                let remaining = self
                    .full_charge_capacity_mwh
                    .saturating_sub(self.remaining_capacity_mwh);
                let minutes = (remaining as u64 * 60) / self.present_rate_mw as u64;
                self.time_remaining_minutes = Some(minutes as u32);
            }
            _ => {
                self.time_remaining_minutes = None;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BATTERY MONITOR
// ═══════════════════════════════════════════════════════════════════════

/// Low battery threshold (%)
static LOW_BATTERY_THRESHOLD: AtomicU8 = AtomicU8::new(15);
/// Critical battery threshold (%)
static CRITICAL_BATTERY_THRESHOLD: AtomicU8 = AtomicU8::new(5);
/// Whether low battery warning has been shown
static LOW_WARNING_SHOWN: AtomicBool = AtomicBool::new(false);
/// Whether critical warning has been shown
static CRITICAL_WARNING_SHOWN: AtomicBool = AtomicBool::new(false);

lazy_static::lazy_static! {
    pub static ref BATTERY_INFO: Mutex<BatteryInfo> = Mutex::new(BatteryInfo::not_present());
}

/// ACPI battery status port (simplified)
const ACPI_BATTERY_STATUS_PORT: u16 = 0xB0;
const ACPI_AC_ADAPTER_PORT: u16 = 0xB1;

/// Read battery status from ACPI (QEMU simulation)
pub fn poll_battery() {
    let mut info = BATTERY_INFO.lock();

    // In QEMU, there's typically no real battery
    // On real hardware, this would read from ACPI _BST and _BIF methods
    // For now, simulate a reasonable battery state

    // Try reading from ACPI embedded controller
    let ac_online = unsafe {
        let val =
            crate::arch_compat::instructions::port::Port::<u8>::new(ACPI_AC_ADAPTER_PORT).read();
        val & 0x01 != 0
    };

    info.ac_adapter = if ac_online {
        AcAdapterState::Online
    } else {
        AcAdapterState::Offline
    };

    // In QEMU, report as "AC powered, no battery"
    // On real hardware, parse _BST (Battery Status) ACPI method
    info.state = if info.ac_adapter == AcAdapterState::Online {
        BatteryState::Full
    } else {
        BatteryState::Discharging
    };

    // Default values for simulation
    info.design_capacity_mwh = 50_000; // 50 Wh
    info.full_charge_capacity_mwh = 48_000; // 96% health
    info.remaining_capacity_mwh = 48_000;
    info.charge_percent = 100;
    info.voltage_mv = 12_600;
    info.temperature_c = 35;
    info.present_rate_mw = 0;
    info.cycle_count = 0;

    info.estimate_time_remaining();
    drop(info);

    check_battery_warnings();
}

/// Check if we need to show battery warnings
fn check_battery_warnings() {
    let info = BATTERY_INFO.lock();
    let percent = info.charge_percent;
    let is_discharging = info.state == BatteryState::Discharging;
    drop(info);

    if !is_discharging {
        LOW_WARNING_SHOWN.store(false, Ordering::Relaxed);
        CRITICAL_WARNING_SHOWN.store(false, Ordering::Relaxed);
        return;
    }

    let critical = CRITICAL_BATTERY_THRESHOLD.load(Ordering::Relaxed);
    let low = LOW_BATTERY_THRESHOLD.load(Ordering::Relaxed);

    if percent <= critical && !CRITICAL_WARNING_SHOWN.load(Ordering::Relaxed) {
        CRITICAL_WARNING_SHOWN.store(true, Ordering::Relaxed);
        serial_println!(
            "[Battery] CRITICAL: {}% - system will shut down soon!",
            percent
        );
        // Could trigger auto-hibernate here
    } else if percent <= low && !LOW_WARNING_SHOWN.load(Ordering::Relaxed) {
        LOW_WARNING_SHOWN.store(true, Ordering::Relaxed);
        serial_println!("[Battery] LOW: {}% - please connect charger", percent);
    }
}

/// Get current battery info
pub fn get_info() -> BatteryInfo {
    BATTERY_INFO.lock().clone()
}

/// Get charge percentage
pub fn charge_percent() -> u8 {
    BATTERY_INFO.lock().charge_percent
}

/// Is AC adapter connected?
pub fn is_ac_connected() -> bool {
    BATTERY_INFO.lock().ac_adapter == AcAdapterState::Online
}

/// Set low battery threshold
pub fn set_low_threshold(percent: u8) {
    LOW_BATTERY_THRESHOLD.store(percent.min(50), Ordering::Relaxed);
}

/// Set critical battery threshold
pub fn set_critical_threshold(percent: u8) {
    CRITICAL_BATTERY_THRESHOLD.store(percent.min(20), Ordering::Relaxed);
}

/// Initialize battery monitoring
pub fn init() {
    poll_battery();
    serial_println!("[KnoxOS] Battery monitor initialized");
}
