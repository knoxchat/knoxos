//! USB XHCI Host Controller Driver
//!
//! Implements the eXtensible Host Controller Interface for USB 3.x support.
//! Provides:
//!   - PCI device discovery for XHCI controllers
//!   - Host controller initialization with real MMIO register programming
//!   - Device Context Base Address Array (DCBAA) in kernel memory
//!   - Command Ring / Event Ring / Transfer Ring with real TRB submission
//!   - USB device enumeration via port status change detection
//!   - USB Mass Storage Class (MSC) Bulk-Only Transport
//!   - SCSI READ(10)/WRITE(10)/READ CAPACITY/INQUIRY over USB bulk pipes
mod audio;
mod cdc;
mod control;
mod enumerate;
mod ethernet;
mod gamepad;
mod msc;
mod pd;
mod ports;
mod trackpad;
mod types;
mod uvc;
mod wacom;
mod xhci;

pub use audio::*;
pub use cdc::*;
pub use control::*;
pub use enumerate::{
    GATE_XHCI_ADDR_MARKER, GATE_XHCI_BOS_MARKER, GATE_XHCI_BULK_MARKER, GATE_XHCI_BULKIN_MARKER,
    GATE_XHCI_CLEAR_MARKER, GATE_XHCI_CONFIG_MARKER, GATE_XHCI_DESC_MARKER,
    GATE_XHCI_DISABLE_MARKER, GATE_XHCI_ENUM_MARKER, GATE_XHCI_EP_MARKER, GATE_XHCI_EVAL_MARKER,
    GATE_XHCI_GETCFG_MARKER, GATE_XHCI_GETIFACE_MARKER, GATE_XHCI_HID_MARKER,
    GATE_XHCI_HIDGETREPORT_MARKER, GATE_XHCI_HIDREPORT_MARKER, GATE_XHCI_HIDSETREPORT_MARKER,
    GATE_XHCI_IFACE_MARKER, GATE_XHCI_INTR_MARKER, GATE_XHCI_ISOCH_MARKER,
    GATE_XHCI_ISOCHDELAY_MARKER, GATE_XHCI_NOOP_MARKER, GATE_XHCI_OSCFG_MARKER,
    GATE_XHCI_QUAL_MARKER, GATE_XHCI_RESETDEV_MARKER, GATE_XHCI_RESETEP_MARKER,
    GATE_XHCI_SET_MARKER, GATE_XHCI_SETDESC_MARKER, GATE_XHCI_SETSEL_MARKER,
    GATE_XHCI_SETTR_MARKER, GATE_XHCI_SLOT_MARKER, GATE_XHCI_STATUS_MARKER, GATE_XHCI_STOP_MARKER,
    GATE_XHCI_STRING_MARKER, GATE_XHCI_SYNCH_MARKER, GATE_XHCI_TRNOOP_MARKER, addr_self_test,
    address_device, bos_self_test, bulk_in, bulk_in_self_test, bulk_out, bulk_self_test,
    clear_feature, clear_self_test, config_self_test, configure_endpoint, descriptor_self_test,
    disable_self_test, disable_slot, enable_slot, enum_self_test, enumerate_all_devices,
    enumerate_device, ep_self_test, eval_self_test, evaluate_context, get_bos_descriptor,
    get_config_descriptor, get_configuration, get_device_descriptor, get_device_qualifier,
    get_hid_descriptor, get_hid_report_descriptor, get_interface, get_other_speed_config,
    get_status, get_string_descriptor, getcfg_self_test, getiface_self_test, getreport_self_test,
    hid_get_report, hid_self_test, hid_set_report, hidreport_self_test, iface_self_test,
    interrupt_in, interrupt_in_self_test, isoch_in, isoch_in_self_test, isochdelay_self_test,
    no_op_command, noop_self_test, oscfg_self_test, poll_events, poll_transfer_event,
    qual_self_test, reset_device, reset_endpoint, resetdev_self_test, resetep_self_test,
    set_configuration, set_descriptor, set_descriptor_self_test, set_feature, set_interface,
    set_isoch_delay, set_sel, set_self_test, set_tr_dequeue, setreport_self_test, setsel_self_test,
    settr_self_test, slot_self_test, status_self_test, stop_endpoint, stop_self_test,
    string_self_test, submit_interrupt_urb, submit_interrupt_urb_for_hid, synch_frame,
    synch_self_test, transfer_noop, trnoop_self_test,
};
pub use ethernet::*;
pub use gamepad::*;
pub use msc::*;
pub use pd::*;
pub use ports::*;
pub use trackpad::*;
pub use types::*;
pub use uvc::*;
pub use wacom::*;
pub use xhci::{
    GATE_XHCI_MMIO_MARKER, TransferRing, Trb, TrbType, XHCI, XHCI_CMD_HCRST, XHCI_CMD_HSEE,
    XHCI_CMD_INTE, XHCI_CMD_RUN, XHCI_CONFIG, XHCI_CRCR, XHCI_DCBAAP, XHCI_DNCTRL, XHCI_PAGESIZE,
    XHCI_STS_CNR, XHCI_STS_EINT, XHCI_STS_HCH, XHCI_STS_HSE, XHCI_STS_PCD, XHCI_USBCMD,
    XHCI_USBSTS, XhciCapRegs, XhciController, device_count, init_controller, is_available,
    list_devices, mmio_self_test,
};

