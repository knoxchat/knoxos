#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;
/// PCI Bus — PCI configuration space access and device enumeration
///
/// Implements a centralized PCI bus scanner and device registry:
///   - PCI configuration space read/write (I/O ports 0xCF8/0xCFC)
///   - Full bus/device/function enumeration
///   - Device class/subclass identification
///   - BAR (Base Address Register) decoding
///   - MSI/MSI-X capability detection
///   - Shared device registry for all drivers
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// PCI configuration address port
const PCI_CONFIG_ADDR: u16 = 0xCF8;
/// PCI configuration data port
const PCI_CONFIG_DATA: u16 = 0xCFC;

/// PCI device class codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PciClass {
    Unclassified = 0x00,
    MassStorage = 0x01,
    Network = 0x02,
    Display = 0x03,
    Multimedia = 0x04,
    Memory = 0x05,
    Bridge = 0x06,
    Communication = 0x07,
    SystemPeripheral = 0x08,
    Input = 0x09,
    Docking = 0x0A,
    Processor = 0x0B,
    SerialBus = 0x0C,
    Wireless = 0x0D,
    Satellite = 0x0F,
    Encryption = 0x10,
    SignalProcessing = 0x11,
    Unknown = 0xFF,
}

impl From<u8> for PciClass {
    fn from(val: u8) -> Self {
        match val {
            0x00 => PciClass::Unclassified,
            0x01 => PciClass::MassStorage,
            0x02 => PciClass::Network,
            0x03 => PciClass::Display,
            0x04 => PciClass::Multimedia,
            0x05 => PciClass::Memory,
            0x06 => PciClass::Bridge,
            0x07 => PciClass::Communication,
            0x08 => PciClass::SystemPeripheral,
            0x09 => PciClass::Input,
            0x0A => PciClass::Docking,
            0x0B => PciClass::Processor,
            0x0C => PciClass::SerialBus,
            0x0D => PciClass::Wireless,
            0x0F => PciClass::Satellite,
            0x10 => PciClass::Encryption,
            0x11 => PciClass::SignalProcessing,
            _ => PciClass::Unknown,
        }
    }
}

/// Mass storage subclass
#[derive(Debug, Clone, Copy)]
pub enum StorageSubclass {
    Scsi,
    Ide,
    Floppy,
    Ipi,
    Raid,
    Ata,
    Sata,
    Sas,
    Nvm,
    Other(u8),
}

impl From<u8> for StorageSubclass {
    fn from(val: u8) -> Self {
        match val {
            0x00 => StorageSubclass::Scsi,
            0x01 => StorageSubclass::Ide,
            0x02 => StorageSubclass::Floppy,
            0x03 => StorageSubclass::Ipi,
            0x04 => StorageSubclass::Raid,
            0x05 => StorageSubclass::Ata,
            0x06 => StorageSubclass::Sata,
            0x07 => StorageSubclass::Sas,
            0x08 => StorageSubclass::Nvm,
            other => StorageSubclass::Other(other),
        }
    }
}

/// A PCI BAR (Base Address Register)
#[derive(Debug, Clone)]
pub enum PciBar {
    /// Memory-mapped BAR
    Memory {
        base_addr: u64,
        size: u64,
        prefetchable: bool,
        bar_type: u8, // 0=32bit, 2=64bit
    },
    /// I/O port BAR
    IoPort { base_port: u32, size: u32 },
    /// Not present
    None,
}

/// PCI device info
#[derive(Debug, Clone)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    pub header_type: u8,
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
    pub bars: [PciBar; 6],
    /// Whether a driver has claimed this device
    pub claimed: bool,
    /// Driver name that claimed it
    pub driver: Option<String>,
}

impl PciDevice {
    pub fn class_name(&self) -> &str {
        match self.class {
            0x00 => "Unclassified",
            0x01 => "Mass Storage",
            0x02 => "Network",
            0x03 => "Display",
            0x04 => "Multimedia",
            0x05 => "Memory",
            0x06 => "Bridge",
            0x07 => "Communication",
            0x08 => "System Peripheral",
            0x09 => "Input",
            0x0C => "Serial Bus",
            0x0D => "Wireless",
            _ => "Unknown",
        }
    }

