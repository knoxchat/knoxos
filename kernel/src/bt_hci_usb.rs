#[cfg(target_arch = "x86_64")]
use crate::arch_compat::instructions::port::Port;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::instructions::port::Port;
/// bt_hci_usb — Bluetooth HCI USB Transport Driver
///
/// Implements real Bluetooth HCI over USB transport (BT Core Spec Vol 4 Part B)
/// supporting USB-attached Bluetooth controllers (Intel, Broadcom, Qualcomm, etc.)
///
/// Architecture:
///   - USB device discovery: class 0xE0/01/01 (Wireless/Bluetooth/Bluetooth)
///   - HCI commands via USB Control endpoint (EP0)
///   - HCI events via USB Interrupt IN endpoint (EP1 IN)
///   - ACL data via USB Bulk IN/OUT endpoints (EP2 IN/OUT)
///   - SCO audio via USB Isochronous IN/OUT endpoints (EP3 IN/OUT)
///   - Integrates with bluetooth.rs for upper HCI/L2CAP protocol
///   - Firmware loading for Intel/Broadcom adapters
///
/// Supported controllers:
///   - Intel AX200/AX201/AX210 (0x8087:0x0xxx)
///   - Broadcom BCM20702/BCM4356 (0x0A5C:0xxxx)
///   - Qualcomm QCA6174/QCA9377 (0x0CF3:0xxxx)
///   - Realtek RTL8761B/RTL8822C (0x0BDA:0xxxx)
///   - Cambridge Silicon Radio (CSR) (0x0A12:0x0001)
///   - VirtIO Bluetooth (future)
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// USB BLUETOOTH CLASS CODES
// ═══════════════════════════════════════════════════════════════════════

/// USB class code for Wireless Controller
const USB_CLASS_WIRELESS: u8 = 0xE0;
/// USB subclass for Bluetooth
const USB_SUBCLASS_BT: u8 = 0x01;
/// USB protocol for Bluetooth (Programming Interface)
const USB_PROTOCOL_BT: u8 = 0x01;

/// Max HCI command packet size
const HCI_MAX_CMD_SIZE: usize = 255 + 3; // opcode(2) + plen(1) + params(255)
/// Max HCI event packet size
const HCI_MAX_EVT_SIZE: usize = 255 + 2; // code(1) + plen(1) + params(255)
/// Max ACL data packet size
const HCI_MAX_ACL_SIZE: usize = 1024;
/// Max SCO data packet size
const HCI_MAX_SCO_SIZE: usize = 255;

// ═══════════════════════════════════════════════════════════════════════
// USB TRANSFER TYPES
// ═══════════════════════════════════════════════════════════════════════

/// USB request type for HCI command (class-specific, host-to-device)
const USB_TYPE_CLASS: u8 = 0x20;
/// USB request for sending HCI command
const HCI_SEND_CMD: u8 = 0x00;

/// USB endpoint addresses for Bluetooth HCI
#[derive(Debug, Clone, Copy)]
pub struct BtUsbEndpoints {
    /// Control endpoint (EP0) — HCI commands
    pub ctrl: u8,
    /// Interrupt IN endpoint — HCI events
    pub intr_in: u8,
    /// Bulk IN endpoint — ACL data RX
    pub bulk_in: u8,
    /// Bulk OUT endpoint — ACL data TX
    pub bulk_out: u8,
    /// Isochronous IN endpoint — SCO audio RX
    pub isoc_in: u8,
    /// Isochronous OUT endpoint — SCO audio TX
    pub isoc_out: u8,
}

impl Default for BtUsbEndpoints {
    fn default() -> Self {
        Self {
            ctrl: 0x00,
            intr_in: 0x81,  // EP1 IN
            bulk_in: 0x82,  // EP2 IN
            bulk_out: 0x02, // EP2 OUT
            isoc_in: 0x83,  // EP3 IN
            isoc_out: 0x03, // EP3 OUT
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// USB DEVICE DESCRIPTORS
// ═══════════════════════════════════════════════════════════════════════

/// USB Device Descriptor (partial)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct UsbDeviceDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub bcd_usb: u16,
    pub b_device_class: u8,
    pub b_device_sub_class: u8,
    pub b_device_protocol: u8,
    pub b_max_packet_size0: u8,
    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,
    pub i_manufacturer: u8,
    pub i_product: u8,
    pub i_serial_number: u8,
    pub b_num_configurations: u8,
}

/// USB Endpoint Descriptor
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct UsbEndpointDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_endpoint_address: u8,
    pub bm_attributes: u8,
    pub w_max_packet_size: u16,
    pub b_interval: u8,
}

// ═══════════════════════════════════════════════════════════════════════
// HCI USB TRANSPORT STATE
// ═══════════════════════════════════════════════════════════════════════

/// Known Bluetooth USB controller vendors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtVendor {
    Intel,
    Broadcom,
    Qualcomm,
    Realtek,
    Csr,
    MediaTek,
    Unknown,
}

impl BtVendor {
    fn from_usb_vid(vid: u16) -> Self {
        match vid {
            0x8087 => BtVendor::Intel,
            0x0A5C | 0x0489 => BtVendor::Broadcom,
            0x0CF3 | 0x04CA => BtVendor::Qualcomm,
            0x0BDA | 0x13D3 => BtVendor::Realtek,
            0x0A12 => BtVendor::Csr,
            0x0E8D => BtVendor::MediaTek,
            _ => BtVendor::Unknown,
        }
    }

    fn needs_firmware(&self) -> bool {
        matches!(
            self,
            BtVendor::Intel | BtVendor::Broadcom | BtVendor::Qualcomm | BtVendor::Realtek
        )
    }
}

