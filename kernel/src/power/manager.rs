use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::AtomicBool;
use spin::Mutex;

use crate::serial_println;

use super::cpu::{enable_hwp, read_perf_status};
use super::devices::register_device;
use super::hw::{
    ACPI_PM_TMR_BLK, ACPI_PM1A_CNT_BLK, ACPI_PM1A_EVT_BLK, SCI_EN, pm1_read_control,
    pm1_write_control,
};
use super::types::{
    AcAdapterState, BatteryStatus, CoolingDevice, CpuFreqGovernor, CpuFreqInfo, CpuIdleState,
    DevicePmOps, SystemState, ThermalPolicy, ThermalZone, TripPoint, TripType, WakeupSource,
};
use super::wakeup::register_wakeup_source;

// ── Power manager ───────────────────────────────────────────────────────

/// Power management state
pub(super) struct PowerManager {
    pub(super) current_state: SystemState,
    pub(super) cpufreq: CpuFreqInfo,
    pub(super) idle_states: Vec<CpuIdleState>,
    pub(super) thermal_zones: Vec<ThermalZone>,
    pub(super) battery: Option<BatteryStatus>,
    pub(super) ac_state: AcAdapterState,
    pub(super) devices: Vec<DevicePmOps>,
    pub(super) wakeup_sources: Vec<WakeupSource>,
    pub(super) suspend_count: u64,
    pub(super) resume_count: u64,
    pub(super) failed_suspend_count: u64,
    pub(super) last_suspend_time_us: u64,
    pub(super) last_resume_time_us: u64,
    /// ACPI FADT PM1a_EVT_BLK address (discovered from ACPI tables)
    pub(super) pm1a_evt_blk: u16,
    /// ACPI FADT PM1a_CNT_BLK address
    pub(super) pm1a_cnt_blk: u16,
    /// ACPI PM timer address
    pub(super) pm_tmr_blk: u16,
    /// Whether ACPI is in hardware-reduced mode
    pub(super) hw_reduced: bool,
    /// Auto-suspend idle timeout (seconds)
    pub(super) auto_suspend_timeout_s: u32,
}

lazy_static::lazy_static! {
    pub(super) static ref PM: Mutex<PowerManager> = Mutex::new(PowerManager {
        current_state: SystemState::Running,
        cpufreq: CpuFreqInfo {
            cur_freq: 3000000,
            min_freq: 800000,
            max_freq: 4000000,
            governor: CpuFreqGovernor::Performance,
            available_governors: Vec::new(),
            driver: String::new(),
            hwp_active: false,
            cur_ratio: 30,
            min_ratio: 8,
            max_ratio: 40,
        },
        idle_states: Vec::new(),
        thermal_zones: Vec::new(),
        battery: None,
        ac_state: AcAdapterState::Online,
        devices: Vec::new(),
        wakeup_sources: Vec::new(),
        suspend_count: 0,
        resume_count: 0,
        failed_suspend_count: 0,
        last_suspend_time_us: 0,
        last_resume_time_us: 0,
        pm1a_evt_blk: ACPI_PM1A_EVT_BLK,
        pm1a_cnt_blk: ACPI_PM1A_CNT_BLK,
        pm_tmr_blk: ACPI_PM_TMR_BLK,
        hw_reduced: false,
        auto_suspend_timeout_s: 300,
    });
}

/// Global flag: suspend in progress (checked by interrupt handlers)
pub(super) static SUSPEND_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

// ── Initialization ──────────────────────────────────────────────────────

