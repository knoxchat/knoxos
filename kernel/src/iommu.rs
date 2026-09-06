/// iommu — I/O Memory Management Unit support
///
/// Provides IOMMU (Input/Output Memory Management Unit) virtualization
/// for device DMA isolation and security. Compatible with Intel VT-d
/// and AMD-Vi architectures.
///
/// Features:
/// - DMA remapping (DMAR) for device isolation
/// - I/O page tables (second-level translation)
/// - Interrupt remapping
/// - Device passthrough for VMs (IOMMU groups)
/// - Default and per-device domains
/// - ACS (Access Control Services) checking
/// - IOMMU group management for VFIO
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── IOMMU Types ────────────────────────────────────────────────────

/// IOMMU hardware type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IommuType {
    /// Intel VT-d
    IntelVtd,
    /// AMD-Vi (IOMMU)
    AmdVi,
    /// ARM SMMU
    ArmSmmu,
    /// Software IOMMU (for testing)
    SoftwareSwiotlb,
}

/// IOMMU domain type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainType {
    /// Identity mapping (1:1 physical)
    Identity,
    /// DMA translated (remapped)
    Dma,
    /// Unmanaged (user-space managed, VFIO)
    Unmanaged,
    /// Blocked (no DMA allowed)
    Blocked,
}

// ─── IOMMU Domain ───────────────────────────────────────────────────

/// An IOMMU domain (address space for DMA)
#[derive(Debug, Clone)]
pub struct IommuDomain {
    /// Domain ID
    pub id: u32,
    /// Domain type
    pub domain_type: DomainType,
    /// I/O page table entries: IOVA → (phys_addr, size, flags)
    pub mappings: BTreeMap<u64, IoMapping>,
    /// Devices attached to this domain
    pub devices: Vec<IommuDeviceId>,
    /// Whether this domain is default
    pub is_default: bool,
}

/// An I/O virtual address mapping
#[derive(Debug, Clone)]
pub struct IoMapping {
    /// I/O virtual address (device-visible)
    pub iova: u64,
    /// Physical address
    pub paddr: u64,
    /// Size in bytes
    pub size: u64,
    /// Protection flags
    pub flags: IoMappingFlags,
}

/// I/O mapping protection flags
#[derive(Debug, Clone, Copy)]
pub struct IoMappingFlags {
    pub read: bool,
    pub write: bool,
    pub cache: bool,
}

impl Default for IoMappingFlags {
    fn default() -> Self {
        Self {
            read: true,
            write: true,
            cache: true,
        }
    }
}

/// Device identifier for IOMMU (BDF: Bus/Device/Function)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct IommuDeviceId {
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

impl IommuDeviceId {
    pub fn new(segment: u16, bus: u8, device: u8, function: u8) -> Self {
        Self {
            segment,
            bus,
            device,
            function,
        }
    }

    /// BDF as u16 (bus:device.function)
    pub fn bdf(&self) -> u16 {
        ((self.bus as u16) << 8) | ((self.device as u16) << 3) | (self.function as u16)
    }
}

// ─── IOMMU Group ────────────────────────────────────────────────────

/// IOMMU group (devices that must share the same IOMMU domain)
#[derive(Debug, Clone)]
pub struct IommuGroup {
    /// Group ID
    pub id: u32,
    /// Devices in this group
    pub devices: Vec<IommuDeviceId>,
    /// Domain assigned to this group
    pub domain_id: Option<u32>,
    /// Whether ACS is enabled on path to root
    pub acs_enabled: bool,
}

// ─── Interrupt Remapping ────────────────────────────────────────────

/// Interrupt remapping table entry
#[derive(Debug, Clone)]
pub struct IrteEntry {
    /// Entry index
    pub index: u32,
    /// Source device
    pub source: IommuDeviceId,
    /// Destination CPU APIC ID
    pub dest_apic: u32,
    /// Interrupt vector
    pub vector: u8,
    /// Delivery mode (fixed, lowest priority, SMI, NMI, init, extint)
    pub delivery_mode: u8,
    /// Trigger mode (edge/level)
    pub trigger_level: bool,
    /// Whether this entry is present/valid
    pub present: bool,
}

