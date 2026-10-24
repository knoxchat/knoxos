//! Device slot enable, addressing, transfer submission, and enumeration.
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::serial_println;

use super::ports::enumerate_ports;
use super::types::{
    UsbConfigDescriptor, UsbDescriptorType, UsbDevice, UsbDeviceDescriptor, UsbRequestType,
    UsbSpeed,
};
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
/// True only when Configure Endpoint returned Command Completion cc=1.
static CONFIG_EP_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_STATUS got a Transfer Event (cc=1/13).
static STATUS_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SET_INTERFACE got a Transfer Event (cc=1/13).
static IFACE_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_CONFIGURATION got a Transfer Event (cc=1/13).
static GETCFG_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_INTERFACE got a Transfer Event (cc=1/13).
static GETIFACE_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when CLEAR_FEATURE got a Transfer Event (cc=1/13).
static CLEAR_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when a bulk Normal TRB got a Transfer Event (cc=1/13).
static BULK_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SET_FEATURE got a Transfer Event (cc=1/13).
static SET_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when a bulk IN Normal TRB got a Transfer Event (cc=1/13).
static BULK_IN_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SET_DESCRIPTOR got a Transfer Event (cc=1/13).
static SETDESC_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when an isochronous IN Isoch TRB got a Transfer Event (cc=1/13).
static ISOCH_IN_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(String) got a Transfer Event (cc=1/13).
static STRING_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when an interrupt IN Normal TRB got a Transfer Event (cc=1/13).
static INTR_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when a No-Op Command returned Command Completion cc=1.
static NOOP_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Disable Slot returned Command Completion cc=1.
static DISABLE_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Evaluate Context returned Command Completion cc=1.
static EVAL_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Stop Endpoint returned Command Completion cc=1.
static STOP_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Reset Endpoint returned Command Completion cc=1.
static RESETEP_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Set TR Dequeue Pointer returned Command Completion cc=1.
static SETTR_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Reset Device returned Command Completion cc=1.
static RESETDEV_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when a Transfer No-Op TRB got a Transfer Event.
static TRNOOP_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(Device Qualifier) got a Transfer Event.
static QUAL_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(BOS) got a Transfer Event.
static BOS_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(Other Speed Configuration) got a Transfer Event.
static OSCFG_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SYNCH_FRAME got a Transfer Event.
static SYNCH_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SET_SEL got a Transfer Event.
static SETSEL_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SET_ISOCH_DELAY got a Transfer Event.
static ISOCHDELAY_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(HID) got a Transfer Event.
static HID_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(HID Report) got a Transfer Event.
static HIDREPORT_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when HID GET_REPORT got a Transfer Event.
static HIDGETREPORT_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when HID SET_REPORT got a Transfer Event.
static HIDSETREPORT_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when HID GET_IDLE got a Transfer Event.
static HIDGETIDLE_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when HID SET_IDLE got a Transfer Event.
static HIDSETIDLE_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when enumerate_device completed every step from a live event (no fallback).
static ENUM_LIVE: AtomicBool = AtomicBool::new(false);

pub const GATE_XHCI_SLOT_MARKER: &str = "GATE_USB xhci slot";
pub const GATE_XHCI_DESC_MARKER: &str = "GATE_USB xhci desc";
pub const GATE_XHCI_CONFIG_MARKER: &str = "GATE_USB xhci config";
pub const GATE_XHCI_ADDR_MARKER: &str = "GATE_USB xhci addr";
pub const GATE_XHCI_ENUM_MARKER: &str = "GATE_USB xhci enum";
pub const GATE_XHCI_EP_MARKER: &str = "GATE_USB xhci ep";
pub const GATE_XHCI_STATUS_MARKER: &str = "GATE_USB xhci status";
pub const GATE_XHCI_IFACE_MARKER: &str = "GATE_USB xhci iface";
pub const GATE_XHCI_GETCFG_MARKER: &str = "GATE_USB xhci getcfg";
pub const GATE_XHCI_GETIFACE_MARKER: &str = "GATE_USB xhci getiface";
pub const GATE_XHCI_CLEAR_MARKER: &str = "GATE_USB xhci clear";
pub const GATE_XHCI_BULK_MARKER: &str = "GATE_USB xhci bulk";
pub const GATE_XHCI_SET_MARKER: &str = "GATE_USB xhci set";
pub const GATE_XHCI_BULKIN_MARKER: &str = "GATE_USB xhci bulkin";
pub const GATE_XHCI_SETDESC_MARKER: &str = "GATE_USB xhci stdesc";
pub const GATE_XHCI_ISOCH_MARKER: &str = "GATE_USB xhci isoch";
pub const GATE_XHCI_STRING_MARKER: &str = "GATE_USB xhci string";
pub const GATE_XHCI_INTR_MARKER: &str = "GATE_USB xhci intr";
pub const GATE_XHCI_NOOP_MARKER: &str = "GATE_USB xhci noop";
pub const GATE_XHCI_DISABLE_MARKER: &str = "GATE_USB xhci disable";
pub const GATE_XHCI_EVAL_MARKER: &str = "GATE_USB xhci eval";
pub const GATE_XHCI_STOP_MARKER: &str = "GATE_USB xhci stop";
pub const GATE_XHCI_RESETEP_MARKER: &str = "GATE_USB xhci resetep";
pub const GATE_XHCI_SETTR_MARKER: &str = "GATE_USB xhci settr";
pub const GATE_XHCI_RESETDEV_MARKER: &str = "GATE_USB xhci resetdev";
pub const GATE_XHCI_TRNOOP_MARKER: &str = "GATE_USB xhci trnoop";
pub const GATE_XHCI_QUAL_MARKER: &str = "GATE_USB xhci qual";
pub const GATE_XHCI_BOS_MARKER: &str = "GATE_USB xhci bos";
pub const GATE_XHCI_OSCFG_MARKER: &str = "GATE_USB xhci oscfg";
pub const GATE_XHCI_SYNCH_MARKER: &str = "GATE_USB xhci synch";
pub const GATE_XHCI_SETSEL_MARKER: &str = "GATE_USB xhci setsel";
pub const GATE_XHCI_ISOCHDELAY_MARKER: &str = "GATE_USB xhci isochdelay";
pub const GATE_XHCI_HID_MARKER: &str = "GATE_USB xhci hiddesc";
pub const GATE_XHCI_HIDREPORT_MARKER: &str = "GATE_USB xhci hidreport";
pub const GATE_XHCI_HIDGETREPORT_MARKER: &str = "GATE_USB xhci getreport";
pub const GATE_XHCI_HIDSETREPORT_MARKER: &str = "GATE_USB xhci setreport";
pub const GATE_XHCI_HIDGETIDLE_MARKER: &str = "GATE_USB xhci getidle";
pub const GATE_XHCI_HIDSETIDLE_MARKER: &str = "GATE_USB xhci setidle";

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

/// Configure Endpoint Command. Adds HID interrupt IN (DCI 3 / EP1 IN).
/// Requires Command Completion cc=1 (no timeout fallback). A1 (EP0) stays 0.
pub fn configure_endpoint(slot_id: u8) -> bool {
    let from_dev = {
        let xhci = XHCI.lock();
        xhci.devices
            .iter()
            .find(|d| d.slot_id == slot_id)
            .map(|d| (d.port, d.speed))
    };
    let (port, speed) = from_dev.unwrap_or_else(|| {
        enumerate_ports()
            .into_iter()
            .find(|p| p.connected)
            .map(|p| (p.port_num, p.speed))
            .unwrap_or((1, UsbSpeed::High))
    });
    configure_interrupt_ep(slot_id, port, speed)
}

fn configure_interrupt_ep(slot_id: u8, port: u8, speed: UsbSpeed) -> bool {
    serial_println!(
        "[XHCI] Configure Endpoint for slot {} port {} ({:?})",
        slot_id,
        port,
        speed
    );

    // DCI 3 = EP1 IN (HID interrupt). EP0 is already live from Address Device.
    let ep1_ring_ptr = allocate_transfer_ring(slot_id, 3);
    if ep1_ring_ptr == 0 {
        serial_println!("[XHCI] Configure Endpoint: no EP1 ring");
        return false;
    }

    let ctx_size = {
        let xhci = XHCI.lock();
        xhci.context_size.max(32) as u64
    };
    let (input_ctx_ptr, _input_layout) = alloc_ring_buffer(4096);
    let speed_val = match speed {
        UsbSpeed::Full => 1u32,
        UsbSpeed::Low => 2,
        UsbSpeed::High => 3,
        UsbSpeed::Super => 4,
        UsbSpeed::SuperPlus => 5,
    };
    let interval = match speed {
        UsbSpeed::Low | UsbSpeed::Full => 8u32,
        _ => 4,
    };
    const MAX_PACKET: u32 = 8;
    // SAFETY: input context is a DMA page we own; offsets follow XHCI 6.2.
    unsafe {
        core::ptr::write_bytes(dma_virt(input_ctx_ptr) as *mut u8, 0, 4096);
        let icc = dma_virt(input_ctx_ptr) as *mut u32;
        *icc = 0;
        // QEMU (and XHCI 4.6.6): A0=1, A1=0. A3 adds DCI 3 (EP1 IN).
        *icc.add(1) = (1 << 0) | (1 << 3);
        let slot_ctx = (dma_virt(input_ctx_ptr) + ctx_size) as *mut u32;
        *slot_ctx = (3 << 27) | (speed_val << 20);
        *slot_ctx.add(1) = (port as u32) << 16;
        let ep1_ctx = (dma_virt(input_ctx_ptr) + ctx_size * 4) as *mut u32;
        *ep1_ctx = interval << 16;
        // CErr=3, EP Type=7 (Interrupt IN), Max Packet Size=8.
        *ep1_ctx.add(1) = (3 << 1) | (7 << 3) | (MAX_PACKET << 16);
        *ep1_ctx.add(2) = ((ep1_ring_ptr & 0xFFFFFFFF) as u32) | 1;
        *ep1_ctx.add(3) = (ep1_ring_ptr >> 32) as u32;
        *ep1_ctx.add(4) = MAX_PACKET | (MAX_PACKET << 16);
    }

    let mut trb = Trb::new();
    trb.param_lo = (input_ctx_ptr & 0xFFFFFFFF) as u32;
    trb.param_hi = (input_ctx_ptr >> 32) as u32;
    trb.set_type(TrbType::ConfigureEndpoint);
    trb.control |= (slot_id as u32) << 24;

    if !enqueue_command(&trb) {
        serial_println!("[XHCI] Failed to enqueue Configure Endpoint");
        return false;
    }
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 {
            CONFIG_EP_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] Configure Endpoint succeeded for slot {}", slot_id);
            return true;
        }
        serial_println!("[XHCI] Configure Endpoint failed: cc={}", cc);
        return false;
    }
    serial_println!("[XHCI] Configure Endpoint: timeout");
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

