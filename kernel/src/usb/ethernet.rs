//! USB Ethernet adapter (CDC ECM / NCM / RNDIS) placeholders.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// USB Ethernet Adapter Driver
// ═══════════════════════════════════════════════════════════════════════

/// USB Ethernet adapter
#[derive(Debug, Clone)]
pub struct UsbEthernet {
    pub device_id: u8,
    pub name: String,
    pub mac: [u8; 6],
    pub speed_mbps: u16,
    pub link_up: bool,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

lazy_static::lazy_static! {
    static ref USB_ETHERNET: Mutex<Vec<UsbEthernet>> = Mutex::new(Vec::new());
}

/// Probe USB Ethernet adapter (CDC ECM, NCM, or RNDIS)
pub fn usb_ethernet_probe(device_id: u8, name: &str, mac: [u8; 6]) -> bool {
    let mut devs = USB_ETHERNET.lock();
    devs.push(UsbEthernet {
        device_id,
        name: String::from(name),
        mac,
        speed_mbps: 100,
        link_up: false,
        rx_packets: 0,
        tx_packets: 0,
    });
    serial_println!(
        "[USB-Ethernet] Adapter '{}' probed (MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x})",
        name,
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5]
    );
    true
}

/// Get USB Ethernet adapter status
pub fn usb_ethernet_status(idx: usize) -> Option<UsbEthernet> {
    USB_ETHERNET.lock().get(idx).cloned()
}

/// Send a frame via USB Ethernet
pub fn usb_ethernet_send(idx: usize, frame: &[u8]) -> bool {
    let mut devs = USB_ETHERNET.lock();
    if let Some(dev) = devs.get_mut(idx) {
        if dev.link_up {
            dev.tx_packets += 1;
            return true;
        }
    }
    false
}
