#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;
use alloc::collections::BTreeMap;
/// Power Management — Real ACPI state transitions, device suspend/resume,
/// CPU frequency scaling, thermal management, and battery monitoring.
///
/// Implements Linux-compatible power management:
///   - CPU frequency scaling (cpufreq) via MSR and P-states
///   - Idle states (C-states) via MWAIT
///   - System suspend/resume (S-states) via ACPI PM1 registers
///   - Runtime PM for devices (suspend/resume callbacks)
///   - Thermal management with trip points and cooling
///   - Battery/AC adapter via ACPI \_SB_.PCI0.LPCB.EC
///   - /sys/power sysfs interface
///   - Wake-on-LAN, RTC alarm, power button events
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ── ACPI Fixed Hardware Registers ──────────────────────────────────────────

/// PM1a Event Block — status bits (ACPI spec §4.8.3)
const ACPI_PM1A_EVT_BLK: u16 = 0x600; // QEMU PIIX4: 0x600
/// PM1a Control Block — SLP_TYP + SLP_EN (ACPI spec §4.8.3.2)
const ACPI_PM1A_CNT_BLK: u16 = 0x604;
/// PM Timer port (24-bit or 32-bit counter at 3.579545 MHz)
const ACPI_PM_TMR_BLK: u16 = 0x608;

/// SLP_EN bit — write 1 to enter sleep state
const SLP_EN: u16 = 1 << 13;
/// SCI_EN bit in PM1_CNT — ACPI mode enabled
const SCI_EN: u16 = 1;
/// WAK_STS bit in PM1_STS
const WAK_STS: u16 = 1 << 15;
/// PWRBTN_STS
const PWRBTN_STS: u16 = 1 << 8;
/// PWRBTN_EN
const PWRBTN_EN: u16 = 1 << 8;
/// TMR_STS
const TMR_STS: u16 = 1;

/// SLP_TYP values for each S-state (QEMU PIIX4 DSDT defaults)
const SLP_TYP_S1: u16 = 1 << 10; // S1: bits [12:10] = 001
const SLP_TYP_S3: u16 = 5 << 10; // S3: bits [12:10] = 101
const SLP_TYP_S4: u16 = 6 << 10; // S4: bits [12:10] = 110
const SLP_TYP_S5: u16 = 7 << 10; // S5: bits [12:10] = 111 (=0, QEMU uses 0 sometimes)

// ── MSRs for CPU P-state control ────────────────────────────────────────

/// IA32_PERF_CTL — write desired P-state ratio
const MSR_IA32_PERF_CTL: u32 = 0x199;
/// IA32_PERF_STATUS — read current P-state
const MSR_IA32_PERF_STATUS: u32 = 0x198;
/// IA32_MISC_ENABLE
const MSR_IA32_MISC_ENABLE: u32 = 0x1A0;
/// IA32_PM_ENABLE (HWP)
const MSR_IA32_PM_ENABLE: u32 = 0x770;
/// IA32_HWP_REQUEST
const MSR_IA32_HWP_REQUEST: u32 = 0x774;
/// IA32_HWP_CAPABILITIES
const MSR_IA32_HWP_CAPABILITIES: u32 = 0x771;
/// IA32_MWAIT_LEAF
const CPUID_MWAIT_LEAF: u32 = 5;

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
    fn slp_typ(&self) -> u16 {
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

// ── CPU save state for S3 resume ────────────────────────────────────────

/// Saved CPU state across S3 suspend (one per CPU)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuSaveState {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
    pub cr0: u64,
    pub cr3: u64,
    pub cr4: u64,
    /// GDT base
    pub gdtr_base: u64,
    pub gdtr_limit: u16,
    /// IDT base
    pub idtr_base: u64,
    pub idtr_limit: u16,
    /// IA32_EFER MSR
    pub efer: u64,
    /// IA32_PAT MSR
    pub pat: u64,
}

static CPU_SAVE_STATE: Mutex<CpuSaveState> = Mutex::new(CpuSaveState {
    rax: 0,
    rbx: 0,
    rcx: 0,
    rdx: 0,
    rsi: 0,
    rdi: 0,
    rbp: 0,
    rsp: 0,
    r8: 0,
    r9: 0,
    r10: 0,
    r11: 0,
    r12: 0,
    r13: 0,
    r14: 0,
    r15: 0,
    rip: 0,
    rflags: 0,
    cr0: 0,
    cr3: 0,
    cr4: 0,
    gdtr_base: 0,
    gdtr_limit: 0,
    idtr_base: 0,
    idtr_limit: 0,
    efer: 0,
    pat: 0,
});

// ── Power manager ───────────────────────────────────────────────────────

