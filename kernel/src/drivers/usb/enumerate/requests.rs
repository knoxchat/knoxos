//! Standard USB control requests on EP0 (descriptors, configuration, features).
use core::sync::atomic::{AtomicBool, Ordering};

use crate::serial_println;

use super::super::types::{
    UsbConfigDescriptor, UsbDescriptorType, UsbDeviceDescriptor, UsbRequestType,
};
use super::super::xhci::{Trb, TrbType, alloc_ring_buffer, dma_virt};
use super::command::{TRB_CH, submit_transfer, wait_command_completion};
use super::live::{
    BOS_LIVE, CLEAR_LIVE, CONFIG_DESC_LIVE, CONFIG_LIVE, DESC_LIVE, GETCFG_LIVE, GETIFACE_LIVE,
    IFACE_LIVE, ISOCHDELAY_LIVE, OSCFG_LIVE, QUAL_LIVE, SET_LIVE, SETDESC_LIVE, SETSEL_LIVE,
    STATUS_LIVE, STRING_LIVE, SYNCH_LIVE,
};
use super::slot::{recover_control_ep, restart_ep};

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

/// GET_DESCRIPTOR of `dtype` — Setup+Data+Status. Requires a Transfer Event.
pub(super) fn control_get_descriptor(
    slot_id: u8,
    dtype: UsbDescriptorType,
    xfer_len: u32,
    live: &AtomicBool,
    what: &str,
) -> Option<usize> {
    control_get_descriptor_at(slot_id, dtype, xfer_len, live, what, 0x80, 0)
}

pub(super) fn control_get_descriptor_at(
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
