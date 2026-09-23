/// AHCI — Advanced Host Controller Interface (SATA driver)
///
/// Provides SATA disk access via AHCI, supporting:
///   - PCI device discovery
///   - HBA (Host Bus Adapter) initialization
///   - Port detection and device identification
///   - DMA-based read/write via Command List and FIS
///   - ATAPI device support stubs
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
    /// Guest-physical DMA region (command list + FIS + table + bounce).
    pub dma_phys: u64,
    /// First SATA port that was started with a real command list.
    pub active_port: u8,
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
            dma_phys: 0,
            active_port: 0xFF,
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

// ─── MMIO + DMA layout ─────────────────────────────────────────────────

const DMA_PAGES: usize = 4;
const CL_OFF: u64 = 0x000;
const FIS_OFF: u64 = 0x400;
const CT_OFF: u64 = 0x800;
const BOUNCE_OFF: u64 = 0x1000;
const BOUNCE_MAX: usize = 4096;
const PORT_TFES: u32 = 1 << 30;

fn mmio_read(addr: u64) -> u32 {
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

fn mmio_write(addr: u64, value: u32) {
    unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
}

fn port_reg(mmio: u64, port: u8, off: u32) -> u64 {
    mmio + 0x100 + (port as u64) * 0x80 + off as u64
}

fn wait_clear(addr: u64, mask: u32, spins: u32) -> bool {
    for _ in 0..spins {
        if mmio_read(addr) & mask == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn stop_port(mmio: u64, port: u8) {
    let cmd = port_reg(mmio, port, PORT_CMD);
    mmio_write(cmd, mmio_read(cmd) & !PORT_CMD_ST);
    let _ = wait_clear(cmd, PORT_CMD_CR, 1_000_000);
    mmio_write(cmd, mmio_read(cmd) & !PORT_CMD_FRE);
    let _ = wait_clear(cmd, PORT_CMD_FR, 1_000_000);
}

fn start_port(mmio: u64, port: u8) {
    let cmd = port_reg(mmio, port, PORT_CMD);
    let _ = wait_clear(cmd, PORT_CMD_CR, 1_000_000);
    mmio_write(
        cmd,
        mmio_read(cmd) | PORT_CMD_FRE | PORT_CMD_SUD | PORT_CMD_POD,
    );
    mmio_write(cmd, mmio_read(cmd) | PORT_CMD_ST);
}

fn ata_string(words: &[u8]) -> String {
    let mut bytes = words.to_vec();
    for chunk in bytes.chunks_exact_mut(2) {
        chunk.swap(0, 1);
    }
    String::from(core::str::from_utf8(&bytes).unwrap_or("").trim())
}

/// Issue one slot-0 DMA command. `write` copies `buf` into the bounce buffer
/// before the command; `!write` copies bounce back into `buf` after.
fn issue_dma(port: u8, command: u8, lba: u64, count: u16, buf: &mut [u8], write: bool) -> bool {
    let (mmio, dma_phys, sector_size) = {
        let ctrl = AHCI.lock();
        if !ctrl.ready || ctrl.dma_phys == 0 || ctrl.mmio_base == 0 {
            return false;
        }
        let Some(info) = ctrl.ports.iter().find(|p| p.port_num == port) else {
            return false;
        };
        if !info.connected || info.device_type != AhciDeviceType::SATA {
            return false;
        }
        (ctrl.mmio_base, ctrl.dma_phys, info.sector_size as usize)
    };

    let total = count as usize * sector_size;
    if total == 0 || total > BOUNCE_MAX || buf.len() < total {
        return false;
    }

    let bounce_phys = dma_phys + BOUNCE_OFF;
    let bounce_virt = crate::vmm::phys_to_virt(bounce_phys);
    let cl_virt = crate::vmm::phys_to_virt(dma_phys + CL_OFF);
    let ct_virt = crate::vmm::phys_to_virt(dma_phys + CT_OFF);
    let ct_phys = dma_phys + CT_OFF;

    unsafe {
        if write {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), bounce_virt as *mut u8, total);
        } else {
            core::ptr::write_bytes(bounce_virt as *mut u8, 0, total);
        }
        core::ptr::write_bytes(ct_virt as *mut u8, 0, 256);
        let cfis = ct_virt as *mut u8;
        *cfis.add(0) = FisType::RegH2D as u8;
        *cfis.add(1) = 0x80;
        *cfis.add(2) = command;
        *cfis.add(4) = lba as u8;
        *cfis.add(5) = (lba >> 8) as u8;
        *cfis.add(6) = (lba >> 16) as u8;
        *cfis.add(7) = 0x40;
        *cfis.add(8) = (lba >> 24) as u8;
        *cfis.add(9) = (lba >> 32) as u8;
        *cfis.add(10) = (lba >> 40) as u8;
        *cfis.add(12) = count as u8;
        *cfis.add(13) = (count >> 8) as u8;

        let prdt = (ct_virt + 128) as *mut u32;
        *prdt.add(0) = bounce_phys as u32;
        *prdt.add(1) = (bounce_phys >> 32) as u32;
        *prdt.add(2) = 0;
        *prdt.add(3) = ((total as u32 - 1) & 0x3F_FFFF) | (1 << 31);

        core::ptr::write_bytes(cl_virt as *mut u8, 0, 32);
        let hdr = cl_virt as *mut u32;
        let mut dw0: u32 = 5;
        if write {
            dw0 |= 1 << 6;
        }
        dw0 |= 1 << 16;
        *hdr.add(0) = dw0;
        *hdr.add(1) = 0;
        *hdr.add(2) = ct_phys as u32;
        *hdr.add(3) = (ct_phys >> 32) as u32;
    }
    core::sync::atomic::fence(Ordering::SeqCst);

    let tfd = port_reg(mmio, port, PORT_TFD);
    if !wait_clear(tfd, 0x88, 1_000_000) {
        serial_println!("[AHCI] TFD busy before command {:#x}", command);
        return false;
    }

    mmio_write(port_reg(mmio, port, PORT_IS), 0xFFFF_FFFF);
    let _ = check_ata_irq_pending();
    mmio_write(port_reg(mmio, port, PORT_CI), 1);
    core::sync::atomic::fence(Ordering::SeqCst);

    let ci = port_reg(mmio, port, PORT_CI);
    let is = port_reg(mmio, port, PORT_IS);
    let start = crate::interrupts::get_ticks();
    let mut spins = 0u32;
    loop {
        if mmio_read(ci) & 1 == 0 {
            break;
        }
        if mmio_read(is) & PORT_TFES != 0 {
            serial_println!("[AHCI] TFES on command {:#x}", command);
            mmio_write(is, PORT_TFES);
            return false;
        }
        if check_ata_irq_pending() && mmio_read(ci) & 1 == 0 {
            break;
        }
        spins = spins.wrapping_add(1);
        if spins > 20_000_000 {
            serial_println!("[AHCI] DMA timeout command {:#x}", command);
            return false;
        }
        if crate::interrupts::get_ticks().wrapping_sub(start) >= 40 {
            serial_println!("[AHCI] DMA tick timeout command {:#x}", command);
            return false;
        }
        core::hint::spin_loop();
    }

    if mmio_read(tfd) & 1 != 0 {
        serial_println!("[AHCI] ERR after command {:#x}", command);
        return false;
    }

    if !write {
        unsafe {
            core::ptr::copy_nonoverlapping(bounce_virt as *const u8, buf.as_mut_ptr(), total);
        }
    }
    true
}

/// Read sectors from an AHCI port
pub fn read_sectors(port: u8, start_lba: u64, count: u16, buf: &mut [u8]) -> bool {
    issue_dma(port, ATA_CMD_READ_DMA_EXT, start_lba, count, buf, false)
}

/// Write sectors to an AHCI port
pub fn write_sectors(port: u8, start_lba: u64, count: u16, data: &[u8]) -> bool {
    let sector_size = 512usize;
    let total = count as usize * sector_size;
    if data.len() < total {
        return false;
    }
    let mut buf = data[..total].to_vec();
    issue_dma(
        port,
        ATA_CMD_WRITE_DMA_EXT,
        start_lba,
        count,
        &mut buf,
        true,
    )
}

pub fn is_available() -> bool {
    AHCI_AVAILABLE.load(Ordering::Relaxed)
}

pub fn port_count() -> usize {
    AHCI.lock().ports.len()
}

pub fn port_info() -> Vec<AhciPort> {
    AHCI.lock().ports.clone()
}

fn start_first_sata_port(mmio: u64, dma_phys: u64) -> Option<AhciPort> {
    let pi = mmio_read(mmio + HBA_PI as u64);
    for port in 0u8..32 {
        if pi & (1 << port) == 0 {
            continue;
        }
        let ssts = mmio_read(port_reg(mmio, port, PORT_SSTS));
        if ssts & 0xF != 3 {
            continue;
        }
        let sig = mmio_read(port_reg(mmio, port, PORT_SIG));
        let device_type = match sig {
            SATA_SIG_ATA => AhciDeviceType::SATA,
            SATA_SIG_ATAPI => AhciDeviceType::SATAPI,
            SATA_SIG_SEMB => AhciDeviceType::SEMB,
            SATA_SIG_PM => AhciDeviceType::PortMultiplier,
            _ => AhciDeviceType::SATA,
        };
        if device_type != AhciDeviceType::SATA {
            continue;
        }

        stop_port(mmio, port);
        mmio_write(port_reg(mmio, port, PORT_CLB), dma_phys as u32);
        mmio_write(port_reg(mmio, port, PORT_CLBU), (dma_phys >> 32) as u32);
        mmio_write(port_reg(mmio, port, PORT_FB), (dma_phys + FIS_OFF) as u32);
        mmio_write(
            port_reg(mmio, port, PORT_FBU),
            ((dma_phys + FIS_OFF) >> 32) as u32,
        );
        mmio_write(port_reg(mmio, port, PORT_SERR), 0xFFFF_FFFF);
        mmio_write(port_reg(mmio, port, PORT_IS), 0xFFFF_FFFF);
        start_port(mmio, port);

        serial_println!(
            "[AHCI] Port {} SATA present (SSTS={:#x} SIG={:#x})",
            port,
            ssts,
            sig
        );
        return Some(AhciPort {
            port_num: port,
            device_type,
            connected: true,
            model: String::new(),
            serial: String::new(),
            firmware: String::new(),
            capacity_sectors: 0,
            sector_size: 512,
        });
    }
    None
}

/// Initialize AHCI/SATA subsystem
pub fn init() {
    if let Some((bus, dev, func, bar5)) = find_ahci_controller() {
        let vendor = pci_read16(bus, dev, func, 0x00);
        let device = pci_read16(bus, dev, func, 0x02);
        let abar_phys = (bar5 & 0xFFFF_FFF0) as u64;

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
            abar_phys
        );

        let cmd = pci_read16(bus, dev, func, 0x04);
        pci_write32(bus, dev, func, 0x04, (cmd | 0x06) as u32);

        let Some(mmio) = crate::vmm::map_mmio(abar_phys, 0x1100) else {
            serial_println!("[AHCI] Failed to map ABAR");
            return;
        };

        mmio_write(mmio + HBA_GHC as u64, GHC_HR);
        if !wait_clear(mmio + HBA_GHC as u64, GHC_HR, 1_000_000) {
            serial_println!("[AHCI] HBA reset timeout");
            return;
        }
        mmio_write(mmio + HBA_GHC as u64, GHC_AE);

        let cap = mmio_read(mmio + HBA_CAP as u64);
        let vs = mmio_read(mmio + HBA_VS as u64);
        serial_println!("[AHCI]   CAP={:#010x} VS={:#010x}", cap, vs);

        let Some(dma_phys) = crate::vmm::allocate_contiguous_frames(DMA_PAGES) else {
            serial_println!("[AHCI] No DMA frames");
            return;
        };
        unsafe {
            core::ptr::write_bytes(
                crate::vmm::phys_to_virt(dma_phys) as *mut u8,
                0,
                DMA_PAGES * 4096,
            );
        }

        let Some(port) = start_first_sata_port(mmio, dma_phys) else {
            serial_println!("[AHCI] No SATA device on any port");
            crate::vmm::free_physical_frame(dma_phys);
            return;
        };

        let mut ctrl = AHCI.lock();
        ctrl.pci_bus = bus;
        ctrl.pci_dev = dev;
        ctrl.pci_func = func;
        ctrl.mmio_base = mmio;
        ctrl.phys_offset = crate::vmm::get_phys_mem_offset();
        ctrl.ports_implemented = mmio_read(mmio + HBA_PI as u64);
        ctrl.num_ports = ((cap & 0x1F) + 1) as u8;
        ctrl.num_cmd_slots = (((cap >> 8) & 0x1F) + 1) as u8;
        ctrl.supports_64bit = cap & (1 << 31) != 0;
        ctrl.version = vs;
        ctrl.dma_phys = dma_phys;
        ctrl.active_port = port.port_num;
        ctrl.ports.push(port);
        ctrl.ready = true;
        drop(ctrl);
        AHCI_AVAILABLE.store(true, Ordering::Relaxed);
        serial_println!("[AHCI] AHCI/SATA controller initialized (DMA live)");
        let _ = dma_self_test();
    } else {
        serial_println!("[AHCI] No AHCI controller found");
    }
}

