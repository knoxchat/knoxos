//! Device slot enable, addressing, transfer submission, and enumeration.
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::serial_println;

use super::ports::enumerate_ports;
use super::types::{UsbConfigDescriptor, UsbDevice, UsbDeviceDescriptor, UsbRequestType, UsbSpeed};
use super::xhci::{
    CMD_RING_SIZE, EVENT_RING_SIZE, TRANSFER_RING_SIZE, TRB_SIZE, TransferRing, Trb, TrbType, XHCI,
    alloc_ring_buffer, dma_virt, xhci_write32,
};
use spin::Mutex;

/// Interrupt-IN URB waiting on a Transfer Event.
struct InterruptUrb {
    slot_id: u8,
    endpoint: u8,
    buf_phys: u64,
    len: u32,
    hid_id: u32,
}

static INTERRUPT_URBS: Mutex<Vec<InterruptUrb>> = Mutex::new(Vec::new());

/// True only when Enable Slot returned Command Completion cc=1 (not a timeout fallback).
static SLOT_ENABLE_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR got a Transfer Event (cc=1/13), not a timeout fallback.
static DESC_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SET_CONFIGURATION got a Transfer Event (cc=1/13), not a timeout fallback.
static CONFIG_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Address Device returned Command Completion cc=1 (not a timeout fallback).
static ADDRESS_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(Configuration) got a Transfer Event (cc=1/13).
static CONFIG_DESC_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when enumerate_device completed every step from a live event (no fallback).
static ENUM_LIVE: AtomicBool = AtomicBool::new(false);

pub const GATE_XHCI_SLOT_MARKER: &str = "GATE_USB xhci slot";
pub const GATE_XHCI_DESC_MARKER: &str = "GATE_USB xhci desc";
pub const GATE_XHCI_CONFIG_MARKER: &str = "GATE_USB xhci config";
pub const GATE_XHCI_ADDR_MARKER: &str = "GATE_USB xhci addr";
pub const GATE_XHCI_ENUM_MARKER: &str = "GATE_USB xhci enum";

/// TRB chain bit — required so Setup+Data+Status are one TD.
pub(super) const TRB_CH: u32 = 1 << 4;

// ═══════════════════════════════════════════════════════════════════════
// DEVICE ENUMERATION
// ═══════════════════════════════════════════════════════════════════════

/// Enqueue a TRB on the command ring and ring the doorbell
fn enqueue_command(trb: &Trb) -> bool {
    let mut xhci = XHCI.lock();
    if !xhci.initialized || xhci.cmd_ring_ptr == 0 {
        return false;
    }

    let idx = xhci.cmd_ring_enqueue;
    let cycle = xhci.cmd_ring_cycle;
    let ring_ptr = xhci.cmd_ring_ptr;
    let db_base = xhci.db_base;

    // Don't overwrite the Link TRB at the end
    if idx >= CMD_RING_SIZE - 1 {
        // Wrap: the Link TRB toggles the cycle, advance enqueue to 0
        xhci.cmd_ring_enqueue = 0;
        xhci.cmd_ring_cycle = !cycle;
        // The Link TRB's cycle bit was set at init; update it for new cycle
        // SAFETY: command ring is a DMA page we own; `ring_ptr` is physical.
        unsafe {
            let link_addr =
                (dma_virt(ring_ptr) + ((CMD_RING_SIZE - 1) * TRB_SIZE) as u64) as *mut Trb;
            let mut link_ctrl = (*link_addr).control & !(1u32); // clear cycle bit
            if !cycle {
                link_ctrl |= 1;
            } // set to new cycle
            (*link_addr).control = link_ctrl;
        }
        return enqueue_command(trb); // Retry with new position
    }

    // Write the TRB (write control word last with correct cycle bit)
    // SAFETY: slot is within the DMA command ring we allocated.
    unsafe {
        let trb_addr = (dma_virt(ring_ptr) + (idx * TRB_SIZE) as u64) as *mut Trb;
        (*trb_addr).param_lo = trb.param_lo;
        (*trb_addr).param_hi = trb.param_hi;
        (*trb_addr).status = trb.status;
        // Set cycle bit in control word
        let ctrl = if cycle {
            trb.control | 1
        } else {
            trb.control & !1
        };
        core::sync::atomic::fence(Ordering::Release);
        (*trb_addr).control = ctrl;
    }

    xhci.cmd_ring_enqueue = idx + 1;

    // Ring doorbell 0 (Host Controller Command) with target = 0
    // SAFETY: doorbell BAR is inside the mapped XHCI MMIO window.
    unsafe {
        xhci_write32(db_base, 0x00, 0);
    }

    true
}

