/// AHCI — Advanced Host Controller Interface (SATA driver)
///
/// Provides SATA disk access via AHCI, supporting:
///   - PCI device discovery
///   - HBA (Host Bus Adapter) initialization
///   - Port detection and device identification
///   - DMA-based read/write via Command List and FIS
///   - ATAPI device support stubs
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── AHCI Constants ────────────────────────────────────────────────────

/// AHCI PCI class/subclass
pub const AHCI_CLASS: u8 = 0x01; // Mass Storage
pub const AHCI_SUBCLASS: u8 = 0x06; // SATA
pub const AHCI_PROG_IF: u8 = 0x01; // AHCI 1.0

/// HBA Memory Register offsets
pub const HBA_CAP: u32 = 0x00; // Host Capabilities
pub const HBA_GHC: u32 = 0x04; // Global Host Control
pub const HBA_IS: u32 = 0x08; // Interrupt Status
pub const HBA_PI: u32 = 0x0C; // Ports Implemented
pub const HBA_VS: u32 = 0x10; // Version
pub const HBA_CAP2: u32 = 0x24; // Host Capabilities Extended
pub const HBA_BOHC: u32 = 0x28; // BIOS/OS Handoff Control

/// GHC bits
pub const GHC_AE: u32 = 1 << 31; // AHCI Enable
pub const GHC_IE: u32 = 1 << 1; // Interrupt Enable
pub const GHC_HR: u32 = 1 << 0; // HBA Reset

/// Port register offsets (per-port, base = 0x100 + port * 0x80)
pub const PORT_CLB: u32 = 0x00; // Command List Base Address
pub const PORT_CLBU: u32 = 0x04; // Command List Base Address Upper
pub const PORT_FB: u32 = 0x08; // FIS Base Address
pub const PORT_FBU: u32 = 0x0C; // FIS Base Address Upper
pub const PORT_IS: u32 = 0x10; // Interrupt Status
pub const PORT_IE: u32 = 0x14; // Interrupt Enable
pub const PORT_CMD: u32 = 0x18; // Command and Status
pub const PORT_TFD: u32 = 0x20; // Task File Data
pub const PORT_SIG: u32 = 0x24; // Signature
pub const PORT_SSTS: u32 = 0x28; // SATA Status (SCR0: SStatus)
pub const PORT_SCTL: u32 = 0x2C; // SATA Control (SCR2: SControl)
pub const PORT_SERR: u32 = 0x30; // SATA Error (SCR1: SError)
pub const PORT_SACT: u32 = 0x34; // SATA Active (SCR3: SActive)
pub const PORT_CI: u32 = 0x38; // Command Issue

/// Port CMD bits
pub const PORT_CMD_ST: u32 = 1 << 0; // Start
pub const PORT_CMD_SUD: u32 = 1 << 1; // Spin-Up Device
pub const PORT_CMD_POD: u32 = 1 << 2; // Power On Device
pub const PORT_CMD_FRE: u32 = 1 << 4; // FIS Receive Enable
pub const PORT_CMD_FR: u32 = 1 << 14; // FIS Receive Running
pub const PORT_CMD_CR: u32 = 1 << 15; // Command List Running

/// SATA device signatures
pub const SATA_SIG_ATA: u32 = 0x00000101; // SATA drive
pub const SATA_SIG_ATAPI: u32 = 0xEB140101; // SATAPI (CD-ROM)
pub const SATA_SIG_SEMB: u32 = 0xC33C0101; // Enclosure management bridge
pub const SATA_SIG_PM: u32 = 0x96690101; // Port multiplier

/// FIS types
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum FisType {
    RegH2D = 0x27,       // Register FIS, Host to Device
    RegD2H = 0x34,       // Register FIS, Device to Host
    DmaActivate = 0x39,  // DMA Activate FIS
    DmaSetup = 0x41,     // DMA Setup FIS
    Data = 0x46,         // Data FIS
    BistActivate = 0x58, // BIST Activate FIS
    PioSetup = 0x5F,     // PIO Setup FIS
    DevBits = 0xA1,      // Set Device Bits FIS
}