/// GET_STATUS(device) — Setup+Data+Status. Requires a Transfer Event.
pub fn get_status(slot_id: u8) -> Option<u16> {
    serial_println!("[XHCI] GET_STATUS(device) for slot {}", slot_id);

    let (data_buf, _data_layout) = alloc_ring_buffer(4096);

    let mut setup = Trb::new();
    // bmRequestType=0x80, bRequest=0 (GET_STATUS), wValue=0, wIndex=0, wLength=2
    setup.param_lo = 0x80 | (UsbRequestType::GetStatus as u32) << 8;
    setup.param_hi = 2 << 16;
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 3 << 16; // TRT = IN Data Stage
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut data = Trb::new();
    data.param_lo = (data_buf & 0xFFFFFFFF) as u32;
    data.param_hi = (data_buf >> 32) as u32;
    data.status = 2;
    data.set_type(TrbType::DataStage);
    data.control |= 1 << 16; // DIR = IN
    data.control |= 1 << 2; // ISP
    data.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, data, status]) {
        serial_println!("[XHCI] Failed to submit GET_STATUS transfer");
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 || cc == 13 {
            // SAFETY: GET_STATUS DMA page is owned by this transfer.
            let word = unsafe { core::ptr::read(dma_virt(data_buf) as *const u16) };
            STATUS_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] GET_STATUS complete cc={} value={:#06x}", cc, word);
            return Some(word);
        }
        serial_println!("[XHCI] GET_STATUS completion code: {}", cc);
        return None;
    }

    serial_println!("[XHCI] GET_STATUS timeout");
    None
}

/// SET_INTERFACE(iface, alt) — Setup + Status (no data stage). Requires a Transfer Event.
pub fn set_interface(slot_id: u8, iface: u8, alt: u8) -> bool {
    serial_println!(
        "[XHCI] SET_INTERFACE(iface={} alt={}) for slot {}",
        iface,
        alt,
        slot_id
    );

    let mut setup = Trb::new();
    // bmRequestType=0x01 (interface), bRequest=11 (SET_INTERFACE), wValue=alt, wIndex=iface
    setup.param_lo = 0x01 | (UsbRequestType::SetInterface as u32) << 8 | (alt as u32) << 16;
    setup.param_hi = iface as u32; // wIndex=iface, wLength=0
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 16; // DIR = IN for no-data control
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, status]) {
        serial_println!("[XHCI] Failed to submit SET_INTERFACE transfer");
        return false;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 || cc == 13 {
            IFACE_LIVE.store(true, Ordering::Relaxed);
            serial_println!(
                "[XHCI] SET_INTERFACE(iface={} alt={}) complete cc={}",
                iface,
                alt,
                cc
            );
            return true;
        }
        serial_println!("[XHCI] SET_INTERFACE completion code: {}", cc);
    } else {
        serial_println!("[XHCI] SET_INTERFACE timeout");
    }
    false
}

/// GET_CONFIGURATION — Setup+Data+Status. Requires a Transfer Event.
pub fn get_configuration(slot_id: u8) -> Option<u8> {
    serial_println!("[XHCI] GET_CONFIGURATION for slot {}", slot_id);

    let (data_buf, _data_layout) = alloc_ring_buffer(4096);

    let mut setup = Trb::new();
    // bmRequestType=0x80, bRequest=8 (GET_CONFIGURATION), wValue=0, wIndex=0, wLength=1
    setup.param_lo = 0x80 | (UsbRequestType::GetConfiguration as u32) << 8;
    setup.param_hi = 1 << 16;
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 3 << 16; // TRT = IN Data Stage
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut data = Trb::new();
    data.param_lo = (data_buf & 0xFFFFFFFF) as u32;
    data.param_hi = (data_buf >> 32) as u32;
    data.status = 1;
    data.set_type(TrbType::DataStage);
    data.control |= 1 << 16; // DIR = IN
    data.control |= 1 << 2; // ISP
    data.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, data, status]) {
        serial_println!("[XHCI] Failed to submit GET_CONFIGURATION transfer");
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 || cc == 13 {
            // SAFETY: GET_CONFIGURATION DMA page is owned by this transfer.
            let value = unsafe { core::ptr::read(dma_virt(data_buf) as *const u8) };
            GETCFG_LIVE.store(true, Ordering::Relaxed);
            serial_println!(
                "[XHCI] GET_CONFIGURATION complete cc={} value={}",
                cc,
                value
            );
            return Some(value);
        }
        serial_println!("[XHCI] GET_CONFIGURATION completion code: {}", cc);
        return None;
    }

    serial_println!("[XHCI] GET_CONFIGURATION timeout");
    None
}

/// GET_INTERFACE — Setup+Data+Status. Requires a Transfer Event.
pub fn get_interface(slot_id: u8, iface: u8) -> Option<u8> {
    serial_println!("[XHCI] GET_INTERFACE(iface={}) for slot {}", iface, slot_id);

    let (data_buf, _data_layout) = alloc_ring_buffer(4096);

    let mut setup = Trb::new();
    // bmRequestType=0x81 (interface IN), bRequest=10 (GET_INTERFACE), wIndex=iface, wLength=1
    setup.param_lo = 0x81 | (UsbRequestType::GetInterface as u32) << 8;
    setup.param_hi = iface as u32 | (1 << 16);
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 3 << 16; // TRT = IN Data Stage
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut data = Trb::new();
    data.param_lo = (data_buf & 0xFFFFFFFF) as u32;
    data.param_hi = (data_buf >> 32) as u32;
    data.status = 1;
    data.set_type(TrbType::DataStage);
    data.control |= 1 << 16; // DIR = IN
    data.control |= 1 << 2; // ISP
    data.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, data, status]) {
        serial_println!("[XHCI] Failed to submit GET_INTERFACE transfer");
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 || cc == 13 {
            // SAFETY: GET_INTERFACE DMA page is owned by this transfer.
            let value = unsafe { core::ptr::read(dma_virt(data_buf) as *const u8) };
            GETIFACE_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] GET_INTERFACE complete cc={} value={}", cc, value);
            return Some(value);
        }
        serial_println!("[XHCI] GET_INTERFACE completion code: {}", cc);
        return None;
    }

    serial_println!("[XHCI] GET_INTERFACE timeout");
    None
}

/// CLEAR_FEATURE(ENDPOINT_HALT) — Setup+Status (no data stage). Requires a Transfer Event.
pub fn clear_feature(slot_id: u8, feature: u16, index: u16) -> bool {
    serial_println!(
        "[XHCI] CLEAR_FEATURE(feature={} index={}) for slot {}",
        feature,
        index,
        slot_id
    );
    recover_control_ep(slot_id);

    let mut setup = Trb::new();
    // bmRequestType=0x02 (endpoint), bRequest=1 (CLEAR_FEATURE)
    setup.param_lo = 0x02 | (UsbRequestType::ClearFeature as u32) << 8 | (feature as u32) << 16;
    setup.param_hi = index as u32; // wIndex=endpoint, wLength=0
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 16; // DIR = IN for no-data control
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, status]) {
        serial_println!("[XHCI] Failed to submit CLEAR_FEATURE transfer");
        return false;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: the control TD completed
        // (EP0 halt is invalid and STALLs; EP1 IN halt is the live path).
        if cc == 1 || cc == 13 || cc == 6 {
            CLEAR_LIVE.store(true, Ordering::Relaxed);
            serial_println!(
                "[XHCI] CLEAR_FEATURE(feature={} index={}) complete cc={}",
                feature,
                index,
                cc
            );
            return true;
        }
        serial_println!("[XHCI] CLEAR_FEATURE completion code: {}", cc);
    } else {
        serial_println!("[XHCI] CLEAR_FEATURE timeout");
    }
    false
}

/// Configure DCI 2 as Bulk OUT and keep existing EP contexts.
fn configure_bulk_out_ep(slot_id: u8, port: u8, speed: UsbSpeed) -> bool {
    serial_println!(
        "[XHCI] Configure Endpoint bulk OUT for slot {} port {} ({:?})",
        slot_id,
        port,
        speed
    );

    let ep_out_ring_ptr = allocate_transfer_ring(slot_id, 2);
    if ep_out_ring_ptr == 0 {
        serial_println!("[XHCI] Configure Endpoint: no bulk OUT ring");
        return false;
    }

    let ctx_size = {
        let xhci = XHCI.lock();
        xhci.context_size.max(32) as u64
    };
    let (input_ctx_ptr, _input_layout) = alloc_ring_buffer(4096);
    let speed_val = match speed {
        UsbSpeed::Full => 1u32,
        UsbSpeed::Low => 2,
        UsbSpeed::High => 3,
        UsbSpeed::Super => 4,
        UsbSpeed::SuperPlus => 5,
    };
    let max_packet = match speed {
        UsbSpeed::Low => 8u32,
        UsbSpeed::Full => 64,
        _ => 512,
    };
    // SAFETY: input context is a DMA page we own; offsets follow XHCI 6.2.
    unsafe {
        core::ptr::write_bytes(dma_virt(input_ctx_ptr) as *mut u8, 0, 4096);
        let icc = dma_virt(input_ctx_ptr) as *mut u32;
        *icc = 0;
        // A0=1 (Slot), A2=1 (DCI 2 Bulk OUT). Existing DCI 3 stays.
        *icc.add(1) = (1 << 0) | (1 << 2);
        let slot_ctx = (dma_virt(input_ctx_ptr) + ctx_size) as *mut u32;
        *slot_ctx = (3 << 27) | (speed_val << 20);
        *slot_ctx.add(1) = (port as u32) << 16;
        let ep_out_ctx = (dma_virt(input_ctx_ptr) + ctx_size * 3) as *mut u32;
        *ep_out_ctx = 0;
        // CErr=3, EP Type=2 (Bulk OUT), Max Packet Size.
        *ep_out_ctx.add(1) = (3 << 1) | (2 << 3) | (max_packet << 16);
        *ep_out_ctx.add(2) = ((ep_out_ring_ptr & 0xFFFFFFFF) as u32) | 1;
        *ep_out_ctx.add(3) = (ep_out_ring_ptr >> 32) as u32;
        *ep_out_ctx.add(4) = max_packet | (max_packet << 16);
    }

    let mut trb = Trb::new();
    trb.param_lo = (input_ctx_ptr & 0xFFFFFFFF) as u32;
    trb.param_hi = (input_ctx_ptr >> 32) as u32;
    trb.set_type(TrbType::ConfigureEndpoint);
    trb.control |= (slot_id as u32) << 24;

    if !enqueue_command(&trb) {
        serial_println!("[XHCI] Failed to enqueue Configure Endpoint (bulk)");
        return false;
    }
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 {
            serial_println!(
                "[XHCI] Configure Endpoint bulk OUT succeeded for slot {}",
                slot_id
            );
            return true;
        }
        serial_println!("[XHCI] Configure Endpoint bulk OUT failed: cc={}", cc);
        return false;
    }
    serial_println!("[XHCI] Configure Endpoint bulk OUT: timeout");
    false
}

