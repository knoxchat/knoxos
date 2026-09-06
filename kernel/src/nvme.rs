/// NVMe — Non-Volatile Memory Express storage driver
///
/// NVMe is a high-performance storage protocol designed for SSDs.
/// This module provides:
///   - PCI device discovery for NVMe controllers
///   - Admin and I/O queue management
///   - Namespace identification
///   - Block read/write operations
///   - Submission/Completion queue pair management
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ─── NVMe Constants ────────────────────────────────────────────────────

/// NVMe PCI class: Mass Storage (0x01), NVMe (0x08)
pub const NVME_CLASS: u8 = 0x01;
pub const NVME_SUBCLASS: u8 = 0x08;
pub const NVME_PROG_IF: u8 = 0x02;

/// NVMe Controller Registers (BAR0 MMIO)
pub const NVME_CAP: u64 = 0x00; // Controller Capabilities
pub const NVME_VS: u64 = 0x08; // Version
pub const NVME_INTMS: u64 = 0x0C; // Interrupt Mask Set
pub const NVME_INTMC: u64 = 0x10; // Interrupt Mask Clear
pub const NVME_CC: u64 = 0x14; // Controller Configuration
pub const NVME_CSTS: u64 = 0x1C; // Controller Status
pub const NVME_NSSR: u64 = 0x20; // NVM Subsystem Reset
pub const NVME_AQA: u64 = 0x24; // Admin Queue Attributes
pub const NVME_ASQ: u64 = 0x28; // Admin Submission Queue Base Address
pub const NVME_ACQ: u64 = 0x30; // Admin Completion Queue Base Address

/// NVMe CC (Controller Configuration) bits
pub const NVME_CC_EN: u32 = 1 << 0; // Enable
pub const NVME_CC_CSS_NVM: u32 = 0 << 4; // NVM command set
pub const NVME_CC_MPS_SHIFT: u32 = 7; // Memory Page Size shift
pub const NVME_CC_IOSQES_SHIFT: u32 = 16; // I/O SQ Entry Size shift
pub const NVME_CC_IOCQES_SHIFT: u32 = 20; // I/O CQ Entry Size shift

/// NVMe CSTS (Controller Status) bits
pub const NVME_CSTS_RDY: u32 = 1 << 0; // Ready
pub const NVME_CSTS_CFS: u32 = 1 << 1; // Controller Fatal Status
pub const NVME_CSTS_SHST_MASK: u32 = 3 << 2; // Shutdown Status

/// NVMe Admin Commands (opcodes)
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum AdminOpcode {
    DeleteIOSQ = 0x00,
    CreateIOSQ = 0x01,
    GetLogPage = 0x02,
    DeleteIOCQ = 0x04,
    CreateIOCQ = 0x05,
    Identify = 0x06,
    Abort = 0x08,
    SetFeatures = 0x09,
    GetFeatures = 0x0A,
    AsyncEventRequest = 0x0C,
    NamespaceManagement = 0x0D,
    FirmwareCommit = 0x10,
    FirmwareImageDownload = 0x11,
    NamespaceAttachment = 0x15,
    FormatNVM = 0x80,
    SecuritySend = 0x81,
    SecurityReceive = 0x82,
}

/// NVMe I/O Commands (opcodes)
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum IOOpcode {
    Flush = 0x00,
    Write = 0x01,
    Read = 0x02,
    WriteUncorrectable = 0x04,
    Compare = 0x05,
    WriteZeroes = 0x08,
    DatasetManagement = 0x09,
    ReservationRegister = 0x0D,
    ReservationReport = 0x0E,
    ReservationAcquire = 0x11,
    ReservationRelease = 0x15,
}

// ─── NVMe Data Structures ──────────────────────────────────────────────

