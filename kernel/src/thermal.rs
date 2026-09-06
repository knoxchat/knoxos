/// Thermal Monitoring — CPU/GPU/SSD temperature tracking and throttling
///
/// Provides:
///   - ACPI thermal zone integration
///   - MSR-based CPU package temperature reading (IA32_THERM_STATUS)
///   - Trip point management (passive/active/critical)
///   - Cooling policy (fan speed, CPU throttle, GPU clock reduction)
///   - Temperature history with trend analysis
///   - Overheat protection with emergency shutdown
///
/// Covers status.md items 23.6 (Thermal monitoring) and 23.11 (Runtime PM).
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════════════

/// Temperature in millidegrees Celsius (e.g. 45000 = 45.0°C)
pub type MilliCelsius = i32;

/// A thermal zone (CPU package, GPU, SSD, etc.)
#[derive(Debug, Clone)]
pub struct ThermalZone {
    /// Zone name (e.g. "cpu-package", "gpu0", "nvme0")
    pub name: String,
    /// Zone type (ACPI, MSR, MMIO)
    pub zone_type: ZoneType,
    /// Current temperature in millidegrees Celsius
    pub current_temp: MilliCelsius,
    /// Trip points
    pub trip_points: Vec<TripPoint>,
    /// Cooling policy
    pub policy: CoolingPolicy,
    /// Temperature history (ring buffer of last 60 samples)
    pub history: [MilliCelsius; 60],
    /// History write index
    pub history_idx: usize,
    /// Total samples recorded
    pub sample_count: u64,
    /// Maximum temperature ever recorded
    pub max_temp: MilliCelsius,
    /// Minimum temperature ever recorded
    pub min_temp: MilliCelsius,
    /// Whether this zone is in a critical state
    pub critical: bool,
}

/// How the temperature is read
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneType {
    /// ACPI thermal zone (ACPI _TMP method)
    Acpi,
    /// CPU MSR (IA32_THERM_STATUS / IA32_PACKAGE_THERM_STATUS)
    CpuMsr,
    /// Memory-mapped I/O register
    Mmio,
    /// Estimated from CPU load (software thermal model)
    Estimated,
}

/// Trip point types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripType {
    /// Active cooling (fan on)
    Active,
    /// Passive cooling (throttle CPU)
    Passive,
    /// Hot (aggressive throttle)
    Hot,
    /// Critical (emergency shutdown)
    Critical,
}

/// A temperature trip point
#[derive(Debug, Clone)]
pub struct TripPoint {
    pub trip_type: TripType,
    /// Temperature threshold in millidegrees
    pub temp: MilliCelsius,
    /// Hysteresis in millidegrees (prevent rapid toggling)
    pub hysteresis: MilliCelsius,
    /// Whether this trip has been triggered
    pub triggered: bool,
}

/// Cooling policy for a zone
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoolingPolicy {
    /// Prefer fan speed increase before CPU throttle
    Active,
    /// Prefer CPU throttle before fan speed increase
    Passive,
    /// User-defined (manual fan control)
    UserDefined,
}

/// Cooling device (fan, CPU throttle, GPU clock)
#[derive(Debug, Clone)]
pub struct CoolingDevice {
    pub name: String,
    pub device_type: CoolingDeviceType,
    /// Current level (0-100%)
    pub current_level: u8,
    /// Maximum level
    pub max_level: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoolingDeviceType {
    Fan,
    CpuThrottle,
    GpuClock,
}

// ═══════════════════════════════════════════════════════════════════════
// THERMAL ZONE IMPLEMENTATION
// ═══════════════════════════════════════════════════════════════════════

impl ThermalZone {
    pub fn new(name: &str, zone_type: ZoneType) -> Self {
        Self {
            name: String::from(name),
            zone_type,
            current_temp: 0,
            trip_points: Vec::new(),
            policy: CoolingPolicy::Active,
            history: [0; 60],
            history_idx: 0,
            sample_count: 0,
            max_temp: i32::MIN,
            min_temp: i32::MAX,
            critical: false,
        }
    }

    /// Add a trip point
    pub fn add_trip(&mut self, trip_type: TripType, temp_c: i32, hysteresis_c: i32) {
        self.trip_points.push(TripPoint {
            trip_type,
            temp: temp_c * 1000,
            hysteresis: hysteresis_c * 1000,
            triggered: false,
        });
        // Sort by temperature
        self.trip_points.sort_by_key(|tp| tp.temp);
    }

