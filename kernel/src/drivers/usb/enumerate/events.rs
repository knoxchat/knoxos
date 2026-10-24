//! Interrupt-IN URB submission and non-blocking event-ring drain.
use alloc::vec;
use alloc::vec::Vec;

use spin::Mutex;

use crate::serial_println;

use super::super::xhci::{EVENT_RING_SIZE, TRB_SIZE, Trb, TrbType, XHCI, dma_virt, xhci_write32};
use super::command::submit_transfer;

/// Interrupt-IN URB waiting on a Transfer Event.
struct InterruptUrb {
    slot_id: u8,
    endpoint: u8,
    buf_phys: u64,
    len: u32,
    hid_id: u32,
}

static INTERRUPT_URBS: Mutex<Vec<InterruptUrb>> = Mutex::new(Vec::new());

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
        let Some(phys) = super::super::xhci::alloc_dma(4096) else {
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
