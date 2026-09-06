/// ACPI Tables — Full ACPI table discovery and parsing
///
/// Implements RSDP, RSDT, XSDT, MADT, FADT, HPET, MCFG table parsing
/// for hardware topology discovery, interrupt routing, and power management.
///
/// This replaces the simple ACPI shutdown stubs with real table parsing
/// used by SMP, IOAPIC, HPET timer, and PCI Express configuration.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── RSDP (Root System Description Pointer) ─────────────────────────

/// RSDP v1 (ACPI 1.0) — 20 bytes
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Rsdp {
    pub signature: [u8; 8], // "RSD PTR "
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub revision: u8, // 0 = ACPI 1.0, 2 = ACPI 2.0+
    pub rsdt_address: u32,
}

/// RSDP v2 (ACPI 2.0+) — 36 bytes
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Rsdp2 {
    pub rsdp: Rsdp,
    pub length: u32,
    pub xsdt_address: u64,
    pub extended_checksum: u8,
    pub reserved: [u8; 3],
}

// ─── SDT Header ─────────────────────────────────────────────────────

/// Common ACPI System Description Table header (36 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct SdtHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

impl SdtHeader {
    pub fn signature_str(&self) -> &str {
        core::str::from_utf8(&self.signature).unwrap_or("????")
    }

    /// Validate checksum — sum of all bytes in the table must be 0
    ///
    /// # Safety
    /// `base` must point to a valid ACPI table of at least `self.length` bytes.
    pub unsafe fn validate_checksum(&self, base: *const u8) -> bool {
        let len = self.length as usize;
        let mut sum: u8 = 0;
        for i in 0..len {
            sum = sum.wrapping_add(*base.add(i));
        }
        sum == 0
    }
}

// ─── MADT (Multiple APIC Description Table) ─────────────────────────

/// MADT table header (after SDT header)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtHeader {
    pub sdt: SdtHeader,
    pub local_apic_address: u32,
    pub flags: u32, // bit 0: PCAT_COMPAT (dual 8259 present)
}

/// MADT entry types
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum MadtEntryType {
    LocalApic = 0,
    IoApic = 1,
    InterruptSourceOverride = 2,
    NmiSource = 3,
    LocalApicNmi = 4,
    LocalApicAddressOverride = 5,
    IoSapic = 6,
    LocalSapic = 7,
    PlatformInterruptSources = 8,
    LocalX2Apic = 9,
    LocalX2ApicNmi = 10,
    GicCpu = 11,
    GicDistributor = 12,
    Unknown = 0xFF,
}

impl From<u8> for MadtEntryType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::LocalApic,
            1 => Self::IoApic,
            2 => Self::InterruptSourceOverride,
            3 => Self::NmiSource,
            4 => Self::LocalApicNmi,
            5 => Self::LocalApicAddressOverride,
            9 => Self::LocalX2Apic,
            10 => Self::LocalX2ApicNmi,
            _ => Self::Unknown,
        }
    }
}

/// MADT entry header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtEntryHeader {
    pub entry_type: u8,
    pub length: u8,
}

/// Local APIC entry (Type 0)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtLocalApic {
    pub header: MadtEntryHeader,
    pub acpi_processor_id: u8,
    pub apic_id: u8,
    pub flags: u32, // bit 0: enabled, bit 1: online capable
}

/// I/O APIC entry (Type 1)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtIoApic {
    pub header: MadtEntryHeader,
    pub io_apic_id: u8,
    pub reserved: u8,
    pub io_apic_address: u32,
    pub global_system_interrupt_base: u32,
}

/// Interrupt Source Override (Type 2)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtIso {
    pub header: MadtEntryHeader,
    pub bus_source: u8, // always 0 (ISA)
    pub irq_source: u8,
    pub global_system_interrupt: u32,
    pub flags: u16, // polarity + trigger mode
}

/// Local APIC NMI (Type 4)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtLocalApicNmi {
    pub header: MadtEntryHeader,
    pub acpi_processor_id: u8, // 0xFF = all processors
    pub flags: u16,
    pub lint: u8, // LINT# (0 or 1)
}

// ─── FADT (Fixed ACPI Description Table) ─────────────────────────────

