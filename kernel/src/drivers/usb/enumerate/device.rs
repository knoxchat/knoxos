//! Full-device enumeration sequence over connected XHCI ports.
use alloc::string::String;
use core::sync::atomic::Ordering;

use crate::serial_println;

use super::super::ports::enumerate_ports;
use super::super::types::{UsbDevice, UsbSpeed};
use super::super::xhci::XHCI;
use super::live::{
    ADDRESS_LIVE, CONFIG_DESC_LIVE, CONFIG_LIVE, DESC_LIVE, ENUM_LIVE, SLOT_ENABLE_LIVE,
};
use super::requests::{get_config_descriptor, get_device_descriptor, set_configuration};
use super::slot::{address_device, configure_interrupt_ep, enable_slot};

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