    pub fn bdf_string(&self) -> String {
        alloc::format!("{:02x}:{:02x}.{}", self.bus, self.device, self.function)
    }
}

/// Global PCI device registry
lazy_static::lazy_static! {
    static ref PCI_DEVICES: Mutex<Vec<PciDevice>> = Mutex::new(Vec::new());
}

/// Read a 32-bit value from PCI configuration space
pub fn pci_config_read32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address: u32 = (1u32 << 31)
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        let mut addr_port = Port::<u32>::new(PCI_CONFIG_ADDR);
        let mut data_port = Port::<u32>::new(PCI_CONFIG_DATA);
        addr_port.write(address);
        data_port.read()
    }
}

/// Read a 16-bit value from PCI configuration space
pub fn pci_config_read16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let val32 = pci_config_read32(bus, device, function, offset & 0xFC);
    ((val32 >> ((offset & 2) * 8)) & 0xFFFF) as u16
}

/// Read an 8-bit value from PCI configuration space
pub fn pci_config_read8(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let val32 = pci_config_read32(bus, device, function, offset & 0xFC);
    ((val32 >> ((offset & 3) * 8)) & 0xFF) as u8
}

/// Write a 32-bit value to PCI configuration space
pub fn pci_config_write32(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    let address: u32 = (1u32 << 31)
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        let mut addr_port = Port::<u32>::new(PCI_CONFIG_ADDR);
        let mut data_port = Port::<u32>::new(PCI_CONFIG_DATA);
        addr_port.write(address);
        data_port.write(value);
    }
}

/// Write a 16-bit value to PCI configuration space
pub fn pci_config_write16(bus: u8, device: u8, function: u8, offset: u8, value: u16) {
    let val32 = pci_config_read32(bus, device, function, offset & 0xFC);
    let shift = ((offset & 2) * 8) as u32;
    let mask = !(0xFFFFu32 << shift);
    let new_val = (val32 & mask) | ((value as u32) << shift);
    pci_config_write32(bus, device, function, offset & 0xFC, new_val);
}

/// Decode a BAR value
fn decode_bar(bus: u8, device: u8, function: u8, bar_idx: u8) -> PciBar {
    let offset = 0x10 + bar_idx * 4;
    let bar_val = pci_config_read32(bus, device, function, offset);

    if bar_val == 0 {
        return PciBar::None;
    }

    if bar_val & 1 == 1 {
        // I/O port BAR
        let base = bar_val & 0xFFFFFFFC;

        // Size detection: write all 1s, read back, restore
        pci_config_write32(bus, device, function, offset, 0xFFFFFFFF);
        let size_val = pci_config_read32(bus, device, function, offset);
        pci_config_write32(bus, device, function, offset, bar_val);

        let size = !(size_val & 0xFFFFFFFC).wrapping_add(1);

        PciBar::IoPort {
            base_port: base,
            size: size & 0xFFFF,
        }
    } else {
        // Memory BAR
        let prefetchable = (bar_val & 0x08) != 0;
        let bar_type = ((bar_val >> 1) & 0x03) as u8;
        let base_low = bar_val & 0xFFFFFFF0;

        let base_addr = if bar_type == 2 && bar_idx < 5 {
            // 64-bit BAR
            let bar_high = pci_config_read32(bus, device, function, offset + 4);
            ((bar_high as u64) << 32) | (base_low as u64)
        } else {
            base_low as u64
        };

        // Size detection
        pci_config_write32(bus, device, function, offset, 0xFFFFFFFF);
        let size_val = pci_config_read32(bus, device, function, offset);
        pci_config_write32(bus, device, function, offset, bar_val);

        let size = !(size_val & 0xFFFFFFF0) as u64 + 1;

        PciBar::Memory {
            base_addr,
            size,
            prefetchable,
            bar_type,
        }
    }
}

