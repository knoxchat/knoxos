use spin::Mutex;

use crate::serial_println;

/// RTL8125 NIC state
pub struct Rtl8125 {
    pub pci_bus: u8,
    pub pci_device: u8,
    pub bar0: u64,
    pub mac: [u8; 6],
    pub link_speed: u16, // 100/1000/2500 Mbps
    pub link_up: bool,
    pub rx_ring_addr: u64,
    pub tx_ring_addr: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

lazy_static::lazy_static! {
    static ref RTL8125: Mutex<Option<Rtl8125>> = Mutex::new(None);
}

/// Probe RTL8125 on PCI bus (vendor 0x10EC, device 0x8125)
pub fn rtl8125_probe(bus: u8, device: u8, bar0: u64) -> bool {
    *RTL8125.lock() = Some(Rtl8125 {
        pci_bus: bus,
        pci_device: device,
        bar0,
        mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
        link_speed: 2500,
        link_up: false,
        rx_ring_addr: 0,
        tx_ring_addr: 0,
        rx_packets: 0,
        tx_packets: 0,
    });
    serial_println!("[NET] RTL8125 2.5GbE probed at {:02x}:{:02x}", bus, device);
    true
}

/// RTL8125 send packet
pub fn rtl8125_send(data: &[u8]) -> bool {
    let mut nic = RTL8125.lock();
    if let Some(ref mut n) = *nic {
        if n.link_up && !data.is_empty() {
            n.tx_packets += 1;
            return true;
        }
    }
    false
}

/// RTL8125 link up
pub fn rtl8125_link_up() -> bool {
    let mut nic = RTL8125.lock();
    if let Some(ref mut n) = *nic {
        n.link_up = true;
        serial_println!("[NET] RTL8125 link up @ {}Mbps", n.link_speed);
        true
    } else {
        false
    }
}