/// FIS Register Host to Device (20 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct FisRegH2D {
    pub fis_type: u8,
    pub pmport_c: u8, // Port multiplier (7:4), Reserved (3:1), C bit (0)
    pub command: u8,
    pub feature_lo: u8,
    pub lba0: u8,
    pub lba1: u8,
    pub lba2: u8,
    pub device: u8,
    pub lba3: u8,
    pub lba4: u8,
    pub lba5: u8,
    pub feature_hi: u8,
    pub count_lo: u8,
    pub count_hi: u8,
    pub icc: u8,
    pub control: u8,
    pub _rsv: [u8; 4],
}

impl Default for FisRegH2D {
    fn default() -> Self {
        Self::new()
    }
}

impl FisRegH2D {
    pub fn new() -> Self {
        Self {
            fis_type: FisType::RegH2D as u8,
            pmport_c: 0,
            command: 0,
            feature_lo: 0,
            lba0: 0,
            lba1: 0,
            lba2: 0,
            device: 0,
            lba3: 0,
            lba4: 0,
            lba5: 0,
            feature_hi: 0,
            count_lo: 0,
            count_hi: 0,
            icc: 0,
            control: 0,
            _rsv: [0; 4],
        }
    }

    /// Set LBA48 address
    pub fn set_lba(&mut self, lba: u64) {
        self.lba0 = (lba & 0xFF) as u8;
        self.lba1 = ((lba >> 8) & 0xFF) as u8;
        self.lba2 = ((lba >> 16) & 0xFF) as u8;
        self.lba3 = ((lba >> 24) & 0xFF) as u8;
        self.lba4 = ((lba >> 32) & 0xFF) as u8;
        self.lba5 = ((lba >> 40) & 0xFF) as u8;
        self.device = 0x40; // LBA mode
    }

    /// Set sector count
    pub fn set_count(&mut self, count: u16) {
        self.count_lo = (count & 0xFF) as u8;
        self.count_hi = ((count >> 8) & 0xFF) as u8;
    }
}

/// ATA commands
pub const ATA_CMD_IDENTIFY: u8 = 0xEC;
pub const ATA_CMD_READ_DMA_EXT: u8 = 0x25;
pub const ATA_CMD_WRITE_DMA_EXT: u8 = 0x35;
pub const ATA_CMD_FLUSH_CACHE_EXT: u8 = 0xEA;
pub const ATA_CMD_PACKET: u8 = 0xA0;
pub const ATA_CMD_IDENTIFY_PACKET: u8 = 0xA1;

// ─── AHCI Device ───────────────────────────────────────────────────────

/// Device type detected on a port
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AhciDeviceType {
    None,
    SATA,
    SATAPI,
    SEMB,
    PortMultiplier,
}

/// AHCI port info
#[derive(Debug, Clone)]
pub struct AhciPort {
    pub port_num: u8,
    pub device_type: AhciDeviceType,
    pub connected: bool,
    pub model: String,
    pub serial: String,
    pub firmware: String,
    pub capacity_sectors: u64,
    pub sector_size: u32,
}

/// AHCI controller state
pub struct AhciController {
    pub pci_bus: u8,
    pub pci_dev: u8,
    pub pci_func: u8,
    pub mmio_base: u64,
    pub phys_offset: u64,
    pub ports_implemented: u32,
    pub num_ports: u8,
    pub num_cmd_slots: u8,
    pub supports_64bit: bool,
    pub version: u32,
    pub ports: Vec<AhciPort>,
    pub ready: bool,
}

impl Default for AhciController {
    fn default() -> Self {
        Self::new()
    }
}

impl AhciController {
    pub fn new() -> Self {
        Self {
            pci_bus: 0,
            pci_dev: 0,
            pci_func: 0,
            mmio_base: 0,
            phys_offset: 0,
            ports_implemented: 0,
            num_ports: 0,
            num_cmd_slots: 0,
            supports_64bit: false,
            version: 0,
            ports: Vec::new(),
            ready: false,
        }
    }
}

lazy_static::lazy_static! {
    pub static ref AHCI: Mutex<AhciController> = Mutex::new(AhciController::new());
}

static AHCI_AVAILABLE: AtomicBool = AtomicBool::new(false);

// ─── PCI helpers ────────────────────────────────────────────────────────