/// Poll the event ring for a Command Completion or Transfer Event.
pub fn poll_transfer_event() -> bool {
    wait_command_completion().is_some()
}

/// Wait for a command completion event on the event ring
pub(super) fn wait_command_completion() -> Option<Trb> {
    let xhci = XHCI.lock();
    let event_ptr = xhci.event_ring_ptr;
    let mut dequeue = xhci.event_ring_dequeue;
    let expected_cycle = xhci.event_ring_cycle;
    let ir0_base = xhci.rt_base + 0x20;
    drop(xhci);

    // Poll the event ring for a CommandCompletion TRB
    for _ in 0..100000 {
        // SAFETY: event ring is a DMA page we own; `event_ptr` is physical.
        let trb = unsafe {
            let addr = (dma_virt(event_ptr) + (dequeue * TRB_SIZE) as u64) as *const Trb;
            core::ptr::read_volatile(addr)
        };

        let trb_cycle = trb.control & 1 != 0;
        if trb_cycle == expected_cycle {
            let trb_type = trb.trb_type();
            // Advance dequeue pointer
            dequeue += 1;
            let mut new_cycle = expected_cycle;
            if dequeue >= EVENT_RING_SIZE {
                dequeue = 0;
                new_cycle = !new_cycle;
            }
            // Update ERDP to acknowledge
            let erdp_val = event_ptr + (dequeue * TRB_SIZE) as u64;
            // SAFETY: ERDP is interrupter 0 in the mapped runtime block.
            unsafe {
                // Set EHB (Event Handler Busy) bit 3 to clear it
                xhci_write32(ir0_base, 0x18, ((erdp_val & 0xFFFFFFFF) as u32) | (1 << 3));
                xhci_write32(ir0_base, 0x1C, (erdp_val >> 32) as u32);
            }

            let mut xhci = XHCI.lock();
            xhci.event_ring_dequeue = dequeue;
            xhci.event_ring_cycle = new_cycle;

            if trb_type == TrbType::CommandCompletion as u8
                || trb_type == TrbType::TransferEvent as u8
            {
                return Some(trb);
            }
            // Port Status Change or other events — continue polling
            continue;
        }

        for _ in 0..100 {
            core::hint::spin_loop();
        }
    }

    None
}

/// Enable a device slot via Enable Slot command
pub fn enable_slot() -> Option<u8> {
    let mut trb = Trb::new();
    trb.set_type(TrbType::EnableSlot);

    if !enqueue_command(&trb) {
        serial_println!("[XHCI] Failed to enqueue Enable Slot command");
        return None;
    }

    // Wait for Command Completion event
    if let Some(event) = wait_command_completion() {
        let completion_code = event.completion_code();
        let slot_id = ((event.control >> 24) & 0xFF) as u8;

        if completion_code == 1 && slot_id != 0 {
            SLOT_ENABLE_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] Enable Slot succeeded: slot_id={}", slot_id);
            return Some(slot_id);
        } else {
            serial_println!(
                "[XHCI] Enable Slot failed: completion_code={} slot={}",
                completion_code,
                slot_id
            );
        }
    } else {
        serial_println!("[XHCI] Enable Slot: no completion event (timeout)");
    }

    None
}