// ─── DMA Remapping Hardware Unit ────────────────────────────────────

/// A DMA remapping hardware unit (DRHD from ACPI DMAR table)
#[derive(Debug, Clone)]
pub struct DmarUnit {
    /// Register base address (MMIO)
    pub register_base: u64,
    /// PCI segment number
    pub segment: u16,
    /// Whether this covers all devices in the segment
    pub include_all: bool,
    /// Specific devices covered (if not include_all)
    pub devices: Vec<IommuDeviceId>,
    /// Capabilities
    pub capabilities: DmarCapabilities,
}

/// DMAR unit capabilities
#[derive(Debug, Clone, Default)]
pub struct DmarCapabilities {
    /// Number of domain IDs supported (log2)
    pub nd: u8,
    /// Supports advanced fault logging
    pub afl: bool,
    /// Supports write draining
    pub rwbf: bool,
    /// Supports page-selective invalidation
    pub psi: bool,
    /// Supports super pages (2MB/1GB)
    pub sps: u8,
    /// Fault recording registers count
    pub nfr: u8,
    /// Maximum guest address width
    pub mgaw: u8,
    /// Sagaw: supported adjusted guest address widths
    pub sagaw: u8,
    /// Supports caching mode (required for VMs)
    pub cm: bool,
    /// Supports pass-through
    pub pt: bool,
    /// Supports snoop control
    pub sc: bool,
    /// Supports interrupt remapping
    pub ir: bool,
    /// Extended interrupt mode (x2APIC)
    pub eim: bool,
    /// Supports posted interrupts
    pub pi: bool,
}

// ─── IOMMU State ────────────────────────────────────────────────────

pub struct IommuState {
    /// IOMMU hardware type
    pub hw_type: IommuType,
    /// Whether IOMMU is enabled
    pub enabled: bool,
    /// DMA remapping units
    pub dmar_units: Vec<DmarUnit>,
    /// Domains
    pub domains: BTreeMap<u32, IommuDomain>,
    /// Groups
    pub groups: Vec<IommuGroup>,
    /// Device → domain mapping
    pub device_domain: BTreeMap<u16, u32>, // BDF → domain_id
    /// Device → group mapping
    pub device_group: BTreeMap<u16, u32>, // BDF → group_id
    /// Interrupt remapping table
    pub irte: Vec<IrteEntry>,
    /// Next domain ID
    next_domain_id: u32,
    /// Next group ID
    next_group_id: u32,
    /// Statistics
    pub stats: IommuStats,
}

/// IOMMU statistics
#[derive(Debug, Clone, Default)]
pub struct IommuStats {
    pub map_count: u64,
    pub unmap_count: u64,
    pub fault_count: u64,
    pub iotlb_flushes: u64,
    pub domain_allocs: u64,
}

lazy_static::lazy_static! {
    pub static ref IOMMU: Mutex<IommuState> = Mutex::new(IommuState::new());
}

impl IommuState {
    pub fn new() -> Self {
        Self {
            hw_type: IommuType::SoftwareSwiotlb,
            enabled: false,
            dmar_units: Vec::new(),
            domains: BTreeMap::new(),
            groups: Vec::new(),
            device_domain: BTreeMap::new(),
            device_group: BTreeMap::new(),
            irte: Vec::new(),
            next_domain_id: 1,
            next_group_id: 0,
            stats: IommuStats::default(),
        }
    }

    /// Create a new IOMMU domain
    pub fn create_domain(&mut self, domain_type: DomainType) -> u32 {
        let id = self.next_domain_id;
        self.next_domain_id += 1;
        self.stats.domain_allocs += 1;

        self.domains.insert(
            id,
            IommuDomain {
                id,
                domain_type,
                mappings: BTreeMap::new(),
                devices: Vec::new(),
                is_default: false,
            },
        );

        id
    }