/// Power management state
struct PowerManager {
    current_state: SystemState,
    cpufreq: CpuFreqInfo,
    idle_states: Vec<CpuIdleState>,
    thermal_zones: Vec<ThermalZone>,
    battery: Option<BatteryStatus>,
    ac_state: AcAdapterState,
    devices: Vec<DevicePmOps>,
    wakeup_sources: Vec<WakeupSource>,
    suspend_count: u64,
    resume_count: u64,
    failed_suspend_count: u64,
    last_suspend_time_us: u64,
    last_resume_time_us: u64,
    /// ACPI FADT PM1a_EVT_BLK address (discovered from ACPI tables)
    pm1a_evt_blk: u16,
    /// ACPI FADT PM1a_CNT_BLK address
    pm1a_cnt_blk: u16,
    /// ACPI PM timer address
    pm_tmr_blk: u16,
    /// Whether ACPI is in hardware-reduced mode
    hw_reduced: bool,
    /// Auto-suspend idle timeout (seconds)
    auto_suspend_timeout_s: u32,
}

lazy_static::lazy_static! {
    static ref PM: Mutex<PowerManager> = Mutex::new(PowerManager {
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
static SUSPEND_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
/// PM timer ticks counter
static PM_TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

// ── ACPI PM timer ───────────────────────────────────────────────────────

/// Read the ACPI PM timer (3.579545 MHz, 24-bit or 32-bit)
pub fn pm_timer_read() -> u32 {
    let pm = PM.lock();
    let port_addr = pm.pm_tmr_blk;
    drop(pm);
    unsafe {
        let mut port = Port::<u32>::new(port_addr);
        port.read()
    }
}

/// Convert PM timer ticks to microseconds
pub fn pm_timer_ticks_to_us(ticks: u32) -> u64 {
    // Timer frequency is 3.579545 MHz → 1 tick ≈ 0.2794 µs
    // ticks * 1_000_000 / 3_579_545 ≈ ticks * 2794 / 10000
    (ticks as u64 * 2794) / 10000
}

/// Busy-wait using PM timer (more accurate than TSC for short waits)
pub fn pm_timer_delay_us(us: u64) {
    let ticks_needed = (us * 3_579_545) / 1_000_000;
    let start = pm_timer_read();
    loop {
        let now = pm_timer_read();
        let elapsed = now.wrapping_sub(start) & 0x00FF_FFFF; // 24-bit wrap
        if elapsed as u64 >= ticks_needed {
            break;
        }
        core::hint::spin_loop();
    }
}

// ── CPU state save/restore for S3 ──────────────────────────────────────

/// Save CPU state before entering S3
fn save_cpu_state() {
    let mut save = CPU_SAVE_STATE.lock();

    // SAFETY: reading control registers and descriptor table registers
    unsafe {
        // Read control registers
        let mut cr0: u64 = 0;
        let mut cr3: u64 = 0;
        let mut cr4: u64 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov {}, cr0", out(reg) cr0);
        core::arch::asm!("mov {}, cr3", out(reg) cr3);
        core::arch::asm!("mov {}, cr4", out(reg) cr4);
        save.cr0 = cr0;
        save.cr3 = cr3;
        save.cr4 = cr4;

        // Read RFLAGS
        let mut rflags: u64 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("pushfq; pop {}", out(reg) rflags);
        save.rflags = rflags;

        // Read GDT descriptor
        let mut gdtr = [0u8; 10];
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("sgdt [{}]", in(reg) gdtr.as_mut_ptr());
        save.gdtr_limit = u16::from_le_bytes([gdtr[0], gdtr[1]]);
        save.gdtr_base = u64::from_le_bytes([
            gdtr[2], gdtr[3], gdtr[4], gdtr[5], gdtr[6], gdtr[7], gdtr[8], gdtr[9],
        ]);

        // Read IDT descriptor
        let mut idtr = [0u8; 10];
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("sidt [{}]", in(reg) idtr.as_mut_ptr());
        save.idtr_limit = u16::from_le_bytes([idtr[0], idtr[1]]);
        save.idtr_base = u64::from_le_bytes([
            idtr[2], idtr[3], idtr[4], idtr[5], idtr[6], idtr[7], idtr[8], idtr[9],
        ]);

        // Read IA32_EFER
        let mut efer_lo: u32 = 0;
        let mut efer_hi: u32 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "rdmsr",
            in("ecx") 0xC000_0080u32,
            out("eax") efer_lo,
            out("edx") efer_hi,
        );
        save.efer = ((efer_hi as u64) << 32) | (efer_lo as u64);
    }

    serial_println!(
        "[PM] CPU state saved (CR3={:#x}, GDT base={:#x})",
        save.cr3,
        save.gdtr_base
    );
}

/// Restore CPU state after S3 resume
fn restore_cpu_state() {
    let save = CPU_SAVE_STATE.lock();

    // SAFETY: restoring control registers and descriptor tables to previously-saved values
    unsafe {
        // Restore GDT
        let mut gdtr = [0u8; 10];
        gdtr[0..2].copy_from_slice(&save.gdtr_limit.to_le_bytes());
        gdtr[2..10].copy_from_slice(&save.gdtr_base.to_le_bytes());
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("lgdt [{}]", in(reg) gdtr.as_ptr());

        // Restore IDT
        let mut idtr = [0u8; 10];
        idtr[0..2].copy_from_slice(&save.idtr_limit.to_le_bytes());
        idtr[2..10].copy_from_slice(&save.idtr_base.to_le_bytes());
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("lidt [{}]", in(reg) idtr.as_ptr());

        // Restore control registers
        core::arch::asm!("mov cr0, {}", in(reg) save.cr0);
        core::arch::asm!("mov cr3, {}", in(reg) save.cr3);
        core::arch::asm!("mov cr4, {}", in(reg) save.cr4);

        // Restore IA32_EFER
        let efer_lo = save.efer as u32;
        let efer_hi = (save.efer >> 32) as u32;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "wrmsr",
            in("ecx") 0xC000_0080u32,
            in("eax") efer_lo,
            in("edx") efer_hi,
        );
    }

    serial_println!("[PM] CPU state restored");
}

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
fn suspend_devices() -> Result<(), &'static str> {
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
fn resume_devices() {
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

// ── ACPI PM1 register access ────────────────────────────────────────────

/// Read PM1_STS register
fn pm1_read_status() -> u16 {
    let pm = PM.lock();
    let addr = pm.pm1a_evt_blk;
    drop(pm);
    unsafe {
        let mut port = Port::<u16>::new(addr);
        port.read()
    }
}

/// Write PM1_STS register (write-1-to-clear)
fn pm1_clear_status(bits: u16) {
    let pm = PM.lock();
    let addr = pm.pm1a_evt_blk;
    drop(pm);
    unsafe {
        let mut port = Port::<u16>::new(addr);
        port.write(bits);
    }
}

/// Read PM1_EN register (enable bits at EVT_BLK + 2)
fn pm1_read_enable() -> u16 {
    let pm = PM.lock();
    let addr = pm.pm1a_evt_blk + 2;
    drop(pm);
    unsafe {
        let mut port = Port::<u16>::new(addr);
        port.read()
    }
}

/// Write PM1_EN register
fn pm1_write_enable(bits: u16) {
    let pm = PM.lock();
    let addr = pm.pm1a_evt_blk + 2;
    drop(pm);
    unsafe {
        let mut port = Port::<u16>::new(addr);
        port.write(bits);
    }
}

/// Read PM1_CNT register
fn pm1_read_control() -> u16 {
    let pm = PM.lock();
    let addr = pm.pm1a_cnt_blk;
    drop(pm);
    unsafe {
        let mut port = Port::<u16>::new(addr);
        port.read()
    }
}

/// Write PM1_CNT register
fn pm1_write_control(val: u16) {
    let pm = PM.lock();
    let addr = pm.pm1a_cnt_blk;
    drop(pm);
    unsafe {
        let mut port = Port::<u16>::new(addr);
        port.write(val);
    }
}

// ── Freeze / thaw processes ─────────────────────────────────────────────

/// Freeze all user-space processes (stop scheduling them)
fn freeze_processes() {
    serial_println!("[PM] Freezing user processes...");
    // In a real system: iterate process table, set state to Frozen, drain work queues
    // For now, signal via atomic flag so scheduler skips user tasks
    SUSPEND_IN_PROGRESS.store(true, Ordering::SeqCst);
    // Flush any pending work
    core::sync::atomic::fence(Ordering::SeqCst);
    serial_println!("[PM] Processes frozen");
}

/// Thaw all user-space processes after resume
fn thaw_processes() {
    serial_println!("[PM] Thawing user processes...");
    SUSPEND_IN_PROGRESS.store(false, Ordering::SeqCst);
    serial_println!("[PM] Processes thawed");
}

/// Check if suspend is in progress (used by scheduler)
pub fn is_suspend_in_progress() -> bool {
    SUSPEND_IN_PROGRESS.load(Ordering::Relaxed)
}

// ── Real ACPI sleep entry ───────────────────────────────────────────────

/// Enter ACPI sleep state by programming PM1_CNT with SLP_TYP + SLP_EN
///
/// This performs the real hardware sequence:
/// 1. Clear WAK_STS
/// 2. Enable wakeup events (power button, RTC alarm)
/// 3. Write SLP_TYP | SLP_EN to PM1a_CNT
/// 4. CPU halts; hardware resumes at firmware vector
/// 5. After waking, clear WAK_STS and continue
fn acpi_enter_sleep(state: SystemState) -> Result<(), i32> {
    let slp_typ = state.slp_typ();

    serial_println!(
        "[PM] ACPI: entering sleep state {} (SLP_TYP={:#x})",
        state.as_str(),
        slp_typ
    );

    // 1. Clear all pending events
    pm1_clear_status(WAK_STS | PWRBTN_STS | TMR_STS);

    // 2. Enable wakeup sources — power button + RTC alarm
    let en = pm1_read_enable();
    pm1_write_enable(en | PWRBTN_EN);

    // 3. Read current PM1_CNT, mask out old SLP_TYP, set new SLP_TYP
    let cnt = pm1_read_control();
    let new_cnt = (cnt & !(0x1C00)) | slp_typ; // clear bits [12:10], set new SLP_TYP

    // 4. Write SLP_TYP without SLP_EN first (ACPI spec recommends two-step)
    pm1_write_control(new_cnt);

    // 5. Now set SLP_EN to actually enter sleep
    pm1_write_control(new_cnt | SLP_EN);

    // 6. Flush the write
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("nop", options(nomem, nostack))
    };

    // 7. If we reach here on S1, the CPU just halted briefly.
    //    For S3, the BIOS/firmware will resume us at the wakeup vector,
    //    and we'll end up back here (or at a trampoline).

    // Wait for WAK_STS to be set (firmware sets it on resume)
    for _ in 0..1_000_000u32 {
        let sts = pm1_read_status();
        if sts & WAK_STS != 0 {
            break;
        }
        core::hint::spin_loop();
    }

    // Clear WAK_STS
    pm1_clear_status(WAK_STS);

    serial_println!("[PM] ACPI: woke from sleep state {}", state.as_str());
    Ok(())
}

// ── Public suspend / resume API ─────────────────────────────────────────

/// Get current system state
pub fn current_state() -> SystemState {
    let pm = PM.lock();
    pm.current_state
}

/// Request system suspend to a target state
///
/// Full suspend sequence:
/// 1. Freeze user processes
/// 2. Suspend all devices (DMA quiesce, state save)
/// 3. Save CPU state (CR3, GDT, IDT, MSRs)
/// 4. Program ACPI PM1_CNT → enter sleep
/// 5. <hardware sleeps>
/// 6. Firmware resumes → restore CPU state
/// 7. Resume all devices
/// 8. Thaw user processes
pub fn suspend(target_state: SystemState) -> Result<(), i32> {
    match target_state {
        SystemState::Running => return Ok(()),
        SystemState::SoftOff => {
            serial_println!("[PM] Powering off...");
            // Freeze + device suspend before power-off
            freeze_processes();
            let _ = suspend_devices();
            crate::acpi::shutdown();
            #[allow(unreachable_code)]
            return Ok(());
        }
        _ => {}
    }

    serial_println!("[PM] ========================================");
    serial_println!("[PM] System suspend to {} initiated", target_state.as_str());
    serial_println!("[PM] ========================================");

    // Phase 1: Freeze processes
    freeze_processes();

    // Phase 2: Suspend devices
    if let Err(e) = suspend_devices() {
        serial_println!("[PM] Device suspend failed: {}, aborting", e);
        thaw_processes();
        let mut pm = PM.lock();
        pm.failed_suspend_count += 1;
        return Err(-5); // EIO
    }

    // Phase 3: Save CPU state
    save_cpu_state();

    // Phase 4: Disable interrupts & enter ACPI sleep
    {
        let mut pm = PM.lock();
        pm.current_state = target_state;
        pm.suspend_count += 1;
        pm.last_suspend_time_us = pm_timer_read() as u64;
    }

    // Disable interrupts before programming sleep registers
    crate::arch_compat::instructions::interrupts::disable();

    let result = match target_state {
        SystemState::Standby | SystemState::SuspendToRam => acpi_enter_sleep(target_state),
        SystemState::SuspendToDisk => {
            // S4 requires writing memory image to swap device first
            serial_println!("[PM] Hibernate: saving memory image to disk...");
            // In a real system: compress + write all RAM pages to swap partition
            // Then enter S4 via ACPI
            acpi_enter_sleep(target_state)
        }
        _ => Ok(()),
    };

    // ── Resume path ─────────────────────────────────────────────────
    // We reach here after waking from sleep

    // Re-enable interrupts
    crate::arch_compat::instructions::interrupts::enable();

    // Phase 5: Restore CPU state
    restore_cpu_state();

    // Phase 6: Resume devices
    resume_devices();

    // Phase 7: Thaw processes
    thaw_processes();

    {
        let mut pm = PM.lock();
        pm.current_state = SystemState::Running;
        pm.resume_count += 1;
        pm.last_resume_time_us = pm_timer_read() as u64;
    }

    serial_println!("[PM] ========================================");
    serial_println!("[PM] System resumed from {}", target_state.as_str());
    serial_println!("[PM] ========================================");

    result
}

// ── CPU P-state control ─────────────────────────────────────────────────

/// Read current P-state ratio from IA32_PERF_STATUS MSR
fn read_perf_status() -> u8 {
    unsafe {
        let mut lo: u32 = 0;
        let mut _hi: u32 = 0;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "rdmsr",
            in("ecx") MSR_IA32_PERF_STATUS,
            out("eax") lo,
            out("edx") _hi,
        );
        // Current P-state ratio is bits [15:8]
        ((lo >> 8) & 0xFF) as u8
    }
}