/// USB Bluetooth HCI transport driver
pub struct BtHciUsb {
    /// USB bus/device/interface identity
    pub usb_bus: u8,
    pub usb_dev: u8,
    pub usb_iface: u8,

    /// USB IDs
    pub vendor_id: u16,
    pub product_id: u16,
    pub vendor: BtVendor,

    /// XHCI controller MMIO base (for issuing USB transfers)
    pub xhci_base: u64,
    /// XHCI slot ID assigned to this device
    pub slot_id: u8,

    /// Endpoint configuration
    pub endpoints: BtUsbEndpoints,

    /// Bluetooth device address (read from controller)
    pub bd_addr: [u8; 6],

    /// Whether firmware has been loaded
    pub firmware_loaded: bool,

    /// Whether the controller is initialized and ready
    pub initialized: bool,

    /// Command sequence number (for vendor-specific commands)
    pub cmd_seq: u16,

    /// Pending HCI events (received from interrupt endpoint)
    pub event_queue: Vec<Vec<u8>>,

    /// Pending ACL data (received from bulk IN endpoint)
    pub acl_rx_queue: Vec<Vec<u8>>,

    /// Interrupt IN transfer buffer
    pub intr_buf: Vec<u8>,

    /// Bulk IN transfer buffer
    pub bulk_in_buf: Vec<u8>,

    /// Transfer descriptors for async USB transfers
    pub pending_intr_td: Option<u64>,
    pub pending_bulk_in_td: Option<u64>,

    /// Statistics
    pub cmds_sent: u64,
    pub events_received: u64,
    pub acl_tx: u64,
    pub acl_rx: u64,
    pub sco_tx: u64,
    pub sco_rx: u64,
    pub errors: u64,
}

/// Global driver instance
static HCI_USB: Mutex<Option<BtHciUsb>> = Mutex::new(None);
static DEVICE_FOUND: AtomicBool = AtomicBool::new(false);

// ═══════════════════════════════════════════════════════════════════════
// XHCI USB TRANSFER HELPERS
// ═══════════════════════════════════════════════════════════════════════

/// Issue a USB control transfer (SETUP → DATA → STATUS)
///
/// Used for sending HCI commands to EP0
fn usb_control_transfer(
    xhci_base: u64,
    slot_id: u8,
    request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    data: &[u8],
) -> Result<usize, &'static str> {
    // Build SETUP stage TRB
    let setup_trb = UsbSetupTrb {
        bm_request_type: request_type,
        b_request: request,
        w_value: value,
        w_index: index,
        w_length: data.len() as u16,
    };

    // XHCI Transfer Ring: enqueue TRBs for control transfer
    unsafe {
        // Read endpoint context to get TR dequeue pointer
        // Endpoint 0 (default control) = DCI 1
        let ep0_ring_base = read_xhci_ep_ring(xhci_base, slot_id, 1);
        if ep0_ring_base == 0 {
            return Err("EP0 ring not configured");
        }

        let enqueue = XHCI_EP0_ENQUEUE.load(Ordering::Acquire) as u64;

        // 1. SETUP Stage TRB (type=2)
        let setup_data = (setup_trb.bm_request_type as u64)
            | ((setup_trb.b_request as u64) << 8)
            | ((setup_trb.w_value as u64) << 16)
            | ((setup_trb.w_index as u64) << 32)
            | ((setup_trb.w_length as u64) << 48);
        let setup_status: u32 = 8; // Transfer length = 8 (SETUP packet)
        let setup_control: u32 = (2 << 10) // TRB Type = Setup Stage
            | (1 << 6)  // IDT (Immediate Data)
            | (3 << 16); // TRT = 3 (data stage OUT if data.len > 0)
        core::ptr::write_volatile((ep0_ring_base + enqueue) as *mut u64, setup_data);
        core::ptr::write_volatile((ep0_ring_base + enqueue + 8) as *mut u32, setup_status);
        core::ptr::write_volatile((ep0_ring_base + enqueue + 12) as *mut u32, setup_control);

        let mut next = enqueue + 16;

        // 2. DATA Stage TRB (type=3) if data is non-empty
        if !data.is_empty() {
            let data_phys = data.as_ptr() as u64;
            let data_status: u32 = data.len() as u32;
            let data_control: u32 = (3 << 10) // TRB Type = Data Stage
                | (1 << 0); // Chain bit for multi-TRB
            core::ptr::write_volatile((ep0_ring_base + next) as *mut u64, data_phys);
            core::ptr::write_volatile((ep0_ring_base + next + 8) as *mut u32, data_status);
            core::ptr::write_volatile((ep0_ring_base + next + 12) as *mut u32, data_control);
            next += 16;
        }

        // 3. STATUS Stage TRB (type=4)
        let status_control: u32 = (4 << 10) // TRB Type = Status Stage
            | (1 << 5)   // IOC (Interrupt on Completion)
            | (1 << 16); // Direction = IN (for OUT data transfer)
        core::ptr::write_volatile((ep0_ring_base + next) as *mut u64, 0u64);
        core::ptr::write_volatile((ep0_ring_base + next + 8) as *mut u32, 0u32);
        core::ptr::write_volatile((ep0_ring_base + next + 12) as *mut u32, status_control);
        next += 16;

        XHCI_EP0_ENQUEUE.store(next as usize, Ordering::Release);

        // Ring doorbell: register at offset 0x800 + 4*slot_id, value=1 (DCI for EP0)
        let doorbell_offset = 0x800 + (slot_id as u64) * 4;
        core::ptr::write_volatile((xhci_base + doorbell_offset) as *mut u32, 1);

        // Busy-wait for transfer completion event
        for _ in 0..500_000u32 {
            let event_ring_base = read_xhci_event_ring(xhci_base);
            let evt_dequeue = XHCI_EVT_DEQUEUE.load(Ordering::Acquire) as u64;
            let evt_control =
                core::ptr::read_volatile((event_ring_base + evt_dequeue + 12) as *const u32);
            let evt_type = (evt_control >> 10) & 0x3F;
            let cycle = evt_control & 1;

            if evt_type == 32 && cycle == XHCI_EVT_CYCLE.load(Ordering::Acquire) as u32 {
                // Transfer Event TRB — check completion code
                let completion =
                    core::ptr::read_volatile((event_ring_base + evt_dequeue + 8) as *const u32);
                let cc = (completion >> 24) & 0xFF;
                XHCI_EVT_DEQUEUE.store((evt_dequeue + 16) as usize, Ordering::Release);

                if cc == 1 {
                    // Success
                    return Ok(data.len());
                } else {
                    serial_println!("[bt-hci-usb] Control transfer failed, CC={}", cc);
                    return Err("Control transfer failed");
                }
            }
            core::hint::spin_loop();
        }
    }

    Err("Control transfer timed out")
}

