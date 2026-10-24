//! Enable Slot, Address Device, Configure Endpoint, and other slot/endpoint commands.
use core::sync::atomic::Ordering;

use crate::serial_println;

use super::super::ports::enumerate_ports;
use super::super::types::UsbSpeed;
use super::super::xhci::{TRB_SIZE, Trb, TrbType, XHCI, alloc_ring_buffer, dma_virt};
use super::command::{allocate_transfer_ring, enqueue_command, wait_command_completion};
use super::live::{
    ADDRESS_LIVE, CONFIG_EP_LIVE, DISABLE_LIVE, EVAL_LIVE, NOOP_LIVE, RESETDEV_LIVE, RESETEP_LIVE,
    SETTR_LIVE, SLOT_ENABLE_LIVE, STOP_LIVE,
};

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

pub(super) fn configure_interrupt_ep(slot_id: u8, port: u8, speed: UsbSpeed) -> bool {
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

/// After a Halted control TD, Reset Endpoint + Set TR Dequeue Pointer so EP0
/// accepts another Setup. No-op if the endpoint is still Running.
pub(super) fn recover_control_ep(slot_id: u8) {
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

/// Halted → Reset Endpoint; Running → Stop Endpoint; then Set TR Dequeue so a
/// doorbell can restart the ring. Reset Endpoint on a Stopped EP is cc≠1.
pub(super) fn restart_ep(slot_id: u8, dci: u8) {
    if !reset_endpoint(slot_id, dci) {
        let _ = stop_endpoint(slot_id, dci);
    }
    let _ = set_tr_dequeue(slot_id, dci);
}
