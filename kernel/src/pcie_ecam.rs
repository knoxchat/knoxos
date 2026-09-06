/// PCIe Enhanced Configuration Access Mechanism (ECAM) Driver
///
/// Provides memory-mapped PCIe configuration space access via MCFG ACPI table.
/// Supports full 4096-byte extended configuration space per function,
/// device enumeration, BAR management, MSI/MSI-X configuration, and
/// power management.
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── PCI Configuration Space Offsets ────────────────────────────────

const PCI_VENDOR_ID: usize = 0x00;
const PCI_DEVICE_ID: usize = 0x02;
const PCI_COMMAND: usize = 0x04;
const PCI_STATUS: usize = 0x06;
const PCI_REVISION: usize = 0x08;
const PCI_PROG_IF: usize = 0x09;
const PCI_SUBCLASS: usize = 0x0A;
const PCI_CLASS: usize = 0x0B;
const PCI_CACHE_LINE: usize = 0x0C;
const PCI_LATENCY: usize = 0x0D;
const PCI_HEADER_TYPE: usize = 0x0E;
const PCI_BIST: usize = 0x0F;
const PCI_BAR0: usize = 0x10;
const PCI_BAR1: usize = 0x14;
const PCI_BAR2: usize = 0x18;
const PCI_BAR3: usize = 0x1C;
const PCI_BAR4: usize = 0x20;
const PCI_BAR5: usize = 0x24;
const PCI_SUBSYSTEM_VENDOR: usize = 0x2C;
const PCI_SUBSYSTEM_ID: usize = 0x2E;
const PCI_CAPABILITIES_PTR: usize = 0x34;
const PCI_INTERRUPT_LINE: usize = 0x3C;
const PCI_INTERRUPT_PIN: usize = 0x3D;

// PCI Command register bits
const PCI_CMD_IO_SPACE: u16 = 1 << 0;
const PCI_CMD_MEMORY_SPACE: u16 = 1 << 1;
const PCI_CMD_BUS_MASTER: u16 = 1 << 2;
const PCI_CMD_INTERRUPT_DISABLE: u16 = 1 << 10;

// Capability IDs
const PCI_CAP_MSI: u8 = 0x05;
const PCI_CAP_MSIX: u8 = 0x11;
const PCI_CAP_PCIE: u8 = 0x10;
const PCI_CAP_PM: u8 = 0x01;