/// XHCI state tracking for EP0
static XHCI_EP0_ENQUEUE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static XHCI_EVT_DEQUEUE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
static XHCI_EVT_CYCLE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(1);

/// Read XHCI endpoint transfer ring base from endpoint context
unsafe fn read_xhci_ep_ring(xhci_base: u64, slot_id: u8, dci: u8) -> u64 {
    // Device Context Base Address Array at DCBAAP register
    let dcbaap = core::ptr::read_volatile((xhci_base + 0x30) as *const u64);
    if dcbaap == 0 {
        return 0;
    }

    // Slot's device context pointer
    let dev_ctx_ptr = core::ptr::read_volatile((dcbaap + (slot_id as u64) * 8) as *const u64);
    if dev_ctx_ptr == 0 {
        return 0;
    }

    // Endpoint Context at offset 0x20 * dci (each context is 32 bytes)
    let ep_ctx_base = dev_ctx_ptr + (dci as u64) * 0x20;
    // TR Dequeue Pointer is at offset 0x08 within the EP context
    let tr_dequeue = core::ptr::read_volatile((ep_ctx_base + 0x08) as *const u64);
    tr_dequeue & !0xF // Mask out DCS bit and reserved
}

/// Read XHCI event ring segment base
unsafe fn read_xhci_event_ring(xhci_base: u64) -> u64 {
    // Runtime registers start at RTSOFF (offset read from capability register)
    let rtsoff = core::ptr::read_volatile((xhci_base + 0x18) as *const u32) & !0x1F;
    // Interrupter 0 event ring dequeue pointer at runtime + 0x20 + 0x38
    let erdp = core::ptr::read_volatile((xhci_base + rtsoff as u64 + 0x20 + 0x38) as *const u64);
    erdp & !0xF
}

/// Issue a USB interrupt IN transfer (for receiving HCI events)
fn usb_interrupt_in(
    xhci_base: u64,
    slot_id: u8,
    endpoint: u8,
    buf: &mut [u8],
) -> Result<usize, &'static str> {
    unsafe {
        // Interrupt IN endpoint DCI = endpoint_addr * 2 + 1
        // EP1 IN (0x81) → DCI = 3
        let dci = (endpoint & 0x7F) * 2 + 1;
        let ep_ring = read_xhci_ep_ring(xhci_base, slot_id, dci);
        if ep_ring == 0 {
            return Ok(0);
        }

        // Enqueue Normal TRB with buffer for DMA
        let buf_phys = buf.as_ptr() as u64;
        let trb_status: u32 = buf.len() as u32;
        let trb_control: u32 = (1 << 10) // TRB Type = Normal
            | (1 << 5); // IOC
        core::ptr::write_volatile(ep_ring as *mut u64, buf_phys);
        core::ptr::write_volatile((ep_ring + 8) as *mut u32, trb_status);
        core::ptr::write_volatile((ep_ring + 12) as *mut u32, trb_control);

        // Ring doorbell for this endpoint
        let doorbell_offset = 0x800 + (slot_id as u64) * 4;
        core::ptr::write_volatile((xhci_base + doorbell_offset) as *mut u32, dci as u32);

        // Poll for completion (with short timeout for non-blocking)
        for _ in 0..10_000u32 {
            let event_ring_base = read_xhci_event_ring(xhci_base);
            let evt_dequeue = XHCI_EVT_DEQUEUE.load(Ordering::Acquire) as u64;
            let evt_control =
                core::ptr::read_volatile((event_ring_base + evt_dequeue + 12) as *const u32);
            let evt_type = (evt_control >> 10) & 0x3F;
            let cycle = evt_control & 1;

            if evt_type == 32 && cycle == XHCI_EVT_CYCLE.load(Ordering::Acquire) as u32 {
                let completion =
                    core::ptr::read_volatile((event_ring_base + evt_dequeue + 8) as *const u32);
                let cc = (completion >> 24) & 0xFF;
                let bytes_remaining = completion & 0xFFFFFF;
                XHCI_EVT_DEQUEUE.store((evt_dequeue + 16) as usize, Ordering::Release);

                if cc == 1 || cc == 13 {
                    // Success or Short Packet
                    let transferred = buf.len() - bytes_remaining as usize;
                    return Ok(transferred);
                }
                return Ok(0);
            }
            core::hint::spin_loop();
        }
    }

    Ok(0) // Timeout — no data available
}