static ATA_IRQ_PENDING: AtomicBool = AtomicBool::new(false);

pub fn handle_ata_interrupt() {
    ATA_IRQ_PENDING.store(true, Ordering::Release);
}

pub fn check_ata_irq_pending() -> bool {
    ATA_IRQ_PENDING.swap(false, Ordering::AcqRel)
}

pub fn read_sectors_dma(port: u8, lba: u64, count: u16) -> Result<Vec<u8>, &'static str> {
    let mut buffer = alloc::vec![0u8; count as usize * 512];
    if issue_dma(port, ATA_CMD_READ_DMA_EXT, lba, count, &mut buffer, false) {
        Ok(buffer)
    } else {
        Err("AHCI DMA read failed")
    }
}

pub fn write_sectors_dma(port: u8, lba: u64, data: &[u8]) -> Result<(), &'static str> {
    let count = data.len().div_ceil(512) as u16;
    if count == 0 {
        return Ok(());
    }
    let mut buf = alloc::vec![0u8; count as usize * 512];
    let n = data.len().min(buf.len());
    buf[..n].copy_from_slice(&data[..n]);
    if issue_dma(port, ATA_CMD_WRITE_DMA_EXT, lba, count, &mut buf, true) {
        Ok(())
    } else {
        Err("AHCI DMA write failed")
    }
}