// ─── Device Structures ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PcieDevice {
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    pub header_type: u8,
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
    pub subsystem_vendor: u16,
    pub subsystem_id: u16,
    pub bars: [PciBar; 6],
    pub has_msi: bool,
    pub has_msix: bool,
    pub is_pcie: bool,
    pub msi_offset: u8,
    pub msix_offset: u8,
    pub pcie_offset: u8,
    pub pm_offset: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct PciBar {
    pub base: u64,
    pub size: u64,
    pub bar_type: BarType,
    pub prefetchable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BarType {
    None,
    Io,
    Memory32,
    Memory64,
}

impl Default for PciBar {
    fn default() -> Self {
        Self {
            base: 0,
            size: 0,
            bar_type: BarType::None,
            prefetchable: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EcamRegion {
    pub base_address: u64,
    pub segment: u16,
    pub start_bus: u8,
    pub end_bus: u8,
}

// ─── Global State ───────────────────────────────────────────────────

static ECAM_INITIALIZED: AtomicBool = AtomicBool::new(false);

lazy_static::lazy_static! {
    static ref ECAM_REGIONS: Mutex<Vec<EcamRegion>> = Mutex::new(Vec::new());
    static ref PCIE_DEVICES: Mutex<Vec<PcieDevice>> = Mutex::new(Vec::new());
}

// ─── ECAM MMIO Access ───────────────────────────────────────────────

/// Calculate the MMIO address for a given BDF (Bus/Device/Function) + register offset
fn ecam_address(
    region: &EcamRegion,
    bus: u8,
    device: u8,
    function: u8,
    offset: usize,
) -> Option<u64> {
    if bus < region.start_bus || bus > region.end_bus {
        return None;
    }
    if device > 31 || function > 7 {
        return None;
    }
    if offset >= 4096 {
        return None;
    }

    let bdf_offset = ((bus as u64 - region.start_bus as u64) << 20)
        | ((device as u64) << 15)
        | ((function as u64) << 12)
        | (offset as u64);

    // Convert physical base address to virtual address via the physical memory offset
    let virt_base = region.base_address + crate::vmm::get_phys_mem_offset();
    Some(virt_base + bdf_offset)
}

/// Read 8 bits from PCIe config space
pub fn ecam_read8(segment: u16, bus: u8, device: u8, function: u8, offset: usize) -> u8 {
    let regions = ECAM_REGIONS.lock();
    for region in regions.iter() {
        if region.segment == segment {
            if let Some(addr) = ecam_address(region, bus, device, function, offset) {
                return unsafe { core::ptr::read_volatile(addr as *const u8) };
            }
        }
    }
    0xFF
}

/// Read 16 bits from PCIe config space
pub fn ecam_read16(segment: u16, bus: u8, device: u8, function: u8, offset: usize) -> u16 {
    let regions = ECAM_REGIONS.lock();
    for region in regions.iter() {
        if region.segment == segment {
            if let Some(addr) = ecam_address(region, bus, device, function, offset & !1) {
                return unsafe { core::ptr::read_volatile(addr as *const u16) };
            }
        }
    }
    0xFFFF
}

/// Read 32 bits from PCIe config space
pub fn ecam_read32(segment: u16, bus: u8, device: u8, function: u8, offset: usize) -> u32 {
    let regions = ECAM_REGIONS.lock();
    for region in regions.iter() {
        if region.segment == segment {
            if let Some(addr) = ecam_address(region, bus, device, function, offset & !3) {
                return unsafe { core::ptr::read_volatile(addr as *const u32) };
            }
        }
    }
    0xFFFF_FFFF
}

/// Write 8 bits to PCIe config space
pub fn ecam_write8(segment: u16, bus: u8, device: u8, function: u8, offset: usize, value: u8) {
    let regions = ECAM_REGIONS.lock();
    for region in regions.iter() {
        if region.segment == segment {
            if let Some(addr) = ecam_address(region, bus, device, function, offset) {
                unsafe { core::ptr::write_volatile(addr as *mut u8, value) };
                return;
            }
        }
    }
}

/// Write 16 bits to PCIe config space
pub fn ecam_write16(segment: u16, bus: u8, device: u8, function: u8, offset: usize, value: u16) {
    let regions = ECAM_REGIONS.lock();
    for region in regions.iter() {
        if region.segment == segment {
            if let Some(addr) = ecam_address(region, bus, device, function, offset & !1) {
                unsafe { core::ptr::write_volatile(addr as *mut u16, value) };
                return;
            }
        }
    }
}

/// Write 32 bits to PCIe config space
pub fn ecam_write32(segment: u16, bus: u8, device: u8, function: u8, offset: usize, value: u32) {
    let regions = ECAM_REGIONS.lock();
    for region in regions.iter() {
        if region.segment == segment {
            if let Some(addr) = ecam_address(region, bus, device, function, offset & !3) {
                unsafe { core::ptr::write_volatile(addr as *mut u32, value) };
                return;
            }
        }
    }
}

// ─── BAR Decoding ───────────────────────────────────────────────────

fn decode_bar(segment: u16, bus: u8, dev: u8, func: u8, bar_index: usize) -> PciBar {
    let offset = PCI_BAR0 + bar_index * 4;
    let raw = ecam_read32(segment, bus, dev, func, offset);

    if raw == 0 {
        return PciBar::default();
    }

    // I/O BAR
    if raw & 1 == 1 {
        let base = (raw & 0xFFFC) as u64;
        // Size detection: write all 1s, read back, restore
        ecam_write32(segment, bus, dev, func, offset, 0xFFFF_FFFF);
        let size_mask = ecam_read32(segment, bus, dev, func, offset);
        ecam_write32(segment, bus, dev, func, offset, raw);
        let size = !(size_mask & 0xFFFC) as u64 + 1;

        return PciBar {
            base,
            size: size & 0xFFFF,
            bar_type: BarType::Io,
            prefetchable: false,
        };
    }

    // Memory BAR
    let mem_type = (raw >> 1) & 3;
    let prefetchable = (raw >> 3) & 1 == 1;

    match mem_type {
        0 => {
            // 32-bit memory BAR
            let base = (raw & 0xFFFF_FFF0) as u64;
            ecam_write32(segment, bus, dev, func, offset, 0xFFFF_FFFF);
            let size_mask = ecam_read32(segment, bus, dev, func, offset);
            ecam_write32(segment, bus, dev, func, offset, raw);
            let size = !(size_mask & 0xFFFF_FFF0) as u64 + 1;

            PciBar {
                base,
                size,
                bar_type: BarType::Memory32,
                prefetchable,
            }
        }
        2 => {
            // 64-bit memory BAR
            let raw_high = ecam_read32(segment, bus, dev, func, offset + 4);
            let base = ((raw_high as u64) << 32) | ((raw & 0xFFFF_FFF0) as u64);

            ecam_write32(segment, bus, dev, func, offset, 0xFFFF_FFFF);
            ecam_write32(segment, bus, dev, func, offset + 4, 0xFFFF_FFFF);
            let size_low = ecam_read32(segment, bus, dev, func, offset);
            let size_high = ecam_read32(segment, bus, dev, func, offset + 4);
            ecam_write32(segment, bus, dev, func, offset, raw);
            ecam_write32(segment, bus, dev, func, offset + 4, raw_high);

            let size_mask = ((size_high as u64) << 32) | ((size_low & 0xFFFF_FFF0) as u64);
            let size = !size_mask + 1;

            PciBar {
                base,
                size,
                bar_type: BarType::Memory64,
                prefetchable,
            }
        }
        _ => PciBar::default(),
    }
}

// ─── Capability Walking ─────────────────────────────────────────────

fn find_capability(segment: u16, bus: u8, dev: u8, func: u8, cap_id: u8) -> Option<u8> {
    let status = ecam_read16(segment, bus, dev, func, PCI_STATUS);
    if status & (1 << 4) == 0 {
        return None; // No capabilities list
    }

    let mut ptr = ecam_read8(segment, bus, dev, func, PCI_CAPABILITIES_PTR) & 0xFC;
    let mut visited = 0u8;

    while ptr != 0 && visited < 48 {
        let id = ecam_read8(segment, bus, dev, func, ptr as usize);
        if id == cap_id {
            return Some(ptr);
        }
        ptr = ecam_read8(segment, bus, dev, func, ptr as usize + 1) & 0xFC;
        visited += 1;
    }

    None
}

// ─── MSI/MSI-X Configuration ───────────────────────────────────────

/// Enable MSI for a device
pub fn enable_msi(device: &PcieDevice, vector: u8, target_cpu: u8) -> bool {
    if !device.has_msi || device.msi_offset == 0 {
        return false;
    }

    let offset = device.msi_offset as usize;

    // MSI address: 0xFEE00000 + (CPU << 12)
    let msi_addr: u32 = 0xFEE00000 | ((target_cpu as u32) << 12);
    let msi_data: u16 = vector as u16;

    // Read message control
    let msg_ctrl = ecam_read16(
        device.segment,
        device.bus,
        device.device,
        device.function,
        offset + 2,
    );
    let is_64bit = (msg_ctrl >> 7) & 1 == 1;

    // Write address
    ecam_write32(
        device.segment,
        device.bus,
        device.device,
        device.function,
        offset + 4,
        msi_addr,
    );

    if is_64bit {
        ecam_write32(
            device.segment,
            device.bus,
            device.device,
            device.function,
            offset + 8,
            0,
        );
        ecam_write16(
            device.segment,
            device.bus,
            device.device,
            device.function,
            offset + 12,
            msi_data,
        );
    } else {
        ecam_write16(
            device.segment,
            device.bus,
            device.device,
            device.function,
            offset + 8,
            msi_data,
        );
    }

    // Enable MSI (bit 0 of message control)
    ecam_write16(
        device.segment,
        device.bus,
        device.device,
        device.function,
        offset + 2,
        msg_ctrl | 1,
    );

    // Disable legacy interrupts
    let cmd = ecam_read16(
        device.segment,
        device.bus,
        device.device,
        device.function,
        PCI_COMMAND,
    );
    ecam_write16(
        device.segment,
        device.bus,
        device.device,
        device.function,
        PCI_COMMAND,
        cmd | PCI_CMD_INTERRUPT_DISABLE,
    );

    true
}

/// Enable bus mastering for a device
pub fn enable_bus_master(device: &PcieDevice) {
    let cmd = ecam_read16(
        device.segment,
        device.bus,
        device.device,
        device.function,
        PCI_COMMAND,
    );
    ecam_write16(
        device.segment,
        device.bus,
        device.device,
        device.function,
        PCI_COMMAND,
        cmd | PCI_CMD_BUS_MASTER | PCI_CMD_MEMORY_SPACE,
    );
}

// ─── Device Enumeration ────────────────────────────────────────────

fn enumerate_function(segment: u16, bus: u8, dev: u8, func: u8) -> Option<PcieDevice> {
    let vendor = ecam_read16(segment, bus, dev, func, PCI_VENDOR_ID);
    if vendor == 0xFFFF || vendor == 0x0000 {
        return None;
    }

    let device_id = ecam_read16(segment, bus, dev, func, PCI_DEVICE_ID);
    let class_code = ecam_read8(segment, bus, dev, func, PCI_CLASS);
    let subclass = ecam_read8(segment, bus, dev, func, PCI_SUBCLASS);
    let prog_if = ecam_read8(segment, bus, dev, func, PCI_PROG_IF);
    let revision = ecam_read8(segment, bus, dev, func, PCI_REVISION);
    let header_type = ecam_read8(segment, bus, dev, func, PCI_HEADER_TYPE);
    let interrupt_line = ecam_read8(segment, bus, dev, func, PCI_INTERRUPT_LINE);
    let interrupt_pin = ecam_read8(segment, bus, dev, func, PCI_INTERRUPT_PIN);
    let subsystem_vendor = ecam_read16(segment, bus, dev, func, PCI_SUBSYSTEM_VENDOR);
    let subsystem_id = ecam_read16(segment, bus, dev, func, PCI_SUBSYSTEM_ID);

    // Decode BARs (only for type 0 headers)
    let mut bars = [PciBar::default(); 6];
    if header_type & 0x7F == 0 {
        let mut i = 0;
        while i < 6 {
            bars[i] = decode_bar(segment, bus, dev, func, i);
            if bars[i].bar_type == BarType::Memory64 {
                i += 1; // Skip next BAR (used as upper 32 bits)
            }
            i += 1;
        }
    }

    // Find capabilities
    let msi = find_capability(segment, bus, dev, func, PCI_CAP_MSI);
    let msix = find_capability(segment, bus, dev, func, PCI_CAP_MSIX);
    let pcie = find_capability(segment, bus, dev, func, PCI_CAP_PCIE);
    let pm = find_capability(segment, bus, dev, func, PCI_CAP_PM);

    Some(PcieDevice {
        segment,
        bus,
        device: dev,
        function: func,
        vendor_id: vendor,
        device_id,
        class_code,
        subclass,
        prog_if,
        revision,
        header_type: header_type & 0x7F,
        interrupt_line,
        interrupt_pin,
        subsystem_vendor,
        subsystem_id,
        bars,
        has_msi: msi.is_some(),
        has_msix: msix.is_some(),
        is_pcie: pcie.is_some(),
        msi_offset: msi.unwrap_or(0),
        msix_offset: msix.unwrap_or(0),
        pcie_offset: pcie.unwrap_or(0),
        pm_offset: pm.unwrap_or(0),
    })
}

fn enumerate_bus(segment: u16, bus: u8, devices: &mut Vec<PcieDevice>) {
    for dev in 0..32u8 {
        let vendor = ecam_read16(segment, bus, dev, 0, PCI_VENDOR_ID);
        if vendor == 0xFFFF || vendor == 0x0000 {
            continue;
        }

        if let Some(device) = enumerate_function(segment, bus, dev, 0) {
            let is_multifunction = device.header_type & 0x80 != 0
                || ecam_read8(segment, bus, dev, 0, PCI_HEADER_TYPE) & 0x80 != 0;
            devices.push(device);

            if is_multifunction {
                for func in 1..8u8 {
                    if let Some(device) = enumerate_function(segment, bus, dev, func) {
                        devices.push(device);
                    }
                }
            }
        }
    }
}

/// Enumerate all PCIe devices
pub fn enumerate_all() -> Vec<PcieDevice> {
    let mut devices = Vec::new();
    let regions = ECAM_REGIONS.lock();

    for region in regions.iter() {
        for bus in region.start_bus..=region.end_bus {
            enumerate_bus(region.segment, bus, &mut devices);
        }
    }

    devices
}

/// Get class name for display
pub fn class_name(class: u8, subclass: u8) -> &'static str {
    match (class, subclass) {
        (0x00, _) => "Unclassified",
        (0x01, 0x00) => "SCSI Controller",
        (0x01, 0x01) => "IDE Controller",
        (0x01, 0x05) => "ATA Controller",
        (0x01, 0x06) => "SATA Controller",
        (0x01, 0x08) => "NVMe Controller",
        (0x01, _) => "Mass Storage",
        (0x02, 0x00) => "Ethernet Controller",
        (0x02, 0x80) => "Network Controller",
        (0x02, _) => "Network Controller",
        (0x03, 0x00) => "VGA Controller",
        (0x03, 0x01) => "XGA Controller",
        (0x03, 0x02) => "3D Controller",
        (0x03, _) => "Display Controller",
        (0x04, 0x00) => "Video Device",
        (0x04, 0x01) => "Audio Device",
        (0x04, 0x03) => "Audio Device",
        (0x04, _) => "Multimedia",
        (0x05, _) => "Memory Controller",
        (0x06, 0x00) => "Host Bridge",
        (0x06, 0x01) => "ISA Bridge",
        (0x06, 0x04) => "PCI-to-PCI Bridge",
        (0x06, _) => "Bridge Device",
        (0x07, _) => "Communication Controller",
        (0x08, _) => "System Peripheral",
        (0x09, _) => "Input Device",
        (0x0A, _) => "Docking Station",
        (0x0B, _) => "Processor",
        (0x0C, 0x03) => "USB Controller",
        (0x0C, _) => "Serial Bus Controller",
        (0x0D, _) => "Wireless Controller",
        (0x0E, _) => "Intelligent I/O",
        (0x0F, _) => "Satellite Controller",
        (0x10, _) => "Encryption Controller",
        (0x11, _) => "Signal Processing",
        (0x12, _) => "Processing Accelerator",
        _ => "Unknown Device",
    }
}

/// Find devices by class/subclass
pub fn find_by_class(class: u8, subclass: u8) -> Vec<PcieDevice> {
    PCIE_DEVICES
        .lock()
        .iter()
        .filter(|d| d.class_code == class && d.subclass == subclass)
        .cloned()
        .collect()
}

/// Find device by vendor/device ID
pub fn find_by_id(vendor: u16, device: u16) -> Option<PcieDevice> {
    PCIE_DEVICES
        .lock()
        .iter()
        .find(|d| d.vendor_id == vendor && d.device_id == device)
        .cloned()
}

/// Get the number of discovered devices
pub fn device_count() -> usize {
    PCIE_DEVICES.lock().len()
}

// ─── Init ───────────────────────────────────────────────────────────

pub fn init() {
    serial_println!("[KnoxOS] PCIe ECAM subsystem initializing...");

    // Get ECAM regions from ACPI MCFG table
    let mcfg_entries = crate::acpi_tables::pcie_ecam_entries();

    if mcfg_entries.is_empty() {
        serial_println!("[ECAM] No MCFG entries found, PCIe ECAM not available");
        return;
    }

    {
        let mut regions = ECAM_REGIONS.lock();
        for entry in &mcfg_entries {
            let seg = entry.segment_group;
            let sbus = entry.start_bus;
            let ebus = entry.end_bus;
            let base = entry.base_address;
            regions.push(EcamRegion {
                base_address: base,
                segment: seg,
                start_bus: sbus,
                end_bus: ebus,
            });
            serial_println!(
                "[ECAM] Region: segment {}, bus {}-{}, base 0x{:X}",
                seg,
                sbus,
                ebus,
                base
            );
        }
    }

    ECAM_INITIALIZED.store(true, Ordering::Relaxed);

    // Enumerate devices
    let devices = enumerate_all();
    let count = devices.len();

    serial_println!("[ECAM] Discovered {} PCIe devices:", count);
    for dev in &devices {
        serial_println!(
            "[ECAM]   {:04X}:{:02X}:{:02X}.{} {:04X}:{:04X} {} [MSI:{} MSI-X:{} PCIe:{}]",
            dev.segment,
            dev.bus,
            dev.device,
            dev.function,
            dev.vendor_id,
            dev.device_id,
            class_name(dev.class_code, dev.subclass),
            dev.has_msi,
            dev.has_msix,
            dev.is_pcie
        );
    }

    *PCIE_DEVICES.lock() = devices;

    serial_println!("[KnoxOS] PCIe ECAM initialized, {} devices found", count);
}

// ─── MCFG ACPI Table Parsing ────────────────────────────────────────

/// MCFG table header (44 bytes standard ACPI header)
#[repr(C, packed)]
struct McfgHeader {
    signature: [u8; 4], // "MCFG"
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
    reserved: [u8; 8], // 8 bytes reserved
}

/// Parse MCFG table from raw ACPI data
/// Returns list of ECAM base address allocation structures
pub fn parse_mcfg_table(data: &[u8]) -> Vec<EcamRegion> {
    let mut regions = Vec::new();

    if data.len() < 44 {
        serial_println!("[ECAM] MCFG table too short: {} bytes", data.len());
        return regions;
    }

    // Verify signature
    if &data[0..4] != b"MCFG" {
        serial_println!("[ECAM] Not an MCFG table");
        return regions;
    }

    let table_length = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;

    // Verify checksum
    let mut sum: u8 = 0;
    for i in 0..table_length.min(data.len()) {
        sum = sum.wrapping_add(data[i]);
    }
    if sum != 0 {
        serial_println!("[ECAM] MCFG checksum invalid");
    }

    // Parse allocation structures (each 16 bytes, starting at offset 44)
    let entry_start = 44;
    let entry_size = 16;
    let num_entries = (table_length.saturating_sub(entry_start)) / entry_size;

    serial_println!(
        "[ECAM] MCFG table: {} bytes, {} allocation entries",
        table_length,
        num_entries
    );

    for i in 0..num_entries {
        let offset = entry_start + i * entry_size;
        if offset + entry_size > data.len() {
            break;
        }

        let base_address = u64::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]);
        let segment = u16::from_le_bytes([data[offset + 8], data[offset + 9]]);
        let start_bus = data[offset + 10];
        let end_bus = data[offset + 11];
        // bytes 12-15 are reserved

        serial_println!(
            "[ECAM] MCFG entry {}: base=0x{:X} segment={} bus={}-{}",
            i,
            base_address,
            segment,
            start_bus,
            end_bus
        );

        regions.push(EcamRegion {
            base_address,
            segment,
            start_bus,
            end_bus,
        });
    }

    regions
}

/// Read from PCIe extended configuration space (offsets 256-4095)
/// This is only available via ECAM memory-mapped access
pub fn ecam_read_extended(
    segment: u16,
    bus: u8,
    device: u8,
    function: u8,
    offset: u16,
) -> Option<u32> {
    if !(256..4096).contains(&offset) {
        return None;
    }

    let regions = ECAM_REGIONS.lock();
    let region = regions
        .iter()
        .find(|r| r.segment == segment && bus >= r.start_bus && bus <= r.end_bus)?;

    let addr = region.base_address
        + (((bus - region.start_bus) as u64) << 20)
        + ((device as u64) << 15)
        + ((function as u64) << 12)
        + offset as u64;

    // Memory-mapped read
    let ptr = addr as *const u32;
    Some(unsafe { core::ptr::read_volatile(ptr) })
}

/// Configure MSI-X for a device using ECAM access
pub fn configure_msix(dev: &PcieDevice, num_vectors: u16) -> Result<(), &'static str> {
    if !dev.has_msix || dev.msix_offset == 0 {
        return Err("device does not support MSI-X");
    }

    serial_println!(
        "[ECAM] Configuring MSI-X for {:04X}:{:04X} ({} vectors)",
        dev.vendor_id,
        dev.device_id,
        num_vectors
    );

    // MSI-X capability structure:
    //   +0: Capability ID (0x11)
    //   +2: Message Control (table size, function mask, enable)
    //   +4: Table Offset/BIR
    //   +8: PBA Offset/BIR

    // Read Message Control
    let cap_offset = dev.msix_offset as u16;
    let msg_ctrl = ecam_config_read16(dev, cap_offset + 2);
    let table_size = (msg_ctrl & 0x7FF) + 1;

    serial_println!(
        "[ECAM] MSI-X table size: {}, requested: {}",
        table_size,
        num_vectors
    );

    // Enable MSI-X (set bit 15 of Message Control)
    let new_msg_ctrl = msg_ctrl | (1 << 15);
    ecam_config_write16(dev, cap_offset + 2, new_msg_ctrl);

    Ok(())
}