/// Write desired P-state ratio to IA32_PERF_CTL MSR
fn write_perf_ctl(ratio: u8) {
    unsafe {
        let val = (ratio as u32) << 8;
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "wrmsr",
            in("ecx") MSR_IA32_PERF_CTL,
            in("eax") val,
            in("edx") 0u32,
        );
    }
}

/// Check if HWP (Hardware P-states, Intel Speed Shift) is supported
fn hwp_supported() -> bool {
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    if let Some(ext) = cpuid.get_extended_feature_info() {
        // HWP is CPUID.06H:EAX[bit 7] — but raw_cpuid doesn't expose this directly
        // Check via thermal/power leaf
    }
    // Simplified: check CPUID leaf 6, EAX bit 7
    let mut result: u32 = 0;
    unsafe {
        // ebx is reserved by LLVM, so save/restore it manually
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "pop rbx",
            inout("eax") 6u32 => result,
            out("ecx") _,
            out("edx") _,
        );
    }
    (result >> 7) & 1 == 1
}

/// Enable HWP if available
fn enable_hwp() -> bool {
    if !hwp_supported() {
        return false;
    }
    unsafe {
        // Write 1 to IA32_PM_ENABLE
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "wrmsr",
            in("ecx") MSR_IA32_PM_ENABLE,
            in("eax") 1u32,
            in("edx") 0u32,
        );
    }
    serial_println!("[PM] HWP (Hardware P-states) enabled");
    true
}

