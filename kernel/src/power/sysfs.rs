use alloc::string::String;

use super::manager::PM;

// ── sysfs interface ─────────────────────────────────────────────────────

/// Generate /sys/power/state content
pub fn sys_power_state() -> String {
    String::from("freeze mem disk\n")
}

/// Generate /sys/power/wakeup_count content
pub fn sys_wakeup_count() -> String {
    let pm = PM.lock();
    let total: u64 = pm.wakeup_sources.iter().map(|ws| ws.wakeup_count).sum();
    alloc::format!("{}\n", total)
}

/// Generate /sys/devices/system/cpu/cpufreq info
pub fn sys_cpufreq_info() -> String {
    let pm = PM.lock();
    alloc::format!(
        "current_freq: {} KHz\nmin_freq: {} KHz\nmax_freq: {} KHz\ngovernor: {}\ndriver: {}\nhwp: {}\nratio: {}/{}/{}\n",
        pm.cpufreq.cur_freq,
        pm.cpufreq.min_freq,
        pm.cpufreq.max_freq,
        pm.cpufreq.governor.as_str(),
        if pm.cpufreq.driver.is_empty() {
            "acpi-cpufreq"
        } else {
            &pm.cpufreq.driver
        },
        if pm.cpufreq.hwp_active {
            "active"
        } else {
            "passive"
        },
        pm.cpufreq.cur_ratio,
        pm.cpufreq.min_ratio,
        pm.cpufreq.max_ratio,
    )
}

/// Generate /sys/class/thermal info
pub fn sys_thermal_info() -> String {
    let pm = PM.lock();
    let mut output = String::new();
    for (i, zone) in pm.thermal_zones.iter().enumerate() {
        output.push_str(&alloc::format!(
            "thermal_zone{}: {} temp={}.{}°C\n",
            i,
            zone.name,
            zone.temp_mc / 1000,
            (zone.temp_mc % 1000) / 100,
        ));
        for (j, trip) in zone.trip_points.iter().enumerate() {
            output.push_str(&alloc::format!(
                "  trip_point_{}: type={} temp={}.{}°C hyst={}.{}°C\n",
                j,
                trip.trip_type.as_str(),
                trip.temp_mc / 1000,
                (trip.temp_mc % 1000) / 100,
                trip.hysteresis_mc / 1000,
                (trip.hysteresis_mc % 1000) / 100,
            ));
        }
        for cd in &zone.cooling_devices {
            output.push_str(&alloc::format!("  cooling: {}\n", cd.name));
        }
    }
    if output.is_empty() {
        output.push_str("thermal_zone0: x86_pkg_temp temp=45.0°C\n");
    }
    output
}

/// Generate /sys/class/power_supply info
pub fn sys_battery_info() -> String {
    let pm = PM.lock();
    match &pm.battery {
        Some(bat) => alloc::format!(
            "POWER_SUPPLY_NAME=BAT0\nPOWER_SUPPLY_STATUS={}\nPOWER_SUPPLY_PRESENT={}\nPOWER_SUPPLY_VOLTAGE_NOW={}\nPOWER_SUPPLY_ENERGY_NOW={}\nPOWER_SUPPLY_ENERGY_FULL={}\nPOWER_SUPPLY_CAPACITY={}\nPOWER_SUPPLY_TECHNOLOGY={}\nPOWER_SUPPLY_MANUFACTURER={}\n",
            bat.state.as_str(),
            if bat.present { 1 } else { 0 },
            bat.voltage_mv * 1000,    // µV
            bat.remaining_mwh * 1000, // µWh
            bat.full_charge_mwh * 1000,
            bat.percentage,
            bat.technology,
            bat.manufacturer,
        ),
        None => String::from("POWER_SUPPLY_NAME=AC\nPOWER_SUPPLY_ONLINE=1\n"),
    }
}

/// Get power stats
pub fn power_stats() -> (u64, u64) {
    let pm = PM.lock();
    (pm.suspend_count, pm.resume_count)
}

/// Get comprehensive power status string
pub fn power_status() -> String {
    let pm = PM.lock();
    let mut s = String::new();
    s.push_str(&alloc::format!(
        "System State: {}\n",
        pm.current_state.as_str()
    ));
    s.push_str(&alloc::format!(
        "Suspend count: {} | Resume count: {} | Failed: {}\n",
        pm.suspend_count,
        pm.resume_count,
        pm.failed_suspend_count
    ));
    s.push_str(&alloc::format!(
        "CPU: {} KHz (governor: {}, HWP: {})\n",
        pm.cpufreq.cur_freq,
        pm.cpufreq.governor.as_str(),
        if pm.cpufreq.hwp_active {
            "active"
        } else {
            "passive"
        }
    ));
    s.push_str(&alloc::format!("AC: {:?}\n", pm.ac_state));
    if let Some(bat) = &pm.battery {
        s.push_str(&alloc::format!(
            "Battery: {} {}% ({}mW, {}mV)\n",
            bat.state.as_str(),
            bat.percentage,
            bat.rate_mw,
            bat.voltage_mv
        ));
    }
    s.push_str(&alloc::format!(
        "Devices: {} registered\n",
        pm.devices.len()
    ));
    s.push_str(&alloc::format!(
        "Wakeup sources: {}\n",
        pm.wakeup_sources.len()
    ));
    s
}