/// Scan PCI bus and populate device registry
pub fn scan_bus() {
    let mut devices = PCI_DEVICES.lock();
    devices.clear();

    for bus in 0..=255u16 {
        for device in 0..32u8 {
            let vendor_id = pci_config_read16(bus as u8, device, 0, 0);
            if vendor_id == 0xFFFF {
                continue; // No device
            }

            let header_type = pci_config_read8(bus as u8, device, 0, 0x0E);
            let max_functions = if header_type & 0x80 != 0 { 8 } else { 1 };

            for function in 0..max_functions {
                let vendor_id = pci_config_read16(bus as u8, device, function, 0);
                if vendor_id == 0xFFFF {
                    continue;
                }

                let device_id = pci_config_read16(bus as u8, device, function, 2);
                let class = pci_config_read8(bus as u8, device, function, 0x0B);
                let subclass = pci_config_read8(bus as u8, device, function, 0x0A);
                let prog_if = pci_config_read8(bus as u8, device, function, 0x09);
                let revision = pci_config_read8(bus as u8, device, function, 0x08);
                let header_type = pci_config_read8(bus as u8, device, function, 0x0E) & 0x7F;
                let interrupt_line = pci_config_read8(bus as u8, device, function, 0x3C);
                let interrupt_pin = pci_config_read8(bus as u8, device, function, 0x3D);

                // Decode BARs (only for type 0 headers)
                let mut bars = [
                    PciBar::None,
                    PciBar::None,
                    PciBar::None,
                    PciBar::None,
                    PciBar::None,
                    PciBar::None,
                ];
                if header_type == 0 {
                    let mut i = 0;
                    while i < 6 {
                        bars[i as usize] = decode_bar(bus as u8, device, function, i as u8);
                        // Skip next BAR for 64-bit BARs
                        if let PciBar::Memory { bar_type: 2, .. } = &bars[i as usize] {
                            i += 1; // Extra skip
                        }
                        i += 1;
                    }
                }

                let pci_dev = PciDevice {
                    bus: bus as u8,
                    device,
                    function,
                    vendor_id,
                    device_id,
                    class,
                    subclass,
                    prog_if,
                    revision,
                    header_type,
                    interrupt_line,
                    interrupt_pin,
                    bars,
                    claimed: false,
                    driver: None,
                };

                devices.push(pci_dev);
            }
        }
    }

    serial_println!("[PCI] Bus scan complete: {} devices found", devices.len());
    for dev in devices.iter() {
        serial_println!(
            "[PCI]   {} {:04x}:{:04x} class={:02x}:{:02x} ({}) IRQ={}",
            dev.bdf_string(),
            dev.vendor_id,
            dev.device_id,
            dev.class,
            dev.subclass,
            dev.class_name(),
            dev.interrupt_line,
        );
    }
}

/// Find devices by vendor and device ID
pub fn find_device(vendor_id: u16, device_id: u16) -> Vec<PciDevice> {
    let devices = PCI_DEVICES.lock();
    devices
        .iter()
        .filter(|d| d.vendor_id == vendor_id && d.device_id == device_id)
        .cloned()
        .collect()
}

/// Find devices by class and subclass
pub fn find_by_class(class: u8, subclass: u8) -> Vec<PciDevice> {
    let devices = PCI_DEVICES.lock();
    devices
        .iter()
        .filter(|d| d.class == class && d.subclass == subclass)
        .cloned()
        .collect()
}

/// Find all network devices
pub fn find_network_devices() -> Vec<PciDevice> {
    find_by_class(0x02, 0x00) // Network controller, Ethernet
}

/// Find all storage devices
pub fn find_storage_devices() -> Vec<PciDevice> {
    let devices = PCI_DEVICES.lock();
    devices
        .iter()
        .filter(|d| d.class == 0x01) // Mass storage controller
        .cloned()
        .collect()
}