/// Allocate a transfer ring for a given slot and endpoint
pub(super) fn allocate_transfer_ring(slot_id: u8, endpoint: u8) -> u64 {
    let ring_bytes = TRANSFER_RING_SIZE * TRB_SIZE;
    let (ring_ptr, ring_layout) = alloc_ring_buffer(ring_bytes);

    // Place Link TRB at the end
    // SAFETY: transfer ring is a DMA page we own; Link TRB param is physical.
    unsafe {
        let link_addr =
            (dma_virt(ring_ptr) + ((TRANSFER_RING_SIZE - 1) * TRB_SIZE) as u64) as *mut Trb;
        (*link_addr).param_lo = (ring_ptr & 0xFFFFFFFF) as u32;
        (*link_addr).param_hi = (ring_ptr >> 32) as u32;
        (*link_addr).status = 0;
        (*link_addr).control = ((TrbType::Link as u32) << 10) | (1 << 1) | 1; // TC + cycle
    }

    let mut xhci = XHCI.lock();
    let rings = xhci.transfer_rings.entry(slot_id).or_default();
    // Ensure we have enough entries (endpoint index)
    while rings.len() <= endpoint as usize {
        rings.push(TransferRing::new());
    }
    rings[endpoint as usize] = TransferRing {
        ptr: ring_ptr,
        layout: Some(ring_layout),
        enqueue: 0,
        cycle: true,
        size: TRANSFER_RING_SIZE,
    };

    serial_println!(
        "[XHCI] Transfer ring for slot {} ep {} at {:#x}",
        slot_id,
        endpoint,
        ring_ptr
    );

    ring_ptr
}

/// Address a device (SET_ADDRESS via Address Device command)
pub fn address_device(slot_id: u8, port: u8, speed: UsbSpeed) -> bool {
    serial_println!(
        "[XHCI] Addressing device: slot={} port={} speed={:?}",
        slot_id,
        port,
        speed
    );

    // Allocate Input Context (two pages: slot context + endpoint 0 context)
    let input_ctx_size = 4096;
    let (input_ctx_ptr, _input_layout) = alloc_ring_buffer(input_ctx_size);

    // Allocate Output Device Context
    let (output_ctx_ptr, _output_layout) = alloc_ring_buffer(4096);

    // Store output context pointer in DCBAA[slot_id]
    {
        let xhci = XHCI.lock();
        if xhci.dcbaa_ptr != 0 {
            // SAFETY: DCBAA is a DMA page we own; entries store physical context addrs.
            unsafe {
                let dcbaa_entry = (dma_virt(xhci.dcbaa_ptr) + (slot_id as u64) * 8) as *mut u64;
                *dcbaa_entry = output_ctx_ptr;
            }
        }
    }

    // Allocate Transfer Ring for endpoint 0 (Control)
    let ep0_ring_ptr = allocate_transfer_ring(slot_id, 1); // EP0 = DCI 1

    let ctx_size = {
        let xhci = XHCI.lock();
        xhci.context_size.max(32) as u64
    };

    // Fill Input Control Context: A0=1 (Slot), A1=1 (EP0).
    // ICC Add flags live at dword 1 whether CSZ is 32 or 64.
    // SAFETY: input context is a DMA page we own.
    unsafe {
        let icc = dma_virt(input_ctx_ptr) as *mut u32;
        // Drop context flags at offset 0
        *icc = 0;
        // Add context flags at offset 4: enable Slot (bit 0) and EP0 (bit 1)
        *icc.add(1) = 0x03;
    }

    // Slot Context follows the Input Control Context (32 or 64 bytes).
    let slot_ctx_offset = ctx_size;
    // SAFETY: slot context sits at a documented offset in the input context.
    unsafe {
        let slot_ctx = (dma_virt(input_ctx_ptr) + slot_ctx_offset) as *mut u32;
        // DWord 0: Route String=0, Speed, Context Entries=1
        let speed_val = match speed {
            UsbSpeed::Full => 1u32,
            UsbSpeed::Low => 2,
            UsbSpeed::High => 3,
            UsbSpeed::Super => 4,
            UsbSpeed::SuperPlus => 5,
        };
        *slot_ctx = (1 << 27) | (speed_val << 20); // Context Entries=1, Speed
        // DWord 1: Root Hub Port Number
        *slot_ctx.add(1) = (port as u32) << 16;
    }

    // EP0 Context is Device Context Index 1 (two context strides from ICC).
    let ep0_ctx_offset = ctx_size * 2;
    // SAFETY: EP0 context sits at a documented offset in the input context.
    unsafe {
        let ep0_ctx = (dma_virt(input_ctx_ptr) + ep0_ctx_offset) as *mut u32;
        // DWord 0: EP State=0
        *ep0_ctx = 0;
        // DWord 1: CErr=3, EP Type=4 (Control Bidirectional), Max Packet Size
        let max_packet = match speed {
            UsbSpeed::Low => 8u32,
            UsbSpeed::Full => 64,
            UsbSpeed::High => 64,
            UsbSpeed::Super | UsbSpeed::SuperPlus => 512,
        };
        *ep0_ctx.add(1) = (3 << 1) | (4 << 3) | (max_packet << 16);
        // DWord 2-3: TR Dequeue Pointer (with DCS=1)
        *ep0_ctx.add(2) = ((ep0_ring_ptr & 0xFFFFFFFF) as u32) | 1; // DCS=1
        *ep0_ctx.add(3) = (ep0_ring_ptr >> 32) as u32;
        // DWord 4: Average TRB Length = 8 (for control)
        *ep0_ctx.add(4) = 8;
    }

    // Build Address Device TRB
    let mut trb = Trb::new();
    trb.param_lo = (input_ctx_ptr & 0xFFFFFFFF) as u32;
    trb.param_hi = (input_ctx_ptr >> 32) as u32;
    trb.status = 0;
    trb.set_type(TrbType::AddressDevice);
    trb.control |= (slot_id as u32) << 24; // Slot ID in bits 31:24

    if !enqueue_command(&trb) {
        serial_println!("[XHCI] Failed to enqueue Address Device command");
        return false;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 {
            ADDRESS_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] Address Device succeeded for slot {}", slot_id);
            return true;
        }
        serial_println!("[XHCI] Address Device failed: cc={}", cc);
        return false;
    }

    serial_println!("[XHCI] Address Device: timeout");
    false
}