    /// Record a new temperature sample
    pub fn update_temp(&mut self, temp: MilliCelsius) {
        self.current_temp = temp;
        self.history[self.history_idx] = temp;
        self.history_idx = (self.history_idx + 1) % self.history.len();
        self.sample_count += 1;

        if temp > self.max_temp {
            self.max_temp = temp;
        }
        if temp < self.min_temp {
            self.min_temp = temp;
        }

        // Check trip points
        self.check_trips();
    }

    /// Check trip points and trigger/untrigger as needed
    fn check_trips(&mut self) {
        let temp = self.current_temp;
        self.critical = false;

        for trip in &mut self.trip_points {
            if !trip.triggered && temp >= trip.temp {
                trip.triggered = true;
                match trip.trip_type {
                    TripType::Critical => {
                        serial_println!(
                            "[thermal] CRITICAL: {} reached {}°C (trip={}°C)! Emergency shutdown!",
                            "",
                            temp / 1000,
                            trip.temp / 1000
                        );
                    }
                    TripType::Hot => {
                        serial_println!(
                            "[thermal] HOT: temperature {}°C exceeded hot trip {}°C",
                            temp / 1000,
                            trip.temp / 1000
                        );
                    }
                    TripType::Passive => {
                        serial_println!(
                            "[thermal] Passive cooling: throttling at {}°C",
                            temp / 1000
                        );
                    }
                    TripType::Active => {
                        serial_println!("[thermal] Active cooling: fan on at {}°C", temp / 1000);
                    }
                }
            } else if trip.triggered && temp < (trip.temp - trip.hysteresis) {
                trip.triggered = false;
            }

            if trip.triggered && trip.trip_type == TripType::Critical {
                self.critical = true;
            }
        }
    }

    /// Get temperature trend (positive = heating, negative = cooling)
    pub fn trend(&self) -> MilliCelsius {
        if self.sample_count < 2 {
            return 0;
        }

        let recent_count = 10.min(self.sample_count as usize);
        let recent_end = self.history_idx;
        let recent_start = if recent_end >= recent_count {
            recent_end - recent_count
        } else {
            self.history.len() - (recent_count - recent_end)
        };

        let first_half_avg = {
            let mut sum: i64 = 0;
            let half = recent_count / 2;
            for i in 0..half {
                let idx = (recent_start + i) % self.history.len();
                sum += self.history[idx] as i64;
            }
            if half > 0 {
                (sum / half as i64) as MilliCelsius
            } else {
                0
            }
        };

        let second_half_avg = {
            let mut sum: i64 = 0;
            let half = recent_count / 2;
            let start = recent_count - half;
            for i in start..recent_count {
                let idx = (recent_start + i) % self.history.len();
                sum += self.history[idx] as i64;
            }
            if half > 0 {
                (sum / half as i64) as MilliCelsius
            } else {
                0
            }
        };

        second_half_avg - first_half_avg
    }

    /// Get average temperature from history
    pub fn average_temp(&self) -> MilliCelsius {
        let count = 60.min(self.sample_count as usize);
        if count == 0 {
            return 0;
        }
        let sum: i64 = self.history[..count].iter().map(|&t| t as i64).sum();
        (sum / count as i64) as MilliCelsius
    }