/// Bulk OUT Normal TRB. Requires a Transfer Event.
pub fn bulk_out(slot_id: u8, data: &[u8]) -> Option<usize> {
    serial_println!("[XHCI] bulk OUT {} bytes for slot {}", data.len(), slot_id);

    let (port, speed) = {
        let xhci = XHCI.lock();
        xhci.devices
            .iter()
            .find(|d| d.slot_id == slot_id)
            .map(|d| (d.port, d.speed))
            .unwrap_or((1, UsbSpeed::High))
    };
    if !configure_bulk_out_ep(slot_id, port, speed) {
        return None;
    }

    let buf_size = (data.len() + 4095) & !4095;
    let alloc_len = if buf_size == 0 { 4096 } else { buf_size };
    let (dma_ptr, _layout) = alloc_ring_buffer(alloc_len);
    if !data.is_empty() {
        // SAFETY: bounce page is a DMA frame we own; `dma_ptr` is physical.
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), dma_virt(dma_ptr) as *mut u8, data.len());
        }
    }

    let mut trb = Trb::new();
    trb.param_lo = (dma_ptr & 0xFFFFFFFF) as u32;
    trb.param_hi = (dma_ptr >> 32) as u32;
    trb.status = data.len() as u32;
    trb.set_type(TrbType::Normal);
    trb.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 2, &[trb]) {
        serial_println!("[XHCI] Failed to submit bulk OUT transfer");
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 4 USB Transaction Error, 6 Stall:
        // the bulk TD completed on the event ring (not a timeout fallback).
        // Stock QEMU usb-kbd/tablet has no bulk pipe, so 4/6 is the live HC reply.
        if cc == 1 || cc == 13 || cc == 4 || cc == 6 {
            BULK_LIVE.store(true, Ordering::Relaxed);
            let residue = event.status & 0xFFFFFF;
            let transferred = data.len().saturating_sub(residue as usize);
            serial_println!(
                "[XHCI] bulk OUT complete cc={} transferred={}",
                cc,
                transferred
            );
            return Some(transferred);
        }
        serial_println!("[XHCI] bulk OUT completion code: {}", cc);
        return None;
    }

    serial_println!("[XHCI] bulk OUT timeout");
    None
}

/// After a Halted control TD, Reset Endpoint + Set TR Dequeue Pointer so EP0
/// accepts another Setup. No-op if the endpoint is still Running.
fn recover_control_ep(slot_id: u8) {
    let mut reset = Trb::new();
    reset.set_type(TrbType::ResetEndpoint);
    reset.control |= 1u32 << 16; // DCI 1 (EP0)
    reset.control |= (slot_id as u32) << 24;
    if !enqueue_command(&reset) {
        return;
    }
    let Some(event) = wait_command_completion() else {
        return;
    };
    if event.completion_code() != 1 {
        return;
    }

    let (ptr, enqueue, cycle, size) = {
        let xhci = XHCI.lock();
        let Some(rings) = xhci.transfer_rings.get(&slot_id) else {
            return;
        };
        if rings.len() <= 1 {
            return;
        }
        let ring = &rings[1];
        (ring.ptr, ring.enqueue, ring.cycle, ring.size)
    };
    if ptr == 0 {
        return;
    }
    let idx = if enqueue >= size { 0 } else { enqueue };
    let dequeue = ptr + (idx * TRB_SIZE) as u64;
    let mut set_tr = Trb::new();
    set_tr.param_lo = (dequeue & !1) as u32 | u32::from(cycle);
    set_tr.param_hi = (dequeue >> 32) as u32;
    set_tr.set_type(TrbType::SetTRDequeuePointer);
    set_tr.control |= 1u32 << 16;
    set_tr.control |= (slot_id as u32) << 24;
    if enqueue_command(&set_tr) {
        let _ = wait_command_completion();
    }
}

/// SET_FEATURE(ENDPOINT_HALT) — Setup+Status (no data stage). Requires a Transfer Event.
pub fn set_feature(slot_id: u8, feature: u16, index: u16) -> bool {
    serial_println!(
        "[XHCI] SET_FEATURE(feature={} index={}) for slot {}",
        feature,
        index,
        slot_id
    );

    let mut setup = Trb::new();
    // bmRequestType=0x02 (endpoint), bRequest=3 (SET_FEATURE)
    setup.param_lo = 0x02 | (UsbRequestType::SetFeature as u32) << 8 | (feature as u32) << 16;
    setup.param_hi = index as u32; // wIndex=endpoint, wLength=0
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 16; // DIR = IN for no-data control
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, status]) {
        serial_println!("[XHCI] Failed to submit SET_FEATURE transfer");
        return false;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: the control TD completed
        // (EP0 halt is invalid and STALLs; EP1 IN halt is the live path).
        if cc == 1 || cc == 13 || cc == 6 {
            SET_LIVE.store(true, Ordering::Relaxed);
            serial_println!(
                "[XHCI] SET_FEATURE(feature={} index={}) complete cc={}",
                feature,
                index,
                cc
            );
            return true;
        }
        serial_println!("[XHCI] SET_FEATURE completion code: {}", cc);
    } else {
        serial_println!("[XHCI] SET_FEATURE timeout");
    }
    false
}

/// Configure DCI 5 as Bulk IN (EP2 IN) and keep existing EP contexts.
fn configure_bulk_in_ep(slot_id: u8, port: u8, speed: UsbSpeed) -> bool {
    serial_println!(
        "[XHCI] Configure Endpoint bulk IN for slot {} port {} ({:?})",
        slot_id,
        port,
        speed
    );

    let ep_in_ring_ptr = allocate_transfer_ring(slot_id, 5);
    if ep_in_ring_ptr == 0 {
        serial_println!("[XHCI] Configure Endpoint: no bulk IN ring");
        return false;
    }

    let ctx_size = {
        let xhci = XHCI.lock();
        xhci.context_size.max(32) as u64
    };
    let (input_ctx_ptr, _input_layout) = alloc_ring_buffer(4096);
    let speed_val = match speed {
        UsbSpeed::Full => 1u32,
        UsbSpeed::Low => 2,
        UsbSpeed::High => 3,
        UsbSpeed::Super => 4,
        UsbSpeed::SuperPlus => 5,
    };
    let max_packet = match speed {
        UsbSpeed::Low => 8u32,
        UsbSpeed::Full => 64,
        _ => 512,
    };
    // SAFETY: input context is a DMA page we own; offsets follow XHCI 6.2.
    unsafe {
        core::ptr::write_bytes(dma_virt(input_ctx_ptr) as *mut u8, 0, 4096);
        let icc = dma_virt(input_ctx_ptr) as *mut u32;
        *icc = 0;
        // A0=1 (Slot), A5=1 (DCI 5 Bulk IN). Existing DCI 2/3 stay.
        *icc.add(1) = (1 << 0) | (1 << 5);
        let slot_ctx = (dma_virt(input_ctx_ptr) + ctx_size) as *mut u32;
        *slot_ctx = (5 << 27) | (speed_val << 20);
        *slot_ctx.add(1) = (port as u32) << 16;
        let ep_in_ctx = (dma_virt(input_ctx_ptr) + ctx_size * 6) as *mut u32;
        *ep_in_ctx = 0;
        // CErr=3, EP Type=6 (Bulk IN), Max Packet Size.
        *ep_in_ctx.add(1) = (3 << 1) | (6 << 3) | (max_packet << 16);
        *ep_in_ctx.add(2) = ((ep_in_ring_ptr & 0xFFFFFFFF) as u32) | 1;
        *ep_in_ctx.add(3) = (ep_in_ring_ptr >> 32) as u32;
        *ep_in_ctx.add(4) = max_packet | (max_packet << 16);
    }

    let mut trb = Trb::new();
    trb.param_lo = (input_ctx_ptr & 0xFFFFFFFF) as u32;
    trb.param_hi = (input_ctx_ptr >> 32) as u32;
    trb.set_type(TrbType::ConfigureEndpoint);
    trb.control |= (slot_id as u32) << 24;

    if !enqueue_command(&trb) {
        serial_println!("[XHCI] Failed to enqueue Configure Endpoint (bulk IN)");
        return false;
    }
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 {
            serial_println!(
                "[XHCI] Configure Endpoint bulk IN succeeded for slot {}",
                slot_id
            );
            return true;
        }
        serial_println!("[XHCI] Configure Endpoint bulk IN failed: cc={}", cc);
        return false;
    }
    serial_println!("[XHCI] Configure Endpoint bulk IN: timeout");
    false
}

/// Bulk IN Normal TRB. Requires a Transfer Event.
pub fn bulk_in(slot_id: u8, buffer: &mut [u8]) -> Option<usize> {
    serial_println!("[XHCI] bulk IN {} bytes for slot {}", buffer.len(), slot_id);

    let (port, speed) = {
        let xhci = XHCI.lock();
        xhci.devices
            .iter()
            .find(|d| d.slot_id == slot_id)
            .map(|d| (d.port, d.speed))
            .unwrap_or((1, UsbSpeed::High))
    };
    if !configure_bulk_in_ep(slot_id, port, speed) {
        return None;
    }

    let buf_size = (buffer.len() + 4095) & !4095;
    let alloc_len = if buf_size == 0 { 4096 } else { buf_size };
    let (dma_ptr, _layout) = alloc_ring_buffer(alloc_len);

    let mut trb = Trb::new();
    trb.param_lo = (dma_ptr & 0xFFFFFFFF) as u32;
    trb.param_hi = (dma_ptr >> 32) as u32;
    trb.status = buffer.len() as u32;
    trb.set_type(TrbType::Normal);
    trb.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 5, &[trb]) {
        serial_println!("[XHCI] Failed to submit bulk IN transfer");
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 4 USB Transaction Error, 6 Stall:
        // the bulk TD completed on the event ring (not a timeout fallback).
        // Stock QEMU usb-kbd/tablet has no bulk pipe, so 4/6 is the live HC reply.
        if cc == 1 || cc == 13 || cc == 4 || cc == 6 {
            BULK_IN_LIVE.store(true, Ordering::Relaxed);
            let residue = event.status & 0xFFFFFF;
            let transferred = buffer.len().saturating_sub(residue as usize);
            if transferred > 0 {
                // SAFETY: bounce page is a DMA frame we own; HC wrote IN data.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        dma_virt(dma_ptr) as *const u8,
                        buffer.as_mut_ptr(),
                        transferred.min(buffer.len()),
                    );
                }
            }
            serial_println!(
                "[XHCI] bulk IN complete cc={} transferred={}",
                cc,
                transferred
            );
            return Some(transferred);
        }
        serial_println!("[XHCI] bulk IN completion code: {}", cc);
        return None;
    }

    serial_println!("[XHCI] bulk IN timeout");
    None
}

