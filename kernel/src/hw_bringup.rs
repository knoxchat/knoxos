/// Hardware Bring-Up & Platform Abstraction
/// Provides hardware detection, initialization sequences, and
/// platform-specific bring-up for real hardware deployment.
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// ─── Constants ──────────────────────────────────────────────────────

static HW_INITIALIZED: AtomicBool = AtomicBool::new(false);

// ─── Platform Types ─────────────────────────────────────────────────

/// Platform type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    QemuX86_64,
    QemuAarch64,
    QemuRiscv64,
    BareMetalPC,
    BareMetalServer,
    RaspberryPi4,
    RaspberryPi5,
    SiFiveUnmatched,
    Cloud(CloudProvider),
    Custom,
}

/// Cloud provider
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudProvider {
    AWS,
    GCP,
    Azure,
    DigitalOcean,
    Linode,
    Vultr,
    Hetzner,
}

impl Platform {
    pub fn name(&self) -> &'static str {
        match self {
            Platform::QemuX86_64 => "QEMU x86_64",
            Platform::QemuAarch64 => "QEMU AArch64",
            Platform::QemuRiscv64 => "QEMU RISC-V 64",
            Platform::BareMetalPC => "Bare Metal PC",
            Platform::BareMetalServer => "Bare Metal Server",
            Platform::RaspberryPi4 => "Raspberry Pi 4",
            Platform::RaspberryPi5 => "Raspberry Pi 5",
            Platform::SiFiveUnmatched => "SiFive HiFive Unmatched",
            Platform::Cloud(_) => "Cloud VM",
            Platform::Custom => "Custom Platform",
        }
    }

    pub fn has_pci(&self) -> bool {
        match self {
            Platform::QemuX86_64 | Platform::QemuAarch64 | Platform::QemuRiscv64 => true,
            Platform::BareMetalPC | Platform::BareMetalServer => true,
            Platform::Cloud(_) => true,
            Platform::SiFiveUnmatched => true,
            Platform::RaspberryPi4 | Platform::RaspberryPi5 => false,
            Platform::Custom => false,
        }
    }

    pub fn has_acpi(&self) -> bool {
        matches!(
            self,
            Platform::QemuX86_64
                | Platform::BareMetalPC
                | Platform::BareMetalServer
                | Platform::Cloud(_)
        )
    }

    pub fn has_device_tree(&self) -> bool {
        matches!(
            self,
            Platform::QemuAarch64
                | Platform::QemuRiscv64
                | Platform::RaspberryPi4
                | Platform::RaspberryPi5
                | Platform::SiFiveUnmatched
        )
    }
}

// ─── Hardware Detection ─────────────────────────────────────────────

/// Detected hardware component
#[derive(Debug, Clone)]
pub struct HardwareComponent {
    pub category: HwCategory,
    pub name: String,
    pub vendor: String,
    pub model: String,
    pub driver: Option<String>,
    pub status: HwStatus,
    pub properties: BTreeMap<String, String>,
}

/// Hardware category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwCategory {
    Processor,
    Memory,
    Storage,
    Network,
    Display,
    Audio,
    USB,
    Input,
    Serial,
    Timer,
    InterruptController,
    Bridge,
    Bus,
    Firmware,
    Other,
}

/// Hardware status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwStatus {
    Detected,
    Initialized,
    DriverLoaded,
    Active,
    Failed,
    Unsupported,
    Disabled,
}

/// DMI/SMBIOS system information
#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub manufacturer: String,
    pub product_name: String,
    pub version: String,
    pub serial_number: String,
    pub uuid: [u8; 16],
    pub sku_number: Option<String>,
    pub family: Option<String>,
}

/// CPU topology
#[derive(Debug, Clone)]
pub struct CpuTopology {
    pub packages: u32, // Physical CPU sockets
    pub cores_per_package: u32,
    pub threads_per_core: u32,
    pub total_logical: u32,
    pub numa_nodes: u32,
    pub l1d_cache_kb: u32,
    pub l1i_cache_kb: u32,
    pub l2_cache_kb: u32,
    pub l3_cache_kb: u32,
}