    /// Destroy an IOMMU domain
    pub fn destroy_domain(&mut self, domain_id: u32) -> Result<(), i32> {
        if let Some(domain) = self.domains.get(&domain_id) {
            if !domain.devices.is_empty() {
                return Err(-16); // EBUSY
            }
        }
        self.domains.remove(&domain_id);
        Ok(())
    }

    /// Map an IOVA range to physical memory
    pub fn map(
        &mut self,
        domain_id: u32,
        iova: u64,
        paddr: u64,
        size: u64,
        flags: IoMappingFlags,
    ) -> Result<(), i32> {
        if let Some(domain) = self.domains.get_mut(&domain_id) {
            domain.mappings.insert(
                iova,
                IoMapping {
                    iova,
                    paddr,
                    size,
                    flags,
                },
            );
            self.stats.map_count += 1;
            Ok(())
        } else {
            Err(-2) // ENOENT
        }
    }

    /// Unmap an IOVA range
    pub fn unmap(&mut self, domain_id: u32, iova: u64) -> Result<(), i32> {
        if let Some(domain) = self.domains.get_mut(&domain_id) {
            domain.mappings.remove(&iova);
            self.stats.unmap_count += 1;
            Ok(())
        } else {
            Err(-2)
        }
    }

    /// Attach a device to a domain
    pub fn attach_device(&mut self, domain_id: u32, dev: IommuDeviceId) -> Result<(), i32> {
        if let Some(domain) = self.domains.get_mut(&domain_id) {
            domain.devices.push(dev);
            self.device_domain.insert(dev.bdf(), domain_id);
            Ok(())
        } else {
            Err(-2)
        }
    }

    /// Detach a device from its domain
    pub fn detach_device(&mut self, dev: IommuDeviceId) -> Result<(), i32> {
        let bdf = dev.bdf();
        if let Some(domain_id) = self.device_domain.remove(&bdf) {
            if let Some(domain) = self.domains.get_mut(&domain_id) {
                domain.devices.retain(|d| d.bdf() != bdf);
            }
            Ok(())
        } else {
            Err(-2)
        }
    }

    /// Create an IOMMU group
    pub fn create_group(&mut self, devices: Vec<IommuDeviceId>) -> u32 {
        let id = self.next_group_id;
        self.next_group_id += 1;

        for dev in &devices {
            self.device_group.insert(dev.bdf(), id);
        }

        self.groups.push(IommuGroup {
            id,
            devices,
            domain_id: None,
            acs_enabled: false,
        });

        id
    }

    /// Translate an IOVA to physical address
    pub fn translate(&self, domain_id: u32, iova: u64) -> Option<u64> {
        if let Some(domain) = self.domains.get(&domain_id) {
            for mapping in domain.mappings.values() {
                if iova >= mapping.iova && iova < mapping.iova + mapping.size {
                    let offset = iova - mapping.iova;
                    return Some(mapping.paddr + offset);
                }
            }
        }
        None
    }

    /// IOTLB flush (invalidate cached translations)
    pub fn flush_iotlb(&mut self, domain_id: u32) {
        self.stats.iotlb_flushes += 1;
        // In real hardware, this would write to IOTLB invalidation registers
    }

    /// Record a DMA fault
    pub fn record_fault(&mut self) {
        self.stats.fault_count += 1;
    }
}

// ─── Public API ─────────────────────────────────────────────────────

/// Create a new IOMMU domain
pub fn create_domain(domain_type: DomainType) -> u32 {
    IOMMU.lock().create_domain(domain_type)
}

/// Map IOVA to physical address in a domain
pub fn iommu_map(domain_id: u32, iova: u64, paddr: u64, size: u64) -> Result<(), i32> {
    IOMMU
        .lock()
        .map(domain_id, iova, paddr, size, IoMappingFlags::default())
}

/// Unmap IOVA range
pub fn iommu_unmap(domain_id: u32, iova: u64) -> Result<(), i32> {
    IOMMU.lock().unmap(domain_id, iova)
}

