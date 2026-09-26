use alloc::string::String;
use alloc::vec::Vec;

use super::hw::{SLP_TYP_S1, SLP_TYP_S3, SLP_TYP_S4, SLP_TYP_S5};

// ── System power state ──────────────────────────────────────────────────

/// System power state (ACPI S-states)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemState {
    /// S0: Running
    Running,
    /// S1: Standby (CPU stopped, RAM powered)
    Standby,
    /// S3: Suspend to RAM
    SuspendToRam,
    /// S4: Suspend to Disk (Hibernate)
    SuspendToDisk,
    /// S5: Soft Off
    SoftOff,
}

impl SystemState {
    pub fn as_str(&self) -> &str {
        match self {
            SystemState::Running => "running",
            SystemState::Standby => "standby",
            SystemState::SuspendToRam => "mem",
            SystemState::SuspendToDisk => "disk",
            SystemState::SoftOff => "off",
        }
    }

    /// ACPI SLP_TYP value for this state
    pub(super) fn slp_typ(&self) -> u16 {
        match self {
            SystemState::Running => 0,
            SystemState::Standby => SLP_TYP_S1,
            SystemState::SuspendToRam => SLP_TYP_S3,
            SystemState::SuspendToDisk => SLP_TYP_S4,
            SystemState::SoftOff => SLP_TYP_S5,
        }
    }
}

// ── CPU frequency governor ──────────────────────────────────────────────

/// CPU frequency governor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuFreqGovernor {
    Performance,
    Powersave,
    Ondemand,
    Conservative,
    Schedutil,
    Userspace,
}

impl CpuFreqGovernor {
    pub fn as_str(&self) -> &str {
        match self {
            CpuFreqGovernor::Performance => "performance",
            CpuFreqGovernor::Powersave => "powersave",
            CpuFreqGovernor::Ondemand => "ondemand",
            CpuFreqGovernor::Conservative => "conservative",
            CpuFreqGovernor::Schedutil => "schedutil",
            CpuFreqGovernor::Userspace => "userspace",
        }
    }

    pub fn parse_governor(s: &str) -> Option<Self> {
        match s {
            "performance" => Some(CpuFreqGovernor::Performance),
            "powersave" => Some(CpuFreqGovernor::Powersave),
            "ondemand" => Some(CpuFreqGovernor::Ondemand),
            "conservative" => Some(CpuFreqGovernor::Conservative),
            "schedutil" => Some(CpuFreqGovernor::Schedutil),
            "userspace" => Some(CpuFreqGovernor::Userspace),
            _ => None,
        }
    }
}

// ── CPU idle states (C-states) ──────────────────────────────────────────

/// CPU idle state (C-states)
#[derive(Debug, Clone)]
pub struct CpuIdleState {
    pub name: String,
    pub desc: String,
    /// MWAIT hint for this C-state
    pub mwait_hint: u32,
    /// Latency to exit this state (microseconds)
    pub latency_us: u32,
    /// Power consumption in this state (milliwatts, estimated)
    pub power_mw: u32,
    /// Time spent in this state (microseconds)
    pub time_us: u64,
    /// Number of times entered
    pub usage: u64,
    /// Is this state disabled?
    pub disabled: bool,
}

// ── CPU frequency info ──────────────────────────────────────────────────

/// Per-CPU frequency info
#[derive(Debug, Clone)]
pub struct CpuFreqInfo {
    /// Current frequency in KHz
    pub cur_freq: u32,
    /// Minimum frequency in KHz
    pub min_freq: u32,
    /// Maximum frequency in KHz
    pub max_freq: u32,
    /// Active governor
    pub governor: CpuFreqGovernor,
    /// Available governors
    pub available_governors: Vec<CpuFreqGovernor>,
    /// Scaling driver
    pub driver: String,
    /// Whether HWP (Hardware P-states) is active
    pub hwp_active: bool,
    /// P-state ratio (multiplier)
    pub cur_ratio: u8,
    pub min_ratio: u8,
    pub max_ratio: u8,
}