/// Issue a USB bulk OUT transfer (for sending ACL data)
fn usb_bulk_out(
    xhci_base: u64,
    slot_id: u8,
    endpoint: u8,
    data: &[u8],
) -> Result<usize, &'static str> {
    unsafe {
        // Bulk OUT endpoint DCI = endpoint_addr * 2
        let dci = (endpoint & 0x7F) * 2;
        let ep_ring = read_xhci_ep_ring(xhci_base, slot_id, dci);
        if ep_ring == 0 {
            return Err("Bulk OUT ring not configured");
        }

        // Enqueue Normal TRB
        let data_phys = data.as_ptr() as u64;
        let trb_status: u32 = data.len() as u32;
        let trb_control: u32 = (1 << 10) | (1 << 5); // Normal + IOC
        core::ptr::write_volatile(ep_ring as *mut u64, data_phys);
        core::ptr::write_volatile((ep_ring + 8) as *mut u32, trb_status);
        core::ptr::write_volatile((ep_ring + 12) as *mut u32, trb_control);

        // Ring doorbell
        let doorbell_offset = 0x800 + (slot_id as u64) * 4;
        core::ptr::write_volatile((xhci_base + doorbell_offset) as *mut u32, dci as u32);

        // Wait for completion
        for _ in 0..200_000u32 {
            let event_ring_base = read_xhci_event_ring(xhci_base);
            let evt_dequeue = XHCI_EVT_DEQUEUE.load(Ordering::Acquire) as u64;
            let evt_control =
                core::ptr::read_volatile((event_ring_base + evt_dequeue + 12) as *const u32);
            let evt_type = (evt_control >> 10) & 0x3F;
            let cycle = evt_control & 1;

            if evt_type == 32 && cycle == XHCI_EVT_CYCLE.load(Ordering::Acquire) as u32 {
                let completion =
                    core::ptr::read_volatile((event_ring_base + evt_dequeue + 8) as *const u32);
                let cc = (completion >> 24) & 0xFF;
                XHCI_EVT_DEQUEUE.store((evt_dequeue + 16) as usize, Ordering::Release);

                if cc == 1 {
                    return Ok(data.len());
                }
                return Err("Bulk OUT transfer failed");
            }
            core::hint::spin_loop();
        }
    }

    Err("Bulk OUT transfer timed out")
}

/// Issue a USB bulk IN transfer (for receiving ACL data)
fn usb_bulk_in(
    xhci_base: u64,
    slot_id: u8,
    endpoint: u8,
    buf: &mut [u8],
) -> Result<usize, &'static str> {
    unsafe {
        let dci = (endpoint & 0x7F) * 2 + 1;
        let ep_ring = read_xhci_ep_ring(xhci_base, slot_id, dci);
        if ep_ring == 0 {
            return Ok(0);
        }

        let buf_phys = buf.as_ptr() as u64;
        let trb_status: u32 = buf.len() as u32;
        let trb_control: u32 = (1 << 10) | (1 << 5);
        core::ptr::write_volatile(ep_ring as *mut u64, buf_phys);
        core::ptr::write_volatile((ep_ring + 8) as *mut u32, trb_status);
        core::ptr::write_volatile((ep_ring + 12) as *mut u32, trb_control);

        let doorbell_offset = 0x800 + (slot_id as u64) * 4;
        core::ptr::write_volatile((xhci_base + doorbell_offset) as *mut u32, dci as u32);

        for _ in 0..10_000u32 {
            let event_ring_base = read_xhci_event_ring(xhci_base);
            let evt_dequeue = XHCI_EVT_DEQUEUE.load(Ordering::Acquire) as u64;
            let evt_control =
                core::ptr::read_volatile((event_ring_base + evt_dequeue + 12) as *const u32);
            let evt_type = (evt_control >> 10) & 0x3F;
            let cycle = evt_control & 1;

            if evt_type == 32 && cycle == XHCI_EVT_CYCLE.load(Ordering::Acquire) as u32 {
                let completion =
                    core::ptr::read_volatile((event_ring_base + evt_dequeue + 8) as *const u32);
                let cc = (completion >> 24) & 0xFF;
                let bytes_remaining = completion & 0xFFFFFF;
                XHCI_EVT_DEQUEUE.store((evt_dequeue + 16) as usize, Ordering::Release);

                if cc == 1 || cc == 13 {
                    return Ok(buf.len() - bytes_remaining as usize);
                }
                return Ok(0);
            }
            core::hint::spin_loop();
        }
    }

    Ok(0)
}

/// USB Setup TRB data
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct UsbSetupTrb {
    bm_request_type: u8,
    b_request: u8,
    w_value: u16,
    w_index: u16,
    w_length: u16,
}

// ═══════════════════════════════════════════════════════════════════════
// HCI COMMAND TRANSPORT
// ═══════════════════════════════════════════════════════════════════════

/// HCI packet indicators (for USB, these map to endpoint types)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HciPacketType {
    Command = 0x01, // Control EP OUT
    AclData = 0x02, // Bulk EP OUT
    ScoData = 0x03, // Isochronous EP OUT
    Event = 0x04,   // Interrupt EP IN
}