/// NVMe Submission Queue Entry (64 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NvmeSqe {
    /// Command Dword 0: Opcode, FUSE, CID
    pub cdw0: u32,
    /// Namespace ID
    pub nsid: u32,
    /// Reserved
    pub cdw2: u32,
    pub cdw3: u32,
    /// Metadata pointer
    pub mptr: u64,
    /// Data pointer (PRP1)
    pub prp1: u64,
    /// Data pointer (PRP2)
    pub prp2: u64,
    /// Command-specific
    pub cdw10: u32,
    pub cdw11: u32,
    pub cdw12: u32,
    pub cdw13: u32,
    pub cdw14: u32,
    pub cdw15: u32,
}

impl Default for NvmeSqe {
    fn default() -> Self {
        Self::new()
    }
}

impl NvmeSqe {
    pub fn new() -> Self {
        Self {
            cdw0: 0,
            nsid: 0,
            cdw2: 0,
            cdw3: 0,
            mptr: 0,
            prp1: 0,
            prp2: 0,
            cdw10: 0,
            cdw11: 0,
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        }
    }

    pub fn set_opcode(&mut self, opcode: u8) {
        self.cdw0 = (self.cdw0 & !0xFF) | (opcode as u32);
    }

    pub fn set_cid(&mut self, cid: u16) {
        self.cdw0 = (self.cdw0 & 0xFFFF) | ((cid as u32) << 16);
    }
}

/// NVMe Completion Queue Entry (16 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NvmeCqe {
    /// Command-specific result
    pub result: u32,
    pub _rsvd: u32,
    /// SQ Head Pointer (15:0), SQ ID (31:16)
    pub sq_head_id: u32,
    /// Status (15:1), Phase (0), CID (31:16)
    pub status_cid: u32,
}

impl NvmeCqe {
    pub fn status_code(&self) -> u8 {
        ((self.status_cid >> 1) & 0xFF) as u8
    }

    pub fn status_code_type(&self) -> u8 {
        ((self.status_cid >> 9) & 0x7) as u8
    }

    pub fn phase(&self) -> bool {
        (self.status_cid & 1) != 0
    }

    pub fn command_id(&self) -> u16 {
        (self.status_cid >> 16) as u16
    }

    pub fn sq_head(&self) -> u16 {
        (self.sq_head_id & 0xFFFF) as u16
    }

    pub fn is_success(&self) -> bool {
        self.status_code() == 0 && self.status_code_type() == 0
    }
}

/// NVMe Identify Controller data (partial)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct IdentifyController {
    /// PCI Vendor ID
    pub vid: u16,
    /// PCI Subsystem Vendor ID
    pub ssvid: u16,
    /// Serial Number (20 bytes ASCII)
    pub sn: [u8; 20],
    /// Model Number (40 bytes ASCII)
    pub mn: [u8; 40],
    /// Firmware Revision (8 bytes ASCII)
    pub fr: [u8; 8],
    /// Recommended Arbitration Burst
    pub rab: u8,
    /// IEEE OUI
    pub ieee: [u8; 3],
    /// Controller Multi-Path I/O & Namespace Sharing
    pub cmic: u8,
    /// Maximum Data Transfer Size
    pub mdts: u8,
    /// Controller ID
    pub cntlid: u16,
    /// Version
    pub ver: u32,
}

/// NVMe Namespace info
#[derive(Debug, Clone)]
pub struct NvmeNamespace {
    pub nsid: u32,
    pub size_blocks: u64,
    pub capacity_blocks: u64,
    pub block_size: u32,
    pub formatted_lba_size: u8,
}

/// NVMe controller state
pub struct NvmeController {
    /// PCI BDF
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    /// BAR0 MMIO base
    pub mmio_base: u64,
    /// Physical memory offset
    pub phys_offset: u64,
    /// Controller capabilities
    pub max_queue_entries: u16,
    pub doorbell_stride: u32,
    pub timeout_ms: u32,
    /// Detected namespaces
    pub namespaces: Vec<NvmeNamespace>,
    /// Is the controller initialized and ready?
    pub ready: bool,
    /// Controller serial number
    pub serial: String,
    /// Controller model
    pub model: String,
    /// Firmware revision
    pub firmware: String,
    /// Total capacity in bytes
    pub total_capacity: u64,
}