pub fn identify_device(port: u8) -> Result<DeviceIdentity, &'static str> {
    let mut ident = [0u8; 512];
    if !issue_dma(port, ATA_CMD_IDENTIFY, 0, 1, &mut ident, false) {
        return Err("IDENTIFY failed");
    }
    let model = ata_string(&ident[54..94]);
    let serial = ata_string(&ident[20..40]);
    let firmware = ata_string(&ident[46..54]);
    let lba48 = u64::from_le_bytes([
        ident[200], ident[201], ident[202], ident[203], ident[204], ident[205], ident[206],
        ident[207],
    ]);
    let lba28 = u32::from_le_bytes([ident[120], ident[121], ident[122], ident[123]]) as u64;
    let total = if lba48 != 0 { lba48 } else { lba28 };
    {
        let mut ctrl = AHCI.lock();
        if let Some(p) = ctrl.ports.iter_mut().find(|p| p.port_num == port) {
            p.model = model.clone();
            p.serial = serial.clone();
            p.firmware = firmware.clone();
            p.capacity_sectors = total;
        }
    }
    Ok(DeviceIdentity {
        model,
        serial,
        firmware,
        total_sectors: total,
        sector_size: 512,
        supports_ncq: ident[164] & 0x02 != 0,
        queue_depth: (ident[150] as u32 & 0x1F) + 1,
    })
}

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