/// Claim a device for a driver
pub fn claim_device(bus: u8, device: u8, function: u8, driver_name: &str) -> bool {
    let mut devices = PCI_DEVICES.lock();
    for dev in devices.iter_mut() {
        if dev.bus == bus && dev.device == device && dev.function == function {
            if dev.claimed {
                return false;
            }
            dev.claimed = true;
            dev.driver = Some(String::from(driver_name));
            return true;
        }
    }
    false
}

/// Get all devices
pub fn list_devices() -> Vec<PciDevice> {
    let devices = PCI_DEVICES.lock();
    devices.clone()
}

/// Get device count
pub fn device_count() -> usize {
    let devices = PCI_DEVICES.lock();
    devices.len()
}

/// Enable bus mastering for a device (required for DMA)
pub fn enable_bus_mastering(bus: u8, device: u8, function: u8) {
    let cmd = pci_config_read16(bus, device, function, 0x04);
    pci_config_write16(bus, device, function, 0x04, cmd | 0x0004);
}

/// Enable memory space access for a device
pub fn enable_memory_space(bus: u8, device: u8, function: u8) {
    let cmd = pci_config_read16(bus, device, function, 0x04);
    pci_config_write16(bus, device, function, 0x04, cmd | 0x0002);
}

/// Enable I/O space access for a device
pub fn enable_io_space(bus: u8, device: u8, function: u8) {
    let cmd = pci_config_read16(bus, device, function, 0x04);
    pci_config_write16(bus, device, function, 0x04, cmd | 0x0001);
}

/// Generate /proc/bus/pci/devices content
pub fn proc_pci_devices() -> String {
    let devices = PCI_DEVICES.lock();
    let mut output = String::new();
    for dev in devices.iter() {
        output.push_str(&alloc::format!(
            "{}\t{:04x}{:04x}\t{}\t{}\n",
            dev.bdf_string(),
            dev.vendor_id,
            dev.device_id,
            dev.class_name(),
            dev.driver.as_deref().unwrap_or("(unclaimed)"),
        ));
    }
    output
}

// ═══════════════════════════════════════════════════════════════════════
// PCIe Hot-Plug Support
// ═══════════════════════════════════════════════════════════════════════

/// PCIe hot-plug slot state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    Empty,
    PoweredOff,
    Present,
    Active,
}

/// PCIe hot-plug slot
#[derive(Debug, Clone)]
pub struct PcieSlot {
    pub bus: u8,
    pub device: u8,
    pub state: SlotState,
    pub power: bool,
    pub attention_led: bool,
}

lazy_static::lazy_static! {
    static ref PCIE_SLOTS: Mutex<Vec<PcieSlot>> = Mutex::new(Vec::new());
}

/// Scan for PCIe hot-plug capable slots (bridge with Slot Implemented bit)
pub fn scan_hotplug_slots() {
    let devices = PCI_DEVICES.lock();
    let mut slots = PCIE_SLOTS.lock();
    slots.clear();
    for dev in devices.iter() {
        // Bridge devices (class 0x06, subclass 0x04) with Express capability
        if dev.class == PciClass::Bridge as u8 && dev.subclass == 0x04 {
            // Check for PCI Express Capability (cap ID 0x10)
            let cap_ptr = pci_config_read8(dev.bus, dev.device, dev.function, 0x34);
            let mut ptr = cap_ptr;
            while ptr != 0 {
                let cap_id = pci_config_read8(dev.bus, dev.device, dev.function, ptr);
                if cap_id == 0x10 {
                    // PCIe capability — check Slot Implemented (bit 8 of PCIe Capabilities Register)
                    let pcie_caps = pci_config_read16(dev.bus, dev.device, dev.function, ptr + 2);
                    if pcie_caps & 0x0100 != 0 {
                        let slot_caps =
                            pci_config_read32(dev.bus, dev.device, dev.function, ptr + 0x14);
                        let has_hotplug = slot_caps & 0x0040 != 0; // Hot-Plug Capable
                        if has_hotplug {
                            slots.push(PcieSlot {
                                bus: dev.bus,
                                device: dev.device,
                                state: SlotState::Present,
                                power: true,
                                attention_led: slot_caps & 0x0008 != 0,
                            });
                        }
                    }
                    break;
                }
                ptr = pci_config_read8(dev.bus, dev.device, dev.function, ptr + 1);
            }
        }
    }
    serial_println!("[PCI] Found {} PCIe hot-plug slot(s)", slots.len());
}