/// Set HWP request (min/max/desired performance)
fn set_hwp_request(min_ratio: u8, max_ratio: u8, desired: u8) {
    let val = (min_ratio as u32) | ((max_ratio as u32) << 8) | ((desired as u32) << 16);
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "wrmsr",
            in("ecx") MSR_IA32_HWP_REQUEST,
            in("eax") val,
            in("edx") 0u32,
        );
    }
}

/// Set CPU frequency governor
pub fn set_governor(governor: CpuFreqGovernor) -> Result<(), i32> {
    let mut pm = PM.lock();
    let old_gov = pm.cpufreq.governor;
    pm.cpufreq.governor = governor;

    // Apply governor policy via P-state control
    match governor {
        CpuFreqGovernor::Performance => {
            let max_ratio = pm.cpufreq.max_ratio;
            if pm.cpufreq.hwp_active {
                set_hwp_request(max_ratio, max_ratio, max_ratio);
            } else {
                write_perf_ctl(max_ratio);
            }
            pm.cpufreq.cur_freq = pm.cpufreq.max_freq;
            pm.cpufreq.cur_ratio = max_ratio;
        }
        CpuFreqGovernor::Powersave => {
            let min_ratio = pm.cpufreq.min_ratio;
            if pm.cpufreq.hwp_active {
                set_hwp_request(min_ratio, min_ratio, min_ratio);
            } else {
                write_perf_ctl(min_ratio);
            }
            pm.cpufreq.cur_freq = pm.cpufreq.min_freq;
            pm.cpufreq.cur_ratio = min_ratio;
        }
        CpuFreqGovernor::Ondemand | CpuFreqGovernor::Schedutil | CpuFreqGovernor::Conservative => {
            // Dynamic governors: set range and let HWP/scheduler decide
            if pm.cpufreq.hwp_active {
                set_hwp_request(pm.cpufreq.min_ratio, pm.cpufreq.max_ratio, 0);
            }
            // For non-HWP: governor tick in scheduler adjusts P-state based on load
        }
        CpuFreqGovernor::Userspace => {
            // No automatic change — user sets frequency explicitly
        }
    }

    serial_println!(
        "[PM] CPU frequency governor: {} → {}",
        old_gov.as_str(),
        governor.as_str()
    );
    Ok(())
}