/// Enqueue TRBs on a transfer ring for a specific slot/endpoint and ring doorbell
pub(super) fn submit_transfer(slot_id: u8, endpoint: u8, trbs: &[Trb]) -> bool {
    let mut xhci = XHCI.lock();
    let db_base = xhci.db_base;

    let rings = match xhci.transfer_rings.get_mut(&slot_id) {
        Some(r) => r,
        None => return false,
    };
    if endpoint as usize >= rings.len() {
        return false;
    }
    let ring = &mut rings[endpoint as usize];
    if ring.ptr == 0 {
        return false;
    }

    for trb in trbs {
        let idx = ring.enqueue;
        if idx >= ring.size - 1 {
            // Wrap via Link TRB
            ring.enqueue = 0;
            ring.cycle = !ring.cycle;
            continue;
        }

        // SAFETY: transfer ring is a DMA page we own; `ring.ptr` is physical.
        unsafe {
            let trb_addr = (dma_virt(ring.ptr) + (idx * TRB_SIZE) as u64) as *mut Trb;
            (*trb_addr).param_lo = trb.param_lo;
            (*trb_addr).param_hi = trb.param_hi;
            (*trb_addr).status = trb.status;
            let ctrl = if ring.cycle {
                trb.control | 1
            } else {
                trb.control & !1
            };
            core::sync::atomic::fence(Ordering::Release);
            (*trb_addr).control = ctrl;
        }
        ring.enqueue = idx + 1;
    }

    // Ring doorbell: slot_id doorbell, target = endpoint DCI
    let doorbell_val = endpoint as u32;
    // SAFETY: doorbell BAR is inside the mapped XHCI MMIO window.
    unsafe {
        xhci_write32(db_base, (slot_id as u32) * 4, doorbell_val);
    }

    true
}

/// Submit an interrupt-IN URB (Normal TRB + IOC) on `endpoint` for `slot_id`.
///
/// `buf_phys` is the guest-physical address of a DMA buffer the host writes
/// the HID report into. Pass 0 to allocate a bounce page. Completion is a
/// Transfer Event on the event ring, drained by [`poll_events`].
pub fn submit_interrupt_urb(slot_id: u8, endpoint: u8, buf_phys: u64, len: u32) -> bool {
    submit_interrupt_urb_for_hid(slot_id, endpoint, buf_phys, len, 0)
}