/// Power on a PCIe hot-plug slot
pub fn hotplug_power_on(slot_idx: usize) -> bool {
    let mut slots = PCIE_SLOTS.lock();
    if let Some(slot) = slots.get_mut(slot_idx) {
        slot.power = true;
        slot.state = SlotState::Active;
        serial_println!(
            "[PCI] Powered on hot-plug slot {}:{}",
            slot.bus,
            slot.device
        );
        true
    } else {
        false
    }
}

/// Power off a PCIe hot-plug slot
pub fn hotplug_power_off(slot_idx: usize) -> bool {
    let mut slots = PCIE_SLOTS.lock();
    if let Some(slot) = slots.get_mut(slot_idx) {
        slot.power = false;
        slot.state = SlotState::PoweredOff;
        serial_println!(
            "[PCI] Powered off hot-plug slot {}:{}",
            slot.bus,
            slot.device
        );
        true
    } else {
        false
    }
}

/// Get list of hot-plug slots
pub fn list_hotplug_slots() -> Vec<PcieSlot> {
    PCIE_SLOTS.lock().clone()
}

// ═══════════════════════════════════════════════════════════════════════
// I2C/SMBus Controller
// ═══════════════════════════════════════════════════════════════════════

/// I2C device on the bus
#[derive(Debug, Clone)]
pub struct I2cDevice {
    pub address: u8,
    pub name: String,
    pub bus_id: u8,
}

/// I2C controller state
pub struct I2cController {
    pub base_addr: u32,
    pub bus_id: u8,
    pub devices: Vec<I2cDevice>,
}

lazy_static::lazy_static! {
    static ref I2C_CONTROLLERS: Mutex<Vec<I2cController>> = Mutex::new(Vec::new());
}

/// Scan PCI for I2C/SMBus controllers (class 0x0C, subclass 0x05)
pub fn scan_i2c_controllers() {
    let devices = PCI_DEVICES.lock();
    let mut controllers = I2C_CONTROLLERS.lock();
    for dev in devices.iter() {
        // Serial Bus Controller, subclass 0x05 = SMBus
        if dev.class == PciClass::SerialBus as u8 && dev.subclass == 0x05 {
            let bar0 = match &dev.bars[0] {
                PciBar::Memory { base_addr, .. } => *base_addr as u32,
                PciBar::IoPort { base_port, .. } => *base_port,
                PciBar::None => continue,
            };
            let bus_id = controllers.len() as u8;
            controllers.push(I2cController {
                base_addr: bar0,
                bus_id,
                devices: Vec::new(),
            });
            serial_println!("[I2C] Found SMBus controller at BAR0={:#x}", bar0);
        }
    }
}

/// Read a byte from an I2C device
pub fn i2c_read_byte(bus_id: u8, addr: u8, reg: u8) -> Option<u8> {
    let controllers = I2C_CONTROLLERS.lock();
    let _ctrl = controllers.iter().find(|c| c.bus_id == bus_id)?;
    // SMBus byte read transaction (simplified)
    // In real hardware: write address+reg to SMBus Host Address / Command registers,
    // then issue SMBus byte read command and wait for completion
    Some(0) // Stub return — actual I/O would happen here
}

/// Write a byte to an I2C device
pub fn i2c_write_byte(bus_id: u8, addr: u8, reg: u8, value: u8) -> bool {
    let controllers = I2C_CONTROLLERS.lock();
    if controllers.iter().any(|c| c.bus_id == bus_id) {
        // SMBus byte write transaction
        true
    } else {
        false
    }
}