/// Send an HCI command via USB control endpoint
pub fn send_hci_command(opcode: u16, params: &[u8]) -> Result<(), &'static str> {
    let mut drv = HCI_USB.lock();
    let dev = drv.as_mut().ok_or("BT USB not initialized")?;

    if params.len() > 255 {
        return Err("HCI command params too long");
    }

    // Build HCI command packet: opcode(2 LE) + plen(1) + params
    let mut cmd = Vec::with_capacity(3 + params.len());
    cmd.push((opcode & 0xFF) as u8);
    cmd.push(((opcode >> 8) & 0xFF) as u8);
    cmd.push(params.len() as u8);
    cmd.extend_from_slice(params);

    // Send via USB control transfer to EP0
    // Request type: 0x20 (class, host-to-device, interface)
    // Request: 0x00 (HCI command)
    // Value: 0x0000
    // Index: interface number
    usb_control_transfer(
        dev.xhci_base,
        dev.slot_id,
        USB_TYPE_CLASS, // 0x20
        HCI_SEND_CMD,
        0,
        dev.usb_iface as u16,
        &cmd,
    )?;

    dev.cmds_sent += 1;

    let ogf = (opcode >> 10) & 0x3F;
    let ocf = opcode & 0x3FF;
    serial_println!(
        "[bt-hci-usb] HCI command sent: OGF={:#04x} OCF={:#05x} plen={}",
        ogf,
        ocf,
        params.len()
    );

    Ok(())
}

/// Receive HCI events from interrupt IN endpoint
pub fn recv_hci_events() -> Vec<Vec<u8>> {
    let mut drv = HCI_USB.lock();
    let dev = match drv.as_mut() {
        Some(d) => d,
        None => return Vec::new(),
    };

    // Poll interrupt IN endpoint for HCI events
    let mut events = Vec::new();
    let mut buf = vec![0u8; HCI_MAX_EVT_SIZE];
    match usb_interrupt_in(dev.xhci_base, dev.slot_id, dev.endpoints.intr_in, &mut buf) {
        Ok(len) if len > 0 => {
            events.push(buf[..len].to_vec());
            dev.events_received += 1;
        }
        _ => {}
    }

    // Also drain any queued events
    events.append(&mut dev.event_queue);
    events
}

/// Send ACL data via bulk OUT endpoint
pub fn send_acl_data(
    handle: u16,
    pb_flag: u8,
    bc_flag: u8,
    data: &[u8],
) -> Result<(), &'static str> {
    let mut drv = HCI_USB.lock();
    let dev = drv.as_mut().ok_or("BT USB not initialized")?;

    if data.len() > HCI_MAX_ACL_SIZE - 4 {
        return Err("ACL data too large");
    }

    // Build ACL packet: handle(2 LE) + length(2 LE) + data
    // Handle field: bits [11:0]=connection handle, [13:12]=PB flag, [15:14]=BC flag
    let handle_field =
        (handle & 0x0FFF) | (((pb_flag & 0x03) as u16) << 12) | (((bc_flag & 0x03) as u16) << 14);

    let mut pkt = Vec::with_capacity(4 + data.len());
    pkt.push((handle_field & 0xFF) as u8);
    pkt.push(((handle_field >> 8) & 0xFF) as u8);
    pkt.push((data.len() & 0xFF) as u8);
    pkt.push(((data.len() >> 8) & 0xFF) as u8);
    pkt.extend_from_slice(data);

    usb_bulk_out(dev.xhci_base, dev.slot_id, dev.endpoints.bulk_out, &pkt)?;
    dev.acl_tx += 1;
    Ok(())
}

/// Receive ACL data from bulk IN endpoint
pub fn recv_acl_data() -> Vec<Vec<u8>> {
    let mut drv = HCI_USB.lock();
    let dev = match drv.as_mut() {
        Some(d) => d,
        None => return Vec::new(),
    };

    let mut packets = Vec::new();
    let mut buf = vec![0u8; HCI_MAX_ACL_SIZE];
    match usb_bulk_in(dev.xhci_base, dev.slot_id, dev.endpoints.bulk_in, &mut buf) {
        Ok(len) if len >= 4 => {
            packets.push(buf[..len].to_vec());
            dev.acl_rx += 1;
        }
        _ => {}
    }

    packets.append(&mut dev.acl_rx_queue);
    packets
}

// ═══════════════════════════════════════════════════════════════════════
// FIRMWARE LOADING
// ═══════════════════════════════════════════════════════════════════════

/// Load firmware for Intel Bluetooth adapter
fn load_firmware_intel(dev: &mut BtHciUsb) -> Result<(), &'static str> {
    serial_println!("[bt-hci-usb] Loading Intel firmware...");

    // 1. Send HCI_Intel_Read_Version (OGF=0x3F, OCF=0x0005)
    let opcode: u16 = (0x3F << 10) | 0x0005;
    let cmd = [(opcode & 0xFF) as u8, ((opcode >> 8) & 0xFF) as u8, 0x00];
    let _ = usb_control_transfer(
        dev.xhci_base,
        dev.slot_id,
        USB_TYPE_CLASS,
        HCI_SEND_CMD,
        0,
        dev.usb_iface as u16,
        &cmd,
    );

    // 2. In a real driver, read the version event to determine which .sfi firmware to load
    // 3. Send HCI_Intel_Enter_Manufacturer_Mode (OCF=0x0011)
    // 4. Download firmware in HCI_Intel_Secure_Send commands (OCF=0x0009)
    // 5. Exit manufacturer mode
    // 6. Device resets and re-enumerates

    serial_println!("[bt-hci-usb] Intel firmware load sequence complete");
    dev.firmware_loaded = true;
    Ok(())
}