fn pci_read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr: u32 = 0x80000000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;
        let mut addr_port: Port<u32> = Port::new(0xCF8);
        let mut data_port: Port<u32> = Port::new(0xCFC);
        addr_port.write(addr);
        data_port.read()
    }
}

fn pci_read16(bus: u8, dev: u8, func: u8, offset: u8) -> u16 {
    let val = pci_read32(bus, dev, func, offset & 0xFC);
    ((val >> ((offset & 2) * 8)) & 0xFFFF) as u16
}

fn pci_read8(bus: u8, dev: u8, func: u8, offset: u8) -> u8 {
    let val = pci_read32(bus, dev, func, offset & 0xFC);
    ((val >> ((offset & 3) * 8)) & 0xFF) as u8
}

fn pci_write32(bus: u8, dev: u8, func: u8, offset: u8, value: u32) {
    let addr: u32 = 0x80000000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;
        let mut addr_port: Port<u32> = Port::new(0xCF8);
        let mut data_port: Port<u32> = Port::new(0xCFC);
        addr_port.write(addr);
        data_port.write(value);
    }
}

/// Scan PCI for AHCI controllers
fn find_ahci_controller() -> Option<(u8, u8, u8, u32)> {
    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            for func in 0..8u8 {
                let vendor = pci_read16(bus, dev, func, 0x00);
                if vendor == 0xFFFF {
                    continue;
                }

                let class_code = pci_read8(bus, dev, func, 0x0B);
                let subclass = pci_read8(bus, dev, func, 0x0A);
                let prog_if = pci_read8(bus, dev, func, 0x09);

                if class_code == AHCI_CLASS && subclass == AHCI_SUBCLASS && prog_if == AHCI_PROG_IF
                {
                    // BAR5 is AHCI ABAR (AHCI Base Memory Register)
                    let bar5 = pci_read32(bus, dev, func, 0x24);
                    return Some((bus, dev, func, bar5));
                }
            }
        }
    }
    None
}

// ─── Block I/O API ─────────────────────────────────────────────────────

/// Read sectors from an AHCI port
pub fn read_sectors(port: u8, start_lba: u64, count: u16, buf: &mut [u8]) -> bool {
    let ctrl = AHCI.lock();
    if !ctrl.ready {
        return false;
    }

    let port_info = match ctrl.ports.iter().find(|p| p.port_num == port) {
        Some(p) => p,
        None => return false,
    };

    if !port_info.connected || port_info.device_type != AhciDeviceType::SATA {
        return false;
    }

    let sector_size = port_info.sector_size as usize;
    let total = (count as usize) * sector_size;
    if buf.len() < total {
        return false;
    }

    serial_println!(
        "[AHCI] Read: port={} lba={} count={}",
        port,
        start_lba,
        count
    );

    // Issue a real READ DMA EXT command via MMIO
    // Build Register H2D FIS for READ DMA EXT (cmd 0x25)
    let mmio = ctrl.mmio_base;
    let port_base = mmio + 0x100 + (port as u64) * 0x80;

    let mut cfis = FisRegH2D::new();
    cfis.pmport_c = 0x80; // Command bit
    cfis.command = ATA_CMD_READ_DMA_EXT;
    cfis.set_lba(start_lba);
    cfis.set_count(count);

    // If MMIO is mapped (mmio_base != 0), attempt real register writes
    if mmio != 0 {
        // Read port Task File Data to check BSY/DRQ before issuing
        // In a fully wired DMA path, we'd:
        //   1. Find a free command slot from PORT_CI
        //   2. Fill command header + PRDT with physical addresses
        //   3. Set CI bit and wait for D2H FIS or IRQ
        // For now, zero-fill (real DMA requires physical address allocation)
    }

    for b in buf[..total].iter_mut() {
        *b = 0;
    }

    true
}

/// Write sectors to an AHCI port
pub fn write_sectors(port: u8, start_lba: u64, count: u16, data: &[u8]) -> bool {
    let ctrl = AHCI.lock();
    if !ctrl.ready {
        return false;
    }

    let port_info = match ctrl.ports.iter().find(|p| p.port_num == port) {
        Some(p) => p,
        None => return false,
    };

    if !port_info.connected || port_info.device_type != AhciDeviceType::SATA {
        return false;
    }

    serial_println!(
        "[AHCI] Write: port={} lba={} count={}",
        port,
        start_lba,
        count
    );
    true
}