// ── Thermal zones ───────────────────────────────────────────────────────

/// Thermal zone info
#[derive(Debug, Clone)]
pub struct ThermalZone {
    pub name: String,
    /// Temperature in millidegrees Celsius
    pub temp_mc: i32,
    /// Trip points (millidegrees Celsius)
    pub trip_points: Vec<TripPoint>,
    /// Cooling devices
    pub cooling_devices: Vec<CoolingDevice>,
    /// Thermal policy
    pub policy: ThermalPolicy,
}

#[derive(Debug, Clone)]
pub struct TripPoint {
    pub name: String,
    pub trip_type: TripType,
    pub temp_mc: i32,
    pub hysteresis_mc: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripType {
    Active,
    Passive,
    Hot,
    Critical,
}

impl TripType {
    pub fn as_str(&self) -> &str {
        match self {
            TripType::Active => "active",
            TripType::Passive => "passive",
            TripType::Hot => "hot",
            TripType::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalPolicy {
    StepWise,
    FairShare,
    UserSpace,
    PowerAllocator,
}

#[derive(Debug, Clone)]
pub struct CoolingDevice {
    pub name: String,
    pub cooling_type: String,
    pub max_state: u32,
    pub cur_state: u32,
}

// ── Battery / AC adapter ────────────────────────────────────────────────

/// Battery status (ACPI \_SB_.PCI0.BAT0._BST)
#[derive(Debug, Clone)]
pub struct BatteryStatus {
    pub present: bool,
    pub state: BatteryState,
    /// Remaining capacity (mWh)
    pub remaining_mwh: u32,
    /// Full charge capacity (mWh)
    pub full_charge_mwh: u32,
    /// Design capacity (mWh)
    pub design_capacity_mwh: u32,
    /// Present rate (mW) — charge/discharge
    pub rate_mw: u32,
    /// Voltage (mV)
    pub voltage_mv: u32,
    /// Percentage 0-100
    pub percentage: u8,
    /// Estimated time to empty (seconds), 0 if charging
    pub time_to_empty_s: u32,
    /// Estimated time to full (seconds), 0 if discharging
    pub time_to_full_s: u32,
    /// Cycle count
    pub cycle_count: u32,
    /// Battery technology
    pub technology: String,
    /// Manufacturer
    pub manufacturer: String,
    /// Model name
    pub model: String,
    /// Serial number
    pub serial: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryState {
    Charging,
    Discharging,
    NotCharging,
    Full,
    Unknown,
}

impl BatteryState {
    pub fn as_str(&self) -> &str {
        match self {
            BatteryState::Charging => "Charging",
            BatteryState::Discharging => "Discharging",
            BatteryState::NotCharging => "Not charging",
            BatteryState::Full => "Full",
            BatteryState::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcAdapterState {
    Online,
    Offline,
    Unknown,
}

// ── Device runtime PM ───────────────────────────────────────────────────

/// Runtime PM state per device
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePmState {
    Active,
    Suspending,
    Suspended,
    Resuming,
    Error,
}

/// Device power management callbacks
pub struct DevicePmOps {
    pub name: String,
    pub state: RuntimePmState,
    /// Suspend callback succeeded?
    pub suspend_ok: bool,
    /// Resume callback succeeded?
    pub resume_ok: bool,
    /// Autosuspend delay (ms)
    pub autosuspend_delay_ms: u32,
    /// Whether autosuspend is enabled
    pub autosuspend_enabled: bool,
    /// Usage count — device stays active while > 0
    pub usage_count: i32,
}

// ── Wakeup source tracking ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WakeupSource {
    pub name: String,
    pub enabled: bool,
    pub active_count: u64,
    pub wakeup_count: u64,
    pub last_time_us: u64,
    pub total_time_us: u64,
}