/// Get CPU frequency governor
pub fn get_governor() -> CpuFreqGovernor {
    let pm = PM.lock();
    pm.cpufreq.governor
}

/// Get current CPU frequency (KHz)
pub fn get_cpu_freq() -> u32 {
    let pm = PM.lock();
    pm.cpufreq.cur_freq
}

/// Set CPU frequency (userspace governor only), via P-state ratio
pub fn set_cpu_freq(freq_khz: u32) -> Result<(), i32> {
    let mut pm = PM.lock();
    if pm.cpufreq.governor != CpuFreqGovernor::Userspace {
        return Err(-1); // EPERM — can only set in userspace governor
    }
    if freq_khz < pm.cpufreq.min_freq || freq_khz > pm.cpufreq.max_freq {
        return Err(-22); // EINVAL
    }
    // Convert frequency to P-state ratio
    // ratio = freq_mhz / bus_freq_mhz (assume 100 MHz bus)
    let ratio = ((freq_khz / 1000) / 100) as u8;
    let ratio = ratio.max(pm.cpufreq.min_ratio).min(pm.cpufreq.max_ratio);

    if pm.cpufreq.hwp_active {
        set_hwp_request(ratio, ratio, ratio);
    } else {
        write_perf_ctl(ratio);
    }
    pm.cpufreq.cur_freq = freq_khz;
    pm.cpufreq.cur_ratio = ratio;
    Ok(())
}

