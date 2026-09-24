//! USB control transfers, class identification, and /proc-style device listing.
use alloc::string::String;
use alloc::vec::Vec;

use crate::serial_println;

use super::enumerate::{submit_transfer, wait_command_completion};
use super::types::{UsbDevice, UsbSetupPacket, UsbSpeed};
use super::xhci::{Trb, TrbType, XHCI, alloc_ring_buffer};

/// Perform a control transfer via XHCI transfer ring
pub fn control_transfer(
    slot_id: u8,
    setup: &UsbSetupPacket,
    data: Option<&mut [u8]>,
) -> Result<usize, &'static str> {
    let bm_request_type = setup.bm_request_type;
    let b_request = setup.b_request;
    let w_value = setup.w_value;
    let w_index = setup.w_index;
    let w_length = setup.w_length;
    let is_in = bm_request_type & 0x80 != 0;

    serial_println!(
        "[XHCI] Control transfer: slot={} req={:#x} val={:#x} idx={:#x} len={}",
        slot_id,
        b_request,
        w_value,
        w_index,
        w_length
    );

    let mut trbs = Vec::new();

    // Setup Stage TRB
    let mut setup_trb = Trb::new();
    setup_trb.param_lo =
        (bm_request_type as u32) | ((b_request as u32) << 8) | ((w_value as u32) << 16);
    setup_trb.param_hi = (w_index as u32) | ((w_length as u32) << 16);
    setup_trb.status = 8;
    setup_trb.set_type(TrbType::SetupStage);
    setup_trb.control |= 1 << 6; // IDT
    if w_length > 0 {
        setup_trb.control |= if is_in { 3 << 16 } else { 2 << 16 }; // TRT
    }
    trbs.push(setup_trb);

    // Data Stage TRB (if data transfer needed)
    let data_buf_ptr: u64;
    let _data_layout: Option<alloc::alloc::Layout>;
    if let Some(ref buf) = data {
        let buf_size = if buf.len() < 4096 {
            4096
        } else {
            (buf.len() + 4095) & !4095
        };
        let (ptr, layout) = alloc_ring_buffer(buf_size);
        data_buf_ptr = ptr;
        _data_layout = Some(layout);

        if !is_in {
            // Copy outgoing data to DMA buffer
            unsafe {
                core::ptr::copy_nonoverlapping(buf.as_ptr(), ptr as *mut u8, buf.len());
            }
        }

        let mut data_trb = Trb::new();
        data_trb.param_lo = (ptr & 0xFFFFFFFF) as u32;
        data_trb.param_hi = (ptr >> 32) as u32;
        data_trb.status = buf.len() as u32;
        data_trb.set_type(TrbType::DataStage);
        if is_in {
            data_trb.control |= 1 << 16; // DIR = IN
        }
        trbs.push(data_trb);
    } else {
        data_buf_ptr = 0;
        _data_layout = None;
    }

    // Status Stage TRB
    let mut status_trb = Trb::new();
    status_trb.set_type(TrbType::StatusStage);
    status_trb.control |= 1 << 5; // IOC
    if data.is_some() && is_in {
        // Status direction is opposite of data direction (OUT for IN data)
    } else if data.is_some() {
        status_trb.control |= 1 << 16; // DIR = IN
    }
    trbs.push(status_trb);

    if !submit_transfer(slot_id, 1, &trbs) {
        return Err("Failed to submit control transfer");
    }

    // Wait for completion
    if let Some(event) = wait_command_completion() {
        let cc = event.completion_code();
        if cc == 1 || cc == 13 {
            // Copy received data back
            if let Some(buf) = data {
                if is_in && data_buf_ptr != 0 {
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            data_buf_ptr as *const u8,
                            buf.as_mut_ptr(),
                            buf.len(),
                        );
                    }
                }
                return Ok(buf.len());
            }
            return Ok(0);
        }
        serial_println!("[XHCI] Control transfer completion code: {}", cc);
    }

    if let Some(buf) = data {
        Ok(buf.len())
    } else {
        Ok(0)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// USB DEVICE CLASSES
// ═══════════════════════════════════════════════════════════════════════

/// Identify device class and load appropriate class driver
pub fn identify_class_driver(device: &UsbDevice) -> &'static str {
    match device.device_class {
        0x00 => "interface-specific", // Check interface descriptors
        0x01 => "audio",
        0x02 => "cdc-acm",
        0x03 => "hid",
        0x08 => "mass-storage",
        0x09 => "hub",
        0x0E => "video",
        0xE0 => "wireless",
        0xFF => "vendor-specific",
        _ => "unknown",
    }
}

/// Get /proc/bus/usb/devices output
pub fn proc_usb_devices() -> String {
    let xhci = XHCI.lock();
    let mut output = String::new();

    for dev in &xhci.devices {
        output.push_str(&alloc::format!(
            "T:  Bus=01 Lev=01 Prnt=01 Port={:02} Cnt=01 Dev#={:3} Spd={} MxCh= 0\n",
            dev.port,
            dev.slot_id,
            match dev.speed {
                UsbSpeed::Low => "1.5",
                UsbSpeed::Full => "12",
                UsbSpeed::High => "480",
                UsbSpeed::Super => "5000",
                UsbSpeed::SuperPlus => "10000",
            }
        ));
        output.push_str(&alloc::format!(
            "D:  Ver={}.{:02x} Cls={:02x}({}) Sub={:02x} Prot={:02x} MxPS={}\n",
            (0x0200 >> 8) & 0xFF,
            0x0200 & 0xFF,
            dev.device_class,
            identify_class_driver(dev),
            dev.device_subclass,
            dev.device_protocol,
            64
        ));
        output.push_str(&alloc::format!(
            "P:  Vendor={:04x} ProdID={:04x} Rev={}\n",
            dev.vendor_id,
            dev.product_id,
            "1.00"
        ));
        output.push_str(&alloc::format!("S:  Manufacturer={}\n", dev.manufacturer));
        output.push_str(&alloc::format!("S:  Product={}\n\n", dev.product));
    }

    if output.is_empty() {
        output.push_str("No USB devices detected\n");
    }
    output
}