pub fn detected_port_count() -> usize {
    AHCI.lock().ports.iter().filter(|p| p.connected).count()
}

pub const GATE_C4_MARKER: &str = "GATE_C4 ahci dma complete";
const GATE_C4_LBA: u64 = 4096;
const GATE_C4_PAYLOAD: &[u8] = b"knoxos-c4-ahci-dma\n";

/// Write a unique sector and read it back through AHCI DMA.
pub fn dma_self_test() -> bool {
    if !is_available() {
        serial_println!("[AHCI] Gate C4 skipped: no AHCI DMA path");
        return false;
    }
    let port = AHCI.lock().active_port;
    if port == 0xFF {
        serial_println!("[AHCI] Gate C4 skipped: no active port");
        return false;
    }
    if let Ok(ident) = identify_device(port) {
        serial_println!(
            "[AHCI] IDENTIFY: '{}' serial={} sectors={}",
            ident.model,
            ident.serial,
            ident.total_sectors
        );
    }

    let mut sector = [0u8; 512];
    sector[..GATE_C4_PAYLOAD.len()].copy_from_slice(GATE_C4_PAYLOAD);
    if !write_sectors(port, GATE_C4_LBA, 1, &sector) {
        serial_println!("[AHCI] Gate C4 FAILED: DMA write");
        return false;
    }
    let mut readback = [0u8; 512];
    if !read_sectors(port, GATE_C4_LBA, 1, &mut readback) {
        serial_println!("[AHCI] Gate C4 FAILED: DMA read");
        return false;
    }
    if readback[..GATE_C4_PAYLOAD.len()] != GATE_C4_PAYLOAD[..] {
        serial_println!("[AHCI] Gate C4 FAILED: readback mismatch");
        return false;
    }
    serial_println!("[AHCI] {}", GATE_C4_MARKER);
    true
}
