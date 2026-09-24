//! XHCI host controller: PCI discovery, MMIO, rings, and controller state.
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::serial_println;

use super::types::UsbDevice;

// ─── XHCI Controller ──────────────────────────────────────────────────

/// XHCI Capability Registers
#[repr(C)]
#[derive(Debug)]
pub struct XhciCapRegs {
    pub cap_length: u8,
    pub _rsvd: u8,
    pub hci_version: u16,
    pub hcs_params1: u32,
    pub hcs_params2: u32,
    pub hcs_params3: u32,
    pub hcc_params1: u32,
    pub db_offset: u32,
    pub rts_offset: u32,
    pub hcc_params2: u32,
}

/// XHCI Operational Registers offsets
pub const XHCI_USBCMD: u32 = 0x00;
pub const XHCI_USBSTS: u32 = 0x04;
pub const XHCI_PAGESIZE: u32 = 0x08;
pub const XHCI_DNCTRL: u32 = 0x14;
pub const XHCI_CRCR: u32 = 0x18;
pub const XHCI_DCBAAP: u32 = 0x30;
pub const XHCI_CONFIG: u32 = 0x38;

/// XHCI USB Command Register bits
pub const XHCI_CMD_RUN: u32 = 1 << 0;
pub const XHCI_CMD_HCRST: u32 = 1 << 1;
pub const XHCI_CMD_INTE: u32 = 1 << 2;
pub const XHCI_CMD_HSEE: u32 = 1 << 3;

/// XHCI USB Status Register bits
pub const XHCI_STS_HCH: u32 = 1 << 0;
pub const XHCI_STS_HSE: u32 = 1 << 2;
pub const XHCI_STS_EINT: u32 = 1 << 3;
pub const XHCI_STS_PCD: u32 = 1 << 4;
pub const XHCI_STS_CNR: u32 = 1 << 11;

/// TRB (Transfer Request Block) types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrbType {
    Normal = 1,
    SetupStage = 2,
    DataStage = 3,
    StatusStage = 4,
    Isoch = 5,
    Link = 6,
    EventData = 7,
    NoOp = 8,
    EnableSlot = 9,
    DisableSlot = 10,
    AddressDevice = 11,
    ConfigureEndpoint = 12,
    EvaluateContext = 13,
    ResetEndpoint = 14,
    StopEndpoint = 15,
    SetTRDequeuePointer = 16,
    ResetDevice = 17,
    NoOpCommand = 23,
    TransferEvent = 32,
    CommandCompletion = 33,
    PortStatusChange = 34,
}

/// Transfer Request Block (16 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Trb {
    pub param_lo: u32,
    pub param_hi: u32,
    pub status: u32,
    pub control: u32,
}

impl Default for Trb {
    fn default() -> Self {
        Self::new()
    }
}

impl Trb {
    pub fn new() -> Self {
        Self {
            param_lo: 0,
            param_hi: 0,
            status: 0,
            control: 0,
        }
    }

    pub fn trb_type(&self) -> u8 {
        ((self.control >> 10) & 0x3F) as u8
    }

    pub fn set_type(&mut self, trb_type: TrbType) {
        self.control = (self.control & !(0x3F << 10)) | ((trb_type as u32) << 10);
    }

    pub fn completion_code(&self) -> u8 {
        ((self.status >> 24) & 0xFF) as u8
    }
}