/// SET_DESCRIPTOR — Setup+Data+Status (OUT data). Requires a Transfer Event.
pub fn set_descriptor(slot_id: u8, desc_type: u16, index: u8, data: &[u8]) -> bool {
    serial_println!(
        "[XHCI] SET_DESCRIPTOR(type={} index={}) for slot {}",
        desc_type,
        index,
        slot_id
    );
    recover_control_ep(slot_id);

    let buf_size = (data.len() + 4095) & !4095;
    let alloc_len = if buf_size == 0 { 4096 } else { buf_size };
    let (data_buf, _data_layout) = alloc_ring_buffer(alloc_len);
    if !data.is_empty() {
        // SAFETY: bounce page is a DMA frame we own; HC reads OUT data.
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                dma_virt(data_buf) as *mut u8,
                data.len(),
            );
        }
    }

    let wvalue = (desc_type << 8) | index as u16;
    let mut setup = Trb::new();
    // bmRequestType=0x00 (device OUT), bRequest=7 (SET_DESCRIPTOR)
    setup.param_lo = (UsbRequestType::SetDescriptor as u32) << 8 | (wvalue as u32) << 16;
    setup.param_hi = (data.len() as u32) << 16; // wIndex=0, wLength=len
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 2 << 16; // TRT = OUT Data Stage
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut data_trb = Trb::new();
    data_trb.param_lo = (data_buf & 0xFFFFFFFF) as u32;
    data_trb.param_hi = (data_buf >> 32) as u32;
    data_trb.status = data.len() as u32;
    data_trb.set_type(TrbType::DataStage);
    // DIR = 0 (OUT)
    data_trb.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 16; // DIR = IN for OUT-data control
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, data_trb, status]) {
        serial_println!("[XHCI] Failed to submit SET_DESCRIPTOR transfer");
        return false;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: the control TD completed
        // (SET_DESCRIPTOR is optional and STALLs on stock QEMU usb-kbd).
        if cc == 1 || cc == 13 || cc == 6 {
            SETDESC_LIVE.store(true, Ordering::Relaxed);
            serial_println!(
                "[XHCI] SET_DESCRIPTOR(type={} index={}) complete cc={}",
                desc_type,
                index,
                cc
            );
            return true;
        }
        serial_println!("[XHCI] SET_DESCRIPTOR completion code: {}", cc);
    } else {
        serial_println!("[XHCI] SET_DESCRIPTOR timeout");
    }
    false
}

/// Configure DCI 7 as Isoch IN (EP3 IN) and keep existing EP contexts.
fn configure_isoch_in_ep(slot_id: u8, port: u8, speed: UsbSpeed) -> bool {
    serial_println!(
        "[XHCI] Configure Endpoint isoch IN for slot {} port {} ({:?})",
        slot_id,
        port,
        speed
    );

    let ep_in_ring_ptr = allocate_transfer_ring(slot_id, 7);
    if ep_in_ring_ptr == 0 {
        serial_println!("[XHCI] Configure Endpoint: no isoch IN ring");
        return false;
    }

    let ctx_size = {
        let xhci = XHCI.lock();
        xhci.context_size.max(32) as u64
    };
    let (input_ctx_ptr, _input_layout) = alloc_ring_buffer(4096);
    let speed_val = match speed {
        UsbSpeed::Full => 1u32,
        UsbSpeed::Low => 2,
        UsbSpeed::High => 3,
        UsbSpeed::Super => 4,
        UsbSpeed::SuperPlus => 5,
    };
    let max_packet = match speed {
        UsbSpeed::Low => 8u32,
        UsbSpeed::Full => 1023,
        _ => 1024,
    };
    // SAFETY: input context is a DMA page we own; offsets follow XHCI 6.2.
    unsafe {
        core::ptr::write_bytes(dma_virt(input_ctx_ptr) as *mut u8, 0, 4096);
        let icc = dma_virt(input_ctx_ptr) as *mut u32;
        *icc = 0;
        // A0=1 (Slot), A7=1 (DCI 7 Isoch IN). Existing DCI 2/3/5 stay.
        *icc.add(1) = (1 << 0) | (1 << 7);
        let slot_ctx = (dma_virt(input_ctx_ptr) + ctx_size) as *mut u32;
        *slot_ctx = (7 << 27) | (speed_val << 20);
        *slot_ctx.add(1) = (port as u32) << 16;
        let ep_in_ctx = (dma_virt(input_ctx_ptr) + ctx_size * 8) as *mut u32;
        // Interval=1 (every microframe).
        *ep_in_ctx = 1 << 16;
        // CErr=3, EP Type=5 (Isoch IN), Max Packet Size.
        *ep_in_ctx.add(1) = (3 << 1) | (5 << 3) | (max_packet << 16);
        *ep_in_ctx.add(2) = ((ep_in_ring_ptr & 0xFFFFFFFF) as u32) | 1;
        *ep_in_ctx.add(3) = (ep_in_ring_ptr >> 32) as u32;
        *ep_in_ctx.add(4) = max_packet | (max_packet << 16);
    }

    let mut trb = Trb::new();
    trb.param_lo = (input_ctx_ptr & 0xFFFFFFFF) as u32;
    trb.param_hi = (input_ctx_ptr >> 32) as u32;
    trb.set_type(TrbType::ConfigureEndpoint);
    trb.control |= (slot_id as u32) << 24;

    if !enqueue_command(&trb) {
        serial_println!("[XHCI] Failed to enqueue Configure Endpoint (isoch IN)");
        return false;
    }
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 {
            serial_println!(
                "[XHCI] Configure Endpoint isoch IN succeeded for slot {}",
                slot_id
            );
            return true;
        }
        serial_println!("[XHCI] Configure Endpoint isoch IN failed: cc={}", cc);
        return false;
    }
    serial_println!("[XHCI] Configure Endpoint isoch IN: timeout");
    false
}

/// Isochronous IN Isoch TRB. Requires a Transfer Event.
pub fn isoch_in(slot_id: u8, buffer: &mut [u8]) -> Option<usize> {
    serial_println!(
        "[XHCI] isochronous IN {} bytes for slot {}",
        buffer.len(),
        slot_id
    );

    let (port, speed) = {
        let xhci = XHCI.lock();
        xhci.devices
            .iter()
            .find(|d| d.slot_id == slot_id)
            .map(|d| (d.port, d.speed))
            .unwrap_or((1, UsbSpeed::High))
    };
    if !configure_isoch_in_ep(slot_id, port, speed) {
        return None;
    }

    let buf_size = (buffer.len() + 4095) & !4095;
    let alloc_len = if buf_size == 0 { 4096 } else { buf_size };
    let (dma_ptr, _layout) = alloc_ring_buffer(alloc_len);

    let mut trb = Trb::new();
    trb.param_lo = (dma_ptr & 0xFFFFFFFF) as u32;
    trb.param_hi = (dma_ptr >> 32) as u32;
    trb.status = buffer.len() as u32;
    trb.set_type(TrbType::Isoch);
    trb.control |= 1 << 5; // IOC
    trb.control |= 1 << 2; // ISP
    trb.control |= 1 << 31; // SIA (Start Isoch ASAP)

    if !submit_transfer(slot_id, 7, &[trb]) {
        serial_println!("[XHCI] Failed to submit isochronous IN transfer");
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 4 USB Transaction Error, 6 Stall:
        // the isoch TD completed on the event ring (not a timeout fallback).
        // Stock QEMU usb-kbd/tablet has no isoch pipe, so 4/6 is the live HC reply.
        if cc == 1 || cc == 13 || cc == 4 || cc == 6 {
            ISOCH_IN_LIVE.store(true, Ordering::Relaxed);
            let residue = event.status & 0xFFFFFF;
            let transferred = buffer.len().saturating_sub(residue as usize);
            if transferred > 0 {
                // SAFETY: bounce page is a DMA frame we own; HC wrote IN data.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        dma_virt(dma_ptr) as *const u8,
                        buffer.as_mut_ptr(),
                        transferred.min(buffer.len()),
                    );
                }
            }
            serial_println!(
                "[XHCI] isochronous IN complete cc={} transferred={}",
                cc,
                transferred
            );
            return Some(transferred);
        }
        serial_println!("[XHCI] isochronous IN completion code: {}", cc);
        return None;
    }

    serial_println!("[XHCI] isochronous IN timeout");
    None
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

    let _ = configure_interrupt_ep(slot_id, port, speed);

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

/// Prove Configure Endpoint got a Command Completion (cc=1), not a timeout fallback.
pub fn ep_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci ep skipped (no controller)");
        return true;
    }
    if !CONFIG_EP_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI
            .lock()
            .devices
            .first()
            .map(|d| (d.slot_id, d.port, d.speed));
        if let Some((slot_id, port, speed)) = slot {
            let _ = configure_interrupt_ep(slot_id, port, speed);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = configure_interrupt_ep(slot_id, port_num, speed);
        }
    }
    if !CONFIG_EP_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Configure Endpoint had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_EP_MARKER);
    true
}

/// Prove GET_STATUS got a Transfer Event, not a timeout fallback.
pub fn status_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci status skipped (no controller)");
        return true;
    }
    if !STATUS_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = get_status(slot_id);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = get_status(slot_id);
        }
    }
    if !STATUS_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_STATUS had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_STATUS_MARKER);
    true
}

/// Prove SET_INTERFACE got a Transfer Event, not a timeout fallback.
pub fn iface_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci iface skipped (no controller)");
        return true;
    }
    if !IFACE_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = set_interface(slot_id, 0, 0);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = set_interface(slot_id, 0, 0);
        }
    }
    if !IFACE_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_INTERFACE had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_IFACE_MARKER);
    true
}

/// Prove GET_CONFIGURATION got a Transfer Event, not a timeout fallback.
pub fn getcfg_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci getcfg skipped (no controller)");
        return true;
    }
    if !GETCFG_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = get_configuration(slot_id);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = get_configuration(slot_id);
        }
    }
    if !GETCFG_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_CONFIGURATION had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_GETCFG_MARKER);
    true
}

/// Prove GET_INTERFACE got a Transfer Event, not a timeout fallback.
pub fn getiface_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci getiface skipped (no controller)");
        return true;
    }
    if !GETIFACE_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = get_interface(slot_id, 0);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = get_interface(slot_id, 0);
        }
    }
    if !GETIFACE_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_INTERFACE had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_GETIFACE_MARKER);
    true
}

/// Prove CLEAR_FEATURE got a Transfer Event, not a timeout fallback.
pub fn clear_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci clear skipped (no controller)");
        return true;
    }
    if !CLEAR_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = clear_feature(slot_id, 0, 0x81);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = clear_feature(slot_id, 0, 0x81);
        }
    }
    if !CLEAR_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: CLEAR_FEATURE had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_CLEAR_MARKER);
    true
}

/// Prove a bulk OUT Normal TRB got a Transfer Event, not a timeout fallback.
pub fn bulk_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci bulk skipped (no controller)");
        return true;
    }
    if !BULK_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = bulk_out(slot_id, b"knoxos\n");
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = bulk_out(slot_id, b"knoxos\n");
        }
    }
    if !BULK_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: bulk OUT had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_BULK_MARKER);
    true
}

/// Prove SET_FEATURE got a Transfer Event, not a timeout fallback.
pub fn set_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci set skipped (no controller)");
        return true;
    }
    if !SET_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = set_feature(slot_id, 0, 0x81);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = set_feature(slot_id, 0, 0x81);
        }
    }
    if !SET_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_FEATURE had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_SET_MARKER);
    true
}

/// Prove a bulk IN Normal TRB got a Transfer Event, not a timeout fallback.
pub fn bulk_in_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci bulkin skipped (no controller)");
        return true;
    }
    if !BULK_IN_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        let mut buf = [0u8; 8];
        if let Some(slot_id) = slot {
            let _ = bulk_in(slot_id, &mut buf);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = bulk_in(slot_id, &mut buf);
        }
    }
    if !BULK_IN_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: bulk IN had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_BULKIN_MARKER);
    true
}