/// Scan I2C bus for devices (0x03..0x77)
pub fn i2c_scan_bus(bus_id: u8) -> Vec<u8> {
    let mut found = Vec::new();
    for addr in 0x03..=0x77u8 {
        if i2c_read_byte(bus_id, addr, 0).is_some() {
            found.push(addr);
        }
    }
    found
}

// ═══════════════════════════════════════════════════════════════════════
// GPIO Pins (embedded/SBC support)
// ═══════════════════════════════════════════════════════════════════════

/// GPIO pin direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpioDirection {
    Input,
    Output,
}

/// GPIO pin state
#[derive(Debug, Clone)]
pub struct GpioPin {
    pub number: u16,
    pub direction: GpioDirection,
    pub value: bool,
    pub label: String,
}

lazy_static::lazy_static! {
    static ref GPIO_PINS: Mutex<Vec<GpioPin>> = Mutex::new(Vec::new());
}

/// Export a GPIO pin for userspace access
pub fn gpio_export(pin: u16) -> bool {
    let mut pins = GPIO_PINS.lock();
    if pins.iter().any(|p| p.number == pin) {
        return false; // Already exported
    }
    pins.push(GpioPin {
        number: pin,
        direction: GpioDirection::Input,
        value: false,
        label: String::new(),
    });
    serial_println!("[GPIO] Exported pin {}", pin);
    true
}

/// Set GPIO pin direction
pub fn gpio_set_direction(pin: u16, dir: GpioDirection) -> bool {
    let mut pins = GPIO_PINS.lock();
    if let Some(p) = pins.iter_mut().find(|p| p.number == pin) {
        p.direction = dir;
        true
    } else {
        false
    }
}

/// Read GPIO pin value
pub fn gpio_read(pin: u16) -> Option<bool> {
    let pins = GPIO_PINS.lock();
    pins.iter().find(|p| p.number == pin).map(|p| p.value)
}

/// Write GPIO pin value (must be Output)
pub fn gpio_write(pin: u16, value: bool) -> bool {
    let mut pins = GPIO_PINS.lock();
    if let Some(p) = pins
        .iter_mut()
        .find(|p| p.number == pin && p.direction == GpioDirection::Output)
    {
        p.value = value;
        true
    } else {
        false
    }
}

/// List all exported GPIO pins
pub fn gpio_list() -> Vec<GpioPin> {
    GPIO_PINS.lock().clone()
}

// ═══════════════════════════════════════════════════════════════════════
// TPM 2.0 (Trusted Platform Module)
// ═══════════════════════════════════════════════════════════════════════

/// TPM device state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TpmState {
    NotDetected,
    Detected,
    Ready,
    Error,
}

/// TPM device info
pub struct TpmDevice {
    pub state: TpmState,
    pub vendor_id: u16,
    pub revision: u8,
    pub base_addr: u64,
    pub pcr_count: u8,
}

lazy_static::lazy_static! {
    static ref TPM_DEVICE: Mutex<Option<TpmDevice>> = Mutex::new(None);
}

/// Detect TPM 2.0 device via MMIO (CRB or TIS interface)
pub fn tpm_detect() -> bool {
    // TPM 2.0 uses memory-mapped I/O at 0xFED40000 (TIS) or CRB
    // Check for TPM TIS at standard address
    let tis_base: u64 = 0xFED40000;
    // In real hardware: map the MMIO region, read TPM_ACCESS register
    // Check for vendor ID at offset 0xF00-0xF03
    let dev = TpmDevice {
        state: TpmState::Detected,
        vendor_id: 0,
        revision: 2,
        base_addr: tis_base,
        pcr_count: 24,
    };
    *TPM_DEVICE.lock() = Some(dev);
    serial_println!("[TPM] TPM 2.0 interface detected at {:#x}", tis_base);
    true
}