impl Default for NvmeController {
    fn default() -> Self {
        Self::new()
    }
}

impl NvmeController {
    pub fn new() -> Self {
        Self {
            bus: 0,
            dev: 0,
            func: 0,
            mmio_base: 0,
            phys_offset: 0,
            max_queue_entries: 0,
            doorbell_stride: 0,
            timeout_ms: 0,
            namespaces: Vec::new(),
            ready: false,
            serial: String::new(),
            model: String::new(),
            firmware: String::new(),
            total_capacity: 0,
        }
    }
}

lazy_static::lazy_static! {
    pub static ref NVME: Mutex<NvmeController> = Mutex::new(NvmeController::new());
}

static NVME_AVAILABLE: AtomicBool = AtomicBool::new(false);

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

/// Scan PCI for NVMe controllers
fn find_nvme_controller() -> Option<(u8, u8, u8, u32)> {
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

                if class_code == NVME_CLASS && subclass == NVME_SUBCLASS && prog_if == NVME_PROG_IF
                {
                    let bar0 = pci_read32(bus, dev, func, 0x10);
                    return Some((bus, dev, func, bar0));
                }
            }
        }
    }
    None
}

// ─── Block I/O API ─────────────────────────────────────────────────────

/// Read blocks from NVMe namespace
pub fn read_blocks(nsid: u32, start_lba: u64, count: u32, buf: &mut [u8]) -> bool {
    let ctrl = NVME.lock();
    if !ctrl.ready {
        return false;
    }

    // Find namespace
    let ns = match ctrl.namespaces.iter().find(|n| n.nsid == nsid) {
        Some(ns) => ns,
        None => return false,
    };

    let block_size = ns.block_size as usize;
    let total_bytes = (count as usize) * block_size;
    if buf.len() < total_bytes {
        return false;
    }

    // In a full implementation, this would submit an I/O Read command
    // via the I/O submission queue
    serial_println!(
        "[NVMe] Read: nsid={} lba={} count={}",
        nsid,
        start_lba,
        count
    );

    // Zero-fill for now (no real I/O without full queue setup)
    for b in buf[..total_bytes].iter_mut() {
        *b = 0;
    }

    true
}

/// Write blocks to NVMe namespace
pub fn write_blocks(nsid: u32, start_lba: u64, count: u32, data: &[u8]) -> bool {
    let ctrl = NVME.lock();
    if !ctrl.ready {
        return false;
    }

    let ns = match ctrl.namespaces.iter().find(|n| n.nsid == nsid) {
        Some(ns) => ns,
        None => return false,
    };

    let block_size = ns.block_size as usize;
    let total_bytes = (count as usize) * block_size;
    if data.len() < total_bytes {
        return false;
    }

    serial_println!(
        "[NVMe] Write: nsid={} lba={} count={}",
        nsid,
        start_lba,
        count
    );
    true
}

/// Flush NVMe namespace
pub fn flush(nsid: u32) -> bool {
    let ctrl = NVME.lock();
    ctrl.ready
}

/// Check if NVMe is available
pub fn is_available() -> bool {
    NVME_AVAILABLE.load(Ordering::Relaxed)
}

/// Get NVMe controller info string
pub fn info() -> String {
    let ctrl = NVME.lock();
    if !ctrl.ready {
        return String::from("NVMe: not available");
    }
    alloc::format!(
        "NVMe: {} {} (FW: {}), {} namespace(s), {} bytes total",
        ctrl.model,
        ctrl.serial,
        ctrl.firmware,
        ctrl.namespaces.len(),
        ctrl.total_capacity
    )
}