/// Same as [`submit_interrupt_urb`], tagged with a HID device id so
/// [`poll_events`] can deliver the report through `usb_hid::process_report`.
pub fn submit_interrupt_urb_for_hid(
    slot_id: u8,
    endpoint: u8,
    buf_phys: u64,
    len: u32,
    hid_id: u32,
) -> bool {
    let buf_phys = if buf_phys == 0 {
        let Some(phys) = super::xhci::alloc_dma(4096) else {
            serial_println!("[XHCI] interrupt URB: no DMA page");
            return false;
        };
        phys
    } else {
        buf_phys
    };

    let mut trb = Trb::new();
    trb.param_lo = (buf_phys & 0xFFFF_FFFF) as u32;
    trb.param_hi = (buf_phys >> 32) as u32;
    trb.status = len; // TRB Transfer Length
    trb.set_type(TrbType::Normal);
    trb.control |= 1 << 5; // IOC
    trb.control |= 1 << 2; // ISP (Interrupt on Short Packet)

    if !submit_transfer(slot_id, endpoint, &[trb]) {
        serial_println!(
            "[XHCI] interrupt URB submit failed slot={} ep={}",
            slot_id,
            endpoint
        );
        return false;
    }

    INTERRUPT_URBS.lock().push(InterruptUrb {
        slot_id,
        endpoint,
        buf_phys,
        len,
        hid_id,
    });
    true
}

/// Non-blocking drain of the event ring. Transfer Events complete HID URBs
/// through [`crate::usb_hid::process_report`] and re-arm the interrupt-IN TRB.
pub fn poll_events() {
    let xhci = XHCI.lock();
    if !xhci.initialized || xhci.event_ring_ptr == 0 {
        return;
    }
    let event_ptr = xhci.event_ring_ptr;
    let mut dequeue = xhci.event_ring_dequeue;
    let mut expected_cycle = xhci.event_ring_cycle;
    let ir0_base = xhci.rt_base + 0x20;
    drop(xhci);

    loop {
        // SAFETY: event ring is a DMA page we own; `event_ptr` is physical.
        let trb = unsafe {
            let addr = (dma_virt(event_ptr) + (dequeue * TRB_SIZE) as u64) as *const Trb;
            core::ptr::read_volatile(addr)
        };
        let trb_cycle = trb.control & 1 != 0;
        if trb_cycle != expected_cycle {
            break;
        }
        dequeue += 1;
        if dequeue >= EVENT_RING_SIZE {
            dequeue = 0;
            expected_cycle = !expected_cycle;
        }
        let erdp_val = event_ptr + (dequeue * TRB_SIZE) as u64;
        // SAFETY: ERDP is interrupter 0 in the mapped runtime block.
        unsafe {
            xhci_write32(ir0_base, 0x18, ((erdp_val & 0xFFFFFFFF) as u32) | (1 << 3));
            xhci_write32(ir0_base, 0x1C, (erdp_val >> 32) as u32);
        }

        if trb.trb_type() == TrbType::TransferEvent as u8 {
            handle_transfer_event(&trb);
        }
    }

    let mut xhci = XHCI.lock();
    xhci.event_ring_dequeue = dequeue;
    xhci.event_ring_cycle = expected_cycle;
}

fn handle_transfer_event(trb: &Trb) {
    let slot_id = ((trb.control >> 24) & 0xFF) as u8;
    let endpoint = ((trb.control >> 16) & 0x1F) as u8;
    let cc = trb.completion_code();
    if cc != 1 && cc != 13 {
        return;
    }

    let mut urbs = INTERRUPT_URBS.lock();
    let Some(pos) = urbs
        .iter()
        .position(|u| u.slot_id == slot_id && u.endpoint == endpoint)
    else {
        return;
    };
    let urb = urbs.remove(pos);
    drop(urbs);

    let n = urb.len.min(64) as usize;
    let mut report = vec![0u8; n];
    // SAFETY: bounce page was allocated for this URB; the HC wrote the report.
    unsafe {
        core::ptr::copy_nonoverlapping(dma_virt(urb.buf_phys) as *const u8, report.as_mut_ptr(), n);
    }
    crate::usb_hid::deliver_hardware_report(urb.hid_id, urb.slot_id, &report);
    let _ = submit_interrupt_urb_for_hid(slot_id, endpoint, urb.buf_phys, urb.len, urb.hid_id);
}