/// Helper: read 16-bit value from device config space
fn ecam_config_read16(dev: &PcieDevice, offset: u16) -> u16 {
    let regions = ECAM_REGIONS.lock();
    if let Some(region) = regions
        .iter()
        .find(|r| r.segment == dev.segment && dev.bus >= r.start_bus && dev.bus <= r.end_bus)
    {
        let addr = region.base_address
            + (((dev.bus - region.start_bus) as u64) << 20)
            + ((dev.device as u64) << 15)
            + ((dev.function as u64) << 12)
            + offset as u64;
        let ptr = addr as *const u16;
        unsafe { core::ptr::read_volatile(ptr) }
    } else {
        0xFFFF
    }
}

/// Helper: write 16-bit value to device config space
fn ecam_config_write16(dev: &PcieDevice, offset: u16, value: u16) {
    let regions = ECAM_REGIONS.lock();
    if let Some(region) = regions
        .iter()
        .find(|r| r.segment == dev.segment && dev.bus >= r.start_bus && dev.bus <= r.end_bus)
    {
        let addr = region.base_address
            + (((dev.bus - region.start_bus) as u64) << 20)
            + ((dev.device as u64) << 15)
            + ((dev.function as u64) << 12)
            + offset as u64;
        let ptr = addr as *mut u16;
        unsafe { core::ptr::write_volatile(ptr, value) }
    }
}

/// Get all discovered PCIe devices
pub fn get_devices() -> Vec<PcieDevice> {
    PCIE_DEVICES.lock().clone()
}

/// Find a device by vendor/device ID
pub fn find_device(vendor_id: u16, device_id: u16) -> Option<PcieDevice> {
    PCIE_DEVICES
        .lock()
        .iter()
        .find(|d| d.vendor_id == vendor_id && d.device_id == device_id)
        .cloned()
}

/// Find all devices matching a class/subclass (alternate lookup)
pub fn find_devices_by_class(class: u8, subclass: u8) -> Vec<PcieDevice> {
    PCIE_DEVICES
        .lock()
        .iter()
        .filter(|d| d.class_code == class && d.subclass == subclass)
        .cloned()
        .collect()
}