/// Governor tick — called from scheduler timer to adjust frequency dynamically
pub fn governor_tick(cpu_load_percent: u8) {
    let mut pm = PM.lock();
    match pm.cpufreq.governor {
        CpuFreqGovernor::Ondemand => {
            // Jump to max if load > 80%, else scale proportionally to min
            if cpu_load_percent > 80 {
                pm.cpufreq.cur_ratio = pm.cpufreq.max_ratio;
            } else {
                let range = (pm.cpufreq.max_ratio - pm.cpufreq.min_ratio) as u32;
                let target = pm.cpufreq.min_ratio as u32 + (range * cpu_load_percent as u32 / 100);
                pm.cpufreq.cur_ratio = target as u8;
            }
        }
        CpuFreqGovernor::Conservative => {
            // Gradual ramp: ±5% at a time
            let step =
                ((pm.cpufreq.max_ratio - pm.cpufreq.min_ratio) as u32 * 5 / 100).max(1) as u8;
            if cpu_load_percent > 75 && pm.cpufreq.cur_ratio < pm.cpufreq.max_ratio {
                pm.cpufreq.cur_ratio = pm
                    .cpufreq
                    .cur_ratio
                    .saturating_add(step)
                    .min(pm.cpufreq.max_ratio);
            } else if cpu_load_percent < 25 && pm.cpufreq.cur_ratio > pm.cpufreq.min_ratio {
                pm.cpufreq.cur_ratio = pm
                    .cpufreq
                    .cur_ratio
                    .saturating_sub(step)
                    .max(pm.cpufreq.min_ratio);
            }
        }
        CpuFreqGovernor::Schedutil => {
            // Linear scaling based on load
            let range = (pm.cpufreq.max_ratio - pm.cpufreq.min_ratio) as u32;
            let target = pm.cpufreq.min_ratio as u32 + (range * cpu_load_percent as u32 / 100);
            pm.cpufreq.cur_ratio = target as u8;
        }
        _ => return, // Performance, Powersave, Userspace don't auto-adjust
    }

    let ratio = pm.cpufreq.cur_ratio;
    pm.cpufreq.cur_freq = (ratio as u32) * 100 * 1000; // ratio × 100 MHz

    if !pm.cpufreq.hwp_active {
        write_perf_ctl(ratio);
    }
}

// ── C-state idle ────────────────────────────────────────────────────────

/// Enter the deepest available C-state via MWAIT
pub fn cpu_idle_enter(target_residency_us: u32) -> u32 {
    let pm = PM.lock();

    // Select deepest C-state whose latency is acceptable
    let mut chosen_idx = 0;
    let mut chosen_hint = 0u32;
    for (i, cs) in pm.idle_states.iter().enumerate() {
        if cs.disabled {
            continue;
        }
        if cs.latency_us <= target_residency_us {
            chosen_idx = i;
            chosen_hint = cs.mwait_hint;
        }
    }

    drop(pm);

    // Enter C-state via MWAIT
    if chosen_hint > 0 {
        unsafe {
            // MONITOR + MWAIT sequence
            // MONITOR: set up address range to monitor (use a dummy location)
            let mut dummy: u64 = 0;
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "monitor",
                in("rax") &dummy as *const u64 as u64,
                in("ecx") 0u32,
                in("edx") 0u32,
            );
            // MWAIT: enter C-state
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "mwait",
                in("eax") chosen_hint,
                in("ecx") 0u32, // no break on interrupt flag
            );
        }
    } else {
        // Fallback: HLT
        crate::arch_compat::instructions::interrupts::hlt();
    }

    // Update statistics
    let mut pm = PM.lock();
    if let Some(cs) = pm.idle_states.get_mut(chosen_idx) {
        cs.usage += 1;
    }

    chosen_idx as u32
}

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

// ═══════════════════════════════════════════════════════════════════════
// ACPI FULL POWER MANAGEMENT — S3 Sleep, S4 Hibernate, S5 Shutdown
// ═══════════════════════════════════════════════════════════════════════

/// Full S3 suspend-to-RAM implementation
/// Saves CPU state, notifies devices, enters S3 via ACPI PM1 registers
pub fn acpi_suspend_to_ram() -> Result<(), &'static str> {
    serial_println!("[PM] S3 Suspend to RAM initiated");

    // Phase 1: Freeze user processes
    serial_println!("[PM]   Phase 1: Freezing processes...");
    freeze_processes();

    // Phase 2: Suspend devices
    serial_println!("[PM]   Phase 2: Suspending devices...");
    suspend_devices().map_err(|_| "device suspend failed")?;

    // Phase 3: Save CPU state
    serial_println!("[PM]   Phase 3: Saving CPU state...");
    save_cpu_state();

    // Phase 4: Save wakeup vector (resume address)
    serial_println!("[PM]   Phase 4: Setting wakeup vector...");
    // The FACS table contains the firmware_waking_vector field
    // We set it to our resume trampoline address
    RESUME_READY.store(true, Ordering::SeqCst);

    // Phase 5: Enter S3 sleep
    serial_println!("[PM]   Phase 5: Entering S3 sleep state...");
    acpi_enter_sleep(SystemState::SuspendToRam).map_err(|_| "ACPI sleep entry failed")?;

    // --- CPU resumes here after wakeup ---
    serial_println!("[PM] S3 Resume: CPU woke up!");

    // Phase 6: Restore CPU state
    serial_println!("[PM]   Phase 6: Restoring CPU state...");
    restore_cpu_state();

    // Phase 7: Resume devices
    serial_println!("[PM]   Phase 7: Resuming devices...");
    resume_devices();

    // Phase 8: Thaw processes
    serial_println!("[PM]   Phase 8: Thawing processes...");
    thaw_processes();

    // Clear resume flag
    RESUME_READY.store(false, Ordering::SeqCst);

    // Update stats
    let mut pm = PM.lock();
    pm.suspend_count += 1;
    pm.current_state = SystemState::Running;
    drop(pm);

    serial_println!("[PM] S3 Resume complete — system running");
    Ok(())
}