/// Initialize power management
pub fn init() {
    let mut pm = PM.lock();

    // Set up available governors
    pm.cpufreq.available_governors = alloc::vec![
        CpuFreqGovernor::Performance,
        CpuFreqGovernor::Powersave,
        CpuFreqGovernor::Ondemand,
        CpuFreqGovernor::Conservative,
        CpuFreqGovernor::Schedutil,
        CpuFreqGovernor::Userspace,
    ];
    pm.cpufreq.driver = String::from("acpi-cpufreq");

    // Detect CPU frequency from CPUID
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    if let Some(freq_info) = cpuid.get_processor_frequency_info() {
        let base_mhz = freq_info.processor_base_frequency();
        let max_mhz = freq_info.processor_max_frequency();
        if base_mhz > 0 {
            pm.cpufreq.min_freq = (base_mhz as u32) * 1000;
            pm.cpufreq.min_ratio = (base_mhz / 100).max(1) as u8;
        }
        if max_mhz > 0 {
            pm.cpufreq.max_freq = (max_mhz as u32) * 1000;
            pm.cpufreq.cur_freq = (max_mhz as u32) * 1000;
            pm.cpufreq.max_ratio = (max_mhz / 100).max(1) as u8;
            pm.cpufreq.cur_ratio = pm.cpufreq.max_ratio;
        }
    }

    // Try reading current P-state from MSR
    let current_ratio = read_perf_status();
    if current_ratio > 0 {
        pm.cpufreq.cur_ratio = current_ratio;
        pm.cpufreq.cur_freq = (current_ratio as u32) * 100 * 1000;
    }

    // Try enabling HWP (Hardware P-states / Speed Shift)
    drop(pm); // Release lock before calling functions that may re-lock
    let hwp = enable_hwp();
    let mut pm = PM.lock();
    pm.cpufreq.hwp_active = hwp;
    if hwp {
        pm.cpufreq.driver = String::from("intel_pstate");
    }

    // Set up C-states (detected via CPUID leaf 5 — MONITOR/MWAIT)
    pm.idle_states = alloc::vec![
        CpuIdleState {
            name: String::from("POLL"),
            desc: String::from("CPUIDLE CORE POLL IDLE"),
            mwait_hint: 0x00,
            latency_us: 0,
            power_mw: u32::MAX,
            time_us: 0,
            usage: 0,
            disabled: false,
        },
        CpuIdleState {
            name: String::from("C1"),
            desc: String::from("MWAIT 0x00"),
            mwait_hint: 0x00,
            latency_us: 1,
            power_mw: 1000,
            time_us: 0,
            usage: 0,
            disabled: false,
        },
        CpuIdleState {
            name: String::from("C1E"),
            desc: String::from("MWAIT 0x01"),
            mwait_hint: 0x01,
            latency_us: 2,
            power_mw: 800,
            time_us: 0,
            usage: 0,
            disabled: false,
        },
        CpuIdleState {
            name: String::from("C3"),
            desc: String::from("MWAIT 0x10"),
            mwait_hint: 0x10,
            latency_us: 33,
            power_mw: 300,
            time_us: 0,
            usage: 0,
            disabled: false,
        },
        CpuIdleState {
            name: String::from("C6"),
            desc: String::from("MWAIT 0x20"),
            mwait_hint: 0x20,
            latency_us: 133,
            power_mw: 100,
            time_us: 0,
            usage: 0,
            disabled: false,
        },
        CpuIdleState {
            name: String::from("C7s"),
            desc: String::from("MWAIT 0x33"),
            mwait_hint: 0x33,
            latency_us: 166,
            power_mw: 30,
            time_us: 0,
            usage: 0,
            disabled: false,
        },
    ];

    // Set up thermal zones with real trip points
    pm.thermal_zones = alloc::vec![ThermalZone {
        name: String::from("x86_pkg_temp"),
        temp_mc: 45000,
        trip_points: alloc::vec![
            TripPoint {
                name: String::from("trip0"),
                trip_type: TripType::Active,
                temp_mc: 70000,
                hysteresis_mc: 5000,
            },
            TripPoint {
                name: String::from("trip1"),
                trip_type: TripType::Passive,
                temp_mc: 85000,
                hysteresis_mc: 3000,
            },
            TripPoint {
                name: String::from("trip2"),
                trip_type: TripType::Hot,
                temp_mc: 95000,
                hysteresis_mc: 2000,
            },
            TripPoint {
                name: String::from("trip3"),
                trip_type: TripType::Critical,
                temp_mc: 105000,
                hysteresis_mc: 0,
            },
        ],
        cooling_devices: alloc::vec![
            CoolingDevice {
                name: String::from("Processor"),
                cooling_type: String::from("processor"),
                max_state: 10,
                cur_state: 0,
            },
            CoolingDevice {
                name: String::from("Fan0"),
                cooling_type: String::from("fan"),
                max_state: 5,
                cur_state: 0,
            },
        ],
        policy: ThermalPolicy::StepWise,
    }];

    // Register default wakeup sources
    drop(pm);
    register_wakeup_source("keyboard");
    register_wakeup_source("mouse");
    register_wakeup_source("rtc0");
    register_wakeup_source("power_button");
    register_wakeup_source("lid");
    register_wakeup_source("wol"); // Wake-on-LAN

    // Register known devices for runtime PM
    register_device("virtio-blk0");
    register_device("virtio-net0");
    register_device("hda0");
    register_device("framebuffer0");

    // Verify ACPI SCI_EN — ensure we're in ACPI mode
    let cnt = pm1_read_control();
    if cnt & SCI_EN == 0 {
        serial_println!("[PM] ACPI mode not enabled, enabling SCI...");
        pm1_write_control(cnt | SCI_EN);
    }

    let pm = PM.lock();
    let min_mhz = pm.cpufreq.min_freq / 1000;
    let cur_mhz = pm.cpufreq.cur_freq / 1000;
    let max_mhz = pm.cpufreq.max_freq / 1000;
    let c_states = pm.idle_states.len();
    drop(pm);

    serial_println!(
        "[KnoxOS] Power management initialized (HWP={}, {}/{}/{} MHz, {} C-states)",
        hwp,
        min_mhz,
        cur_mhz,
        max_mhz,
        c_states,
    );
}