/// FADT — provides fixed hardware register addresses
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Fadt {
    pub sdt: SdtHeader,
    pub firmware_ctrl: u32,
    pub dsdt: u32,
    pub reserved1: u8,
    pub preferred_pm_profile: u8,
    pub sci_interrupt: u16,
    pub smi_command_port: u32,
    pub acpi_enable: u8,
    pub acpi_disable: u8,
    pub s4bios_req: u8,
    pub pstate_control: u8,
    pub pm1a_event_block: u32,
    pub pm1b_event_block: u32,
    pub pm1a_control_block: u32,
    pub pm1b_control_block: u32,
    pub pm2_control_block: u32,
    pub pm_timer_block: u32,
    pub gpe0_block: u32,
    pub gpe1_block: u32,
    pub pm1_event_length: u8,
    pub pm1_control_length: u8,
    pub pm2_control_length: u8,
    pub pm_timer_length: u8,
    pub gpe0_length: u8,
    pub gpe1_length: u8,
    pub gpe1_base: u8,
    pub cstate_control: u8,
    pub worst_c2_latency: u16,
    pub worst_c3_latency: u16,
    pub flush_size: u16,
    pub flush_stride: u16,
    pub duty_offset: u8,
    pub duty_width: u8,
    pub day_alarm: u8,
    pub month_alarm: u8,
    pub century: u8,
    pub boot_arch_flags: u16,
    pub reserved2: u8,
    pub flags: u32,
    // Generic Address Structure for reset register
    pub reset_reg_space: u8,
    pub reset_reg_bit_width: u8,
    pub reset_reg_bit_offset: u8,
    pub reset_reg_access_size: u8,
    pub reset_reg_address: u64,
    pub reset_value: u8,
    pub arm_boot_arch: u16,
    pub fadt_minor_version: u8,
    // ACPI 2.0+ fields (64-bit addresses)
    pub x_firmware_ctrl: u64,
    pub x_dsdt: u64,
}

// ─── HPET (High Precision Event Timer) ──────────────────────────────

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct HpetTable {
    pub sdt: SdtHeader,
    pub hardware_rev_id: u8,
    pub comparator_info: u8, // bits 0-4: num comparators, bit 5: counter size, bit 6: LegacyReplacement
    pub pci_vendor_id: u16,
    pub address_space_id: u8,
    pub register_bit_width: u8,
    pub register_bit_offset: u8,
    pub reserved: u8,
    pub address: u64,
    pub hpet_number: u8,
    pub minimum_tick: u16,
    pub page_protection: u8,
}