/// XHCI controller state
pub struct XhciController {
    /// PCI BDF (bus, device, function)
    pub pci_bus: u8,
    pub pci_dev: u8,
    pub pci_func: u8,
    /// BAR0 MMIO base
    pub mmio_base: u64,
    /// Capability register length
    pub cap_length: u8,
    /// Operational registers base
    pub op_base: u64,
    /// Runtime registers base
    pub rt_base: u64,
    /// Doorbell registers base
    pub db_base: u64,
    /// Maximum device slots
    pub max_slots: u8,
    /// Maximum ports
    pub max_ports: u8,
    /// Maximum interrupters
    pub max_intrs: u16,
    /// Is the controller initialized?
    pub initialized: bool,
    /// Connected devices
    pub devices: Vec<UsbDevice>,
    /// DCBAA — kernel-allocated, page-aligned, physical pointer array
    pub dcbaa_ptr: u64,
    pub dcbaa_layout: Option<alloc::alloc::Layout>,
    /// Command Ring — kernel-allocated TRB ring
    pub cmd_ring_ptr: u64,
    pub cmd_ring_layout: Option<alloc::alloc::Layout>,
    pub cmd_ring_enqueue: usize,
    pub cmd_ring_cycle: bool,
    /// Event Ring Segment Table + Event Ring
    pub event_ring_ptr: u64,
    pub event_ring_layout: Option<alloc::alloc::Layout>,
    pub erst_ptr: u64,
    pub erst_layout: Option<alloc::alloc::Layout>,
    pub event_ring_dequeue: usize,
    pub event_ring_cycle: bool,
    /// Per-slot transfer rings (slot_id → (ring_ptr, layout, enqueue_idx, cycle_bit))
    pub transfer_rings: BTreeMap<u8, Vec<TransferRing>>,
    /// Page size reported by controller
    pub page_size: u32,
}

/// A transfer ring for a specific endpoint
pub struct TransferRing {
    pub ptr: u64,
    pub layout: Option<alloc::alloc::Layout>,
    pub enqueue: usize,
    pub cycle: bool,
    pub size: usize,
}

impl TransferRing {
    pub fn new() -> Self {
        Self {
            ptr: 0,
            layout: None,
            enqueue: 0,
            cycle: true,
            size: 0,
        }
    }
}

impl Default for XhciController {
    fn default() -> Self {
        Self::new()
    }
}

impl XhciController {
    pub fn new() -> Self {
        Self {
            pci_bus: 0,
            pci_dev: 0,
            pci_func: 0,
            mmio_base: 0,
            cap_length: 0,
            op_base: 0,
            rt_base: 0,
            db_base: 0,
            max_slots: 0,
            max_ports: 0,
            max_intrs: 0,
            initialized: false,
            devices: Vec::new(),
            dcbaa_ptr: 0,
            dcbaa_layout: None,
            cmd_ring_ptr: 0,
            cmd_ring_layout: None,
            cmd_ring_enqueue: 0,
            cmd_ring_cycle: true,
            event_ring_ptr: 0,
            event_ring_layout: None,
            erst_ptr: 0,
            erst_layout: None,
            event_ring_dequeue: 0,
            event_ring_cycle: true,
            transfer_rings: BTreeMap::new(),
            page_size: 4096,
        }
    }
}

lazy_static::lazy_static! {
    pub static ref XHCI: Mutex<XhciController> = Mutex::new(XhciController::new());
}

pub(super) static XHCI_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Scan PCI bus for XHCI controllers
pub(super) fn find_xhci_controller() -> Option<(u8, u8, u8, u32)> {
    // XHCI class: 0x0C (Serial Bus), subclass: 0x03 (USB), prog IF: 0x30 (XHCI)
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

                if class_code == 0x0C && subclass == 0x03 && prog_if == 0x30 {
                    let bar0 = pci_read32(bus, dev, func, 0x10);
                    return Some((bus, dev, func, bar0));
                }
            }
        }
    }
    None
}

/// PCI configuration space read helpers
pub(super) fn pci_read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
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

pub(super) fn pci_read16(bus: u8, dev: u8, func: u8, offset: u8) -> u16 {
    let val = pci_read32(bus, dev, func, offset & 0xFC);
    ((val >> ((offset & 2) * 8)) & 0xFFFF) as u16
}

pub(super) fn pci_read8(bus: u8, dev: u8, func: u8, offset: u8) -> u8 {
    let val = pci_read32(bus, dev, func, offset & 0xFC);
    ((val >> ((offset & 3) * 8)) & 0xFF) as u8
}