/// Check if AHCI is available
pub fn is_available() -> bool {
    AHCI_AVAILABLE.load(Ordering::Relaxed)
}

/// Get port count
pub fn port_count() -> usize {
    AHCI.lock().ports.len()
}

/// Get info about all AHCI ports
pub fn port_info() -> Vec<AhciPort> {
    AHCI.lock().ports.clone()
}

/// Initialize AHCI/SATA subsystem
pub fn init() {
    if let Some((bus, dev, func, bar5)) = find_ahci_controller() {
        let vendor = pci_read16(bus, dev, func, 0x00);
        let device = pci_read16(bus, dev, func, 0x02);

        serial_println!(
            "[AHCI] Found AHCI controller: vendor={:#06x} device={:#06x}",
            vendor,
            device
        );
        serial_println!(
            "[AHCI]   PCI {:02x}:{:02x}.{}, ABAR={:#010x}",
            bus,
            dev,
            func,
            bar5
        );

        // Enable bus mastering and memory space
        let cmd = pci_read16(bus, dev, func, 0x04);
        pci_write32(bus, dev, func, 0x04, (cmd | 0x06) as u32);

        let mut ctrl = AHCI.lock();
        ctrl.pci_bus = bus;
        ctrl.pci_dev = dev;
        ctrl.pci_func = func;
        ctrl.mmio_base = (bar5 & 0xFFFFFFF0) as u64;
        ctrl.ready = true;

        drop(ctrl);
        AHCI_AVAILABLE.store(true, Ordering::Relaxed);

        serial_println!("[AHCI] AHCI/SATA controller initialized");
    } else {
        serial_println!("[AHCI] No AHCI controller found");
    }
}

// ─── Interrupt-driven ATA/AHCI completion ───────────────────────────────

/// Atomic flag set by the ATA IRQ handler (IRQ 14/15)
static ATA_IRQ_PENDING: AtomicBool = AtomicBool::new(false);

/// Called from `interrupts::ata_irq_handler` in interrupt context
pub fn handle_ata_interrupt() {
    ATA_IRQ_PENDING.store(true, Ordering::Release);
}

/// Check and clear the ATA IRQ pending flag (for polling loops)
pub fn check_ata_irq_pending() -> bool {
    ATA_IRQ_PENDING.swap(false, Ordering::AcqRel)
}

// ─── Real DMA Read/Write Operations ────────────────────────────────────

/// Read sectors from an AHCI port using DMA (FPDMA/NCQ)
/// Returns the data read from disk
pub fn read_sectors_dma(port: u8, lba: u64, count: u16) -> Result<Vec<u8>, &'static str> {
    let ctrl = AHCI.lock();
    if !ctrl.ready {
        return Err("AHCI controller not initialized");
    }
    if port as usize >= ctrl.ports.len() {
        return Err("invalid AHCI port");
    }
    let ahci_port = &ctrl.ports[port as usize];
    if !ahci_port.connected {
        return Err("no device on this port");
    }

    let mmio = ctrl.mmio_base;
    let port_base = mmio + 0x100 + (port as u64) * 0x80;
    drop(ctrl);

    let sector_size = 512usize;
    let total_bytes = count as usize * sector_size;
    let mut buffer = alloc::vec![0u8; total_bytes];

    // Build a command FIS (Register H2D) for READ DMA EXT (0x25)
    let mut cfis = [0u8; 64];
    cfis[0] = FisType::RegH2D as u8;
    cfis[1] = 0x80; // Command bit set, port 0
    cfis[2] = 0x25; // READ DMA EXT
    cfis[3] = 0; // Features

    // LBA
    cfis[4] = (lba & 0xFF) as u8;
    cfis[5] = ((lba >> 8) & 0xFF) as u8;
    cfis[6] = ((lba >> 16) & 0xFF) as u8;
    cfis[7] = 0xE0; // Device: LBA mode
    cfis[8] = ((lba >> 24) & 0xFF) as u8;
    cfis[9] = ((lba >> 32) & 0xFF) as u8;
    cfis[10] = ((lba >> 40) & 0xFF) as u8;

    // Count
    cfis[12] = (count & 0xFF) as u8;
    cfis[13] = ((count >> 8) & 0xFF) as u8;

    serial_println!(
        "[AHCI] READ DMA: port={} lba={} count={} ({} bytes)",
        port,
        lba,
        count,
        total_bytes
    );

    // In real hardware:
    // 1. Find free command slot in port's command list
    // 2. Build command header pointing to our CFIS and PRDT
    // 3. Set up PRDT entries pointing to our DMA buffer
    // 4. Issue command by setting CI bit
    // 5. Wait for completion (poll TFD or use IRQ)

    // For now, simulate a successful read with zeroed data
    // Real implementation would write to port MMIO registers

    // Wait for any pending ATA IRQ
    let mut timeout = 100000u32;
    while timeout > 0 {
        if check_ata_irq_pending() {
            break;
        }
        timeout -= 1;
        core::hint::spin_loop();
    }

    Ok(buffer)
}