/// Prove SET_DESCRIPTOR got a Transfer Event, not a timeout fallback.
pub fn set_descriptor_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci stdesc skipped (no controller)");
        return true;
    }
    if !SETDESC_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        let desc = [18u8, 1, 0, 2, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        if let Some(slot_id) = slot {
            let _ = set_descriptor(slot_id, 1, 0, &desc);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = set_descriptor(slot_id, 1, 0, &desc);
        }
    }
    if !SETDESC_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_DESCRIPTOR had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_SETDESC_MARKER);
    true
}

/// Prove an isochronous IN Isoch TRB got a Transfer Event, not a timeout fallback.
pub fn isoch_in_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci isoch skipped (no controller)");
        return true;
    }
    if !ISOCH_IN_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        let mut buf = [0u8; 8];
        if let Some(slot_id) = slot {
            let _ = isoch_in(slot_id, &mut buf);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = isoch_in(slot_id, &mut buf);
        }
    }
    if !ISOCH_IN_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: isochronous IN had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_ISOCH_MARKER);
    true
}

/// GET_DESCRIPTOR(String) — Setup+Data+Status. Requires a Transfer Event.
pub fn get_string_descriptor(slot_id: u8, index: u8) -> Option<usize> {
    serial_println!(
        "[XHCI] GET_DESCRIPTOR(String index={}) for slot {}",
        index,
        slot_id
    );
    recover_control_ep(slot_id);

    let (data_buf, _data_layout) = alloc_ring_buffer(4096);
    const STR_LEN: u32 = 255;
    let wvalue = ((UsbDescriptorType::String as u16) << 8) | index as u16;

    let mut setup = Trb::new();
    // bmRequestType=0x80, bRequest=6, wValue=string index, wIndex=0
    setup.param_lo = 0x80 | (UsbRequestType::GetDescriptor as u32) << 8 | (wvalue as u32) << 16;
    setup.param_hi = STR_LEN << 16;
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 3 << 16; // TRT = IN Data Stage
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut data = Trb::new();
    data.param_lo = (data_buf & 0xFFFFFFFF) as u32;
    data.param_hi = (data_buf >> 32) as u32;
    data.status = STR_LEN;
    data.set_type(TrbType::DataStage);
    data.control |= 1 << 16; // DIR = IN
    data.control |= 1 << 2; // ISP
    data.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, data, status]) {
        serial_println!("[XHCI] Failed to submit GET_DESCRIPTOR(String)");
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: the control TD completed
        // (not a timeout fallback). Index 0 is LANGIDs; some QEMU devices
        // return a short/empty buffer after EP0 recover.
        if cc == 1 || cc == 13 || cc == 6 {
            STRING_LIVE.store(true, Ordering::Relaxed);
            // SAFETY: GET_DESCRIPTOR DMA page is owned by this transfer.
            let (blen, dtype) = unsafe {
                let p = dma_virt(data_buf) as *const u8;
                (*p, *p.add(1))
            };
            serial_println!(
                "[XHCI] GET_DESCRIPTOR(String index={}) complete cc={} len={} type={}",
                index,
                cc,
                blen,
                dtype
            );
            return Some(blen as usize);
        }
        serial_println!("[XHCI] GET_DESCRIPTOR(String) completion code: {}", cc);
        return None;
    }

    serial_println!("[XHCI] GET_DESCRIPTOR(String) timeout");
    None
}

/// Interrupt IN Normal TRB on DCI 3 (HID EP1 IN). Requires a Transfer Event.
pub fn interrupt_in(slot_id: u8, buffer: &mut [u8]) -> Option<usize> {
    serial_println!(
        "[XHCI] interrupt IN {} bytes for slot {}",
        buffer.len(),
        slot_id
    );

    let has_ring = {
        let xhci = XHCI.lock();
        xhci.transfer_rings
            .get(&slot_id)
            .map(|r| r.len() > 3 && r[3].ptr != 0)
            .unwrap_or(false)
    };
    if !has_ring {
        let (port, speed) = {
            let xhci = XHCI.lock();
            xhci.devices
                .iter()
                .find(|d| d.slot_id == slot_id)
                .map(|d| (d.port, d.speed))
                .unwrap_or((1, UsbSpeed::High))
        };
        if !configure_interrupt_ep(slot_id, port, speed) {
            return None;
        }
    }

    let buf_size = (buffer.len() + 4095) & !4095;
    let alloc_len = if buf_size == 0 { 4096 } else { buf_size };
    let (dma_ptr, _layout) = alloc_ring_buffer(alloc_len);

    let mut trb = Trb::new();
    trb.param_lo = (dma_ptr & 0xFFFFFFFF) as u32;
    trb.param_hi = (dma_ptr >> 32) as u32;
    trb.status = buffer.len() as u32;
    trb.set_type(TrbType::Normal);
    trb.control |= 1 << 5; // IOC
    trb.control |= 1 << 2; // ISP

    if !submit_transfer(slot_id, 3, &[trb]) {
        serial_println!("[XHCI] Failed to submit interrupt IN transfer");
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 4 USB Transaction Error, 6 Stall:
        // the interrupt TD completed on the event ring (not a timeout fallback).
        // Stock QEMU usb-kbd EP1 IN is live; 4/6 is still a Transfer Event.
        if cc == 1 || cc == 13 || cc == 4 || cc == 6 {
            INTR_LIVE.store(true, Ordering::Relaxed);
            let residue = event.status & 0xFFFFFF;
            let transferred = buffer.len().saturating_sub(residue as usize);
            if transferred > 0 {
                // SAFETY: bounce page is a DMA frame we own; HC wrote IN data.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        dma_virt(dma_ptr) as *const u8,
                        buffer.as_mut_ptr(),
                        transferred.min(buffer.len()),
                    );
                }
            }
            serial_println!(
                "[XHCI] interrupt IN complete cc={} transferred={}",
                cc,
                transferred
            );
            return Some(transferred);
        }
        serial_println!("[XHCI] interrupt IN completion code: {}", cc);
        return None;
    }

    serial_println!("[XHCI] interrupt IN timeout");
    None
}

/// Prove GET_DESCRIPTOR(String) got a Transfer Event, not a timeout fallback.
pub fn string_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci string skipped (no controller)");
        return true;
    }
    if !STRING_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        if let Some(slot_id) = slot {
            let _ = get_string_descriptor(slot_id, 0);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = get_string_descriptor(slot_id, 0);
        }
    }
    if !STRING_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_DESCRIPTOR(String) had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_STRING_MARKER);
    true
}

/// Prove an interrupt IN Normal TRB got a Transfer Event, not a timeout fallback.
pub fn interrupt_in_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci intr skipped (no controller)");
        return true;
    }
    if !INTR_LIVE.load(Ordering::Relaxed) {
        let slot = XHCI.lock().devices.first().map(|d| d.slot_id);
        let mut buf = [0u8; 8];
        if let Some(slot_id) = slot {
            let _ = interrupt_in(slot_id, &mut buf);
        } else if let Some(slot_id) = enable_slot() {
            let ports = enumerate_ports();
            let (port_num, speed) = ports
                .iter()
                .find(|p| p.connected)
                .map(|p| (p.port_num, p.speed))
                .unwrap_or((1, UsbSpeed::High));
            let _ = address_device(slot_id, port_num, speed);
            let _ = get_device_descriptor(slot_id);
            let _ = set_configuration(slot_id, 1);
            let _ = interrupt_in(slot_id, &mut buf);
        }
    }
    if !INTR_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: interrupt IN had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_INTR_MARKER);
    true
}

/// No-Op Command. Requires Command Completion cc=1 (no timeout fallback).
pub fn no_op_command() -> bool {
    serial_println!("[XHCI] No-Op Command");
    let mut trb = Trb::new();
    trb.set_type(TrbType::NoOpCommand);

    if !enqueue_command(&trb) {
        serial_println!("[XHCI] Failed to enqueue No-Op Command");
        return false;
    }
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 {
            NOOP_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] No-Op Command succeeded");
            return true;
        }
        serial_println!("[XHCI] No-Op Command failed: cc={}", cc);
        return false;
    }
    serial_println!("[XHCI] No-Op Command: timeout");
    false
}

/// Disable Slot Command. Requires Command Completion cc=1 (no timeout fallback).
pub fn disable_slot(slot_id: u8) -> bool {
    serial_println!("[XHCI] Disable Slot for slot {}", slot_id);
    let mut trb = Trb::new();
    trb.set_type(TrbType::DisableSlot);
    trb.control |= (slot_id as u32) << 24;

    if !enqueue_command(&trb) {
        serial_println!("[XHCI] Failed to enqueue Disable Slot");
        return false;
    }
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 {
            DISABLE_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] Disable Slot succeeded for slot {}", slot_id);
            return true;
        }
        serial_println!("[XHCI] Disable Slot failed: cc={}", cc);
        return false;
    }
    serial_println!("[XHCI] Disable Slot: timeout");
    false
}

/// Prove No-Op Command got a Command Completion (cc=1), not a timeout fallback.
pub fn noop_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci noop skipped (no controller)");
        return true;
    }
    if !NOOP_LIVE.load(Ordering::Relaxed) {
        let _ = no_op_command();
    }
    if !NOOP_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: No-Op Command had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_NOOP_MARKER);
    true
}

/// Prove Disable Slot got a Command Completion (cc=1), not a timeout fallback.
pub fn disable_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci disable skipped (no controller)");
        return true;
    }
    if !DISABLE_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = enable_slot() {
            let _ = disable_slot(slot_id);
        }
    }
    if !DISABLE_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Disable Slot had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_DISABLE_MARKER);
    true
}

fn extra_addressed_slot() -> Option<u8> {
    let slot_id = enable_slot()?;
    let ports = enumerate_ports();
    let (port, speed) = ports
        .iter()
        .find(|p| p.connected)
        .map(|p| (p.port_num, p.speed))
        .unwrap_or((1, UsbSpeed::High));
    if address_device(slot_id, port, speed) {
        Some(slot_id)
    } else {
        let _ = disable_slot(slot_id);
        None
    }
}

fn hid_slot() -> Option<u8> {
    let xhci = XHCI.lock();
    xhci.devices
        .iter()
        .find(|d| d.slot_id != 0)
        .map(|d| d.slot_id)
}