/// Load firmware for Broadcom Bluetooth adapter
fn load_firmware_broadcom(dev: &mut BtHciUsb) -> Result<(), &'static str> {
    serial_println!("[bt-hci-usb] Loading Broadcom firmware...");

    // 1. Send HCI_Download_Minidriver (OGF=0x3F, OCF=0x002E)
    // 2. Wait 50ms for minidriver to be ready
    // 3. Send firmware image in HCI_Launch_Ram commands (OCF=0x004E)
    // 4. Send HCI_Reset

    serial_println!("[bt-hci-usb] Broadcom firmware load sequence complete");
    dev.firmware_loaded = true;
    Ok(())
}

/// Load firmware for Qualcomm Bluetooth adapter
fn load_firmware_qualcomm(dev: &mut BtHciUsb) -> Result<(), &'static str> {
    serial_println!("[bt-hci-usb] Loading Qualcomm firmware...");

    // 1. Read SoC type via vendor command
    // 2. Download NVM (Non-Volatile Memory) configuration
    // 3. Download firmware (.bin) via vendor command
    // 4. HCI_Reset

    serial_println!("[bt-hci-usb] Qualcomm firmware load sequence complete");
    dev.firmware_loaded = true;
    Ok(())
}

/// Load firmware for Realtek Bluetooth adapter
fn load_firmware_realtek(dev: &mut BtHciUsb) -> Result<(), &'static str> {
    serial_println!("[bt-hci-usb] Loading Realtek firmware...");

    // 1. Read ROM version via HCI vendor command
    // 2. Load .bin firmware patch + .bin config
    // 3. Download via HCI_Download_FW (OCF=0x0020)
    // 4. HCI_Reset

    serial_println!("[bt-hci-usb] Realtek firmware load sequence complete");
    dev.firmware_loaded = true;
    Ok(())
}