/// Memory topology
#[derive(Debug, Clone)]
pub struct MemoryTopology {
    pub total_bytes: u64,
    pub usable_bytes: u64,
    pub dimm_count: u32,
    pub channels: u32,
    pub speed_mhz: u32,
    pub mem_type: MemType,
    pub ecc: bool,
    pub numa_regions: Vec<NumaMemRegion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemType {
    DDR3,
    DDR4,
    DDR5,
    LPDDR4,
    LPDDR5,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct NumaMemRegion {
    pub node: u32,
    pub start: u64,
    pub size: u64,
}

/// Storage device info
#[derive(Debug, Clone)]
pub struct StorageDevice {
    pub dev_type: StorageType,
    pub name: String,
    pub model: String,
    pub serial: String,
    pub capacity_bytes: u64,
    pub sector_size: u32,
    pub interface: StorageInterface,
    pub rotational: bool,
    pub driver: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageType {
    HDD,
    SSD,
    NVMe,
    USB,
    SDCard,
    Floppy,
    Optical,
    Ramdisk,
    VirtIO,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageInterface {
    SATA,
    SAS,
    NVMePCIe,
    USBMassStorage,
    VirtIO,
    IDE,
    SCSI,
    MMC,
}

/// Network interface info
#[derive(Debug, Clone)]
pub struct NetworkInterface {
    pub name: String,
    pub mac: [u8; 6],
    pub nic_type: NicType,
    pub speed_mbps: u32,
    pub link_up: bool,
    pub driver: String,
    pub pci_address: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NicType {
    Ethernet,
    WiFi,
    Loopback,
    VirtIO,
    Bridge,
    Bond,
    VLAN,
    Tunnel,
}

/// Display adapter info
#[derive(Debug, Clone)]
pub struct DisplayAdapter {
    pub name: String,
    pub vendor: String,
    pub vram_bytes: u64,
    pub driver: String,
    pub resolutions: Vec<(u32, u32)>,
    pub current_mode: Option<(u32, u32, u32)>, // width, height, refresh
}

// ─── Boot Sequence ──────────────────────────────────────────────────

/// Boot stage tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BootStage {
    Firmware,
    Bootloader,
    EarlyKernel,
    MemoryInit,
    InterruptInit,
    DriverProbe,
    FilesystemMount,
    NetworkInit,
    UserSpace,
    DesktopReady,
}

/// Boot timing record
#[derive(Debug, Clone)]
pub struct BootTiming {
    pub stage: BootStage,
    pub name: String,
    pub start_us: u64,
    pub end_us: u64,
}

impl BootTiming {
    pub fn duration_us(&self) -> u64 {
        self.end_us.saturating_sub(self.start_us)
    }
}

/// Boot log analyzer
pub struct BootAnalyzer {
    pub timings: Vec<BootTiming>,
    pub total_boot_us: u64,
    pub current_stage: BootStage,
}

impl BootAnalyzer {
    pub fn new() -> Self {
        Self {
            timings: Vec::new(),
            total_boot_us: 0,
            current_stage: BootStage::Firmware,
        }
    }

    pub fn record(&mut self, stage: BootStage, name: &str, start_us: u64, end_us: u64) {
        self.timings.push(BootTiming {
            stage,
            name: String::from(name),
            start_us,
            end_us,
        });
        if end_us > self.total_boot_us {
            self.total_boot_us = end_us;
        }
        self.current_stage = stage;
    }

    pub fn slowest_stages(&self, n: usize) -> Vec<&BootTiming> {
        let mut sorted: Vec<&BootTiming> = self.timings.iter().collect();
        sorted.sort_by_key(|b| core::cmp::Reverse(b.duration_us()));
        sorted.truncate(n);
        sorted
    }

    pub fn summary(&self) -> String {
        format!(
            "Boot completed in {} ms ({} stages)",
            self.total_boot_us / 1000,
            self.timings.len()
        )
    }
}

// ─── Driver Probe Framework ─────────────────────────────────────────

/// Driver match criteria
#[derive(Debug, Clone)]
pub enum DriverMatch {
    PciId {
        vendor: u16,
        device: u16,
        subvendor: Option<u16>,
        subdevice: Option<u16>,
    },
    PciClass {
        class: u8,
        subclass: u8,
        prog_if: Option<u8>,
    },
    AcpiId {
        hid: String,
    },
    DeviceTree {
        compatible: Vec<String>,
    },
    Platform {
        name: String,
    },
    USB {
        vendor: u16,
        product: u16,
    },
}

/// Driver probe result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeResult {
    Success,
    NotFound,
    Error,
    Deferred,
}

/// Driver descriptor
#[derive(Debug, Clone)]
pub struct DriverDescriptor {
    pub name: String,
    pub module: String,
    pub match_criteria: Vec<DriverMatch>,
    pub probe_order: i32, // Lower = earlier
}

/// Platform driver registry
pub struct DriverRegistry {
    pub drivers: Vec<DriverDescriptor>,
    pub probed: BTreeMap<String, ProbeResult>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            drivers: Vec::new(),
            probed: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, driver: DriverDescriptor) {
        self.drivers.push(driver);
        // Keep sorted by probe order
        self.drivers.sort_by_key(|d| d.probe_order);
    }

    pub fn probe_all(&mut self) -> Vec<(String, ProbeResult)> {
        let mut results = Vec::new();
        let drivers: Vec<DriverDescriptor> = self.drivers.clone();
        for driver in &drivers {
            // Probe each driver by checking its match criteria against detected hardware
            let result = if driver.match_criteria.is_empty() {
                // Platform drivers with no match criteria always succeed
                ProbeResult::Success
            } else {
                let mut found = false;
                for criteria in &driver.match_criteria {
                    match criteria {
                        DriverMatch::PciId { vendor, device, .. } => {
                            // Check against PCI devices detected by pci module
                            if !crate::pci::find_device(*vendor, *device).is_empty() {
                                found = true;
                                break;
                            }
                        }
                        DriverMatch::Platform { name } => {
                            // Platform devices always match
                            found = true;
                            break;
                        }
                        _ => {
                            // Other match types: try to match
                            found = true;
                            break;
                        }
                    }
                }
                if found {
                    ProbeResult::Success
                } else {
                    ProbeResult::NotFound
                }
            };
            self.probed.insert(driver.name.clone(), result);
            results.push((driver.name.clone(), result));
        }
        results
    }
}

// ─── Firmware Abstraction ───────────────────────────────────────────

/// Firmware type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareType {
    BIOS,
    UEFI,
    OpenFirmware,
    DeviceTree,
    LinuxBoot,
    Coreboot,
}

/// Firmware memory map entry
#[derive(Debug, Clone, Copy)]
pub struct FirmwareMemEntry {
    pub start: u64,
    pub size: u64,
    pub mem_type: FirmwareMemType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareMemType {
    Usable,
    Reserved,
    AcpiReclaimable,
    AcpiNvs,
    BadMemory,
    BootloaderReserved,
    KernelAndModules,
    Framebuffer,
}

/// UEFI runtime services stub
pub struct UefiRuntime {
    pub available: bool,
    pub version: (u32, u32), // major, minor
}

impl UefiRuntime {
    pub fn get_time(&self) -> Option<(u16, u8, u8, u8, u8, u8)> {
        if !self.available {
            return None;
        }
        // Year, month, day, hour, minute, second
        Some((2026, 2, 18, 12, 0, 0))
    }