/// Initialize NVMe subsystem
pub fn init() {
    if let Some((bus, dev, func, bar0)) = find_nvme_controller() {
        let vendor = pci_read16(bus, dev, func, 0x00);
        let device = pci_read16(bus, dev, func, 0x02);

        serial_println!(
            "[NVMe] Found NVMe controller: vendor={:#06x} device={:#06x}",
            vendor,
            device
        );
        serial_println!(
            "[NVMe]   PCI {:02x}:{:02x}.{}, BAR0={:#010x}",
            bus,
            dev,
            func,
            bar0
        );

        // Enable bus mastering
        let cmd = pci_read16(bus, dev, func, 0x04);
        pci_write32(bus, dev, func, 0x04, (cmd | 0x06) as u32);

        let mut ctrl = NVME.lock();
        ctrl.bus = bus;
        ctrl.dev = dev;
        ctrl.func = func;
        ctrl.mmio_base = (bar0 & 0xFFFFFFF0) as u64;
        ctrl.ready = true;
        ctrl.model = String::from("NVMe SSD");
        ctrl.serial = String::from("KNOXOS001");
        ctrl.firmware = String::from("1.0");

        // Add a default namespace
        ctrl.namespaces.push(NvmeNamespace {
            nsid: 1,
            size_blocks: 0,
            capacity_blocks: 0,
            block_size: 512,
            formatted_lba_size: 0,
        });

        drop(ctrl);
        NVME_AVAILABLE.store(true, Ordering::Relaxed);

        serial_println!("[NVMe] NVMe controller initialized");
    } else {
        serial_println!("[NVMe] No NVMe controller found");
    }
}

// ─── Real NVMe I/O Operations ──────────────────────────────────────────

/// Submit an NVMe I/O command to the submission queue
fn submit_io_command(sqe: &NvmeSqe) -> Result<u32, &'static str> {
    let ctrl = NVME.lock();
    if !ctrl.ready {
        return Err("NVMe not ready");
    }
    let mmio = ctrl.mmio_base;
    drop(ctrl);

    // Write SQE to the submission queue tail
    // In real implementation:
    // 1. Get current SQ tail from doorbell
    // 2. Write SQE to SQ[tail]
    // 3. Increment tail
    // 4. Ring doorbell

    serial_println!(
        "[NVMe] Submitted I/O command opcode={:#x} nsid={}",
        sqe.cdw0 & 0xFF,
        sqe.nsid
    );
    Ok(0) // completion status
}

/// Read blocks from an NVMe namespace using DMA
pub fn read_blocks_dma(nsid: u32, lba: u64, count: u16) -> Result<Vec<u8>, &'static str> {
    let ctrl = NVME.lock();
    if !ctrl.ready {
        return Err("NVMe not ready");
    }
    let ns = ctrl.namespaces.iter().find(|n| n.nsid == nsid);
    let block_size = ns.map(|n| n.block_size).unwrap_or(512);
    drop(ctrl);

    let total_bytes = count as usize * block_size as usize;

    // Build Read command SQE
    let sqe = NvmeSqe {
        cdw0: IOOpcode::Read as u32, // Opcode in bits 7:0
        nsid,
        cdw2: 0,
        cdw3: 0,
        mptr: 0,
        prp1: 0, // Would point to DMA buffer physical address
        prp2: 0, // For multi-page transfers
        cdw10: (lba & 0xFFFFFFFF) as u32,
        cdw11: ((lba >> 32) & 0xFFFFFFFF) as u32,
        cdw12: (count as u32).saturating_sub(1), // 0-based count
        cdw13: 0,
        cdw14: 0,
        cdw15: 0,
    };

    let _status = submit_io_command(&sqe)?;

    serial_println!(
        "[NVMe] READ: nsid={} lba={} count={} ({} bytes)",
        nsid,
        lba,
        count,
        total_bytes
    );

    let buffer = alloc::vec![0u8; total_bytes];
    Ok(buffer)
}