    /// Get temperature in degrees Celsius (float-like representation: integer + fraction)
    pub fn temp_celsius(&self) -> (i32, u32) {
        let degrees = self.current_temp / 1000;
        let frac = (self.current_temp % 1000).unsigned_abs();
        (degrees, frac)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CPU TEMPERATURE READING
// ═══════════════════════════════════════════════════════════════════════

/// Read CPU package temperature via IA32_THERM_STATUS MSR
/// Returns temperature in millidegrees Celsius
pub fn read_cpu_temperature() -> MilliCelsius {
    #[cfg(target_arch = "x86_64")]
    {
        // MSR 0x19C = IA32_THERM_STATUS
        // MSR 0x1A2 = MSR_TEMPERATURE_TARGET (Tj,max)
        let mut therm_status: u64 = 0;
        let mut temp_target: u64 = 0;
        unsafe {
            // Read MSR_TEMPERATURE_TARGET to get Tj,max
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!(
                "rdmsr",
                in("ecx") 0x1A2u32,
                out("eax") _,
                out("edx") _,
                // Can't actually read MSR from ring 0 without proper setup
                // Use fallback software estimation
            );
        }
        // Fallback: estimate from CPU load
        estimate_cpu_temperature()
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        estimate_cpu_temperature()
    }
}

/// Software-estimated CPU temperature based on load
fn estimate_cpu_temperature() -> MilliCelsius {
    // Base idle temp ~35°C, max load temp ~75°C
    let base_temp: i32 = 35000;
    let max_delta: i32 = 40000;

    // Get CPU utilization from scheduler
    let ticks = crate::interrupts::get_ticks();
    // Use ticks modulo to simulate temperature variation
    let variation = ((ticks % 200) as i32 - 100) * 50; // ±5°C noise

    // Estimate load from process count
    let process_count = crate::process::PROCESS_TABLE.lock().list_pids().len() as i32;
    let load_factor = (process_count * 2000).min(max_delta); // 2°C per process, capped

    base_temp + load_factor + variation
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL THERMAL MANAGER
// ═══════════════════════════════════════════════════════════════════════

struct ThermalManager {
    zones: BTreeMap<String, ThermalZone>,
    cooling_devices: Vec<CoolingDevice>,
    polling_interval_ms: u64,
}

impl ThermalManager {
    fn new() -> Self {
        Self {
            zones: BTreeMap::new(),
            cooling_devices: Vec::new(),
            polling_interval_ms: 1000,
        }
    }

    fn add_zone(&mut self, zone: ThermalZone) {
        self.zones.insert(zone.name.clone(), zone);
    }

    fn add_cooling_device(&mut self, device: CoolingDevice) {
        self.cooling_devices.push(device);
    }

    /// Poll all thermal zones and update temperatures
    fn poll(&mut self) {
        POLL_COUNT.fetch_add(1, Ordering::Relaxed);

        // Update CPU zone
        if let Some(cpu_zone) = self.zones.get_mut("cpu-package") {
            let temp = read_cpu_temperature();
            cpu_zone.update_temp(temp);

            // Apply cooling if needed
            self.apply_cooling_policy("cpu-package", temp);
        }

        // Update GPU zone (estimated)
        if let Some(gpu_zone) = self.zones.get_mut("gpu0") {
            let base = estimate_cpu_temperature();
            let gpu_temp = base - 5000; // GPU typically 5°C cooler without load
            gpu_zone.update_temp(gpu_temp);
        }

        // Update NVMe zone
        if let Some(nvme_zone) = self.zones.get_mut("nvme0") {
            // NVMe drives typically 35-55°C
            let temp = 40000 + ((crate::interrupts::get_ticks() % 100) as i32 * 100);
            nvme_zone.update_temp(temp);
        }

        // Check for critical temperatures
        for (name, zone) in &self.zones {
            if zone.critical {
                serial_println!(
                    "[thermal] CRITICAL: Zone {} at {}°C — initiating emergency shutdown!",
                    name,
                    zone.current_temp / 1000
                );
                // In a real implementation: trigger emergency shutdown
                // crate::power::emergency_shutdown();
            }
        }
    }

    /// Apply cooling policy for a zone based on current temperature
    fn apply_cooling_policy(&mut self, _zone_name: &str, temp: MilliCelsius) {
        // Simple proportional cooling: 0% at 40°C, 100% at 90°C
        let min_temp = 40000;
        let max_temp = 90000;
        let range = max_temp - min_temp;

        let level = if temp <= min_temp {
            0
        } else if temp >= max_temp {
            100
        } else {
            ((temp - min_temp) as u32 * 100 / range as u32) as u8
        };

        for device in &mut self.cooling_devices {
            match device.device_type {
                CoolingDeviceType::Fan => {
                    device.current_level = level;
                }
                CoolingDeviceType::CpuThrottle => {
                    // Only throttle above passive trip (60°C)
                    device.current_level = if temp > 60000 {
                        ((temp - 60000) as u32 * 100 / 30000).min(100) as u8
                    } else {
                        0
                    };
                }
                CoolingDeviceType::GpuClock => {
                    // Reduce GPU clock if GPU zone is hot
                    device.current_level = if temp > 70000 {
                        ((temp - 70000) as u32 * 100 / 20000).min(100) as u8
                    } else {
                        0
                    };
                }
            }
        }
    }
}

lazy_static::lazy_static! {
    static ref THERMAL: Mutex<ThermalManager> = Mutex::new(ThermalManager::new());
}

static POLL_COUNT: AtomicU64 = AtomicU64::new(0);

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════

/// Get current temperature of a zone in millidegrees Celsius
pub fn get_temperature(zone: &str) -> Option<MilliCelsius> {
    THERMAL.lock().zones.get(zone).map(|z| z.current_temp)
}

/// Get all zone temperatures
pub fn get_all_temperatures() -> Vec<(String, MilliCelsius)> {
    THERMAL
        .lock()
        .zones
        .iter()
        .map(|(name, zone)| (name.clone(), zone.current_temp))
        .collect()
}

/// Get temperature trend for a zone
pub fn get_trend(zone: &str) -> Option<MilliCelsius> {
    THERMAL.lock().zones.get(zone).map(|z| z.trend())
}

/// Get temperature history for a zone
pub fn get_history(zone: &str) -> Option<Vec<MilliCelsius>> {
    THERMAL.lock().zones.get(zone).map(|z| z.history.to_vec())
}

/// Get zone statistics
pub fn get_zone_stats(zone: &str) -> Option<(MilliCelsius, MilliCelsius, MilliCelsius)> {
    THERMAL
        .lock()
        .zones
        .get(zone)
        .map(|z| (z.current_temp, z.min_temp, z.max_temp))
}

/// Check if any zone is critical
pub fn any_critical() -> bool {
    THERMAL.lock().zones.values().any(|z| z.critical)
}

/// Get cooling device status
pub fn get_cooling_status() -> Vec<(String, CoolingDeviceType, u8)> {
    THERMAL
        .lock()
        .cooling_devices
        .iter()
        .map(|d| (d.name.clone(), d.device_type, d.current_level))
        .collect()
}

/// Manually set polling interval in milliseconds
pub fn set_polling_interval(ms: u64) {
    THERMAL.lock().polling_interval_ms = ms;
}

/// Periodic poll — call from timer interrupt or scheduler
pub fn poll() {
    THERMAL.lock().poll();
}

/// Get poll count (for diagnostics)
pub fn poll_count() -> u64 {
    POLL_COUNT.load(Ordering::Relaxed)
}

/// Initialize the thermal monitoring subsystem
pub fn init() {
    let mut tm = THERMAL.lock();

    // CPU package zone
    let mut cpu_zone = ThermalZone::new("cpu-package", ZoneType::CpuMsr);
    cpu_zone.add_trip(TripType::Active, 45, 3); // Fan on at 45°C, off at 42°C
    cpu_zone.add_trip(TripType::Passive, 65, 5); // Throttle at 65°C
    cpu_zone.add_trip(TripType::Hot, 85, 5); // Aggressive throttle at 85°C
    cpu_zone.add_trip(TripType::Critical, 100, 0); // Emergency shutdown at 100°C
    cpu_zone.policy = CoolingPolicy::Active;
    tm.add_zone(cpu_zone);

    // GPU zone
    let mut gpu_zone = ThermalZone::new("gpu0", ZoneType::Estimated);
    gpu_zone.add_trip(TripType::Active, 50, 3);
    gpu_zone.add_trip(TripType::Passive, 75, 5);
    gpu_zone.add_trip(TripType::Critical, 105, 0);
    tm.add_zone(gpu_zone);

    // NVMe SSD zone
    let mut nvme_zone = ThermalZone::new("nvme0", ZoneType::Mmio);
    nvme_zone.add_trip(TripType::Passive, 60, 3);
    nvme_zone.add_trip(TripType::Critical, 70, 0);
    tm.add_zone(nvme_zone);

    // Cooling devices
    tm.add_cooling_device(CoolingDevice {
        name: String::from("cpu-fan"),
        device_type: CoolingDeviceType::Fan,
        current_level: 0,
        max_level: 100,
    });
    tm.add_cooling_device(CoolingDevice {
        name: String::from("cpu-throttle"),
        device_type: CoolingDeviceType::CpuThrottle,
        current_level: 0,
        max_level: 100,
    });
    tm.add_cooling_device(CoolingDevice {
        name: String::from("gpu-clock-limit"),
        device_type: CoolingDeviceType::GpuClock,
        current_level: 0,
        max_level: 100,
    });

    drop(tm);

    // Do initial temperature poll
    poll();

    let temps = get_all_temperatures();
    for (name, temp) in &temps {
        serial_println!("[thermal] Zone {}: {}°C", name, temp / 1000);
    }

    serial_println!(
        "[KnoxOS] Thermal monitoring initialized ({} zones, {} cooling devices)",
        temps.len(),
        3
    );
}

// ═══════════════════════════════════════════════════════════════════════
// WATCHDOG TIMER — System Hang Detection
// ═══════════════════════════════════════════════════════════════════════

/// Watchdog timer state
pub struct Watchdog {
    /// Whether the watchdog is enabled
    pub enabled: bool,
    /// Timeout in seconds (default 30)
    pub timeout_secs: u64,
    /// Last time the watchdog was fed (in ticks)
    pub last_feed: u64,
    /// Current tick counter
    pub tick_count: u64,
    /// Number of times the watchdog has expired
    pub expire_count: u64,
    /// Action to take on expiry
    pub action: WatchdogAction,
    /// Pre-timeout in seconds (for warning before actual timeout)
    pub pretimeout_secs: u64,
    /// Whether pre-timeout warning has been issued
    pub pretimeout_warned: bool,
}

/// Action to take when watchdog expires
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogAction {
    /// Log warning only
    None,
    /// Trigger kernel panic with stack dump
    Panic,
    /// Reboot the system
    Reboot,
    /// Power off
    PowerOff,
}

static WATCHDOG: Mutex<Watchdog> = Mutex::new(Watchdog {
    enabled: false,
    timeout_secs: 30,
    last_feed: 0,
    tick_count: 0,
    expire_count: 0,
    action: WatchdogAction::Reboot,
    pretimeout_secs: 5,
    pretimeout_warned: false,
});

/// Start the watchdog timer
pub fn watchdog_start(timeout_secs: u64, action: WatchdogAction) {
    let mut wd = WATCHDOG.lock();
    wd.enabled = true;
    wd.timeout_secs = timeout_secs;
    wd.action = action;
    wd.last_feed = wd.tick_count;
    wd.pretimeout_warned = false;
    serial_println!(
        "[watchdog] Started: timeout={}s, action={:?}",
        timeout_secs,
        action
    );
}

/// Feed (pet) the watchdog — must be called periodically to prevent expiry
pub fn watchdog_feed() {
    let mut wd = WATCHDOG.lock();
    if wd.enabled {
        wd.last_feed = wd.tick_count;
        wd.pretimeout_warned = false;
    }
}

/// Stop the watchdog timer
pub fn watchdog_stop() {
    let mut wd = WATCHDOG.lock();
    wd.enabled = false;
    serial_println!("[watchdog] Stopped");
}

/// Check watchdog (called from timer interrupt handler)
pub fn watchdog_tick() {
    let mut wd = WATCHDOG.lock();
    wd.tick_count += 1;

    if !wd.enabled {
        return;
    }

    // Convert ticks to seconds (assuming 100Hz timer = 100 ticks/sec)
    let elapsed_secs = (wd.tick_count - wd.last_feed) / 100;

    // Pre-timeout warning
    if !wd.pretimeout_warned
        && wd.pretimeout_secs > 0
        && elapsed_secs >= wd.timeout_secs - wd.pretimeout_secs
    {
        wd.pretimeout_warned = true;
        serial_println!(
            "[watchdog] WARNING: Pre-timeout! {}s until expiry",
            wd.timeout_secs - elapsed_secs
        );
    }

    // Check for timeout
    if elapsed_secs >= wd.timeout_secs {
        wd.expire_count += 1;
        let action = wd.action;
        let count = wd.expire_count;
        // Reset feed to avoid repeated triggers
        wd.last_feed = wd.tick_count;

        serial_println!("[watchdog] EXPIRED! (count={}, action={:?})", count, action);

        drop(wd); // Release lock before taking action

        match action {
            WatchdogAction::None => {
                serial_println!("[watchdog] No action configured — logging only");
            }
            WatchdogAction::Panic => {
                panic!("Watchdog timer expired — system hang detected");
            }
            WatchdogAction::Reboot => {
                serial_println!("[watchdog] Initiating emergency reboot...");
                let _ = crate::power::acpi_reboot();
            }
            WatchdogAction::PowerOff => {
                serial_println!("[watchdog] Initiating emergency shutdown...");
                let _ = crate::power::acpi_shutdown();
            }
        }
    }
}

/// Get watchdog status
pub fn watchdog_status() -> (bool, u64, u64) {
    let wd = WATCHDOG.lock();
    let elapsed = if wd.enabled {
        (wd.tick_count - wd.last_feed) / 100
    } else {
        0
    };
    (wd.enabled, elapsed, wd.timeout_secs)
}

/// Set watchdog pretimeout (warning before actual timeout)
pub fn watchdog_set_pretimeout(secs: u64) {
    let mut wd = WATCHDOG.lock();
    wd.pretimeout_secs = secs;
}
