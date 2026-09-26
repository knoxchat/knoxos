use alloc::string::String;

use super::manager::PM;
use super::types::{AcAdapterState, BatteryState, BatteryStatus};

// ── Battery monitoring ──────────────────────────────────────────────────

/// Get battery status (reads ACPI _BST object)
pub fn battery_status() -> Option<BatteryStatus> {
    let pm = PM.lock();
    pm.battery.clone()
}

/// Get AC adapter state
pub fn ac_adapter_state() -> AcAdapterState {
    let pm = PM.lock();
    pm.ac_state
}

/// Update battery info (called periodically or on ACPI notify)
pub fn update_battery(
    present: bool,
    charging: bool,
    remaining_mwh: u32,
    full_charge_mwh: u32,
    rate_mw: u32,
    voltage_mv: u32,
) {
    let mut pm = PM.lock();
    let pct = if full_charge_mwh > 0 {
        ((remaining_mwh as u64 * 100) / full_charge_mwh as u64) as u8
    } else {
        0
    };

    let tte = if !charging && rate_mw > 0 {
        (remaining_mwh as u64 * 3600 / rate_mw as u64) as u32
    } else {
        0
    };

    let ttf = if charging && rate_mw > 0 && full_charge_mwh > remaining_mwh {
        ((full_charge_mwh - remaining_mwh) as u64 * 3600 / rate_mw as u64) as u32
    } else {
        0
    };

    let state = if !present {
        BatteryState::Unknown
    } else if charging && pct >= 100 {
        BatteryState::Full
    } else if charging {
        BatteryState::Charging
    } else {
        BatteryState::Discharging
    };

    pm.battery = Some(BatteryStatus {
        present,
        state,
        remaining_mwh,
        full_charge_mwh,
        design_capacity_mwh: full_charge_mwh,
        rate_mw,
        voltage_mv,
        percentage: pct.min(100),
        time_to_empty_s: tte,
        time_to_full_s: ttf,
        cycle_count: 0,
        technology: String::from("Li-ion"),
        manufacturer: String::from("KnoxOS Virtual Battery"),
        model: String::from("BAT0"),
        serial: String::from("0001"),
    });

    pm.ac_state = if charging {
        AcAdapterState::Online
    } else {
        AcAdapterState::Offline
    };
}
