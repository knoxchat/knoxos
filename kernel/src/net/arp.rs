use alloc::collections::BTreeMap;
use spin::Mutex;

use super::ethernet::MacAddress;
use super::ipv4::Ipv4Address;

/// ARP packet
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ArpPacket {
    pub htype: u16,   // Hardware type (1 = Ethernet)
    pub ptype: u16,   // Protocol type (0x0800 = IPv4)
    pub hlen: u8,     // Hardware address length (6)
    pub plen: u8,     // Protocol address length (4)
    pub oper: u16,    // Operation (1 = Request, 2 = Reply)
    pub sha: [u8; 6], // Sender hardware address
    pub spa: [u8; 4], // Sender protocol address
    pub tha: [u8; 6], // Target hardware address
    pub tpa: [u8; 4], // Target protocol address
}

/// ARP cache: IP -> MAC mapping
lazy_static::lazy_static! {
    pub static ref ARP_CACHE: Mutex<BTreeMap<u32, MacAddress>> = Mutex::new(BTreeMap::new());
}

/// Resolve IP to MAC via ARP cache
pub fn arp_lookup(ip: Ipv4Address) -> Option<MacAddress> {
    ARP_CACHE.lock().get(&ip.to_u32()).copied()
}

/// Insert into ARP cache
pub fn arp_insert(ip: Ipv4Address, mac: MacAddress) {
    ARP_CACHE.lock().insert(ip.to_u32(), mac);
}