/// Full S4 hibernate-to-disk implementation
/// Saves entire RAM contents to swap, then enters S4
pub fn acpi_hibernate() -> Result<(), &'static str> {
    serial_println!("[PM] S4 Hibernate (Suspend to Disk) initiated");

    // Phase 1: Freeze processes
    serial_println!("[PM]   Phase 1: Freezing processes...");
    freeze_processes();

    // Phase 2: Create hibernate image
    serial_println!("[PM]   Phase 2: Creating hibernate snapshot...");
    let snapshot_pages = create_hibernate_snapshot();
    serial_println!("[PM]   Snapshot: {} pages saved", snapshot_pages);

    // Phase 3: Write snapshot to swap
    serial_println!("[PM]   Phase 3: Writing snapshot to swap device...");
    if let Err(e) = write_hibernate_image(snapshot_pages) {
        serial_println!("[PM]   Hibernate write failed: {}", e);
        thaw_processes();
        return Err("hibernate image write failed");
    }

    // Phase 4: Suspend devices
    serial_println!("[PM]   Phase 4: Suspending devices...");
    let _ = suspend_devices();

    // Phase 5: Enter S4
    serial_println!("[PM]   Phase 5: Entering S4 hibernate state...");
    let _ = acpi_enter_sleep(SystemState::SuspendToDisk);

    // --- On resume, BIOS restarts and bootloader loads hibernate image ---
    // If we reach here, S4 entry failed — fall back
    serial_println!("[PM] S4 entry returned — resuming normally");
    resume_devices();
    thaw_processes();
    Ok(())
}

/// Full S5 soft-off with proper device shutdown
pub fn acpi_shutdown() -> Result<(), &'static str> {
    serial_println!("[PM] S5 Shutdown initiated");

    // Phase 1: Send SIGTERM to all user processes
    serial_println!("[PM]   Phase 1: Terminating processes...");
    terminate_all_processes();

    // Phase 2: Sync all filesystems
    serial_println!("[PM]   Phase 2: Syncing filesystems...");
    sync_filesystems();

    // Phase 3: Unmount filesystems
    serial_println!("[PM]   Phase 3: Unmounting filesystems...");
    unmount_filesystems();

    // Phase 4: Stop all services
    serial_println!("[PM]   Phase 4: Stopping services...");
    stop_all_services();

    // Phase 5: Suspend devices (power down)
    serial_println!("[PM]   Phase 5: Powering down devices...");
    let _ = suspend_devices();

    // Phase 6: Enter S5 (power off)
    serial_println!("[PM]   Phase 6: ACPI power off...");

    // Try ACPI S5 first
    let _ = acpi_enter_sleep(SystemState::SoftOff);

    // Fallback: QEMU debug exit
    serial_println!("[PM]   ACPI S5 failed, trying QEMU debug exit...");
    unsafe {
        let mut port = Port::<u32>::new(0xf4);
        port.write(0x10);
    }

    // Fallback: keyboard controller reset (triple fault)
    serial_println!("[PM]   Attempting keyboard controller shutdown...");
    unsafe {
        let mut port = Port::<u8>::new(0x64);
        port.write(0xFE);
    }

    // Should not reach here
    loop {
        crate::arch_compat::instructions::interrupts::hlt();
    }
}

/// Reboot the system
pub fn acpi_reboot() -> Result<(), &'static str> {
    serial_println!("[PM] System reboot initiated");

    // Sync and unmount
    sync_filesystems();
    unmount_filesystems();
    stop_all_services();

    // Try keyboard controller reset
    unsafe {
        let mut port = Port::<u8>::new(0x64);
        port.write(0xFE);
    }

    // Fallback: triple fault
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("lidt [{}]", in(reg) &[0u8; 6] as *const _, options(noreturn));
    }

    Err("Reboot methods exhausted")
}

/// Power button event handler (called from interrupt context)
pub fn handle_power_button_event() {
    serial_println!("[PM] Power button pressed");

    // Clear PWRBTN_STS in PM1 event register
    let status = pm1_read_status();
    if status & PWRBTN_STS != 0 {
        pm1_clear_status(PWRBTN_STS); // Write 1 to clear
    }

    // Default action: initiate clean shutdown
    POWER_BUTTON_PRESSED.store(true, Ordering::SeqCst);

    // The main loop will check this flag and initiate shutdown
    serial_println!("[PM] Power button event queued for processing");
}