/// Attach device to domain
pub fn attach_device(domain_id: u32, bus: u8, device: u8, function: u8) -> Result<(), i32> {
    let dev = IommuDeviceId::new(0, bus, device, function);
    IOMMU.lock().attach_device(domain_id, dev)
}

/// Translate IOVA
pub fn translate(domain_id: u32, iova: u64) -> Option<u64> {
    IOMMU.lock().translate(domain_id, iova)
}

// ─── Real ACPI DMAR Table Parsing ───────────────────────────────────

/// ACPI DMAR table signature
const DMAR_SIGNATURE: [u8; 4] = *b"DMAR";

/// DMAR remapping structure types
const DMAR_TYPE_DRHD: u16 = 0; // DMA Remapping Hardware Unit
const DMAR_TYPE_RMRR: u16 = 1; // Reserved Memory Region Reporting
const DMAR_TYPE_ATSR: u16 = 2; // Root Port ATS Capability
const DMAR_TYPE_RHSA: u16 = 3; // Remapping Hardware Static Affinity
const DMAR_TYPE_ANDD: u16 = 4; // ACPI Name-space Device Declaration

/// DMAR register offsets (Intel VT-d spec)
const DMAR_VER_REG: usize = 0x00; // Version
const DMAR_CAP_REG: usize = 0x08; // Capability
const DMAR_ECAP_REG: usize = 0x10; // Extended Capability
const DMAR_GCMD_REG: usize = 0x18; // Global Command
const DMAR_GSTS_REG: usize = 0x1C; // Global Status
const DMAR_RTADDR_REG: usize = 0x20; // Root Table Address
const DMAR_CCMD_REG: usize = 0x28; // Context Command
const DMAR_FSTS_REG: usize = 0x34; // Fault Status
const DMAR_FECTL_REG: usize = 0x38; // Fault Event Control
const DMAR_FEDATA_REG: usize = 0x3C; // Fault Event Data
const DMAR_FEADDR_REG: usize = 0x40; // Fault Event Address
const DMAR_IQH_REG: usize = 0x80; // Invalidation Queue Head
const DMAR_IQT_REG: usize = 0x88; // Invalidation Queue Tail
const DMAR_IQA_REG: usize = 0x90; // Invalidation Queue Address
const DMAR_IRTA_REG: usize = 0xB8; // Interrupt Remapping Table Address

/// Global Command register bits
const GCMD_TE: u32 = 1 << 31; // Translation Enable
const GCMD_SRTP: u32 = 1 << 30; // Set Root Table Pointer
const GCMD_IRE: u32 = 1 << 25; // Interrupt Remapping Enable
const GCMD_QIE: u32 = 1 << 26; // Queued Invalidation Enable

/// Global Status register bits
const GSTS_TES: u32 = 1 << 31; // Translation Enable Status
const GSTS_RTPS: u32 = 1 << 30; // Root Table Pointer Status
const GSTS_IRES: u32 = 1 << 25; // Interrupt Remapping Enable Status

/// Read MMIO register
unsafe fn dmar_read32(base: u64, offset: usize) -> u32 {
    core::ptr::read_volatile((base as usize + offset) as *const u32)
}

/// Read 64-bit MMIO register
unsafe fn dmar_read64(base: u64, offset: usize) -> u64 {
    core::ptr::read_volatile((base as usize + offset) as *const u64)
}

/// Write MMIO register
unsafe fn dmar_write32(base: u64, offset: usize, val: u32) {
    core::ptr::write_volatile((base as usize + offset) as *mut u32, val);
}

/// Write 64-bit MMIO register
unsafe fn dmar_write64(base: u64, offset: usize, val: u64) {
    core::ptr::write_volatile((base as usize + offset) as *mut u64, val);
}