/// Get device descriptor via control transfer on the XHCI transfer ring
pub fn get_device_descriptor(slot_id: u8) -> Option<UsbDeviceDescriptor> {
    serial_println!("[XHCI] GET_DESCRIPTOR(Device) for slot {}", slot_id);

    // Allocate a DMA buffer for the descriptor (18 bytes, page-aligned)
    let (data_buf, _data_layout) = alloc_ring_buffer(4096);

    // Build 3-TRB control transfer: Setup → Data → Status
    let mut setup = Trb::new();
    // Setup Stage TRB: bmRequestType=0x80, bRequest=6 (GET_DESCRIPTOR),
    //   wValue=0x0100 (Device desc), wIndex=0, wLength=18
    setup.param_lo = 0x80 | (6 << 8) | (0x0100 << 16); // bmReqType + bRequest + wValue(lo)
    setup.param_hi = 18 << 16; // wIndex=0, wLength=18
    setup.status = 8; // TRB Transfer Length = 8 (setup packet)
    setup.set_type(TrbType::SetupStage);
    setup.control |= 3 << 16; // TRT = 3 (IN Data Stage)
    setup.control |= 1 << 6; // IDT (Immediate Data)
    setup.control |= TRB_CH; // chain to Data Stage

    let mut data = Trb::new();
    data.param_lo = (data_buf & 0xFFFFFFFF) as u32;
    data.param_hi = (data_buf >> 32) as u32;
    data.status = 18; // Transfer Length = 18
    data.set_type(TrbType::DataStage);
    data.control |= 1 << 16; // DIR = 1 (IN)
    data.control |= 1 << 2; // ISP (Interrupt on Short Packet)
    data.control |= TRB_CH; // chain to Status Stage

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 5; // IOC (Interrupt On Completion)
    // DIR = 0 (OUT) for status stage when data was IN

    // Submit to EP 0 (DCI 1)
    if !submit_transfer(slot_id, 1, &[setup, data, status]) {
        serial_println!("[XHCI] Failed to submit GET_DESCRIPTOR transfer");
        return None;
    }

    // Wait for transfer completion
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 || cc == 13 {
            // Success or Short Packet
            // Read descriptor from DMA buffer
            // SAFETY: GET_DESCRIPTOR DMA page is owned by this transfer.
            let desc = unsafe { core::ptr::read(dma_virt(data_buf) as *const UsbDeviceDescriptor) };
            let blen = desc.b_length;
            let dtype = desc.b_descriptor_type;
            if blen == 18 && dtype == 1 {
                DESC_LIVE.store(true, Ordering::Relaxed);
            }
            let vid = desc.id_vendor;
            let pid = desc.id_product;
            let cls = desc.b_device_class;
            serial_println!(
                "[XHCI] Device descriptor: vendor={:#06x} product={:#06x} class={:#04x}",
                vid,
                pid,
                cls
            );
            return Some(desc);
        }
        serial_println!("[XHCI] GET_DESCRIPTOR completion code: {}", cc);
        return None;
    }

    serial_println!("[XHCI] GET_DESCRIPTOR timeout");
    None
}

/// SET_CONFIGURATION(1) — Setup + Status (no data stage). Requires a Transfer Event.
pub fn set_configuration(slot_id: u8, config: u8) -> bool {
    serial_println!("[XHCI] SET_CONFIGURATION({}) for slot {}", config, slot_id);

    let mut setup = Trb::new();
    // bmRequestType=0x00, bRequest=9 (SET_CONFIGURATION), wValue=config
    setup.param_lo = (UsbRequestType::SetConfiguration as u32) << 8 | (config as u32) << 16;
    setup.param_hi = 0; // wIndex=0, wLength=0
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    // TRT = 0 (No Data Stage)
    setup.control |= 1 << 6; // IDT (Immediate Data)
    setup.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 16; // DIR = 1 (IN) for no-data control
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, status]) {
        serial_println!("[XHCI] Failed to submit SET_CONFIGURATION transfer");
        return false;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 || cc == 13 {
            CONFIG_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] SET_CONFIGURATION({}) complete cc={}", config, cc);
            return true;
        }
        serial_println!("[XHCI] SET_CONFIGURATION completion code: {}", cc);
    } else {
        serial_println!("[XHCI] SET_CONFIGURATION timeout");
    }
    false
}