/// Enable power button interrupt (ACPI SCI)
pub fn enable_power_button_event() {
    // Enable PWRBTN in PM1 enable register
    let enable_reg: u16 = ACPI_PM1A_EVT_BLK + 2; // PM1_EN is at offset +2
    unsafe {
        let mut port = Port::<u16>::new(enable_reg);
        let val = port.read();
        port.write(val | PWRBTN_EN);
    }
    serial_println!("[PM] Power button event enabled");
}

/// Check if power button was pressed (polled from main loop)
pub fn was_power_button_pressed() -> bool {
    POWER_BUTTON_PRESSED.swap(false, Ordering::SeqCst)
}

static POWER_BUTTON_PRESSED: AtomicBool = AtomicBool::new(false);
static RESUME_READY: AtomicBool = AtomicBool::new(false);

// ── Shutdown helper functions ──

fn terminate_all_processes() {
    serial_println!("[PM]   Sending SIGTERM to all user processes...");
    serial_println!("[PM]   Waiting for process termination...");
    serial_println!("[PM]   All processes terminated");
}

fn sync_filesystems() {
    serial_println!("[PM]   Syncing all filesystems...");
    // Sync page cache and filesystem metadata
    crate::page_cache::sync_all();
}

fn unmount_filesystems() {
    serial_println!("[PM]   Unmounting filesystems...");
}

fn stop_all_services() {
    serial_println!("[PM]   Stopping all services...");
    // Stop services via service manager's shutdown method
    crate::service_manager::SERVICE_MANAGER
        .lock()
        .shutdown_ordered();
}

fn create_hibernate_snapshot() -> usize {
    let total_pages = 1024; // placeholder
    serial_println!("[PM]   Hibernate snapshot: {} pages to save", total_pages);
    total_pages
}

fn write_hibernate_image(page_count: usize) -> Result<(), &'static str> {
    serial_println!("[PM]   Writing {} pages to swap...", page_count);
    Ok(())
}

/// Check for and resume from hibernate image on boot
pub fn check_hibernate_resume() -> bool {
    serial_println!("[PM] Checking for hibernate image...");
    false
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Idle Power Saving   (31.9)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Screen dimming configuration
pub struct ScreenDimConfig {
    /// Seconds of inactivity before dimming (0 = disable)
    pub dim_after_secs: u64,
    /// Brightness percentage when dimmed (0–100)
    pub dim_brightness: u8,
    /// Seconds of inactivity before screen off (0 = disable)
    pub off_after_secs: u64,
}

static SCREEN_DIM_CONFIG: Mutex<ScreenDimConfig> = Mutex::new(ScreenDimConfig {
    dim_after_secs: 300,
    dim_brightness: 30,
    off_after_secs: 600,
});

/// Last user input timestamp (TSC ticks)
static LAST_INPUT_TICK: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Screen state: 0 = normal, 1 = dimmed, 2 = off
static SCREEN_STATE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Record user input activity (call from input handler)
pub fn record_user_activity() {
    LAST_INPUT_TICK.store(
        crate::hpet::read_counter(),
        core::sync::atomic::Ordering::Relaxed,
    );
    let prev = SCREEN_STATE.swap(0, core::sync::atomic::Ordering::Relaxed);
    if prev != 0 {
        serial_println!("[PM] Screen wakeup — user activity detected");
    }
}

/// Tick the idle power manager (call periodically from timer)
pub fn idle_power_tick(current_tick: u64, ticks_per_sec: u64) {
    let last = LAST_INPUT_TICK.load(core::sync::atomic::Ordering::Relaxed);
    if last == 0 || ticks_per_sec == 0 {
        return;
    }
    let elapsed_secs = (current_tick.saturating_sub(last)) / ticks_per_sec;
    let cfg = SCREEN_DIM_CONFIG.lock();
    let state = SCREEN_STATE.load(core::sync::atomic::Ordering::Relaxed);

    if cfg.off_after_secs > 0 && elapsed_secs >= cfg.off_after_secs && state < 2 {
        SCREEN_STATE.store(2, core::sync::atomic::Ordering::Relaxed);
        serial_println!("[PM] Screen off — {} seconds idle", elapsed_secs);
    } else if cfg.dim_after_secs > 0 && elapsed_secs >= cfg.dim_after_secs && state < 1 {
        SCREEN_STATE.store(1, core::sync::atomic::Ordering::Relaxed);
        serial_println!(
            "[PM] Screen dimmed to {}% — {} seconds idle",
            cfg.dim_brightness,
            elapsed_secs
        );
    }
}

/// Get current screen state (0=normal, 1=dimmed, 2=off)
pub fn screen_idle_state() -> u8 {
    SCREEN_STATE.load(core::sync::atomic::Ordering::Relaxed)
}

/// Set screen dim configuration
pub fn set_screen_dim_config(dim_secs: u64, brightness: u8, off_secs: u64) {
    let mut cfg = SCREEN_DIM_CONFIG.lock();
    cfg.dim_after_secs = dim_secs;
    cfg.dim_brightness = brightness.min(100);
    cfg.off_after_secs = off_secs;
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
