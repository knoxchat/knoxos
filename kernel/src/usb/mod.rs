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

pub use cdc::*;
pub use control::*;
pub use enumerate::{
    address_device, enable_slot, enumerate_all_devices, enumerate_device, get_device_descriptor,
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
    TransferRing, Trb, TrbType, XHCI, XHCI_CMD_HCRST, XHCI_CMD_HSEE, XHCI_CMD_INTE, XHCI_CMD_RUN,
    XHCI_CONFIG, XHCI_CRCR, XHCI_DCBAAP, XHCI_DNCTRL, XHCI_PAGESIZE, XHCI_STS_CNR, XHCI_STS_EINT,
    XHCI_STS_HCH, XHCI_STS_HSE, XHCI_STS_PCD, XHCI_USBCMD, XHCI_USBSTS, XhciCapRegs,
    XhciController, device_count, init_controller, is_available, list_devices,
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

            // Enumerate connected devices via real PORTSC polling
            enumerate_all_devices();

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
}
