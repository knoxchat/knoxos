//! USB CDC ECM — Ethernet frames over bulk with software loopback.
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

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
    rx_queue: VecDeque<Vec<u8>>,
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
        rx_queue: VecDeque::new(),
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

pub fn usb_ethernet_status(idx: usize) -> Option<UsbEthernet> {
    USB_ETHERNET.lock().get(idx).cloned()
}

pub fn usb_ethernet_set_link(idx: usize, up: bool) -> bool {
    let mut devs = USB_ETHERNET.lock();
    if let Some(dev) = devs.get_mut(idx) {
        dev.link_up = up;
        true
    } else {
        false
    }
}

/// CDC ECM has no extra USB header — the bulk payload is the Ethernet frame.
pub fn parse_cdc_ecm_frame(frame: &[u8]) -> Option<u16> {
    if frame.len() < 14 {
        return None;
    }
    Some(u16::from_be_bytes([frame[12], frame[13]]))
}

/// Send a frame via USB Ethernet and loop it into the RX queue.
pub fn usb_ethernet_send(idx: usize, frame: &[u8]) -> bool {
    if parse_cdc_ecm_frame(frame).is_none() {
        return false;
    }
    let mut devs = USB_ETHERNET.lock();
    if let Some(dev) = devs.get_mut(idx) {
        if dev.link_up {
            dev.tx_packets += 1;
            dev.rx_queue.push_back(frame.to_vec());
            dev.rx_packets += 1;
            return true;
        }
    }
    false
}

pub fn usb_ethernet_recv(idx: usize) -> Option<Vec<u8>> {
    let mut devs = USB_ETHERNET.lock();
    devs.get_mut(idx)?.rx_queue.pop_front()
}

pub const GATE_USB_ETHERNET_MARKER: &str = "GATE_USB ethernet";

pub fn usb_ethernet_self_test() -> bool {
    let mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
    let _ = usb_ethernet_probe(1, "CDC-ECM", mac);
    if !usb_ethernet_set_link(0, true) {
        serial_println!("[USB-Ethernet] GATE_USB FAILED: link");
        return false;
    }
    let mut frame = [0u8; 64];
    frame[0..6].copy_from_slice(&[0xFF; 6]);
    frame[6..12].copy_from_slice(&mac);
    frame[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
    frame[14] = 0x45;
    if !usb_ethernet_send(0, &frame) {
        serial_println!("[USB-Ethernet] GATE_USB FAILED: send");
        return false;
    }
    match usb_ethernet_recv(0) {
        Some(rx) if rx == frame && parse_cdc_ecm_frame(&rx) == Some(0x0800) => {
            serial_println!(
                "[USB-Ethernet] {} (CDC ECM IPv4 loopback {} bytes)",
                GATE_USB_ETHERNET_MARKER,
                rx.len()
            );
            true
        }
        other => {
            serial_println!(
                "[USB-Ethernet] GATE_USB FAILED recv_len={}",
                other.map(|f| f.len()).unwrap_or(0)
            );
            false
        }
    }
}