pub(super) fn pci_write32(bus: u8, dev: u8, func: u8, offset: u8, value: u32) {
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

/// Number of TRBs in each ring
pub(super) const CMD_RING_SIZE: usize = 256;
pub(super) const EVENT_RING_SIZE: usize = 256;
pub(super) const TRANSFER_RING_SIZE: usize = 256;

/// Size of a TRB in bytes
pub(super) const TRB_SIZE: usize = 16;

/// Allocate a page-aligned, zeroed buffer and return (virtual_addr, Layout)
pub(super) fn alloc_ring_buffer(size: usize) -> (u64, alloc::alloc::Layout) {
    let layout = alloc::alloc::Layout::from_size_align(size, 4096).unwrap();
    let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
    if ptr.is_null() {
        panic!("[XHCI] Failed to allocate ring buffer of {} bytes", size);
    }
    (ptr as u64, layout)
}

/// Initialize the XHCI controller — real register programming
pub fn init_controller(bus: u8, dev: u8, func: u8, bar0: u32) -> bool {
    let mut xhci = XHCI.lock();

    // Enable bus mastering, memory space, disable legacy interrupts
    let cmd = pci_read16(bus, dev, func, 0x04);
    let new_cmd = (cmd | 0x06) & !0x0400; // Bus Master + Memory Space, disable INTx
    pci_write32(bus, dev, func, 0x04, new_cmd as u32);

    // Get MMIO base from BAR0 (handle 32-bit and 64-bit BARs)
    let mut mmio_base = (bar0 & 0xFFFFFFF0) as u64;
    if bar0 & 0x04 != 0 {
        let bar1 = pci_read32(bus, dev, func, 0x14);
        mmio_base |= (bar1 as u64) << 32;
    }

    xhci.pci_bus = bus;
    xhci.pci_dev = dev;
    xhci.pci_func = func;
    xhci.mmio_base = mmio_base;

    serial_println!(
        "[XHCI] Controller at PCI {:02x}:{:02x}.{}, MMIO={:#x}",
        bus,
        dev,
        func,
        mmio_base
    );

    // ─── Read Capability Registers ───────────────────────────────
    let cap_length: u8;
    let hci_version: u16;
    let hcs_params1: u32;
    let hcc_params1: u32;
    let db_offset: u32;
    let rts_offset: u32;

    unsafe {
        let cap0 = xhci_read32(mmio_base, 0x00);
        cap_length = (cap0 & 0xFF) as u8;
        hci_version = ((cap0 >> 16) & 0xFFFF) as u16;
        hcs_params1 = xhci_read32(mmio_base, 0x04);
        hcc_params1 = xhci_read32(mmio_base, 0x10);
        db_offset = xhci_read32(mmio_base, 0x14);
        rts_offset = xhci_read32(mmio_base, 0x18);
    }

    let max_slots = (hcs_params1 & 0xFF) as u8;
    let max_intrs = ((hcs_params1 >> 8) & 0x7FF) as u16;
    let max_ports = ((hcs_params1 >> 24) & 0xFF) as u8;

    xhci.cap_length = cap_length;
    xhci.op_base = mmio_base + cap_length as u64;
    xhci.rt_base = mmio_base + rts_offset as u64;
    xhci.db_base = mmio_base + db_offset as u64;
    xhci.max_slots = max_slots;
    xhci.max_ports = max_ports;
    xhci.max_intrs = max_intrs;

    serial_println!(
        "[XHCI] HCI v{}.{}, CapLen={}, MaxSlots={}, MaxPorts={}, MaxIntrs={}",
        hci_version >> 8,
        hci_version & 0xFF,
        cap_length,
        max_slots,
        max_ports,
        max_intrs
    );

    let op_base = xhci.op_base;

    // ─── Halt the controller ─────────────────────────────────────
    unsafe {
        let usbcmd = xhci_read32(op_base, XHCI_USBCMD);
        if usbcmd & XHCI_CMD_RUN != 0 {
            xhci_write32(op_base, XHCI_USBCMD, usbcmd & !XHCI_CMD_RUN);
            // Wait for HCH (Halted) bit
            for _ in 0..1000 {
                let sts = xhci_read32(op_base, XHCI_USBSTS);
                if sts & XHCI_STS_HCH != 0 {
                    break;
                }
                for _ in 0..10000 {
                    core::hint::spin_loop();
                }
            }
        }
    }

    // ─── Reset the controller ────────────────────────────────────
    unsafe {
        xhci_write32(op_base, XHCI_USBCMD, XHCI_CMD_HCRST);
        // Wait for HCRST to clear and CNR to clear
        for _ in 0..10000 {
            let cmd_val = xhci_read32(op_base, XHCI_USBCMD);
            let sts = xhci_read32(op_base, XHCI_USBSTS);
            if cmd_val & XHCI_CMD_HCRST == 0 && sts & XHCI_STS_CNR == 0 {
                break;
            }
            for _ in 0..10000 {
                core::hint::spin_loop();
            }
        }
        serial_println!("[XHCI] Controller reset complete");
    }

    // ─── Read page size ──────────────────────────────────────────
    unsafe {
        let ps = xhci_read32(op_base, XHCI_PAGESIZE);
        xhci.page_size = (ps & 0xFFFF) << 12; // Bit n means 2^(n+12) page size
        if xhci.page_size == 0 {
            xhci.page_size = 4096;
        }
        serial_println!("[XHCI] Page size: {} bytes", xhci.page_size);
    }

    // ─── Set Max Device Slots Enabled ────────────────────────────
    unsafe {
        xhci_write32(op_base, XHCI_CONFIG, max_slots as u32);
        serial_println!("[XHCI] MaxSlotsEn set to {}", max_slots);
    }

    // ─── Allocate DCBAA (Device Context Base Address Array) ──────
    // Need (max_slots + 1) * 8 bytes, 64-byte aligned (we use page aligned)
    let dcbaa_size = ((max_slots as usize) + 1) * 8;
    let dcbaa_alloc_size = (dcbaa_size + 4095) & !4095; // Round up to page
    let (dcbaa_ptr, dcbaa_layout) = alloc_ring_buffer(dcbaa_alloc_size);
    xhci.dcbaa_ptr = dcbaa_ptr;
    xhci.dcbaa_layout = Some(dcbaa_layout);

    unsafe {
        // Write DCBAAP (64-bit, two 32-bit writes)
        xhci_write32(op_base, XHCI_DCBAAP, (dcbaa_ptr & 0xFFFFFFFF) as u32);
        xhci_write32(op_base, XHCI_DCBAAP + 4, (dcbaa_ptr >> 32) as u32);
    }
    serial_println!("[XHCI] DCBAA at {:#x} ({} slots)", dcbaa_ptr, max_slots);

    // ─── Allocate Command Ring ───────────────────────────────────
    let cmd_ring_bytes = CMD_RING_SIZE * TRB_SIZE;
    let (cmd_ring_ptr, cmd_ring_layout) = alloc_ring_buffer(cmd_ring_bytes);
    xhci.cmd_ring_ptr = cmd_ring_ptr;
    xhci.cmd_ring_layout = Some(cmd_ring_layout);
    xhci.cmd_ring_enqueue = 0;
    xhci.cmd_ring_cycle = true;

    // Place a Link TRB at the last entry pointing back to the start
    unsafe {
        let link_trb_addr = cmd_ring_ptr + ((CMD_RING_SIZE - 1) * TRB_SIZE) as u64;
        let link = link_trb_addr as *mut Trb;
        (*link).param_lo = (cmd_ring_ptr & 0xFFFFFFFF) as u32;
        (*link).param_hi = (cmd_ring_ptr >> 32) as u32;
        (*link).status = 0;
        // TRB type = Link (6), Toggle Cycle bit
        (*link).control = ((TrbType::Link as u32) << 10) | (1 << 1) | 1; // TC=1, C=1
    }

    unsafe {
        // Write CRCR — command ring pointer with cycle bit
        let crcr_val = cmd_ring_ptr | 1; // RCS = 1 (Ring Cycle State)
        xhci_write32(op_base, XHCI_CRCR, (crcr_val & 0xFFFFFFFF) as u32);
        xhci_write32(op_base, XHCI_CRCR + 4, (crcr_val >> 32) as u32);
    }
    serial_println!(
        "[XHCI] Command Ring at {:#x} ({} TRBs)",
        cmd_ring_ptr,
        CMD_RING_SIZE
    );

    // ─── Allocate Event Ring + ERST (Event Ring Segment Table) ───
    let event_ring_bytes = EVENT_RING_SIZE * TRB_SIZE;
    let (event_ring_ptr, event_ring_layout) = alloc_ring_buffer(event_ring_bytes);
    xhci.event_ring_ptr = event_ring_ptr;
    xhci.event_ring_layout = Some(event_ring_layout);
    xhci.event_ring_dequeue = 0;
    xhci.event_ring_cycle = true;

    // ERST entry: 16 bytes (segment base addr[8] + segment size[4] + rsvd[4])
    let (erst_ptr, erst_layout) = alloc_ring_buffer(4096); // one page for ERST
    xhci.erst_ptr = erst_ptr;
    xhci.erst_layout = Some(erst_layout);

    unsafe {
        // Fill ERST entry 0: base = event_ring_ptr, size = EVENT_RING_SIZE
        let erst = erst_ptr as *mut u64;
        *erst = event_ring_ptr;
        let erst_size = (erst_ptr + 8) as *mut u32;
        *erst_size = EVENT_RING_SIZE as u32;
        // rsvd at +12 already zeroed
    }

    // Program interrupter 0: ERSTSZ, ERSTBA, ERDP
    let ir0_base = xhci.rt_base + 0x20; // Interrupter Register Set 0
    unsafe {
        // ERSTSZ — number of segments
        xhci_write32(ir0_base, 0x08, 1);
        // ERDP — Event Ring Dequeue Pointer (must be written before ERSTBA)
        xhci_write32(ir0_base, 0x18, (event_ring_ptr & 0xFFFFFFFF) as u32);
        xhci_write32(ir0_base, 0x1C, (event_ring_ptr >> 32) as u32);
        // ERSTBA — Event Ring Segment Table Base Address
        xhci_write32(ir0_base, 0x10, (erst_ptr & 0xFFFFFFFF) as u32);
        xhci_write32(ir0_base, 0x14, (erst_ptr >> 32) as u32);
        // Enable interrupter 0: IMAN bit 1 (IE)
        let iman = xhci_read32(ir0_base, 0x00);
        xhci_write32(ir0_base, 0x00, iman | 0x02);
    }
    serial_println!(
        "[XHCI] Event Ring at {:#x}, ERST at {:#x}",
        event_ring_ptr,
        erst_ptr
    );

    // ─── Start the controller ────────────────────────────────────
    unsafe {
        let usbcmd = xhci_read32(op_base, XHCI_USBCMD);
        xhci_write32(op_base, XHCI_USBCMD, usbcmd | XHCI_CMD_RUN | XHCI_CMD_INTE);
        // Wait for HCH to clear (controller running)
        for _ in 0..1000 {
            let sts = xhci_read32(op_base, XHCI_USBSTS);
            if sts & XHCI_STS_HCH == 0 {
                break;
            }
            for _ in 0..10000 {
                core::hint::spin_loop();
            }
        }
    }
    serial_println!("[XHCI] Controller started (RUN + INTE)");

    xhci.initialized = true;
    true
}

/// Check if XHCI is available
pub fn is_available() -> bool {
    XHCI_AVAILABLE.load(Ordering::Relaxed)
}

/// Get list of connected USB devices
pub fn list_devices() -> Vec<UsbDevice> {
    XHCI.lock().devices.clone()
}

/// Get device count
pub fn device_count() -> usize {
    XHCI.lock().devices.len()
}

// ═══════════════════════════════════════════════════════════════════════
// XHCI REGISTER ACCESS (MMIO)
// ═══════════════════════════════════════════════════════════════════════

/// Read a 32-bit XHCI register via MMIO
pub(super) unsafe fn xhci_read32(base: u64, offset: u32) -> u32 {
    let addr = (base + offset as u64) as *const u32;
    core::ptr::read_volatile(addr)
}

/// Write a 32-bit XHCI register via MMIO
pub(super) unsafe fn xhci_write32(base: u64, offset: u32, val: u32) {
    let addr = (base + offset as u64) as *mut u32;
    core::ptr::write_volatile(addr, val);
}