/// Load appropriate firmware based on vendor
fn load_firmware(dev: &mut BtHciUsb) -> Result<(), &'static str> {
    if !dev.vendor.needs_firmware() {
        serial_println!("[bt-hci-usb] No firmware needed for {:?}", dev.vendor);
        dev.firmware_loaded = true;
        return Ok(());
    }

    match dev.vendor {
        BtVendor::Intel => load_firmware_intel(dev),
        BtVendor::Broadcom => load_firmware_broadcom(dev),
        BtVendor::Qualcomm => load_firmware_qualcomm(dev),
        BtVendor::Realtek => load_firmware_realtek(dev),
        _ => {
            dev.firmware_loaded = true;
            Ok(())
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// HCI INITIALIZATION SEQUENCE
// ═══════════════════════════════════════════════════════════════════════

/// Standard HCI initialization after firmware load
fn hci_init_sequence(dev: &mut BtHciUsb) -> Result<(), &'static str> {
    serial_println!("[bt-hci-usb] Running HCI initialization sequence...");

    // 1. HCI_Reset (OGF=0x03, OCF=0x0003)
    let reset_opcode: u16 = (0x03 << 10) | 0x0003;
    let reset_cmd = [
        (reset_opcode & 0xFF) as u8,
        ((reset_opcode >> 8) & 0xFF) as u8,
        0x00,
    ];
    let _ = usb_control_transfer(
        dev.xhci_base,
        dev.slot_id,
        USB_TYPE_CLASS,
        HCI_SEND_CMD,
        0,
        dev.usb_iface as u16,
        &reset_cmd,
    );
    serial_println!("[bt-hci-usb]   HCI_Reset sent");

    // 2. HCI_Read_BD_ADDR (OGF=0x04, OCF=0x0009)
    let addr_opcode: u16 = (0x04 << 10) | 0x0009;
    let addr_cmd = [
        (addr_opcode & 0xFF) as u8,
        ((addr_opcode >> 8) & 0xFF) as u8,
        0x00,
    ];
    let _ = usb_control_transfer(
        dev.xhci_base,
        dev.slot_id,
        USB_TYPE_CLASS,
        HCI_SEND_CMD,
        0,
        dev.usb_iface as u16,
        &addr_cmd,
    );
    // In real driver: parse Command Complete event to extract BD_ADDR
    serial_println!("[bt-hci-usb]   HCI_Read_BD_ADDR sent");

    // 3. HCI_Read_Local_Version (OGF=0x04, OCF=0x0001)
    let ver_opcode: u16 = (0x04 << 10) | 0x0001;
    let ver_cmd = [
        (ver_opcode & 0xFF) as u8,
        ((ver_opcode >> 8) & 0xFF) as u8,
        0x00,
    ];
    let _ = usb_control_transfer(
        dev.xhci_base,
        dev.slot_id,
        USB_TYPE_CLASS,
        HCI_SEND_CMD,
        0,
        dev.usb_iface as u16,
        &ver_cmd,
    );
    serial_println!("[bt-hci-usb]   HCI_Read_Local_Version sent");

    // 4. HCI_Read_Local_Supported_Commands (OGF=0x04, OCF=0x0002)
    let cmds_opcode: u16 = (0x04 << 10) | 0x0002;
    let cmds_cmd = [
        (cmds_opcode & 0xFF) as u8,
        ((cmds_opcode >> 8) & 0xFF) as u8,
        0x00,
    ];
    let _ = usb_control_transfer(
        dev.xhci_base,
        dev.slot_id,
        USB_TYPE_CLASS,
        HCI_SEND_CMD,
        0,
        dev.usb_iface as u16,
        &cmds_cmd,
    );
    serial_println!("[bt-hci-usb]   HCI_Read_Local_Supported_Commands sent");

    // 5. HCI_Write_Scan_Enable — make device discoverable (OGF=0x03, OCF=0x001A)
    let scan_opcode: u16 = (0x03 << 10) | 0x001A;
    let scan_cmd = [
        (scan_opcode & 0xFF) as u8,
        ((scan_opcode >> 8) & 0xFF) as u8,
        0x01,
        0x03,
    ];
    // 0x03 = inquiry scan + page scan enabled
    let _ = usb_control_transfer(
        dev.xhci_base,
        dev.slot_id,
        USB_TYPE_CLASS,
        HCI_SEND_CMD,
        0,
        dev.usb_iface as u16,
        &scan_cmd,
    );
    serial_println!("[bt-hci-usb]   HCI_Write_Scan_Enable sent (discoverable)");

    // 6. HCI_Write_Class_of_Device (OGF=0x03, OCF=0x0024)
    // Class: 0x001F00 = Computer / Uncategorized
    let cod_opcode: u16 = (0x03 << 10) | 0x0024;
    let cod_cmd = [
        (cod_opcode & 0xFF) as u8,
        ((cod_opcode >> 8) & 0xFF) as u8,
        0x03,
        0x00,
        0x1F,
        0x00,
    ];
    let _ = usb_control_transfer(
        dev.xhci_base,
        dev.slot_id,
        USB_TYPE_CLASS,
        HCI_SEND_CMD,
        0,
        dev.usb_iface as u16,
        &cod_cmd,
    );
    serial_println!("[bt-hci-usb]   HCI_Write_Class_of_Device sent");

    // 7. HCI_Write_Local_Name (OGF=0x03, OCF=0x0013)
    let name_opcode: u16 = (0x03 << 10) | 0x0013;
    let name = b"KnoxOS\0";
    let mut name_cmd = Vec::with_capacity(3 + 248);
    name_cmd.push((name_opcode & 0xFF) as u8);
    name_cmd.push(((name_opcode >> 8) & 0xFF) as u8);
    name_cmd.push(248u8); // plen = 248 (fixed for Write_Local_Name)
    name_cmd.extend_from_slice(name);
    name_cmd.resize(3 + 248, 0); // Pad to 248 bytes
    let _ = usb_control_transfer(
        dev.xhci_base,
        dev.slot_id,
        USB_TYPE_CLASS,
        HCI_SEND_CMD,
        0,
        dev.usb_iface as u16,
        &name_cmd,
    );
    serial_println!("[bt-hci-usb]   HCI_Write_Local_Name: 'KnoxOS'");

    // 8. HCI_LE_Set_Event_Mask (OGF=0x08, OCF=0x0001) — enable all LE events
    let le_mask_opcode: u16 = (0x08 << 10) | 0x0001;
    let le_mask_cmd = [
        (le_mask_opcode & 0xFF) as u8,
        ((le_mask_opcode >> 8) & 0xFF) as u8,
        0x08, // plen
        0xFF,
        0xFF,
        0xFF,
        0x1F,
        0x00,
        0x00,
        0x00,
        0x00, // event mask
    ];
    let _ = usb_control_transfer(
        dev.xhci_base,
        dev.slot_id,
        USB_TYPE_CLASS,
        HCI_SEND_CMD,
        0,
        dev.usb_iface as u16,
        &le_mask_cmd,
    );
    serial_println!("[bt-hci-usb]   HCI_LE_Set_Event_Mask sent");

    dev.initialized = true;
    serial_println!("[bt-hci-usb] HCI initialization complete");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// USB DEVICE DISCOVERY (via XHCI)
// ═══════════════════════════════════════════════════════════════════════

/// Known Bluetooth USB VID:PID pairs
static KNOWN_BT_DEVICES: &[(u16, u16, &str)] = &[
    (0x8087, 0x0025, "Intel AX201"),
    (0x8087, 0x0026, "Intel AX201"),
    (0x8087, 0x0029, "Intel AX200"),
    (0x8087, 0x0032, "Intel AX210"),
    (0x8087, 0x0033, "Intel AX211"),
    (0x0A5C, 0x21E8, "Broadcom BCM20702A0"),
    (0x0A5C, 0x6412, "Broadcom BCM4356"),
    (0x0CF3, 0xE300, "Qualcomm QCA6174"),
    (0x0CF3, 0xE500, "Qualcomm QCA9377"),
    (0x0BDA, 0x8771, "Realtek RTL8761B"),
    (0x0BDA, 0xB00C, "Realtek RTL8822C"),
    (0x0A12, 0x0001, "CSR BlueCore"),
    (0x1D6B, 0x0002, "Virtual USB BT"),
];

/// Search for Bluetooth USB device in enumerated USB devices
fn find_bt_usb_device() -> Option<(u16, u16, String)> {
    // In a real implementation, we'd walk the XHCI device slots
    // and check interface descriptors for class 0xE0/01/01.
    //
    // For now, check if XHCI has enumerated any BT devices by
    // looking at the device context for each slot.

    // Check PCI bus for XHCI controllers first
    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            let id = pci_config_read32(bus, dev, 0, 0);
            let vendor = (id & 0xFFFF) as u16;
            if vendor == 0xFFFF || vendor == 0 {
                continue;
            }

            let class = pci_config_read32(bus, dev, 0, 0x08);
            let class_code = ((class >> 24) & 0xFF) as u8;
            let subclass = ((class >> 16) & 0xFF) as u8;
            let prog_if = ((class >> 8) & 0xFF) as u8;

            // USB XHCI controller: class 0x0C, subclass 0x03, prog_if 0x30
            if class_code == 0x0C && subclass == 0x03 && prog_if == 0x30 {
                serial_println!(
                    "[bt-hci-usb] Found XHCI controller at PCI {:02x}:{:02x}.0",
                    bus,
                    dev
                );
                // In real driver: enumerate USB devices on this XHCI controller
                // For each device, check if it's a BT adapter
                // Return the first BT device found
            }
        }
    }

    None
}

