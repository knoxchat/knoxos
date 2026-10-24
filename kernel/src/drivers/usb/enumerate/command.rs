//! Command / transfer ring enqueue, completion wait, and doorbell ringing.
use core::sync::atomic::Ordering;

use crate::serial_println;

use super::super::xhci::{
    CMD_RING_SIZE, EVENT_RING_SIZE, TRANSFER_RING_SIZE, TRB_SIZE, TransferRing, Trb, TrbType, XHCI,
    alloc_ring_buffer, dma_virt, xhci_write32,
};

/// TRB chain bit — required so Setup+Data+Status are one TD.
pub(in crate::drivers::usb) const TRB_CH: u32 = 1 << 4;

/// Enqueue a TRB on the command ring and ring the doorbell
pub(super) fn enqueue_command(trb: &Trb) -> bool {
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
pub(in crate::drivers::usb) fn wait_command_completion() -> Option<Trb> {
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

/// Allocate a transfer ring for a given slot and endpoint
pub(in crate::drivers::usb) fn allocate_transfer_ring(slot_id: u8, endpoint: u8) -> u64 {
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

/// Enqueue TRBs on a transfer ring for a specific slot/endpoint and ring doorbell
pub(in crate::drivers::usb) fn submit_transfer(slot_id: u8, endpoint: u8, trbs: &[Trb]) -> bool {
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