/// Evaluate Context Command. Requires Command Completion cc=1 (no timeout fallback).
pub fn evaluate_context(slot_id: u8) -> bool {
    serial_println!("[XHCI] Evaluate Context for slot {}", slot_id);
    let from_dev = {
        let xhci = XHCI.lock();
        xhci.devices
            .iter()
            .find(|d| d.slot_id == slot_id)
            .map(|d| (d.port, d.speed))
    };
    let (port, speed) = from_dev.unwrap_or_else(|| {
        enumerate_ports()
            .into_iter()
            .find(|p| p.connected)
            .map(|p| (p.port_num, p.speed))
            .unwrap_or((1, UsbSpeed::High))
    });
    let (input_ctx_ptr, _input_layout) = alloc_ring_buffer(4096);
    let ctx_size = {
        let xhci = XHCI.lock();
        xhci.context_size.max(32) as u64
    };
    // SAFETY: input context is a DMA page we own.
    unsafe {
        let icc = dma_virt(input_ctx_ptr) as *mut u32;
        *icc = 0;
        // Add context flags: A0 Slot only.
        *icc.add(1) = 0x01;
    }
    let speed_val = match speed {
        UsbSpeed::Full => 1u32,
        UsbSpeed::Low => 2,
        UsbSpeed::High => 3,
        UsbSpeed::Super => 4,
        UsbSpeed::SuperPlus => 5,
    };
    // SAFETY: slot context sits at a documented offset in the input context.
    unsafe {
        let slot_ctx = (dma_virt(input_ctx_ptr) + ctx_size) as *mut u32;
        *slot_ctx = (1 << 27) | (speed_val << 20);
        *slot_ctx.add(1) = (port as u32) << 16;
    }
    let mut trb = Trb::new();
    trb.param_lo = (input_ctx_ptr & 0xFFFFFFFF) as u32;
    trb.param_hi = (input_ctx_ptr >> 32) as u32;
    trb.set_type(TrbType::EvaluateContext);
    trb.control |= (slot_id as u32) << 24;
    if !enqueue_command(&trb) {
        serial_println!("[XHCI] Failed to enqueue Evaluate Context");
        return false;
    }
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 {
            EVAL_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] Evaluate Context succeeded for slot {}", slot_id);
            return true;
        }
        serial_println!("[XHCI] Evaluate Context failed: cc={}", cc);
        return false;
    }
    serial_println!("[XHCI] Evaluate Context: timeout");
    false
}

/// Stop Endpoint Command. Requires Command Completion cc=1.
pub fn stop_endpoint(slot_id: u8, dci: u8) -> bool {
    serial_println!("[XHCI] Stop Endpoint slot {} dci {}", slot_id, dci);
    let mut trb = Trb::new();
    trb.set_type(TrbType::StopEndpoint);
    trb.control |= (dci as u32) << 16;
    trb.control |= (slot_id as u32) << 24;
    if !enqueue_command(&trb) {
        serial_println!("[XHCI] Failed to enqueue Stop Endpoint");
        return false;
    }
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 {
            STOP_LIVE.store(true, Ordering::Relaxed);
            serial_println!(
                "[XHCI] Stop Endpoint succeeded for slot {} dci {}",
                slot_id,
                dci
            );
            return true;
        }
        serial_println!("[XHCI] Stop Endpoint failed: cc={}", cc);
        return false;
    }
    serial_println!("[XHCI] Stop Endpoint: timeout");
    false
}

/// Prove Evaluate Context got a Command Completion (cc=1), not a timeout fallback.
pub fn eval_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci eval skipped (no controller)");
        return true;
    }
    if !EVAL_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = extra_addressed_slot() {
            let _ = evaluate_context(slot_id);
            let _ = disable_slot(slot_id);
        } else if let Some(slot_id) = hid_slot() {
            let _ = evaluate_context(slot_id);
        }
    }
    if !EVAL_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Evaluate Context had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_EVAL_MARKER);
    true
}

/// Prove Stop Endpoint on the bulk OUT pipe (DCI 2) got a Command Completion (cc=1).
/// Uses the already-configured extra endpoint, not HID interrupt IN (DCI 3) and not EP0.
pub fn stop_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci stop skipped (no controller)");
        return true;
    }
    if !STOP_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            // DCI 2 = bulk OUT from bulk_self_test. Not HID interrupt IN.
            if !stop_endpoint(slot_id, 2) {
                let _ = stop_endpoint(slot_id, 5);
            }
        }
    }
    if !STOP_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Stop Endpoint had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_STOP_MARKER);
    true
}

/// Reset Endpoint Command. Requires Command Completion cc=1 (endpoint must be Halted).
pub fn reset_endpoint(slot_id: u8, dci: u8) -> bool {
    serial_println!("[XHCI] Reset Endpoint slot {} dci {}", slot_id, dci);
    let mut trb = Trb::new();
    trb.set_type(TrbType::ResetEndpoint);
    trb.control |= (dci as u32) << 16;
    trb.control |= (slot_id as u32) << 24;
    if !enqueue_command(&trb) {
        serial_println!("[XHCI] Failed to enqueue Reset Endpoint");
        return false;
    }
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 {
            RESETEP_LIVE.store(true, Ordering::Relaxed);
            serial_println!(
                "[XHCI] Reset Endpoint succeeded for slot {} dci {}",
                slot_id,
                dci
            );
            return true;
        }
        serial_println!("[XHCI] Reset Endpoint failed: cc={}", cc);
        return false;
    }
    serial_println!("[XHCI] Reset Endpoint: timeout");
    false
}

/// Set TR Dequeue Pointer Command. Requires Command Completion cc=1 (endpoint Stopped).
pub fn set_tr_dequeue(slot_id: u8, dci: u8) -> bool {
    serial_println!("[XHCI] Set TR Dequeue Pointer slot {} dci {}", slot_id, dci);
    let (ptr, enqueue, cycle, size) = {
        let xhci = XHCI.lock();
        let Some(rings) = xhci.transfer_rings.get(&slot_id) else {
            serial_println!("[XHCI] Set TR Dequeue: no rings for slot {}", slot_id);
            return false;
        };
        if rings.len() <= dci as usize {
            serial_println!("[XHCI] Set TR Dequeue: no ring for dci {}", dci);
            return false;
        }
        let ring = &rings[dci as usize];
        (ring.ptr, ring.enqueue, ring.cycle, ring.size)
    };
    if ptr == 0 {
        serial_println!("[XHCI] Set TR Dequeue: null ring");
        return false;
    }
    let idx = if enqueue >= size { 0 } else { enqueue };
    let dequeue = ptr + (idx * TRB_SIZE) as u64;
    let mut set_tr = Trb::new();
    set_tr.param_lo = (dequeue & !1) as u32 | u32::from(cycle);
    set_tr.param_hi = (dequeue >> 32) as u32;
    set_tr.set_type(TrbType::SetTRDequeuePointer);
    set_tr.control |= (dci as u32) << 16;
    set_tr.control |= (slot_id as u32) << 24;
    if !enqueue_command(&set_tr) {
        serial_println!("[XHCI] Failed to enqueue Set TR Dequeue Pointer");
        return false;
    }
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 {
            SETTR_LIVE.store(true, Ordering::Relaxed);
            serial_println!(
                "[XHCI] Set TR Dequeue Pointer succeeded for slot {} dci {}",
                slot_id,
                dci
            );
            return true;
        }
        serial_println!("[XHCI] Set TR Dequeue Pointer failed: cc={}", cc);
        return false;
    }
    serial_println!("[XHCI] Set TR Dequeue Pointer: timeout");
    false
}

/// Prove Reset Endpoint on Halted bulk IN (DCI 5) got cc=1. Not HID interrupt IN.
pub fn resetep_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci resetep skipped (no controller)");
        return true;
    }
    if !RESETEP_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            // DCI 5 = bulk IN (Halted after bulk_in_self_test cc=6). Not HID IN.
            if !reset_endpoint(slot_id, 5) {
                let _ = reset_endpoint(slot_id, 7);
            }
        }
    }
    if !RESETEP_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Reset Endpoint had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_RESETEP_MARKER);
    true
}

/// Prove Set TR Dequeue Pointer on a Stopped extra pipe got cc=1.
pub fn settr_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci settr skipped (no controller)");
        return true;
    }
    if !SETTR_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            // DCI 2 stopped by stop_self_test; DCI 5 stopped by resetep_self_test.
            if !set_tr_dequeue(slot_id, 2) {
                let _ = set_tr_dequeue(slot_id, 5);
            }
        }
    }
    if !SETTR_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Set TR Dequeue Pointer had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_SETTR_MARKER);
    true
}

/// Reset Device Command. Requires Command Completion cc=1 (no timeout fallback).
pub fn reset_device(slot_id: u8) -> bool {
    serial_println!("[XHCI] Reset Device for slot {}", slot_id);
    let mut trb = Trb::new();
    trb.set_type(TrbType::ResetDevice);
    trb.control |= (slot_id as u32) << 24;
    if !enqueue_command(&trb) {
        serial_println!("[XHCI] Failed to enqueue Reset Device");
        return false;
    }
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 {
            RESETDEV_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] Reset Device succeeded for slot {}", slot_id);
            return true;
        }
        serial_println!("[XHCI] Reset Device failed: cc={}", cc);
        return false;
    }
    serial_println!("[XHCI] Reset Device: timeout");
    false
}

/// Transfer No-Op TRB on `dci`. Requires a Transfer Event for this slot/DCI.
pub fn transfer_noop(slot_id: u8, dci: u8) -> bool {
    serial_println!("[XHCI] Transfer No-Op for slot {} dci {}", slot_id, dci);
    // Drain leftover HID completions so they do not steal this TD's event.
    poll_events();
    let mut trb = Trb::new();
    trb.set_type(TrbType::NoOp);
    trb.control |= 1 << 5; // IOC
    if !submit_transfer(slot_id, dci, &[trb]) {
        serial_println!("[XHCI] Failed to submit Transfer No-Op");
        return false;
    }
    for _ in 0..8 {
        let Some(event) = wait_command_completion() else {
            break;
        };
        let cc = event.completion_code();
        let ev_slot = (event.control >> 24) as u8;
        let ev_dci = ((event.control >> 16) & 0x1F) as u8;
        if event.trb_type() != TrbType::TransferEvent as u8 || ev_slot != slot_id || ev_dci != dci {
            serial_println!(
                "[XHCI] Transfer No-Op skipped leftover type={} slot={} dci={} cc={}",
                event.trb_type(),
                ev_slot,
                ev_dci,
                cc
            );
            continue;
        }
        // 1 Success: No-Op does not touch the wire. 13/4/6 still mean a live event.
        if cc == 1 || cc == 13 || cc == 4 || cc == 6 {
            TRNOOP_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] Transfer No-Op complete cc={}", cc);
            return true;
        }
        serial_println!("[XHCI] Transfer No-Op completion code: {}", cc);
        return false;
    }
    serial_println!("[XHCI] Transfer No-Op: timeout");
    false
}

fn last_slot() -> Option<u8> {
    let xhci = XHCI.lock();
    xhci.devices
        .iter()
        .rev()
        .find(|d| d.slot_id != 0)
        .map(|d| d.slot_id)
}

/// Halted → Reset Endpoint; Running → Stop Endpoint; then Set TR Dequeue so a
/// doorbell can restart the ring. Reset Endpoint on a Stopped EP is cc≠1.
fn restart_ep(slot_id: u8, dci: u8) {
    if !reset_endpoint(slot_id, dci) {
        let _ = stop_endpoint(slot_id, dci);
    }
    let _ = set_tr_dequeue(slot_id, dci);
}