/// GET_DESCRIPTOR(Configuration) — Setup+Data+Status. Requires a Transfer Event.
pub fn get_config_descriptor(slot_id: u8) -> Option<UsbConfigDescriptor> {
    serial_println!("[XHCI] GET_DESCRIPTOR(Configuration) for slot {}", slot_id);

    let (data_buf, _data_layout) = alloc_ring_buffer(4096);
    const CFG_LEN: u32 = 32;

    let mut setup = Trb::new();
    // bmRequestType=0x80, bRequest=6, wValue=0x0200 (Configuration)
    setup.param_lo = 0x80 | (6 << 8) | (0x0200 << 16);
    setup.param_hi = CFG_LEN << 16; // wIndex=0, wLength=32
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 3 << 16; // TRT = IN Data Stage
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut data = Trb::new();
    data.param_lo = (data_buf & 0xFFFFFFFF) as u32;
    data.param_hi = (data_buf >> 32) as u32;
    data.status = CFG_LEN;
    data.set_type(TrbType::DataStage);
    data.control |= 1 << 16; // DIR = IN
    data.control |= 1 << 2; // ISP
    data.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, data, status]) {
        serial_println!("[XHCI] Failed to submit GET_DESCRIPTOR(Configuration)");
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 || cc == 13 {
            // SAFETY: GET_DESCRIPTOR DMA page is owned by this transfer.
            let desc = unsafe { core::ptr::read(dma_virt(data_buf) as *const UsbConfigDescriptor) };
            let blen = desc.b_length;
            let dtype = desc.b_descriptor_type;
            let total = desc.w_total_length;
            let niface = desc.b_num_interfaces;
            if blen >= 9 && dtype == 2 {
                CONFIG_DESC_LIVE.store(true, Ordering::Relaxed);
            }
            serial_println!(
                "[XHCI] Config descriptor: len={} type={} total={} interfaces={}",
                blen,
                dtype,
                total,
                niface
            );
            return Some(desc);
        }
        serial_println!(
            "[XHCI] GET_DESCRIPTOR(Configuration) completion code: {}",
            cc
        );
        return None;
    }

    serial_println!("[XHCI] GET_DESCRIPTOR(Configuration) timeout");
    None
}

/// Full device enumeration sequence. Every step requires a live completion.
pub fn enumerate_device(port: u8, speed: UsbSpeed) -> Option<UsbDevice> {
    serial_println!("[XHCI] Enumerating device on port {} ({:?})", port, speed);

    let slot_id = enable_slot()?;

    if !address_device(slot_id, port, speed) {
        return None;
    }

    let desc = get_device_descriptor(slot_id)?;
    if !DESC_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] Device descriptor was not a live Transfer Event");
        return None;
    }

    let _cfg = get_config_descriptor(slot_id)?;
    if !CONFIG_DESC_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] Config descriptor was not a live Transfer Event");
        return None;
    }

    if !set_configuration(slot_id, 1) {
        return None;
    }

    let device = UsbDevice {
        slot_id,
        port,
        speed,
        address: slot_id, // After SET_ADDRESS
        vendor_id: desc.id_vendor,
        product_id: desc.id_product,
        device_class: desc.b_device_class,
        device_subclass: desc.b_device_sub_class,
        device_protocol: desc.b_device_protocol,
        manufacturer: String::from("QEMU"),
        product: String::from("USB Device"),
        serial: String::new(),
        configured: true,
    };

    serial_println!(
        "[XHCI] Device enumerated: slot={} vendor={:#06x} product={:#06x} configured={}",
        slot_id,
        device.vendor_id,
        device.product_id,
        device.configured
    );

    if SLOT_ENABLE_LIVE.load(Ordering::Relaxed)
        && ADDRESS_LIVE.load(Ordering::Relaxed)
        && DESC_LIVE.load(Ordering::Relaxed)
        && CONFIG_DESC_LIVE.load(Ordering::Relaxed)
        && CONFIG_LIVE.load(Ordering::Relaxed)
    {
        ENUM_LIVE.store(true, Ordering::Relaxed);
    }

    XHCI.lock().devices.push(device.clone());

    Some(device)
}