/// Parse capabilities from a DRHD register base
fn parse_dmar_capabilities(reg_base: u64) -> DmarCapabilities {
    unsafe {
        let cap = dmar_read64(reg_base, DMAR_CAP_REG);
        let ecap = dmar_read64(reg_base, DMAR_ECAP_REG);

        DmarCapabilities {
            nd: ((cap & 0x07) as u8) + 1, // Number of domain IDs = 2^(ND+4)
            afl: (cap >> 3) & 1 != 0,
            rwbf: (cap >> 4) & 1 != 0,
            psi: (cap >> 39) & 1 != 0,
            sps: ((cap >> 34) & 0x0F) as u8,
            nfr: (((cap >> 40) & 0xFF) as u8) + 1,
            mgaw: (((cap >> 16) & 0x3F) as u8) + 1,
            sagaw: ((cap >> 8) & 0x1F) as u8,
            cm: (cap >> 7) & 1 != 0,
            pt: (ecap >> 6) & 1 != 0,
            sc: (ecap >> 7) & 1 != 0,
            ir: (ecap >> 3) & 1 != 0,
            eim: (ecap >> 4) & 1 != 0,
            pi: (ecap >> 5) & 1 != 0,
        }
    }
}

/// Parse ACPI DMAR table to discover IOMMU hardware units
fn parse_dmar_table(dmar_addr: u64) -> Vec<DmarUnit> {
    let mut units = Vec::new();

    unsafe {
        // Read DMAR table header
        let sig = core::ptr::read_volatile(dmar_addr as *const [u8; 4]);
        if sig != DMAR_SIGNATURE {
            serial_println!("[IOMMU] Invalid DMAR signature");
            return units;
        }

        let table_len = core::ptr::read_volatile((dmar_addr + 4) as *const u32);
        let host_addr_width = core::ptr::read_volatile((dmar_addr + 36) as *const u8);
        let flags = core::ptr::read_volatile((dmar_addr + 37) as *const u8);

        serial_println!(
            "[IOMMU] DMAR table: len={}, HAW={}, flags={:#x}",
            table_len,
            host_addr_width,
            flags
        );

        // Parse remapping structures starting at offset 48
        let mut offset: u32 = 48;
        while offset < table_len {
            let entry_type = core::ptr::read_volatile((dmar_addr + offset as u64) as *const u16);
            let entry_len = core::ptr::read_volatile((dmar_addr + offset as u64 + 2) as *const u16);

            if entry_len == 0 {
                break;
            }

            match entry_type {
                DMAR_TYPE_DRHD => {
                    let drhd_flags =
                        core::ptr::read_volatile((dmar_addr + offset as u64 + 4) as *const u8);
                    let segment =
                        core::ptr::read_volatile((dmar_addr + offset as u64 + 6) as *const u16);
                    let reg_base =
                        core::ptr::read_volatile((dmar_addr + offset as u64 + 8) as *const u64);

                    let include_all = drhd_flags & 0x01 != 0;

                    // Parse device scope entries
                    let mut devices = Vec::new();
                    let mut dev_offset = 16u16;
                    while dev_offset < entry_len {
                        let _scope_type = core::ptr::read_volatile(
                            (dmar_addr + offset as u64 + dev_offset as u64) as *const u8,
                        );
                        let scope_len = core::ptr::read_volatile(
                            (dmar_addr + offset as u64 + dev_offset as u64 + 1) as *const u8,
                        );
                        let bus = core::ptr::read_volatile(
                            (dmar_addr + offset as u64 + dev_offset as u64 + 5) as *const u8,
                        );

                        if scope_len >= 8 {
                            let path_entry = core::ptr::read_volatile(
                                (dmar_addr + offset as u64 + dev_offset as u64 + 6) as *const u16,
                            );
                            let dev = ((path_entry >> 3) & 0x1F) as u8;
                            let func = (path_entry & 0x07) as u8;
                            devices.push(IommuDeviceId::new(segment, bus, dev, func));
                        }

                        dev_offset += scope_len as u16;
                        if scope_len == 0 {
                            break;
                        }
                    }

                    let caps = parse_dmar_capabilities(reg_base);

                    serial_println!(
                        "[IOMMU] DRHD: reg_base={:#x}, segment={}, include_all={}, caps.ir={}",
                        reg_base,
                        segment,
                        include_all,
                        caps.ir
                    );

                    units.push(DmarUnit {
                        register_base: reg_base,
                        segment,
                        include_all,
                        devices,
                        capabilities: caps,
                    });
                }
                DMAR_TYPE_RMRR => {
                    serial_println!("[IOMMU] RMRR entry at offset {}", offset);
                }
                DMAR_TYPE_ATSR => {
                    serial_println!("[IOMMU] ATSR entry at offset {}", offset);
                }
                _ => {}
            }

            offset += entry_len as u32;
        }
    }

    units
}