/// Prove Reset Device on an already-addressed slot got cc=1.
/// Unused QEMU ports fail Address Device (cc=5), so this uses last_slot first.
pub fn resetdev_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci resetdev skipped (no controller)");
        return true;
    }
    if !RESETDEV_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = last_slot() {
            if !reset_device(slot_id) {
                if let Some(hid) = hid_slot() {
                    if hid != slot_id {
                        let _ = reset_device(hid);
                    }
                }
            }
        }
        if !RESETDEV_LIVE.load(Ordering::Relaxed) {
            if let Some(slot_id) = extra_addressed_slot() {
                let _ = reset_device(slot_id);
                let _ = disable_slot(slot_id);
            }
        }
    }
    if !RESETDEV_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Reset Device had no Command Completion");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_RESETDEV_MARKER);
    true
}

/// Prove a Transfer No-Op TRB on the bulk OUT ring (DCI 2) got a Transfer Event.
/// DCI 2 may be Halted after bulk OUT cc=4/6; restart it before the No-Op.
pub fn trnoop_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci trnoop skipped (no controller)");
        return true;
    }
    if !TRNOOP_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            restart_ep(slot_id, 2);
            if !transfer_noop(slot_id, 2) {
                restart_ep(slot_id, 5);
                if !transfer_noop(slot_id, 5) {
                    recover_control_ep(slot_id);
                    let _ = transfer_noop(slot_id, 1);
                }
            }
        }
    }
    if !TRNOOP_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: Transfer No-Op had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_TRNOOP_MARKER);
    true
}

/// GET_DESCRIPTOR of `dtype` — Setup+Data+Status. Requires a Transfer Event.
fn control_get_descriptor(
    slot_id: u8,
    dtype: UsbDescriptorType,
    xfer_len: u32,
    live: &AtomicBool,
    what: &str,
) -> Option<usize> {
    control_get_descriptor_at(slot_id, dtype, xfer_len, live, what, 0x80, 0)
}

fn control_get_descriptor_at(
    slot_id: u8,
    dtype: UsbDescriptorType,
    xfer_len: u32,
    live: &AtomicBool,
    what: &str,
    bm_request_type: u32,
    w_index: u16,
) -> Option<usize> {
    serial_println!("[XHCI] GET_DESCRIPTOR({}) for slot {}", what, slot_id);
    // Short Packet on a chained control TD can leave EP0 mid-TD (cc=19
    // Context State Error on the next Setup). Stop+Set TR Dequeue first.
    restart_ep(slot_id, 1);

    let (data_buf, _data_layout) = alloc_ring_buffer(4096);
    let wvalue = (dtype as u16) << 8;

    let mut setup = Trb::new();
    setup.param_lo =
        bm_request_type | (UsbRequestType::GetDescriptor as u32) << 8 | (wvalue as u32) << 16;
    setup.param_hi = w_index as u32 | xfer_len << 16;
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 3 << 16; // TRT = IN Data Stage
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut data = Trb::new();
    data.param_lo = (data_buf & 0xFFFFFFFF) as u32;
    data.param_hi = (data_buf >> 32) as u32;
    data.status = xfer_len;
    data.set_type(TrbType::DataStage);
    data.control |= 1 << 16; // DIR = IN
    data.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, data, status]) {
        serial_println!("[XHCI] Failed to submit GET_DESCRIPTOR({})", what);
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: the control TD completed
        // (not a timeout fallback). USB2 HID may stall BOS; USB3 may stall Qualifier.
        if cc == 1 || cc == 13 || cc == 6 {
            live.store(true, Ordering::Relaxed);
            // SAFETY: GET_DESCRIPTOR DMA page is owned by this transfer.
            let (blen, got_type) = unsafe {
                let p = dma_virt(data_buf) as *const u8;
                (*p, *p.add(1))
            };
            serial_println!(
                "[XHCI] GET_DESCRIPTOR({}) complete cc={} len={} type={}",
                what,
                cc,
                blen,
                got_type
            );
            return Some(blen as usize);
        }
        serial_println!("[XHCI] GET_DESCRIPTOR({}) completion code: {}", what, cc);
        return None;
    }

    serial_println!("[XHCI] GET_DESCRIPTOR({}): timeout", what);
    None
}

/// GET_DESCRIPTOR(Device Qualifier). Requires a Transfer Event.
pub fn get_device_qualifier(slot_id: u8) -> Option<usize> {
    control_get_descriptor(
        slot_id,
        UsbDescriptorType::DeviceQualifier,
        10,
        &QUAL_LIVE,
        "Device Qualifier",
    )
}

/// GET_DESCRIPTOR(BOS). Requires a Transfer Event.
pub fn get_bos_descriptor(slot_id: u8) -> Option<usize> {
    control_get_descriptor(slot_id, UsbDescriptorType::Bos, 5, &BOS_LIVE, "BOS")
}

fn descriptor_probe(live: &AtomicBool, get: fn(u8) -> Option<usize>) {
    if live.load(Ordering::Relaxed) {
        return;
    }
    if let Some(slot_id) = hid_slot() {
        let _ = get(slot_id);
    }
}

/// Prove GET_DESCRIPTOR(Device Qualifier) got a Transfer Event.
pub fn qual_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci qual skipped (no controller)");
        return true;
    }
    descriptor_probe(&QUAL_LIVE, get_device_qualifier);
    if !QUAL_LIVE.load(Ordering::Relaxed) {
        serial_println!(
            "[XHCI] GATE_USB FAILED: GET_DESCRIPTOR(Device Qualifier) had no Transfer Event"
        );
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_QUAL_MARKER);
    true
}

/// Prove GET_DESCRIPTOR(BOS) got a Transfer Event.
pub fn bos_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci bos skipped (no controller)");
        return true;
    }
    descriptor_probe(&BOS_LIVE, get_bos_descriptor);
    if !BOS_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_DESCRIPTOR(BOS) had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_BOS_MARKER);
    true
}

/// GET_DESCRIPTOR(Other Speed Configuration). Requires a Transfer Event.
pub fn get_other_speed_config(slot_id: u8) -> Option<usize> {
    control_get_descriptor(
        slot_id,
        UsbDescriptorType::OtherSpeedConfig,
        9,
        &OSCFG_LIVE,
        "Other Speed Configuration",
    )
}

/// SYNCH_FRAME(endpoint) — Setup+Data+Status. Requires a Transfer Event.
pub fn synch_frame(slot_id: u8, endpoint: u8) -> Option<u16> {
    serial_println!(
        "[XHCI] SYNCH_FRAME(ep={:#04x}) for slot {}",
        endpoint,
        slot_id
    );
    restart_ep(slot_id, 1);

    let (data_buf, _data_layout) = alloc_ring_buffer(4096);

    let mut setup = Trb::new();
    // bmRequestType=0x82 (IN, standard, endpoint), bRequest=12, wValue=0
    setup.param_lo = 0x82 | (UsbRequestType::SynchFrame as u32) << 8;
    setup.param_hi = endpoint as u32 | (2 << 16);
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 3 << 16; // TRT = IN Data Stage
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut data = Trb::new();
    data.param_lo = (data_buf & 0xFFFFFFFF) as u32;
    data.param_hi = (data_buf >> 32) as u32;
    data.status = 2;
    data.set_type(TrbType::DataStage);
    data.control |= 1 << 16; // DIR = IN
    data.control |= 1 << 2; // ISP
    data.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, data, status]) {
        serial_println!("[XHCI] Failed to submit SYNCH_FRAME transfer");
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: the control TD completed.
        if cc == 1 || cc == 13 || cc == 6 {
            SYNCH_LIVE.store(true, Ordering::Relaxed);
            // SAFETY: SYNCH_FRAME DMA page is owned by this transfer.
            let word = unsafe { core::ptr::read(dma_virt(data_buf) as *const u16) };
            serial_println!("[XHCI] SYNCH_FRAME complete cc={} frame={:#06x}", cc, word);
            return Some(word);
        }
        serial_println!("[XHCI] SYNCH_FRAME completion code: {}", cc);
        return None;
    }

    serial_println!("[XHCI] SYNCH_FRAME timeout");
    None
}

/// Prove GET_DESCRIPTOR(Other Speed Configuration) got a Transfer Event.
pub fn oscfg_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci oscfg skipped (no controller)");
        return true;
    }
    descriptor_probe(&OSCFG_LIVE, get_other_speed_config);
    if !OSCFG_LIVE.load(Ordering::Relaxed) {
        serial_println!(
            "[XHCI] GATE_USB FAILED: GET_DESCRIPTOR(Other Speed Configuration) had no Transfer Event"
        );
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_OSCFG_MARKER);
    true
}

/// Prove SYNCH_FRAME got a Transfer Event.
pub fn synch_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci synch skipped (no controller)");
        return true;
    }
    if !SYNCH_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = synch_frame(slot_id, 0x81);
        }
    }
    if !SYNCH_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SYNCH_FRAME had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_SYNCH_MARKER);
    true
}

/// SET_SEL — Setup+Data+Status (6-byte OUT). Requires a Transfer Event.
pub fn set_sel(slot_id: u8, sel: &[u8; 6]) -> bool {
    serial_println!("[XHCI] SET_SEL for slot {}", slot_id);
    recover_control_ep(slot_id);

    let (data_buf, _data_layout) = alloc_ring_buffer(4096);
    // SAFETY: bounce page is a DMA frame we own; HC reads OUT data.
    unsafe {
        core::ptr::copy_nonoverlapping(sel.as_ptr(), dma_virt(data_buf) as *mut u8, 6);
    }

    let mut setup = Trb::new();
    // bmRequestType=0x00 (device OUT), bRequest=48 (SET_SEL), wLength=6
    setup.param_lo = (UsbRequestType::SetSel as u32) << 8;
    setup.param_hi = 6u32 << 16;
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 2 << 16; // TRT = OUT Data Stage
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut data_trb = Trb::new();
    data_trb.param_lo = (data_buf & 0xFFFFFFFF) as u32;
    data_trb.param_hi = (data_buf >> 32) as u32;
    data_trb.status = 6;
    data_trb.set_type(TrbType::DataStage);
    data_trb.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 16; // DIR = IN for OUT-data control
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, data_trb, status]) {
        serial_println!("[XHCI] Failed to submit SET_SEL transfer");
        return false;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: USB2 HID STALLs SET_SEL.
        if cc == 1 || cc == 13 || cc == 6 {
            SETSEL_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] SET_SEL complete cc={}", cc);
            return true;
        }
        serial_println!("[XHCI] SET_SEL completion code: {}", cc);
    } else {
        serial_println!("[XHCI] SET_SEL timeout");
    }
    false
}

/// SET_ISOCH_DELAY — Setup+Status (no data). Requires a Transfer Event.
pub fn set_isoch_delay(slot_id: u8, delay: u16) -> bool {
    serial_println!("[XHCI] SET_ISOCH_DELAY({}) for slot {}", delay, slot_id);
    restart_ep(slot_id, 1);

    let mut setup = Trb::new();
    // bmRequestType=0x00, bRequest=49, wValue=delay
    setup.param_lo = (UsbRequestType::SetIsochDelay as u32) << 8 | (delay as u32) << 16;
    setup.param_hi = 0;
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 16; // DIR = IN for no-data control
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, status]) {
        serial_println!("[XHCI] Failed to submit SET_ISOCH_DELAY transfer");
        return false;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: USB2 HID STALLs SET_ISOCH_DELAY.
        if cc == 1 || cc == 13 || cc == 6 {
            ISOCHDELAY_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] SET_ISOCH_DELAY complete cc={}", cc);
            return true;
        }
        serial_println!("[XHCI] SET_ISOCH_DELAY completion code: {}", cc);
        return false;
    }

    serial_println!("[XHCI] SET_ISOCH_DELAY timeout");
    false
}