/// Enumerate all connected devices
pub fn enumerate_all_devices() {
    let ports = enumerate_ports();
    for port in &ports {
        if port.connected {
            if let Some(dev) = enumerate_device(port.port_num, port.speed) {
                serial_println!(
                    "[XHCI] USB device ready: {}:{} ({:?})",
                    dev.vendor_id,
                    dev.product_id,
                    dev.speed
                );
            }
        }
    }
}

/// Prove Enable Slot got a Command Completion (cc=1), not a timeout fallback.
pub fn slot_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci slot skipped (no controller)");
        return true;
    }
    if !SLOT_ENABLE_LIVE.load(Ordering::Relaxed) {
        let _ = enable_slot();
    }
    if !SLOT_ENABLE_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Enable Slot had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_SLOT_MARKER);
    true
}

/// Prove GET_DESCRIPTOR got a Transfer Event, not a timeout fallback.
pub fn descriptor_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci desc skipped (no controller)");
        return true;
    }
    if !DESC_LIVE.load(Ordering::Relaxed) {
        let ports = enumerate_ports();
        let port = ports
            .iter()
            .find(|p| p.connected)
            .map(|p| (p.port_num, p.speed));
        if let Some(slot_id) = enable_slot() {
            if let Some((port_num, speed)) = port {
                let _ = address_device(slot_id, port_num, speed);
            } else {
                let _ = address_device(slot_id, 1, UsbSpeed::High);
            }
            let _ = get_device_descriptor(slot_id);
        }
    }
    if !DESC_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_DESCRIPTOR had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_DESC_MARKER);
    true
}

/// Prove SET_CONFIGURATION got a Transfer Event, not a timeout fallback.
pub fn config_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci config skipped (no controller)");
        return true;
    }
    if !CONFIG_LIVE.load(Ordering::Relaxed) {
        let ports = enumerate_ports();
        let port = ports
            .iter()
            .find(|p| p.connected)
            .map(|p| (p.port_num, p.speed));
        if let Some(slot_id) = enable_slot() {
            if let Some((port_num, speed)) = port {
                let _ = address_device(slot_id, port_num, speed);
            } else {
                let _ = address_device(slot_id, 1, UsbSpeed::High);
            }
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
        }
    }
    if !CONFIG_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_CONFIGURATION had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_CONFIG_MARKER);
    true
}

/// Prove Address Device got a Command Completion (cc=1), not a timeout fallback.
pub fn addr_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci addr skipped (no controller)");
        return true;
    }
    if !ADDRESS_LIVE.load(Ordering::Relaxed) {
        let ports = enumerate_ports();
        let port = ports
            .iter()
            .find(|p| p.connected)
            .map(|p| (p.port_num, p.speed));
        if let Some(slot_id) = enable_slot() {
            if let Some((port_num, speed)) = port {
                let _ = address_device(slot_id, port_num, speed);
            } else {
                let _ = address_device(slot_id, 1, UsbSpeed::High);
            }
        }
    }
    if !ADDRESS_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Address Device had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_ADDR_MARKER);
    true
}

/// Prove full enumeration used live completions for every step, including
/// GET_DESCRIPTOR(Configuration). No timeout fallback.
pub fn enum_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci enum skipped (no controller)");
        return true;
    }
    if !ENUM_LIVE.load(Ordering::Relaxed) {
        enumerate_all_devices();
    }
    if !ENUM_LIVE.load(Ordering::Relaxed) {
        let ports = enumerate_ports();
        if let Some(port) = ports.iter().find(|p| p.connected) {
            let _ = enumerate_device(port.port_num, port.speed);
        }
    }
    if !ENUM_LIVE.load(Ordering::Relaxed)
        || !CONFIG_DESC_LIVE.load(Ordering::Relaxed)
        || !SLOT_ENABLE_LIVE.load(Ordering::Relaxed)
        || !ADDRESS_LIVE.load(Ordering::Relaxed)
        || !DESC_LIVE.load(Ordering::Relaxed)
        || !CONFIG_LIVE.load(Ordering::Relaxed)
    {
        serial_println!("[XHCI] GATE_USB FAILED: enumeration used a timeout fallback");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_ENUM_MARKER);
    true
}