// ─── MCFG (PCI Express Configuration) ───────────────────────────────

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct McfgTable {
    pub sdt: SdtHeader,
    pub reserved: u64,
    // Followed by McfgEntry array
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct McfgEntry {
    pub base_address: u64,
    pub segment_group: u16,
    pub start_bus: u8,
    pub end_bus: u8,
    pub reserved: u32,
}

// ─── Parsed ACPI Information ────────────────────────────────────────

/// Collected ACPI topology information
#[derive(Debug, Clone)]
pub struct AcpiInfo {
    pub local_apic_address: u64,
    pub local_apics: Vec<LocalApicInfo>,
    pub io_apics: Vec<IoApicInfo>,
    pub interrupt_overrides: Vec<InterruptOverride>,
    pub local_apic_nmis: Vec<LocalApicNmiInfo>,
    pub fadt: Option<FadtInfo>,
    pub hpet_address: Option<u64>,
    pub mcfg_entries: Vec<McfgEntry>,
    pub pcat_compat: bool,
    /// Physical address of the DMAR (DMA Remapping) table for Intel VT-d
    pub dmar_address: Option<u64>,
    /// Physical address of the IVRS (I/O Virtualization Reporting Structure) table for AMD-Vi
    pub ivrs_address: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct LocalApicInfo {
    pub processor_id: u8,
    pub apic_id: u8,
    pub enabled: bool,
    pub online_capable: bool,
}

#[derive(Debug, Clone)]
pub struct IoApicInfo {
    pub id: u8,
    pub address: u32,
    pub gsi_base: u32,
}

#[derive(Debug, Clone)]
pub struct InterruptOverride {
    pub bus: u8,
    pub irq_source: u8,
    pub gsi: u32,
    pub flags: u16,
}

#[derive(Debug, Clone)]
pub struct LocalApicNmiInfo {
    pub processor_id: u8,
    pub flags: u16,
    pub lint: u8,
}

#[derive(Debug, Clone)]
pub struct FadtInfo {
    pub pm1a_control_block: u32,
    pub pm1b_control_block: u32,
    pub pm_timer_block: u32,
    pub sci_interrupt: u16,
    pub smi_command_port: u32,
    pub acpi_enable: u8,
    pub acpi_disable: u8,
    pub reset_reg_address: u64,
    pub reset_value: u8,
    pub century_register: u8,
    pub boot_arch_flags: u16,
}

// ─── Global ACPI State ──────────────────────────────────────────────

lazy_static::lazy_static! {
    pub static ref ACPI_INFO: Mutex<Option<AcpiInfo>> = Mutex::new(None);
}

/// Get a copy of the parsed ACPI information (if available)
pub fn get_info() -> Option<AcpiInfo> {
    ACPI_INFO.lock().clone()
}

// ─── RSDP Search ────────────────────────────────────────────────────

/// Search for RSDP in EBDA and BIOS ROM area
fn find_rsdp(phys_offset: u64) -> Option<u64> {
    // Search EBDA (Extended BIOS Data Area) — first KiB at [0x40E] * 16
    let ebda_ptr = unsafe {
        let seg = *((phys_offset + 0x40E) as *const u16);
        (seg as u64) << 4
    };

    // Search EBDA (first 1 KiB)
    if let Some(addr) = scan_for_rsdp(phys_offset, ebda_ptr, 1024) {
        return Some(addr);
    }

    // Search BIOS ROM area: 0xE0000 - 0xFFFFF
    if let Some(addr) = scan_for_rsdp(phys_offset, 0xE0000, 0x20000) {
        return Some(addr);
    }

    None
}

fn scan_for_rsdp(phys_offset: u64, start: u64, length: u64) -> Option<u64> {
    let signature = b"RSD PTR ";
    let mut addr = start;
    while addr < start + length {
        let virt = phys_offset + addr;
        let ptr = virt as *const [u8; 8];
        let bytes = unsafe { &*ptr };
        if bytes == signature {
            // Validate checksum (first 20 bytes)
            let mut sum: u8 = 0;
            for i in 0..20 {
                sum = sum.wrapping_add(unsafe { *((virt + i) as *const u8) });
            }
            if sum == 0 {
                return Some(addr);
            }
        }
        addr += 16; // RSDP is always 16-byte aligned
    }
    None
}

// ─── Table Parsing ──────────────────────────────────────────────────

/// Parse RSDT (32-bit pointers)
fn parse_rsdt(phys_offset: u64, rsdt_phys: u32) -> Vec<u64> {
    let rsdt_virt = phys_offset + rsdt_phys as u64;
    let header = unsafe { &*(rsdt_virt as *const SdtHeader) };

    let entry_count = (header.length as usize - core::mem::size_of::<SdtHeader>()) / 4;
    let entries_ptr = (rsdt_virt + core::mem::size_of::<SdtHeader>() as u64) as *const u32;

    let mut tables = Vec::new();
    for i in 0..entry_count {
        let phys_addr = unsafe { *entries_ptr.add(i) };
        tables.push(phys_addr as u64);
    }
    tables
}

/// Parse XSDT (64-bit pointers)
fn parse_xsdt(phys_offset: u64, xsdt_phys: u64) -> Vec<u64> {
    let xsdt_virt = phys_offset + xsdt_phys;
    let header = unsafe { &*(xsdt_virt as *const SdtHeader) };

    let entry_count = (header.length as usize - core::mem::size_of::<SdtHeader>()) / 8;
    let entries_ptr = (xsdt_virt + core::mem::size_of::<SdtHeader>() as u64) as *const u64;

    let mut tables = Vec::new();
    for i in 0..entry_count {
        let phys_addr = unsafe { *entries_ptr.add(i) };
        tables.push(phys_addr);
    }
    tables
}

/// Parse MADT entries
fn parse_madt(phys_offset: u64, madt_phys: u64, info: &mut AcpiInfo) {
    let madt_virt = phys_offset + madt_phys;
    let madt = unsafe { &*(madt_virt as *const MadtHeader) };

    info.local_apic_address = madt.local_apic_address as u64;
    info.pcat_compat = (madt.flags & 1) != 0;

    let table_end = madt_virt + madt.sdt.length as u64;
    let mut offset = madt_virt + core::mem::size_of::<MadtHeader>() as u64;

    while offset + 2 <= table_end {
        let entry_header = unsafe { &*(offset as *const MadtEntryHeader) };
        let entry_type = MadtEntryType::from(entry_header.entry_type);

        match entry_type {
            MadtEntryType::LocalApic => {
                let entry = unsafe { &*(offset as *const MadtLocalApic) };
                info.local_apics.push(LocalApicInfo {
                    processor_id: entry.acpi_processor_id,
                    apic_id: entry.apic_id,
                    enabled: (entry.flags & 1) != 0,
                    online_capable: (entry.flags & 2) != 0,
                });
            }
            MadtEntryType::IoApic => {
                let entry = unsafe { &*(offset as *const MadtIoApic) };
                info.io_apics.push(IoApicInfo {
                    id: entry.io_apic_id,
                    address: entry.io_apic_address,
                    gsi_base: entry.global_system_interrupt_base,
                });
            }
            MadtEntryType::InterruptSourceOverride => {
                let entry = unsafe { &*(offset as *const MadtIso) };
                info.interrupt_overrides.push(InterruptOverride {
                    bus: entry.bus_source,
                    irq_source: entry.irq_source,
                    gsi: entry.global_system_interrupt,
                    flags: entry.flags,
                });
            }
            MadtEntryType::LocalApicNmi => {
                let entry = unsafe { &*(offset as *const MadtLocalApicNmi) };
                info.local_apic_nmis.push(LocalApicNmiInfo {
                    processor_id: entry.acpi_processor_id,
                    flags: entry.flags,
                    lint: entry.lint,
                });
            }
            MadtEntryType::LocalApicAddressOverride
                // 64-bit override of local APIC address
                if entry_header.length >= 12 => {
                    let addr = unsafe { *((offset + 4) as *const u64) };
                    info.local_apic_address = addr;
                }
            _ => {}
        }

        offset += entry_header.length as u64;
        if entry_header.length == 0 {
            break; // Prevent infinite loop
        }
    }
}

/// Parse FADT
fn parse_fadt(phys_offset: u64, fadt_phys: u64, info: &mut AcpiInfo) {
    let fadt_virt = phys_offset + fadt_phys;
    let fadt = unsafe { &*(fadt_virt as *const Fadt) };

    info.fadt = Some(FadtInfo {
        pm1a_control_block: fadt.pm1a_control_block,
        pm1b_control_block: fadt.pm1b_control_block,
        pm_timer_block: fadt.pm_timer_block,
        sci_interrupt: fadt.sci_interrupt,
        smi_command_port: fadt.smi_command_port,
        acpi_enable: fadt.acpi_enable,
        acpi_disable: fadt.acpi_disable,
        reset_reg_address: fadt.reset_reg_address,
        reset_value: fadt.reset_value,
        century_register: fadt.century,
        boot_arch_flags: fadt.boot_arch_flags,
    });
}

/// Parse HPET table
fn parse_hpet(phys_offset: u64, hpet_phys: u64, info: &mut AcpiInfo) {
    let hpet_virt = phys_offset + hpet_phys;
    let hpet = unsafe { &*(hpet_virt as *const HpetTable) };
    info.hpet_address = Some(hpet.address);
}

/// Parse MCFG (PCIe ECAM) table
fn parse_mcfg(phys_offset: u64, mcfg_phys: u64, info: &mut AcpiInfo) {
    let mcfg_virt = phys_offset + mcfg_phys;
    let header = unsafe { &*(mcfg_virt as *const SdtHeader) };

    let entries_start = mcfg_virt + core::mem::size_of::<McfgTable>() as u64;
    let entry_count = (header.length as usize - core::mem::size_of::<McfgTable>())
        / core::mem::size_of::<McfgEntry>();

    for i in 0..entry_count {
        let entry = unsafe {
            &*((entries_start + (i * core::mem::size_of::<McfgEntry>()) as u64) as *const McfgEntry)
        };
        info.mcfg_entries.push(*entry);
    }
}

// ─── Main ACPI Init ─────────────────────────────────────────────────

/// Initialize ACPI table parsing
/// Call with physical memory offset from bootloader
pub fn init(phys_offset: u64) {
    serial_println!("[KnoxOS] ACPI: Searching for RSDP...");

    let mut info = AcpiInfo {
        local_apic_address: 0xFEE0_0000, // Default
        local_apics: Vec::new(),
        io_apics: Vec::new(),
        interrupt_overrides: Vec::new(),
        local_apic_nmis: Vec::new(),
        fadt: None,
        hpet_address: None,
        mcfg_entries: Vec::new(),
        pcat_compat: true,
        dmar_address: None,
        ivrs_address: None,
    };

    match find_rsdp(phys_offset) {
        Some(rsdp_phys) => {
            let rsdp_virt = phys_offset + rsdp_phys;
            let rsdp_ptr = rsdp_virt as *const Rsdp;
            let revision = unsafe { (*rsdp_ptr).revision };

            serial_println!(
                "[KnoxOS] ACPI: RSDP found at {:#x} (revision {})",
                rsdp_phys,
                revision
            );

            // Get table pointers from RSDT or XSDT
            let table_addrs = if revision >= 2 {
                let rsdp2 = rsdp_virt as *const Rsdp2;
                let xsdt_addr =
                    unsafe { core::ptr::addr_of!((*rsdp2).xsdt_address).read_unaligned() };
                if xsdt_addr != 0 {
                    serial_println!("[KnoxOS] ACPI: Using XSDT at {:#x}", xsdt_addr);
                    parse_xsdt(phys_offset, xsdt_addr)
                } else {
                    let rsdt_addr =
                        unsafe { core::ptr::addr_of!((*rsdp_ptr).rsdt_address).read_unaligned() };
                    serial_println!(
                        "[KnoxOS] ACPI: XSDT null, falling back to RSDT at {:#x}",
                        rsdt_addr
                    );
                    parse_rsdt(phys_offset, rsdt_addr)
                }
            } else {
                let rsdt_addr =
                    unsafe { core::ptr::addr_of!((*rsdp_ptr).rsdt_address).read_unaligned() };
                serial_println!("[KnoxOS] ACPI: Using RSDT at {:#x}", rsdt_addr);
                parse_rsdt(phys_offset, rsdt_addr)
            };

            serial_println!("[KnoxOS] ACPI: Found {} tables", table_addrs.len());

            // Parse each table based on signature
            for &table_phys in &table_addrs {
                let table_virt = phys_offset + table_phys;
                let header = unsafe { &*(table_virt as *const SdtHeader) };
                let sig = header.signature_str();

                match sig {
                    "APIC" => {
                        serial_println!("[KnoxOS] ACPI: Parsing MADT...");
                        parse_madt(phys_offset, table_phys, &mut info);
                    }
                    "FACP" => {
                        serial_println!("[KnoxOS] ACPI: Parsing FADT...");
                        parse_fadt(phys_offset, table_phys, &mut info);
                    }
                    "HPET" => {
                        serial_println!("[KnoxOS] ACPI: Parsing HPET...");
                        parse_hpet(phys_offset, table_phys, &mut info);
                    }
                    "MCFG" => {
                        serial_println!("[KnoxOS] ACPI: Parsing MCFG (PCIe ECAM)...");
                        parse_mcfg(phys_offset, table_phys, &mut info);
                    }
                    "DMAR" => {
                        serial_println!(
                            "[KnoxOS] ACPI: Found DMAR (Intel VT-d) at {:#x}",
                            table_phys
                        );
                        info.dmar_address = Some(table_phys);
                    }
                    "IVRS" => {
                        serial_println!("[KnoxOS] ACPI: Found IVRS (AMD-Vi) at {:#x}", table_phys);
                        info.ivrs_address = Some(table_phys);
                    }
                    _ => {
                        serial_println!("[KnoxOS] ACPI: Skipping table '{}'", sig);
                    }
                }
            }

            serial_println!(
                "[KnoxOS] ACPI: {} CPUs (Local APICs), {} I/O APICs, {} IRQ overrides",
                info.local_apics.len(),
                info.io_apics.len(),
                info.interrupt_overrides.len()
            );
            if let Some(ref fadt) = info.fadt {
                serial_println!(
                    "[KnoxOS] ACPI: FADT: PM1a={:#x}, PM Timer={:#x}, SCI IRQ={}",
                    fadt.pm1a_control_block,
                    fadt.pm_timer_block,
                    fadt.sci_interrupt
                );
            }
            if let Some(hpet_addr) = info.hpet_address {
                serial_println!("[KnoxOS] ACPI: HPET at {:#x}", hpet_addr);
            }
            if !info.mcfg_entries.is_empty() {
                serial_println!(
                    "[KnoxOS] ACPI: {} PCIe ECAM region(s)",
                    info.mcfg_entries.len()
                );
            }
        }
        None => {
            serial_println!("[KnoxOS] ACPI: RSDP not found — using defaults");
            // Add a default CPU
            info.local_apics.push(LocalApicInfo {
                processor_id: 0,
                apic_id: 0,
                enabled: true,
                online_capable: true,
            });
        }
    }

    *ACPI_INFO.lock() = Some(info);
    serial_println!("[KnoxOS] ACPI table parsing initialized");
}

// ─── Query Functions ────────────────────────────────────────────────

/// Get the number of enabled CPUs from ACPI
pub fn cpu_count() -> usize {
    ACPI_INFO
        .lock()
        .as_ref()
        .map(|info| {
            info.local_apics
                .iter()
                .filter(|a| a.enabled || a.online_capable)
                .count()
        })
        .unwrap_or(1)
}

/// Get local APIC base address
pub fn local_apic_address() -> u64 {
    ACPI_INFO
        .lock()
        .as_ref()
        .map(|info| info.local_apic_address)
        .unwrap_or(0xFEE0_0000)
}

/// Get I/O APIC information
pub fn io_apic_info() -> Vec<IoApicInfo> {
    ACPI_INFO
        .lock()
        .as_ref()
        .map(|info| info.io_apics.clone())
        .unwrap_or_default()
}

/// Get interrupt source overrides
pub fn interrupt_overrides() -> Vec<InterruptOverride> {
    ACPI_INFO
        .lock()
        .as_ref()
        .map(|info| info.interrupt_overrides.clone())
        .unwrap_or_default()
}

/// Get HPET base address
pub fn hpet_address() -> Option<u64> {
    ACPI_INFO.lock().as_ref().and_then(|info| info.hpet_address)
}

/// Get PCIe ECAM entries
pub fn pcie_ecam_entries() -> Vec<McfgEntry> {
    ACPI_INFO
        .lock()
        .as_ref()
        .map(|info| info.mcfg_entries.clone())
        .unwrap_or_default()
}

/// Check if legacy PIC compatibility is indicated
pub fn has_legacy_pic() -> bool {
    ACPI_INFO
        .lock()
        .as_ref()
        .map(|info| info.pcat_compat)
        .unwrap_or(true)
}

/// ACPI system reset via FADT reset register
pub fn acpi_reset() -> ! {
    if let Some(fadt) = ACPI_INFO.lock().as_ref().and_then(|info| info.fadt.clone()) {
        if fadt.reset_reg_address != 0 {
            serial_println!(
                "[KnoxOS] ACPI reset via register at {:#x}",
                fadt.reset_reg_address
            );
            unsafe {
                let ptr = fadt.reset_reg_address as *mut u8;
                core::ptr::write_volatile(ptr, fadt.reset_value);
            }
        }
    }
    // Fallback to keyboard controller reset
    crate::acpi::reboot();
}

/// Enable ACPI mode (transition from legacy to ACPI)
pub fn enable_acpi_mode() {
    if let Some(fadt) = ACPI_INFO.lock().as_ref().and_then(|info| info.fadt.clone()) {
        if fadt.smi_command_port != 0 && fadt.acpi_enable != 0 {
            serial_println!(
                "[KnoxOS] ACPI: Enabling ACPI mode via SMI cmd port {:#x}",
                fadt.smi_command_port
            );
            unsafe {
                let mut port = crate::arch_compat::instructions::port::Port::<u8>::new(
                    fadt.smi_command_port as u16,
                );
                port.write(fadt.acpi_enable);
            }
            // Wait for SCI_EN bit to be set in PM1a_CNT
            for _ in 0..1000 {
                unsafe {
                    let mut pm1a = crate::arch_compat::instructions::port::Port::<u16>::new(
                        fadt.pm1a_control_block as u16,
                    );
                    let val = pm1a.read();
                    if val & 1 != 0 {
                        serial_println!("[KnoxOS] ACPI: ACPI mode enabled (SCI_EN set)");
                        return;
                    }
                }
                for _ in 0..10000 {
                    core::hint::spin_loop();
                }
            }
            serial_println!("[KnoxOS] ACPI: Warning — SCI_EN not set after enable");
        }
    }
}

/// Get PM timer value (24 or 32 bit counter at ~3.58 MHz)
pub fn read_pm_timer() -> u32 {
    if let Some(fadt) = ACPI_INFO.lock().as_ref().and_then(|info| info.fadt.clone()) {
        if fadt.pm_timer_block != 0 {
            unsafe {
                let mut port = crate::arch_compat::instructions::port::Port::<u32>::new(
                    fadt.pm_timer_block as u16,
                );
                return port.read();
            }
        }
    }
    0
}
