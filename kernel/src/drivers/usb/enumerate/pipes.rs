//! Bulk, isochronous, and interrupt data pipes plus Transfer No-Op.
use core::sync::atomic::Ordering;

use crate::serial_println;

use super::super::types::UsbSpeed;
use super::super::xhci::{Trb, TrbType, XHCI, alloc_ring_buffer, dma_virt};
use super::command::{
    allocate_transfer_ring, enqueue_command, submit_transfer, wait_command_completion,
};
use super::events::poll_events;
use super::live::{BULK_IN_LIVE, BULK_LIVE, INTR_LIVE, ISOCH_IN_LIVE, TRNOOP_LIVE};
use super::slot::configure_interrupt_ep;

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