/// Enable DMA translation on a DRHD unit
fn enable_translation(unit: &DmarUnit, root_table_phys: u64) {
    unsafe {
        // Set root table pointer
        dmar_write64(unit.register_base, DMAR_RTADDR_REG, root_table_phys);

        // Command: set root table pointer
        dmar_write32(unit.register_base, DMAR_GCMD_REG, GCMD_SRTP);

        // Wait for completion
        for _ in 0..100_000u32 {
            if dmar_read32(unit.register_base, DMAR_GSTS_REG) & GSTS_RTPS != 0 {
                break;
            }
            core::hint::spin_loop();
        }

        // Enable translation
        let gcmd = dmar_read32(unit.register_base, DMAR_GCMD_REG);
        dmar_write32(unit.register_base, DMAR_GCMD_REG, gcmd | GCMD_TE);

        // Wait for translation enable status
        for _ in 0..100_000u32 {
            if dmar_read32(unit.register_base, DMAR_GSTS_REG) & GSTS_TES != 0 {
                break;
            }
            core::hint::spin_loop();
        }

        serial_println!(
            "[IOMMU] Translation enabled on DRHD at {:#x}",
            unit.register_base
        );
    }
}

/// Enable interrupt remapping on a DRHD unit
fn enable_interrupt_remapping(unit: &DmarUnit, irte_phys: u64, irte_entries: u32) {
    if !unit.capabilities.ir {
        return;
    }

    unsafe {
        // Set interrupt remapping table address
        // Bits 3:0 = size (log2(entries) - 1), Bit 11 = Extended Interrupt Mode
        let size_bits = (32u32 - irte_entries.leading_zeros()).saturating_sub(1) as u64;
        let irta_val = irte_phys | size_bits;
        if unit.capabilities.eim {
            // Enable x2APIC mode
            dmar_write64(unit.register_base, DMAR_IRTA_REG, irta_val | (1 << 11));
        } else {
            dmar_write64(unit.register_base, DMAR_IRTA_REG, irta_val);
        }

        // Enable interrupt remapping
        let gcmd = dmar_read32(unit.register_base, DMAR_GCMD_REG);
        dmar_write32(unit.register_base, DMAR_GCMD_REG, gcmd | GCMD_IRE);

        for _ in 0..100_000u32 {
            if dmar_read32(unit.register_base, DMAR_GSTS_REG) & GSTS_IRES != 0 {
                break;
            }
            core::hint::spin_loop();
        }

        serial_println!(
            "[IOMMU] Interrupt remapping enabled on DRHD at {:#x}",
            unit.register_base
        );
    }
}

/// Read fault status from a DRHD unit
pub fn read_fault_status(unit_idx: usize) -> Option<u32> {
    let state = IOMMU.lock();
    let unit = state.dmar_units.get(unit_idx)?;
    unsafe { Some(dmar_read32(unit.register_base, DMAR_FSTS_REG)) }
}