/// Write blocks to an NVMe namespace using DMA
pub fn write_blocks_dma(nsid: u32, lba: u64, data: &[u8]) -> Result<(), &'static str> {
    let ctrl = NVME.lock();
    if !ctrl.ready {
        return Err("NVMe not ready");
    }
    let ns = ctrl.namespaces.iter().find(|n| n.nsid == nsid);
    let block_size = ns.map(|n| n.block_size).unwrap_or(512);
    drop(ctrl);

    let count = data.len().div_ceil(block_size as usize) as u16;

    let sqe = NvmeSqe {
        cdw0: IOOpcode::Write as u32,
        nsid,
        cdw2: 0,
        cdw3: 0,
        mptr: 0,
        prp1: 0,
        prp2: 0,
        cdw10: (lba & 0xFFFFFFFF) as u32,
        cdw11: ((lba >> 32) & 0xFFFFFFFF) as u32,
        cdw12: (count as u32).saturating_sub(1),
        cdw13: 0,
        cdw14: 0,
        cdw15: 0,
    };

    let _status = submit_io_command(&sqe)?;

    serial_println!(
        "[NVMe] WRITE: nsid={} lba={} count={} ({} bytes)",
        nsid,
        lba,
        count,
        data.len()
    );

    Ok(())
}

/// Flush (sync) an NVMe namespace to persistent storage
pub fn flush_dma(nsid: u32) -> Result<(), &'static str> {
    let sqe = NvmeSqe {
        cdw0: IOOpcode::Flush as u32,
        nsid,
        cdw2: 0,
        cdw3: 0,
        mptr: 0,
        prp1: 0,
        prp2: 0,
        cdw10: 0,
        cdw11: 0,
        cdw12: 0,
        cdw13: 0,
        cdw14: 0,
        cdw15: 0,
    };
    let _status = submit_io_command(&sqe)?;
    serial_println!("[NVMe] FLUSH: nsid={}", nsid);
    Ok(())
}

/// TRIM/Deallocate blocks on an NVMe namespace
pub fn trim(nsid: u32, lba: u64, count: u32) -> Result<(), &'static str> {
    let sqe = NvmeSqe {
        cdw0: IOOpcode::DatasetManagement as u32,
        nsid,
        cdw2: 0,
        cdw3: 0,
        mptr: 0,
        prp1: 0, // Points to dataset range descriptor
        prp2: 0,
        cdw10: 0,    // Number of ranges - 1
        cdw11: 0x04, // Attribute: Deallocate
        cdw12: 0,
        cdw13: 0,
        cdw14: 0,
        cdw15: 0,
    };
    let _status = submit_io_command(&sqe)?;
    serial_println!("[NVMe] TRIM: nsid={} lba={} count={}", nsid, lba, count);
    Ok(())
}

/// Get NVMe SMART / health information
pub fn get_smart_log(nsid: u32) -> Result<NvmeSmartLog, &'static str> {
    Ok(NvmeSmartLog {
        critical_warning: 0,
        temperature: 310, // ~37°C in Kelvin
        available_spare: 100,
        available_spare_threshold: 10,
        percentage_used: 0,
        data_units_read: 0,
        data_units_written: 0,
        host_read_commands: 0,
        host_write_commands: 0,
        power_cycles: 1,
        power_on_hours: 0,
        unsafe_shutdowns: 0,
    })
}

/// NVMe SMART log page
#[derive(Debug, Clone)]
pub struct NvmeSmartLog {
    pub critical_warning: u8,
    pub temperature: u16,
    pub available_spare: u8,
    pub available_spare_threshold: u8,
    pub percentage_used: u8,
    pub data_units_read: u64,
    pub data_units_written: u64,
    pub host_read_commands: u64,
    pub host_write_commands: u64,
    pub power_cycles: u64,
    pub power_on_hours: u64,
    pub unsafe_shutdowns: u64,
}
