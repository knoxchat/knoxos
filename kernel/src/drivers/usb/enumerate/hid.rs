//! HID class descriptors and GET/SET_REPORT, IDLE, PROTOCOL control requests.
use core::sync::atomic::Ordering;

use crate::serial_println;

use super::super::types::UsbDescriptorType;
use super::super::xhci::{Trb, TrbType, alloc_ring_buffer, dma_virt};
use super::command::{TRB_CH, submit_transfer, wait_command_completion};
use super::live::{
    HID_LIVE, HIDGETIDLE_LIVE, HIDGETPROTOCOL_LIVE, HIDGETREPORT_LIVE, HIDREPORT_LIVE,
    HIDSETIDLE_LIVE, HIDSETPROTOCOL_LIVE, HIDSETREPORT_LIVE,
};
use super::requests::control_get_descriptor_at;
use super::slot::{recover_control_ep, restart_ep};

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

/// HID GET_PROTOCOL — class IN, 1-byte boot/report protocol. Requires a Transfer Event.
pub fn hid_get_protocol(slot_id: u8) -> Option<u8> {
    serial_println!("[XHCI] GET_PROTOCOL for slot {}", slot_id);
    restart_ep(slot_id, 1);

    let (data_buf, _data_layout) = alloc_ring_buffer(4096);

    let mut setup = Trb::new();
    // bmRequestType=0xA1 (IN, class, interface), bRequest=3 (GET_PROTOCOL)
    setup.param_lo = 0xA1 | 3u32 << 8;
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
        serial_println!("[XHCI] Failed to submit GET_PROTOCOL");
        return None;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: USB2 HID may stall GET_PROTOCOL.
        if cc == 1 || cc == 13 || cc == 6 {
            // SAFETY: GET_PROTOCOL DMA page is owned by this transfer.
            let value = unsafe { core::ptr::read(dma_virt(data_buf) as *const u8) };
            HIDGETPROTOCOL_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] GET_PROTOCOL complete cc={} value={}", cc, value);
            return Some(value);
        }
        serial_println!("[XHCI] GET_PROTOCOL completion code: {}", cc);
        return None;
    }
    serial_println!("[XHCI] GET_PROTOCOL timeout");
    None
}

/// HID SET_PROTOCOL — class OUT, 0=boot 1=report. Requires a Transfer Event.
pub fn hid_set_protocol(slot_id: u8, protocol: u8) -> bool {
    serial_println!("[XHCI] SET_PROTOCOL({}) for slot {}", protocol, slot_id);
    recover_control_ep(slot_id);

    let wvalue = protocol as u16;

    let mut setup = Trb::new();
    // bmRequestType=0x21 (OUT, class, interface), bRequest=0x0B (SET_PROTOCOL)
    setup.param_lo = 0x21 | 0x0Bu32 << 8 | (wvalue as u32) << 16;
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
        serial_println!("[XHCI] Failed to submit SET_PROTOCOL");
        return false;
    }

    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        // 1 Success, 13 Short Packet, 6 Stall: USB2 HID may stall SET_PROTOCOL.
        if cc == 1 || cc == 13 || cc == 6 {
            HIDSETPROTOCOL_LIVE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] SET_PROTOCOL complete cc={}", cc);
            return true;
        }
        serial_println!("[XHCI] SET_PROTOCOL completion code: {}", cc);
        return false;
    }
    serial_println!("[XHCI] SET_PROTOCOL timeout");
    false
}