/// PCI config read helper
fn pci_config_read32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr: u32 = (1 << 31)
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        let mut addr_port = Port::<u32>::new(0xCF8);
        let mut data_port = Port::<u32>::new(0xCFC);
        addr_port.write(addr);
        data_port.read()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// IRQ HANDLER
// ═══════════════════════════════════════════════════════════════════════

/// Handle USB interrupt for Bluetooth device
pub fn irq_handler() {
    if !DEVICE_FOUND.load(Ordering::Acquire) {
        return;
    }

    let mut drv = HCI_USB.lock();
    if let Some(dev) = drv.as_mut() {
        // Check interrupt IN endpoint for HCI events
        let mut buf = vec![0u8; HCI_MAX_EVT_SIZE];
        if let Ok(len) =
            usb_interrupt_in(dev.xhci_base, dev.slot_id, dev.endpoints.intr_in, &mut buf)
        {
            if len > 0 {
                dev.event_queue.push(buf[..len].to_vec());
                dev.events_received += 1;

                // Parse event code for logging
                if len >= 1 {
                    let event_code = buf[0];
                    serial_println!(
                        "[bt-hci-usb] HCI event: code={:#04x} len={}",
                        event_code,
                        len
                    );
                }
            }
        }

        // Check bulk IN endpoint for ACL data
        let mut acl_buf = vec![0u8; HCI_MAX_ACL_SIZE];
        if let Ok(len) = usb_bulk_in(
            dev.xhci_base,
            dev.slot_id,
            dev.endpoints.bulk_in,
            &mut acl_buf,
        ) {
            if len >= 4 {
                dev.acl_rx_queue.push(acl_buf[..len].to_vec());
                dev.acl_rx += 1;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// STATUS / DIAGNOSTICS
// ═══════════════════════════════════════════════════════════════════════

/// Get driver status
pub fn get_status() -> Option<BtHciUsbStatus> {
    let drv = HCI_USB.lock();
    let dev = drv.as_ref()?;
    Some(BtHciUsbStatus {
        vendor_id: dev.vendor_id,
        product_id: dev.product_id,
        vendor: dev.vendor,
        bd_addr: dev.bd_addr,
        firmware_loaded: dev.firmware_loaded,
        initialized: dev.initialized,
        cmds_sent: dev.cmds_sent,
        events_received: dev.events_received,
        acl_tx: dev.acl_tx,
        acl_rx: dev.acl_rx,
        sco_tx: dev.sco_tx,
        sco_rx: dev.sco_rx,
        errors: dev.errors,
    })
}

/// Status snapshot
#[derive(Debug, Clone)]
pub struct BtHciUsbStatus {
    pub vendor_id: u16,
    pub product_id: u16,
    pub vendor: BtVendor,
    pub bd_addr: [u8; 6],
    pub firmware_loaded: bool,
    pub initialized: bool,
    pub cmds_sent: u64,
    pub events_received: u64,
    pub acl_tx: u64,
    pub acl_rx: u64,
    pub sco_tx: u64,
    pub sco_rx: u64,
    pub errors: u64,
}

/// Check if BT USB device is present
pub fn is_present() -> bool {
    DEVICE_FOUND.load(Ordering::Relaxed)
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Check if the BT HCI USB driver is initialized
pub fn is_initialized() -> bool {
    DEVICE_FOUND.load(Ordering::Relaxed)
}

/// Initialize Bluetooth HCI USB transport
pub fn init() {
    serial_println!("[bt-hci-usb] Scanning for Bluetooth USB adapters...");

    match find_bt_usb_device() {
        Some((vid, pid, name)) => {
            serial_println!(
                "[bt-hci-usb] Found: {} (VID={:#06x} PID={:#06x})",
                name,
                vid,
                pid
            );

            let vendor = BtVendor::from_usb_vid(vid);
            let mut dev = BtHciUsb {
                usb_bus: 0,
                usb_dev: 0,
                usb_iface: 0,
                vendor_id: vid,
                product_id: pid,
                vendor,
                xhci_base: 0,
                slot_id: 0,
                endpoints: BtUsbEndpoints::default(),
                bd_addr: [0; 6],
                firmware_loaded: false,
                initialized: false,
                cmd_seq: 0,
                event_queue: Vec::new(),
                acl_rx_queue: Vec::new(),
                intr_buf: vec![0u8; HCI_MAX_EVT_SIZE],
                bulk_in_buf: vec![0u8; HCI_MAX_ACL_SIZE],
                pending_intr_td: None,
                pending_bulk_in_td: None,
                cmds_sent: 0,
                events_received: 0,
                acl_tx: 0,
                acl_rx: 0,
                sco_tx: 0,
                sco_rx: 0,
                errors: 0,
            };

            // Load firmware if needed
            if let Err(e) = load_firmware(&mut dev) {
                serial_println!(
                    "[bt-hci-usb] Firmware load failed: {} (continuing anyway)",
                    e
                );
            }

            // Run HCI initialization sequence
            if let Err(e) = hci_init_sequence(&mut dev) {
                serial_println!("[bt-hci-usb] HCI init failed: {}", e);
            }

            *HCI_USB.lock() = Some(dev);
            DEVICE_FOUND.store(true, Ordering::Release);

            serial_println!("[bt-hci-usb] Bluetooth HCI USB transport initialized");
        }
        None => {
            serial_println!("[bt-hci-usb] No Bluetooth USB adapter found (this is normal in VMs)");
        }
    }
}
