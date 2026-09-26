//! Power Management — Real ACPI state transitions, device suspend/resume,
//! CPU frequency scaling, thermal management, and battery monitoring.
//!
//! Implements Linux-compatible power management:
//!   - CPU frequency scaling (cpufreq) via MSR and P-states
//!   - Idle states (C-states) via MWAIT
//!   - System suspend/resume (S-states) via ACPI PM1 registers
//!   - Runtime PM for devices (suspend/resume callbacks)
//!   - Thermal management with trip points and cooling
//!   - Battery/AC adapter via ACPI \_SB_.PCI0.LPCB.EC
//!   - /sys/power sysfs interface
//!   - Wake-on-LAN, RTC alarm, power button events
//!
//! Module layout:
//!   types     — public enums and structs
//!   hw        — ACPI PM1 / PM timer ports and constants
//!   manager   — global PowerManager state and init
//!   cpu       — P-states, governors, C-states, S3 CPU save/restore
//!   devices   — runtime PM register / suspend / resume
//!   sleep     — freeze/thaw, ACPI S-state entry, S3/S4 sequences
//!   thermal   — trip points, MSR temp, throttle levels
//!   battery   — battery / AC adapter status
//!   wakeup    — wakeup source tracking
//!   sysfs     — /sys/power style status strings
//!   shutdown  — S5, reboot, power button
//!   idle      — screen dim / idle tick
//!   boot      — boot-stage timing

mod battery;
mod boot;
mod cpu;
mod devices;
mod hw;
mod idle;
mod manager;
mod shutdown;
mod sleep;
mod sysfs;
mod thermal;
mod types;
mod wakeup;

pub use battery::{ac_adapter_state, battery_status, update_battery};
pub use boot::{
    BootStage, boot_stage_end, boot_stage_start, boot_timing_report, print_boot_timing,
};
pub use cpu::{
    CpuSaveState, cpu_idle_enter, get_cpu_freq, get_governor, governor_tick, set_cpu_freq,
    set_governor,
};
pub use devices::{register_device, runtime_resume_device, runtime_suspend_device};
pub use hw::{pm_timer_delay_us, pm_timer_read, pm_timer_ticks_to_us};
pub use idle::{
    ScreenDimConfig, idle_power_tick, record_user_activity, screen_idle_state,
    set_screen_dim_config,
};
pub use manager::init;
pub use shutdown::{
    acpi_reboot, acpi_shutdown, enable_power_button_event, handle_power_button_event,
    was_power_button_pressed,
};
pub use sleep::{
    acpi_hibernate, acpi_suspend_to_ram, check_hibernate_resume, current_state,
    is_suspend_in_progress, suspend,
};
pub use sysfs::{
    power_stats, power_status, sys_battery_info, sys_cpufreq_info, sys_power_state,
    sys_thermal_info, sys_wakeup_count,
};
pub use thermal::{
    ThrottleLevel, get_cpu_temp, thermal_check, thermal_throttle_level, update_thermal_throttle,
};
pub use types::{
    AcAdapterState, BatteryState, BatteryStatus, CoolingDevice, CpuFreqGovernor, CpuFreqInfo,
    CpuIdleState, DevicePmOps, RuntimePmState, SystemState, ThermalPolicy, ThermalZone, TripPoint,
    TripType, WakeupSource,
};
pub use wakeup::{register_wakeup_source, set_wakeup_source_enabled, wakeup_event};