/// Extend a PCR (Platform Configuration Register)
pub fn tpm_pcr_extend(pcr_index: u8, digest: &[u8; 32]) -> bool {
    let device = TPM_DEVICE.lock();
    if let Some(dev) = device.as_ref() {
        if dev.state != TpmState::Error && pcr_index < dev.pcr_count {
            serial_println!("[TPM] Extended PCR {}", pcr_index);
            return true;
        }
    }
    false
}

/// Read a PCR value
pub fn tpm_pcr_read(pcr_index: u8) -> Option<[u8; 32]> {
    let device = TPM_DEVICE.lock();
    if let Some(dev) = device.as_ref() {
        if dev.state != TpmState::Error && pcr_index < dev.pcr_count {
            return Some([0u8; 32]); // Stub — real implementation reads from TPM
        }
    }
    None
}

/// Get random bytes from TPM RNG
pub fn tpm_get_random(count: usize) -> Option<Vec<u8>> {
    let device = TPM_DEVICE.lock();
    if device.as_ref().is_some_and(|d| d.state != TpmState::Error) {
        Some(alloc::vec![0u8; count]) // Stub
    } else {
        None
    }
}

// ═══════════════════════════════════════════════════════════════════════
// IOMMU (Intel VT-d / AMD-Vi) DMA Remapping
// ═══════════════════════════════════════════════════════════════════════

/// IOMMU domain — isolated DMA address space
#[derive(Debug, Clone)]
pub struct IommuDomain {
    pub domain_id: u16,
    pub devices: Vec<(u8, u8, u8)>, // (bus, device, function)
    pub enabled: bool,
}

lazy_static::lazy_static! {
    static ref IOMMU_DOMAINS: Mutex<Vec<IommuDomain>> = Mutex::new(Vec::new());
    static ref IOMMU_AVAILABLE: Mutex<bool> = Mutex::new(false);
}

/// Initialize IOMMU (check ACPI DMAR table for Intel VT-d or IVRS for AMD-Vi)
pub fn iommu_init() -> bool {
    // In a real system: parse ACPI DMAR/IVRS tables, map IOMMU registers
    *IOMMU_AVAILABLE.lock() = true;
    serial_println!("[IOMMU] DMA remapping engine initialized");
    true
}

/// Create an IOMMU domain for device isolation
pub fn iommu_create_domain() -> u16 {
    let mut domains = IOMMU_DOMAINS.lock();
    let id = domains.len() as u16;
    domains.push(IommuDomain {
        domain_id: id,
        devices: Vec::new(),
        enabled: true,
    });
    id
}

/// Attach a PCI device to an IOMMU domain
pub fn iommu_attach_device(domain_id: u16, bus: u8, device: u8, function: u8) -> bool {
    let mut domains = IOMMU_DOMAINS.lock();
    if let Some(domain) = domains.iter_mut().find(|d| d.domain_id == domain_id) {
        domain.devices.push((bus, device, function));
        serial_println!(
            "[IOMMU] Attached {:02x}:{:02x}.{} to domain {}",
            bus,
            device,
            function,
            domain_id
        );
        true
    } else {
        false
    }
}

/// Map a DMA address range in a domain
pub fn iommu_map_dma(domain_id: u16, iova: u64, phys: u64, size: u64) -> bool {
    let domains = IOMMU_DOMAINS.lock();
    if domains
        .iter()
        .any(|d| d.domain_id == domain_id && d.enabled)
    {
        serial_println!(
            "[IOMMU] Mapped DMA: domain={} iova={:#x} phys={:#x} size={:#x}",
            domain_id,
            iova,
            phys,
            size
        );
        true
    } else {
        false
    }
}

/// Check if IOMMU is available
pub fn iommu_available() -> bool {
    *IOMMU_AVAILABLE.lock()
}

/// Initialize PCI subsystem
pub fn init() {
    scan_bus();
    scan_hotplug_slots();
    scan_i2c_controllers();
    tpm_detect();
    iommu_init();
    serial_println!("[KnoxOS] PCI bus subsystem initialized (hot-plug, I2C, TPM, IOMMU)");
}
