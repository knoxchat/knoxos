use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use super::ethernet::MacAddress;
use super::ipv4::Ipv4Address;

/// Network interface configuration
pub struct NetworkInterface {
    pub name: String,
    pub mac: MacAddress,
    pub ip: Ipv4Address,
    pub netmask: Ipv4Address,
    pub gateway: Ipv4Address,
    pub dns: Ipv4Address,
    pub mtu: u16,
    pub is_up: bool,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

lazy_static::lazy_static! {
    pub static ref NETWORK_INTERFACES: Mutex<Vec<NetworkInterface>> = {
        let interfaces = vec![
            // Loopback interface
            NetworkInterface {
                name: String::from("lo"),
                mac: MacAddress::ZERO,
                ip: Ipv4Address::LOOPBACK,
                netmask: Ipv4Address::new(255, 0, 0, 0),
                gateway: Ipv4Address::UNSPECIFIED,
                dns: Ipv4Address::UNSPECIFIED,
                mtu: 65535,
                is_up: true,
                rx_bytes: 0,
                tx_bytes: 0,
                rx_packets: 0,
                tx_packets: 0,
            },
            // Primary ethernet (for virtio-net)
            NetworkInterface {
                name: String::from("eth0"),
                mac: MacAddress::new([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]), // QEMU default
                ip: Ipv4Address::UNSPECIFIED,
                netmask: Ipv4Address::new(255, 255, 255, 0),
                gateway: Ipv4Address::UNSPECIFIED,
                dns: Ipv4Address::new(10, 0, 2, 3),
                mtu: 1500,
                is_up: false, // Raised when DHCP writes a lease
                rx_bytes: 0,
                tx_bytes: 0,
                rx_packets: 0,
                tx_packets: 0,
            },
        ];

        Mutex::new(interfaces)
    };
}

/// Write IPv4 + default route onto a named interface (Gate D3).
pub fn configure_ipv4(
    name: &str,
    ip: Ipv4Address,
    netmask: Ipv4Address,
    gateway: Ipv4Address,
    dns: Ipv4Address,
) {
    let mac = crate::netint::get_mac();
    let mut interfaces = NETWORK_INTERFACES.lock();
    if let Some(iface) = interfaces.iter_mut().find(|i| i.name == name) {
        iface.ip = ip;
        iface.netmask = netmask;
        iface.gateway = gateway;
        iface.dns = dns;
        iface.mac = MacAddress(mac);
        iface.is_up = true;
    }
}

pub fn eth0_config() -> (Ipv4Address, bool, Ipv4Address) {
    let interfaces = NETWORK_INTERFACES.lock();
    if let Some(iface) = interfaces.iter().find(|i| i.name == "eth0") {
        (iface.ip, iface.is_up, iface.gateway)
    } else {
        (Ipv4Address::UNSPECIFIED, false, Ipv4Address::UNSPECIFIED)
    }
}