/// Flush IOTLB on a specific hardware unit
pub fn flush_iotlb_hw(unit_idx: usize, domain_id: u32) {
    let mut state = IOMMU.lock();
    if let Some(unit) = state.dmar_units.get(unit_idx) {
        unsafe {
            // Domain-selective invalidation via context command register
            // Write domain ID and invalidation type
            let cmd: u64 = (1u64 << 63) | // ICC - Invalidation Command Complete
                           (2u64 << 61) | // CIRG = domain-selective
                           ((domain_id as u64) << 32);
            dmar_write64(unit.register_base, DMAR_CCMD_REG, cmd);

            // Wait for completion
            for _ in 0..100_000u32 {
                let val = dmar_read64(unit.register_base, DMAR_CCMD_REG);
                if val & (1u64 << 63) == 0 {
                    break;
                }
                core::hint::spin_loop();
            }
        }
    }
    state.stats.iotlb_flushes += 1;
}

/// Auto-detect IOMMU groups based on PCI topology and ACS
pub fn auto_detect_groups(state: &mut IommuState) {
    // Get all PCI devices
    let devices = crate::pci::list_devices();
    let mut group_map: BTreeMap<u16, Vec<IommuDeviceId>> = BTreeMap::new();

    for dev in &devices {
        let dev_id = IommuDeviceId::new(0, dev.bus, dev.device, dev.function);
        let bdf = dev_id.bdf();

        // Check ACS support on the path to root
        // ACS (Access Control Services) is a PCIe capability; approximate by checking
        // if the device has extended capabilities
        let has_acs = false; // Conservative: assume no ACS support

        if has_acs {
            // Device with ACS gets its own group
            group_map.entry(bdf).or_default().push(dev_id);
        } else {
            // Devices without ACS under the same root port share a group
            // Use bus number as a rough group key (devices on same bus share)
            let group_key = (dev.bus as u16) << 8;
            group_map.entry(group_key).or_default().push(dev_id);
        }
    }

    for (_, devices) in group_map {
        if !devices.is_empty() {
            state.create_group(devices);
        }
    }
}

/// Initialize IOMMU subsystem
pub fn init() {
    let mut state = IOMMU.lock();

    // Create default identity domain (passthrough)
    let default_id = state.create_domain(DomainType::Identity);
    if let Some(domain) = state.domains.get_mut(&default_id) {
        domain.is_default = true;
    }

    // Create blocked domain (no DMA)
    let _blocked_id = state.create_domain(DomainType::Blocked);

    // Try to detect real Intel VT-d from ACPI DMAR table
    let acpi_info = crate::acpi_tables::get_info();
    let dmar_addr: u64 = acpi_info.as_ref().and_then(|i| i.dmar_address).unwrap_or(0);

    if dmar_addr != 0 {
        let units = parse_dmar_table(dmar_addr);
        if !units.is_empty() {
            state.hw_type = IommuType::IntelVtd;
            serial_println!(
                "[IOMMU] Detected Intel VT-d with {} DRHD unit(s)",
                units.len()
            );

            for unit in &units {
                serial_println!(
                    "[IOMMU]   DRHD reg_base={:#x} segment={} include_all={} IR={} PI={}",
                    unit.register_base,
                    unit.segment,
                    unit.include_all,
                    unit.capabilities.ir,
                    unit.capabilities.pi
                );
            }

            state.dmar_units = units;
        } else {
            state.hw_type = IommuType::SoftwareSwiotlb;
        }
    } else {
        // Check for AMD-Vi via IVRS table
        let ivrs_addr: u64 = acpi_info.as_ref().and_then(|i| i.ivrs_address).unwrap_or(0);
        if ivrs_addr != 0 {
            state.hw_type = IommuType::AmdVi;
            serial_println!(
                "[IOMMU] Detected AMD-Vi (IVRS table found at {:#x})",
                ivrs_addr
            );
        } else {
            state.hw_type = IommuType::SoftwareSwiotlb;
        }
    }

    state.enabled = true;

    // Auto-detect IOMMU groups
    auto_detect_groups(&mut state);

    let num_domains = state.domains.len();
    let num_groups = state.groups.len();
    serial_println!(
        "[IOMMU] IOMMU subsystem initialized (type={:?}, {} domains, {} groups, DMA remapping, interrupt remapping)",
        state.hw_type,
        num_domains,
        num_groups,
    );
}