use core::sync::atomic::Ordering;

use crate::serial_println;

use xhci::{XHCI_AVAILABLE, find_xhci_controller, pci_read16};

/// Initialize USB XHCI subsystem with real controller programming
pub fn init() {
    if let Some((bus, dev, func, bar0)) = find_xhci_controller() {
        let vendor = pci_read16(bus, dev, func, 0x00);
        let device = pci_read16(bus, dev, func, 0x02);

        serial_println!(
            "[XHCI] Found XHCI controller: vendor={:#06x} device={:#06x}",
            vendor,
            device
        );

        if init_controller(bus, dev, func, bar0) {
            XHCI_AVAILABLE.store(true, Ordering::Relaxed);
            serial_println!("[XHCI] USB 3.x Host Controller initialized");
            let _ = mmio_self_test();

            // Enumerate connected devices via real PORTSC polling
            enumerate_all_devices();
            let _ = slot_self_test();
            let _ = addr_self_test();
            let _ = descriptor_self_test();
            let _ = config_self_test();
            let _ = ep_self_test();
            let _ = status_self_test();
            let _ = iface_self_test();
            let _ = getcfg_self_test();
            let _ = getiface_self_test();
            let _ = set_self_test();
            let _ = set_descriptor_self_test();
            let _ = clear_self_test();
            let _ = bulk_self_test();
            let _ = bulk_in_self_test();
            let _ = isoch_in_self_test();
            let _ = string_self_test();
            let _ = interrupt_in_self_test();
            let _ = noop_self_test();
            let _ = disable_self_test();
            let _ = eval_self_test();
            let _ = stop_self_test();
            let _ = resetep_self_test();
            let _ = settr_self_test();
            let _ = trnoop_self_test();
            let _ = bos_self_test();
            let _ = qual_self_test();
            let _ = oscfg_self_test();
            let _ = synch_self_test();
            let _ = setsel_self_test();
            let _ = isochdelay_self_test();
            let _ = hid_self_test();
            let _ = hidreport_self_test();
            let _ = getreport_self_test();
            let _ = setreport_self_test();
            let _ = resetdev_self_test();
            let _ = enum_self_test();

            let dev_count = device_count();
            serial_println!("[XHCI] {} USB device(s) enumerated", dev_count);

            // Probe all enumerated devices for mass storage class
            let devices = list_devices();
            for usb_dev in &devices {
                if let Some(mut msc_dev) = probe_mass_storage(usb_dev) {
                    serial_println!(
                        "[USB-MSC] Attempting to mount mass storage device {}...",
                        msc_dev.device_id
                    );
                    match mount_device(&mut msc_dev) {
                        Ok(()) => {
                            serial_println!(
                                "[USB-MSC] Device {} mounted successfully ({} MB)",
                                msc_dev.device_id,
                                (msc_dev.sector_count * msc_dev.sector_size as u64) / (1024 * 1024)
                            );
                        }
                        Err(e) => {
                            serial_println!(
                                "[USB-MSC] Failed to mount device {}: {}",
                                msc_dev.device_id,
                                e
                            );
                        }
                    }
                }
            }

            let msc_count = msc_device_count();
            if msc_count > 0 {
                serial_println!("[USB-MSC] {} mass storage device(s) ready", msc_count);
            }
        } else {
            serial_println!("[XHCI] Failed to initialize controller");
        }
    } else {
        serial_println!("[XHCI] No XHCI controller found (USB not available)");
    }
    msc::init();
    let _ = ports::hub_self_test();
    let _ = gamepad::gamepad_self_test();
    let _ = audio::audio_self_test();
    let _ = trackpad::trackpad_self_test();
    let _ = wacom::wacom_self_test();
    let _ = uvc::uvc_self_test();
    let _ = ethernet::usb_ethernet_self_test();
}