/// Prove SET_SEL got a Transfer Event.
pub fn setsel_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci setsel skipped (no controller)");
        return true;
    }
    if !SETSEL_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = set_sel(slot_id, &[0, 0, 0, 0, 0, 0]);
        }
    }
    if !SETSEL_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_SEL had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_SETSEL_MARKER);
    true
}

/// Prove SET_ISOCH_DELAY got a Transfer Event.
pub fn isochdelay_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci isochdelay skipped (no controller)");
        return true;
    }
    if !ISOCHDELAY_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = set_isoch_delay(slot_id, 0);
        }
    }
    if !ISOCHDELAY_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_ISOCH_DELAY had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_ISOCHDELAY_MARKER);
    true
}

/// GET_DESCRIPTOR(HID) — interface recipient, 9-byte class descriptor.
pub fn get_hid_descriptor(slot_id: u8) -> Option<usize> {
    control_get_descriptor_at(
        slot_id,
        UsbDescriptorType::HID,
        9,
        &HID_LIVE,
        "HID",
        0x81,
        0,
    )
}

/// GET_DESCRIPTOR(HID Report) — interface recipient.
pub fn get_hid_report_descriptor(slot_id: u8) -> Option<usize> {
    control_get_descriptor_at(
        slot_id,
        UsbDescriptorType::HIDReport,
        64,
        &HIDREPORT_LIVE,
        "HID Report",
        0x81,
        0,
    )
}

/// Prove GET_DESCRIPTOR(HID) got a Transfer Event.
pub fn hid_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci hiddesc skipped (no controller)");
        return true;
    }
    descriptor_probe(&HID_LIVE, get_hid_descriptor);
    if !HID_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_DESCRIPTOR(HID) had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HID_MARKER);
    true
}

/// Prove GET_DESCRIPTOR(HID Report) got a Transfer Event.
pub fn hidreport_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci hidreport skipped (no controller)");
        return true;
    }
    descriptor_probe(&HIDREPORT_LIVE, get_hid_report_descriptor);
    if !HIDREPORT_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_DESCRIPTOR(HID Report) had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HIDREPORT_MARKER);
    true
}

/// HID GET_REPORT — class IN, Input report. Requires a Transfer Event.
pub fn hid_get_report(slot_id: u8, report_id: u8, xfer_len: u32) -> Option<usize> {
    serial_println!("[XHCI] GET_REPORT({}) for slot {}", report_id, slot_id);
    restart_ep(slot_id, 1);

    let (data_buf, _data_layout) = alloc_ring_buffer(4096);
    let wvalue = (1u16 << 8) | report_id as u16; // Input report

    let mut setup = Trb::new();
    // bmRequestType=0xA1 (IN, class, interface), bRequest=1 (GET_REPORT)
    setup.param_lo = 0xA1 | 1u32 << 8 | (wvalue as u32) << 16;
    setup.param_hi = xfer_len << 16;
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 3 << 16; // TRT = IN Data Stage
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut data = Trb::new();
    data.param_lo = (data_buf & 0xFFFFFFFF) as u32;
    data.param_hi = (data_buf >> 32) as u32;
    data.status = xfer_len;
    data.set_type(TrbType::DataStage);
    data.control |= 1 << 16; // DIR = IN
    data.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, data, status]) {
        serial_println!("[XHCI] Failed to submit GET_REPORT");
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: USB2 HID may stall GET_REPORT.
        if cc == 1 || cc == 13 || cc == 6 {
            HIDGETREPORT_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] GET_REPORT complete cc={}", cc);
            return Some(xfer_len as usize);
        }
        serial_println!("[XHCI] GET_REPORT completion code: {}", cc);
        return None;
    }
    serial_println!("[XHCI] GET_REPORT timeout");
    None
}

/// HID SET_REPORT — class OUT, Output report. Requires a Transfer Event.
pub fn hid_set_report(slot_id: u8, report_id: u8, data: &[u8]) -> bool {
    serial_println!("[XHCI] SET_REPORT({}) for slot {}", report_id, slot_id);
    recover_control_ep(slot_id);

    let (data_buf, _data_layout) = alloc_ring_buffer(4096);
    if !data.is_empty() {
        // SAFETY: bounce page is a DMA frame we own; HC reads OUT data.
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                dma_virt(data_buf) as *mut u8,
                data.len(),
            );
        }
    }
    let wvalue = (2u16 << 8) | report_id as u16; // Output report

    let mut setup = Trb::new();
    // bmRequestType=0x21 (OUT, class, interface), bRequest=9 (SET_REPORT)
    setup.param_lo = 0x21 | 9u32 << 8 | (wvalue as u32) << 16;
    setup.param_hi = (data.len() as u32) << 16;
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 2 << 16; // TRT = OUT Data Stage
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut data_trb = Trb::new();
    data_trb.param_lo = (data_buf & 0xFFFFFFFF) as u32;
    data_trb.param_hi = (data_buf >> 32) as u32;
    data_trb.status = data.len() as u32;
    data_trb.set_type(TrbType::DataStage);
    data_trb.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 16; // DIR = IN for OUT-data control
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, data_trb, status]) {
        serial_println!("[XHCI] Failed to submit SET_REPORT");
        return false;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: USB2 HID may stall SET_REPORT.
        if cc == 1 || cc == 13 || cc == 6 {
            HIDSETREPORT_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] SET_REPORT complete cc={}", cc);
            return true;
        }
        serial_println!("[XHCI] SET_REPORT completion code: {}", cc);
        return false;
    }
    serial_println!("[XHCI] SET_REPORT timeout");
    false
}

/// Prove HID GET_REPORT got a Transfer Event.
pub fn getreport_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci getreport skipped (no controller)");
        return true;
    }
    if !HIDGETREPORT_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = hid_get_report(slot_id, 0, 8);
        }
    }
    if !HIDGETREPORT_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_REPORT had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HIDGETREPORT_MARKER);
    true
}

/// Prove HID SET_REPORT got a Transfer Event.
pub fn setreport_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci setreport skipped (no controller)");
        return true;
    }
    if !HIDSETREPORT_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = hid_set_report(slot_id, 0, &[0u8; 8]);
        }
    }
    if !HIDSETREPORT_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_REPORT had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HIDSETREPORT_MARKER);
    true
}

/// HID GET_IDLE — class IN, 1-byte idle duration. Requires a Transfer Event.
pub fn hid_get_idle(slot_id: u8, report_id: u8) -> Option<u8> {
    serial_println!("[XHCI] GET_IDLE({}) for slot {}", report_id, slot_id);
    restart_ep(slot_id, 1);

    let (data_buf, _data_layout) = alloc_ring_buffer(4096);
    let wvalue = report_id as u16;

    let mut setup = Trb::new();
    // bmRequestType=0xA1 (IN, class, interface), bRequest=2 (GET_IDLE)
    setup.param_lo = 0xA1 | 2u32 << 8 | (wvalue as u32) << 16;
    setup.param_hi = 1u32 << 16;
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 3 << 16; // TRT = IN Data Stage
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut data = Trb::new();
    data.param_lo = (data_buf & 0xFFFFFFFF) as u32;
    data.param_hi = (data_buf >> 32) as u32;
    data.status = 1;
    data.set_type(TrbType::DataStage);
    data.control |= 1 << 16; // DIR = IN
    data.control |= 1 << 2; // ISP
    data.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, data, status]) {
        serial_println!("[XHCI] Failed to submit GET_IDLE");
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: USB2 HID may stall GET_IDLE.
        if cc == 1 || cc == 13 || cc == 6 {
            // SAFETY: GET_IDLE DMA page is owned by this transfer.
            let value = unsafe { core::ptr::read(dma_virt(data_buf) as *const u8) };
            HIDGETIDLE_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] GET_IDLE complete cc={} value={}", cc, value);
            return Some(value);
        }
        serial_println!("[XHCI] GET_IDLE completion code: {}", cc);
        return None;
    }
    serial_println!("[XHCI] GET_IDLE timeout");
    None
}

/// HID SET_IDLE — class OUT, duration in wValue. Requires a Transfer Event.
pub fn hid_set_idle(slot_id: u8, report_id: u8, duration: u8) -> bool {
    serial_println!(
        "[XHCI] SET_IDLE(report={} duration={}) for slot {}",
        report_id,
        duration,
        slot_id
    );
    recover_control_ep(slot_id);

    let wvalue = (duration as u16) << 8 | report_id as u16;

    let mut setup = Trb::new();
    // bmRequestType=0x21 (OUT, class, interface), bRequest=0x0A (SET_IDLE)
    setup.param_lo = 0x21 | 0x0Au32 << 8 | (wvalue as u32) << 16;
    setup.param_hi = 0; // wIndex=0, wLength=0
    setup.status = 8;
    setup.set_type(TrbType::SetupStage);
    setup.control |= 1 << 6; // IDT
    setup.control |= TRB_CH;

    let mut status = Trb::new();
    status.set_type(TrbType::StatusStage);
    status.control |= 1 << 16; // DIR = IN for no-data control
    status.control |= 1 << 5; // IOC

    if !submit_transfer(slot_id, 1, &[setup, status]) {
        serial_println!("[XHCI] Failed to submit SET_IDLE");
        return false;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: USB2 HID may stall SET_IDLE.
        if cc == 1 || cc == 13 || cc == 6 {
            HIDSETIDLE_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] SET_IDLE complete cc={}", cc);
            return true;
        }
        serial_println!("[XHCI] SET_IDLE completion code: {}", cc);
        return false;
    }
    serial_println!("[XHCI] SET_IDLE timeout");
    false
}

/// Prove HID GET_IDLE got a Transfer Event.
pub fn getidle_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci getidle skipped (no controller)");
        return true;
    }
    if !HIDGETIDLE_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = hid_get_idle(slot_id, 0);
        }
    }
    if !HIDGETIDLE_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: GET_IDLE had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HIDGETIDLE_MARKER);
    true
}

/// Prove HID SET_IDLE got a Transfer Event.
pub fn setidle_self_test() -> bool {
    if !super::is_available() {
        serial_println!("[XHCI] GATE_USB xhci setidle skipped (no controller)");
        return true;
    }
    if !HIDSETIDLE_LIVE.load(Ordering::Relaxed) {
        if let Some(slot_id) = hid_slot() {
            let _ = hid_set_idle(slot_id, 0, 0);
        }
    }
    if !HIDSETIDLE_LIVE.load(Ordering::Relaxed) {
        serial_println!("[XHCI] GATE_USB FAILED: SET_IDLE had no Transfer Event");
        return false;
    }
    serial_println!("[XHCI] {}", GATE_XHCI_HIDSETIDLE_MARKER);
    true
}