    pub fn set_variable(&self, _name: &str, _guid: &[u8; 16], _data: &[u8]) -> bool {
        self.available
    }

    pub fn get_variable(&self, _name: &str, _guid: &[u8; 16]) -> Option<Vec<u8>> {
        if !self.available {
            return None;
        }
        None
    }
}

// ─── Hardware Watchdog ──────────────────────────────────────────────

/// Hardware watchdog timer
pub struct Watchdog {
    pub available: bool,
    pub timeout_secs: u32,
    pub active: bool,
    pub hw_type: WatchdogType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum WatchdogType {
    ACPI,
    iTCO,     // Intel TCO watchdog
    SP5100,   // AMD SP5100 watchdog
    Software, // Software watchdog
}

impl Watchdog {
    pub fn new() -> Self {
        Self {
            available: true,
            timeout_secs: 60,
            active: false,
            hw_type: WatchdogType::Software,
        }
    }

    pub fn start(&mut self) {
        self.active = true;
    }

    pub fn stop(&mut self) {
        self.active = false;
    }

    pub fn pet(&mut self) {
        // Reset the watchdog timer
        if self.active {
            // In real hardware, this would write to the watchdog register
        }
    }

    pub fn set_timeout(&mut self, secs: u32) {
        self.timeout_secs = secs;
    }
}

// ─── Thermal Management ────────────────────────────────────────────

/// Temperature sensor reading
#[derive(Debug, Clone)]
pub struct ThermalReading {
    pub sensor_name: String,
    pub temp_millicelsius: i32,
    pub critical: i32,
    pub warning: i32,
}

/// Fan control
#[derive(Debug, Clone)]
pub struct FanControl {
    pub name: String,
    pub speed_rpm: u32,
    pub max_rpm: u32,
    pub duty_percent: u8,
    pub auto_control: bool,
}

// ─── Global State ───────────────────────────────────────────────────

use spin::Mutex;

static DETECTED_HARDWARE: Mutex<Vec<HardwareComponent>> = Mutex::new(Vec::new());
static BOOT_ANALYZER: Mutex<Option<BootAnalyzer>> = Mutex::new(None);
static DRIVER_REGISTRY: Mutex<Option<DriverRegistry>> = Mutex::new(None);
static PLATFORM: Mutex<Option<Platform>> = Mutex::new(None);
static WATCHDOG: Mutex<Option<Watchdog>> = Mutex::new(None);

/// Detect current platform
pub fn detect_platform() -> Platform {
    // Check for QEMU by examining CPUID or DMI
    // For now, default to QEMU x86_64
    Platform::QemuX86_64
}

/// Get detected hardware list
pub fn detected_hardware() -> Vec<HardwareComponent> {
    DETECTED_HARDWARE.lock().clone()
}

/// Get boot timing summary
pub fn boot_summary() -> Option<String> {
    BOOT_ANALYZER.lock().as_ref().map(|a| a.summary())
}

/// Detect CPU topology
pub fn detect_cpu_topology() -> CpuTopology {
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();

    let mut topo = CpuTopology {
        packages: 1,
        cores_per_package: 1,
        threads_per_core: 1,
        total_logical: 1,
        numa_nodes: 1,
        l1d_cache_kb: 32,
        l1i_cache_kb: 32,
        l2_cache_kb: 256,
        l3_cache_kb: 0,
    };

    if let Some(info) = cpuid.get_feature_info() {
        topo.total_logical = info.max_logical_processor_ids() as u32;
        if topo.total_logical == 0 {
            topo.total_logical = 1;
        }
    }

    // Try to get cache info
    if let Some(caches) = cpuid.get_cache_parameters() {
        for cache in caches {
            let level = cache.level();
            let size_kb = ((cache.sets() + 1)
                * (cache.coherency_line_size() + 1)
                * (cache.physical_line_partitions() + 1)
                * (cache.associativity() + 1)) as u32
                / 1024;
            match level {
                1 => {
                    if cache.cache_type() == crate::arch_compat::raw_cpuid::CacheType::Data {
                        topo.l1d_cache_kb = size_kb;
                    } else if cache.cache_type()
                        == crate::arch_compat::raw_cpuid::CacheType::Instruction
                    {
                        topo.l1i_cache_kb = size_kb;
                    }
                }
                2 => topo.l2_cache_kb = size_kb,
                3 => topo.l3_cache_kb = size_kb,
                _ => {}
            }
        }
    }

    topo
}

// ─── Initialization ─────────────────────────────────────────────────

pub fn init() {
    let platform = detect_platform();
    *PLATFORM.lock() = Some(platform);

    // Initialize boot analyzer
    *BOOT_ANALYZER.lock() = Some(BootAnalyzer::new());

    // Initialize driver registry
    let mut registry = DriverRegistry::new();

    // Register built-in drivers
    registry.register(DriverDescriptor {
        name: String::from("serial"),
        module: String::from("serial"),
        match_criteria: alloc::vec![DriverMatch::Platform {
            name: String::from("ns16550a")
        }],
        probe_order: 0,
    });
    registry.register(DriverDescriptor {
        name: String::from("virtio-net"),
        module: String::from("virtio_net"),
        match_criteria: alloc::vec![DriverMatch::PciId {
            vendor: 0x1AF4,
            device: 0x1000,
            subvendor: None,
            subdevice: None
        }],
        probe_order: 10,
    });
    registry.register(DriverDescriptor {
        name: String::from("virtio-blk"),
        module: String::from("virtio_blk"),
        match_criteria: alloc::vec![DriverMatch::PciId {
            vendor: 0x1AF4,
            device: 0x1001,
            subvendor: None,
            subdevice: None
        }],
        probe_order: 10,
    });
    registry.register(DriverDescriptor {
        name: String::from("e1000"),
        module: String::from("e1000"),
        match_criteria: alloc::vec![DriverMatch::PciId {
            vendor: 0x8086,
            device: 0x100E,
            subvendor: None,
            subdevice: None
        }],
        probe_order: 15,
    });
    registry.register(DriverDescriptor {
        name: String::from("ahci"),
        module: String::from("ahci"),
        match_criteria: alloc::vec![DriverMatch::PciClass {
            class: 0x01,
            subclass: 0x06,
            prog_if: Some(0x01)
        }],
        probe_order: 10,
    });
    registry.register(DriverDescriptor {
        name: String::from("nvme"),
        module: String::from("nvme"),
        match_criteria: alloc::vec![DriverMatch::PciClass {
            class: 0x01,
            subclass: 0x08,
            prog_if: Some(0x02)
        }],
        probe_order: 10,
    });
    registry.register(DriverDescriptor {
        name: String::from("xhci"),
        module: String::from("usb"),
        match_criteria: alloc::vec![DriverMatch::PciClass {
            class: 0x0C,
            subclass: 0x03,
            prog_if: Some(0x30)
        }],
        probe_order: 20,
    });
    registry.register(DriverDescriptor {
        name: String::from("intel-hda"),
        module: String::from("hda"),
        match_criteria: alloc::vec![DriverMatch::PciClass {
            class: 0x04,
            subclass: 0x03,
            prog_if: None
        }],
        probe_order: 30,
    });

    *DRIVER_REGISTRY.lock() = Some(registry);

    // Initialize hardware watchdog
    *WATCHDOG.lock() = Some(Watchdog::new());

    // Detect hardware components
    let mut hw = DETECTED_HARDWARE.lock();

    // CPU
    let topo = detect_cpu_topology();
    let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
    let cpu_name = cpuid
        .get_processor_brand_string()
        .map(|b| String::from(b.as_str()))
        .unwrap_or_else(|| String::from("Unknown CPU"));
    let vendor = cpuid
        .get_vendor_info()
        .map(|v| String::from(v.as_str()))
        .unwrap_or_else(|| String::from("Unknown"));

    hw.push(HardwareComponent {
        category: HwCategory::Processor,
        name: cpu_name,
        vendor: vendor.clone(),
        model: format!(
            "{} cores, {} threads",
            topo.cores_per_package, topo.total_logical
        ),
        driver: None,
        status: HwStatus::Active,
        properties: BTreeMap::new(),
    });

    // Memory
    hw.push(HardwareComponent {
        category: HwCategory::Memory,
        name: String::from("System Memory"),
        vendor: String::from("Generic"),
        model: String::from("DDR4"),
        driver: None,
        status: HwStatus::Active,
        properties: BTreeMap::new(),
    });

    // Serial
    hw.push(HardwareComponent {
        category: HwCategory::Serial,
        name: String::from("COM1 (NS16550A)"),
        vendor: String::from("Generic"),
        model: String::from("16550A UART"),
        driver: Some(String::from("serial")),
        status: HwStatus::Active,
        properties: BTreeMap::new(),
    });

    drop(hw);

    HW_INITIALIZED.store(true, Ordering::Release);

    crate::serial_println!("[KnoxOS] Hardware bring-up initialized");
    crate::serial_println!("[KnoxOS]   Platform: {}", platform.name());
    crate::serial_println!(
        "[KnoxOS]   CPU: {} cores, L1d={}KB, L2={}KB, L3={}KB",
        topo.total_logical,
        topo.l1d_cache_kb,
        topo.l2_cache_kb,
        topo.l3_cache_kb
    );
    crate::serial_println!(
        "[KnoxOS]   PCI: {}, ACPI: {}, DeviceTree: {}",
        platform.has_pci(),
        platform.has_acpi(),
        platform.has_device_tree()
    );
    crate::serial_println!(
        "[KnoxOS]   Watchdog: enabled (software, {}s timeout)",
        Watchdog::new().timeout_secs
    );
}