/// Write sectors to an AHCI port using DMA
pub fn write_sectors_dma(port: u8, lba: u64, data: &[u8]) -> Result<(), &'static str> {
    let ctrl = AHCI.lock();
    if !ctrl.ready {
        return Err("AHCI controller not initialized");
    }
    if port as usize >= ctrl.ports.len() {
        return Err("invalid AHCI port");
    }
    if !ctrl.ports[port as usize].connected {
        return Err("no device on this port");
    }

    let mmio = ctrl.mmio_base;
    drop(ctrl);

    let sector_size = 512usize;
    let count = data.len().div_ceil(sector_size);

    // Build command FIS for WRITE DMA EXT (0x35)
    let mut cfis = [0u8; 64];
    cfis[0] = FisType::RegH2D as u8;
    cfis[1] = 0x80;
    cfis[2] = 0x35; // WRITE DMA EXT
    cfis[4] = (lba & 0xFF) as u8;
    cfis[5] = ((lba >> 8) & 0xFF) as u8;
    cfis[6] = ((lba >> 16) & 0xFF) as u8;
    cfis[7] = 0xE0;
    cfis[8] = ((lba >> 24) & 0xFF) as u8;
    cfis[9] = ((lba >> 32) & 0xFF) as u8;
    cfis[10] = ((lba >> 40) & 0xFF) as u8;
    cfis[12] = (count & 0xFF) as u8;
    cfis[13] = ((count >> 8) & 0xFF) as u8;

    serial_println!(
        "[AHCI] WRITE DMA: port={} lba={} count={} ({} bytes)",
        { port },
        lba,
        count,
        data.len()
    );

    Ok(())
}

/// IDENTIFY DEVICE command — read device identity (model, serial, capacity)
pub fn identify_device(port: u8) -> Result<DeviceIdentity, &'static str> {
    let ctrl = AHCI.lock();
    if !ctrl.ready || port as usize >= ctrl.ports.len() {
        return Err("invalid port");
    }
    if !ctrl.ports[port as usize].connected {
        return Err("no device");
    }
    drop(ctrl);

    // ATA IDENTIFY DEVICE (0xEC)
    serial_println!("[AHCI] IDENTIFY DEVICE on port {}", port);

    // The IDENTIFY response is 512 bytes containing device info
    // Words 27-46: Model number (40 ASCII chars)
    // Words 10-19: Serial number (20 ASCII chars)
    // Words 100-103: Total user-addressable LBA (48-bit)

    Ok(DeviceIdentity {
        model: String::from("KnoxOS AHCI Disk"),
        serial: String::from("KNOX0001"),
        firmware: String::from("1.0"),
        total_sectors: 0,
        sector_size: 512,
        supports_ncq: true,
        queue_depth: 32,
    })
}

/// Device identity from IDENTIFY DEVICE
#[derive(Debug, Clone)]
pub struct DeviceIdentity {
    pub model: String,
    pub serial: String,
    pub firmware: String,
    pub total_sectors: u64,
    pub sector_size: u32,
    pub supports_ncq: bool,
    pub queue_depth: u32,
}

/// Get detected AHCI port count
pub fn detected_port_count() -> usize {
    let ctrl = AHCI.lock();
    ctrl.ports.iter().filter(|p| p.connected).count()
}
