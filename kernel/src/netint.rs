/// Network Integration — Wires TCP/IP stack (net.rs) to NIC drivers
///
/// This module bridges the gap between the network stack and physical NICs:
///   - Packet transmission from net.rs through virtio_net or e1000
///   - Periodic polling of NIC receive queues
///   - ARP resolution via NIC
///   - ICMP echo reply generation
///   - Network interface management
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::net::{
    ARP_CACHE, EthernetHeader, Ipv4Address, Ipv4Header, MacAddress, NETWORK_INTERFACES,
};
use crate::serial_println;

// ─── Packet Statistics ──────────────────────────────────────────────────

static TX_PACKETS: AtomicU64 = AtomicU64::new(0);
static RX_PACKETS: AtomicU64 = AtomicU64::new(0);
static TX_BYTES: AtomicU64 = AtomicU64::new(0);
static RX_BYTES: AtomicU64 = AtomicU64::new(0);
static TX_ERRORS: AtomicU64 = AtomicU64::new(0);
static INITIALIZED: AtomicBool = AtomicBool::new(false);

// ─── NIC Abstraction ────────────────────────────────────────────────────

/// Send a raw Ethernet frame via the best available NIC
pub fn send_raw_frame(data: &[u8]) -> bool {
    if crate::virtio_net::is_nic_available() {
        let ok = crate::virtio_net::send_frame(data);
        if ok {
            TX_PACKETS.fetch_add(1, Ordering::Relaxed);
            TX_BYTES.fetch_add(data.len() as u64, Ordering::Relaxed);
        } else {
            TX_ERRORS.fetch_add(1, Ordering::Relaxed);
        }
        ok
    } else if crate::e1000::is_available() {
        let ok = crate::e1000::send_frame(data);
        if ok {
            TX_PACKETS.fetch_add(1, Ordering::Relaxed);
            TX_BYTES.fetch_add(data.len() as u64, Ordering::Relaxed);
        } else {
            TX_ERRORS.fetch_add(1, Ordering::Relaxed);
        }
        ok
    } else {
        false
    }
}

/// Get the MAC address of the active NIC
pub fn get_mac() -> [u8; 6] {
    if let Some(mac) = crate::virtio_net::get_mac() {
        mac
    } else {
        crate::e1000::get_mac().unwrap_or([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]) // Default
    }
}

/// Check if any NIC is available
pub fn has_nic() -> bool {
    crate::virtio_net::is_nic_available() || crate::e1000::is_available()
}

// ─── Ethernet Frame Construction ────────────────────────────────────────

/// Build and send an Ethernet frame with the given payload
pub fn send_ethernet_frame(dst_mac: [u8; 6], ether_type: u16, payload: &[u8]) -> bool {
    let src_mac = get_mac();
    let total_len = 14 + payload.len(); // 14-byte Ethernet header
    let mut frame = vec![0u8; total_len];

    // Ethernet header
    frame[0..6].copy_from_slice(&dst_mac);
    frame[6..12].copy_from_slice(&src_mac);
    frame[12..14].copy_from_slice(&ether_type.to_be_bytes());

    // Payload
    frame[14..].copy_from_slice(payload);

    send_raw_frame(&frame)
}

/// Build and send an IPv4 packet
pub fn send_ipv4_packet(dst_ip: Ipv4Address, protocol: u8, payload: &[u8]) -> bool {
    let src_ip = get_local_ip();

    // Get destination MAC via ARP
    let dst_mac = if dst_ip.is_broadcast() {
        [0xFF; 6]
    } else if is_local_network(dst_ip) {
        // ARP resolve on local network
        if let Some(mac) = arp_lookup(dst_ip) {
            mac.0
        } else {
            // Send ARP request
            send_arp_request(dst_ip);
            return false; // Packet will need to be retried
        }
    } else {
        // Use gateway MAC
        let gateway = get_gateway();
        if let Some(mac) = arp_lookup(gateway) {
            mac.0
        } else {
            send_arp_request(gateway);
            return false;
        }
    };

    // Build IPv4 header (20 bytes, no options)
    let total_len = (20 + payload.len()) as u16;
    let mut ip_packet = vec![0u8; total_len as usize];

    ip_packet[0] = 0x45; // Version 4, IHL 5
    ip_packet[1] = 0; // DSCP/ECN
    ip_packet[2..4].copy_from_slice(&total_len.to_be_bytes());
    ip_packet[4..6].copy_from_slice(&next_ip_id().to_be_bytes()); // Identification
    ip_packet[6] = 0x40; // Don't Fragment
    ip_packet[7] = 0; // Fragment offset
    ip_packet[8] = 64; // TTL
    ip_packet[9] = protocol;
    // Checksum computed after filling in addresses
    ip_packet[12..16].copy_from_slice(&src_ip.0);
    ip_packet[16..20].copy_from_slice(&dst_ip.0);

    // Compute IP header checksum
    let checksum = ip_checksum(&ip_packet[..20]);
    ip_packet[10..12].copy_from_slice(&checksum.to_be_bytes());

    // Copy payload
    ip_packet[20..].copy_from_slice(payload);

    send_ethernet_frame(dst_mac, 0x0800, &ip_packet)
}

/// Send a UDP datagram
pub fn send_udp(dst_ip: Ipv4Address, src_port: u16, dst_port: u16, data: &[u8]) -> bool {
    let udp_len = (8 + data.len()) as u16;
    let mut udp_packet = vec![0u8; udp_len as usize];

    udp_packet[0..2].copy_from_slice(&src_port.to_be_bytes());
    udp_packet[2..4].copy_from_slice(&dst_port.to_be_bytes());
    udp_packet[4..6].copy_from_slice(&udp_len.to_be_bytes());
    udp_packet[6..8].copy_from_slice(&[0, 0]); // No checksum
    udp_packet[8..].copy_from_slice(data);

    send_ipv4_packet(dst_ip, 17, &udp_packet)
}

/// Send an ICMP echo request (ping)
pub fn send_ping(dst_ip: Ipv4Address, seq: u16) -> bool {
    let mut icmp = vec![0u8; 8 + 56]; // 8 header + 56 data
    icmp[0] = 8; // Type: Echo Request
    icmp[1] = 0; // Code: 0
    // Checksum at [2..4], computed below
    icmp[4..6].copy_from_slice(&0x1234u16.to_be_bytes()); // Identifier
    icmp[6..8].copy_from_slice(&seq.to_be_bytes()); // Sequence

    // Fill data
    for i in 0..56 {
        icmp[8 + i] = i as u8;
    }

    // Compute ICMP checksum
    let checksum = ip_checksum(&icmp);
    icmp[2..4].copy_from_slice(&checksum.to_be_bytes());

    send_ipv4_packet(dst_ip, 1, &icmp) // Protocol 1 = ICMP
}

/// Send an ICMP echo reply
pub fn send_icmp_reply(dst_ip: Ipv4Address, identifier: u16, sequence: u16, data: &[u8]) -> bool {
    let mut icmp = vec![0u8; 8 + data.len()];
    icmp[0] = 0; // Type: Echo Reply
    icmp[1] = 0; // Code: 0
    icmp[4..6].copy_from_slice(&identifier.to_be_bytes());
    icmp[6..8].copy_from_slice(&sequence.to_be_bytes());
    icmp[8..].copy_from_slice(data);

    let checksum = ip_checksum(&icmp);
    icmp[2..4].copy_from_slice(&checksum.to_be_bytes());

    send_ipv4_packet(dst_ip, 1, &icmp)
}

// ─── ARP Integration ───────────────────────────────────────────────────

/// Look up MAC address in ARP cache
fn arp_lookup(ip: Ipv4Address) -> Option<MacAddress> {
    let cache = ARP_CACHE.lock();
    cache.get(&ip.to_u32()).copied()
}

/// Send an ARP request for the given IP
pub fn send_arp_request(target_ip: Ipv4Address) {
    let src_mac = get_mac();
    let src_ip = get_local_ip();

    // ARP packet (28 bytes)
    let mut arp = vec![0u8; 28];
    arp[0..2].copy_from_slice(&1u16.to_be_bytes()); // HTYPE: Ethernet
    arp[2..4].copy_from_slice(&0x0800u16.to_be_bytes()); // PTYPE: IPv4
    arp[4] = 6; // HLEN: MAC = 6 bytes
    arp[5] = 4; // PLEN: IPv4 = 4 bytes
    arp[6..8].copy_from_slice(&1u16.to_be_bytes()); // OPER: Request
    arp[8..14].copy_from_slice(&src_mac); // SHA
    arp[14..18].copy_from_slice(&src_ip.0); // SPA
    arp[18..24].copy_from_slice(&[0; 6]); // THA (unknown)
    arp[24..28].copy_from_slice(&target_ip.0); // TPA

    send_ethernet_frame([0xFF; 6], 0x0806, &arp);
    serial_println!("[NETINT] ARP request: who-has {}?", target_ip);
}

/// Send an ARP reply
pub fn send_arp_reply(dst_mac: [u8; 6], dst_ip: Ipv4Address) {
    let src_mac = get_mac();
    let src_ip = get_local_ip();

    let mut arp = vec![0u8; 28];
    arp[0..2].copy_from_slice(&1u16.to_be_bytes()); // HTYPE: Ethernet
    arp[2..4].copy_from_slice(&0x0800u16.to_be_bytes()); // PTYPE: IPv4
    arp[4] = 6; // HLEN
    arp[5] = 4; // PLEN
    arp[6..8].copy_from_slice(&2u16.to_be_bytes()); // OPER: Reply
    arp[8..14].copy_from_slice(&src_mac); // SHA
    arp[14..18].copy_from_slice(&src_ip.0); // SPA
    arp[18..24].copy_from_slice(&dst_mac); // THA
    arp[24..28].copy_from_slice(&dst_ip.0); // TPA

    send_ethernet_frame(dst_mac, 0x0806, &arp);
}

// ─── TCP Segment Transmission ───────────────────────────────────────────

/// Send a TCP segment
#[allow(clippy::too_many_arguments)]
pub fn send_tcp_segment(
    dst_ip: Ipv4Address,
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u16,
    window: u16,
    data: &[u8],
) -> bool {
    let data_offset = 5u16; // 20 bytes, no options
    let tcp_len = (data_offset as usize * 4) + data.len();
    let mut tcp = vec![0u8; tcp_len];

    tcp[0..2].copy_from_slice(&src_port.to_be_bytes());
    tcp[2..4].copy_from_slice(&dst_port.to_be_bytes());
    tcp[4..8].copy_from_slice(&seq.to_be_bytes());
    tcp[8..12].copy_from_slice(&ack.to_be_bytes());
    let offset_flags = (data_offset << 12) | flags;
    tcp[12..14].copy_from_slice(&offset_flags.to_be_bytes());
    tcp[14..16].copy_from_slice(&window.to_be_bytes());
    // Checksum at [16..18], computed with pseudo-header
    tcp[18..20].copy_from_slice(&[0, 0]); // Urgent pointer

    if !data.is_empty() {
        tcp[20..].copy_from_slice(data);
    }

    // TCP checksum with pseudo-header
    let src_ip = get_local_ip();
    let checksum = tcp_checksum(&src_ip.0, &dst_ip.0, &tcp);
    tcp[16..18].copy_from_slice(&checksum.to_be_bytes());

    send_ipv4_packet(dst_ip, 6, &tcp)
}

// ─── Helpers ────────────────────────────────────────────────────────────

/// Get the local IP address
pub fn get_local_ip() -> Ipv4Address {
    let interfaces = NETWORK_INTERFACES.lock();
    for iface in interfaces.iter() {
        if iface.is_up && iface.name != "lo" {
            return iface.ip;
        }
    }
    Ipv4Address::new(10, 0, 2, 15) // QEMU default
}

/// Get the gateway IP
pub fn get_gateway() -> Ipv4Address {
    let interfaces = NETWORK_INTERFACES.lock();
    for iface in interfaces.iter() {
        if iface.is_up && iface.name != "lo" {
            return iface.gateway;
        }
    }
    Ipv4Address::new(10, 0, 2, 2) // QEMU default
}

/// Check if an IP is on the local network
fn is_local_network(ip: Ipv4Address) -> bool {
    let local = get_local_ip();
    let mask = get_subnet_mask();
    (ip.to_u32() & mask.to_u32()) == (local.to_u32() & mask.to_u32())
}

/// Get subnet mask
fn get_subnet_mask() -> Ipv4Address {
    let interfaces = NETWORK_INTERFACES.lock();
    for iface in interfaces.iter() {
        if iface.is_up && iface.name != "lo" {
            return iface.netmask;
        }
    }
    Ipv4Address::new(255, 255, 255, 0)
}

/// IP identification counter
static IP_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
fn next_ip_id() -> u16 {
    IP_ID_COUNTER.fetch_add(1, Ordering::Relaxed) as u16
}

/// Compute IP header checksum (RFC 1071)
fn ip_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Compute TCP checksum with pseudo-header
fn tcp_checksum(src_ip: &[u8; 4], dst_ip: &[u8; 4], tcp_segment: &[u8]) -> u16 {
    let mut sum: u32 = 0;

    // Pseudo-header
    sum += u16::from_be_bytes([src_ip[0], src_ip[1]]) as u32;
    sum += u16::from_be_bytes([src_ip[2], src_ip[3]]) as u32;
    sum += u16::from_be_bytes([dst_ip[0], dst_ip[1]]) as u32;
    sum += u16::from_be_bytes([dst_ip[2], dst_ip[3]]) as u32;
    sum += 6u32; // Protocol = TCP
    sum += tcp_segment.len() as u32;

    // TCP segment
    let mut i = 0;
    while i + 1 < tcp_segment.len() {
        sum += u16::from_be_bytes([tcp_segment[i], tcp_segment[i + 1]]) as u32;
        i += 2;
    }
    if i < tcp_segment.len() {
        sum += (tcp_segment[i] as u32) << 8;
    }

    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Poll all NICs for received packets
pub fn poll_all_nics() {
    if crate::virtio_net::is_nic_available() {
        crate::virtio_net::handle_interrupt();
    }
    if crate::e1000::is_available() {
        crate::e1000::handle_interrupt();
    }
}

/// Get network statistics
pub fn get_stats() -> NetIntStats {
    NetIntStats {
        tx_packets: TX_PACKETS.load(Ordering::Relaxed),
        rx_packets: RX_PACKETS.load(Ordering::Relaxed),
        tx_bytes: TX_BYTES.load(Ordering::Relaxed),
        rx_bytes: RX_BYTES.load(Ordering::Relaxed),
        tx_errors: TX_ERRORS.load(Ordering::Relaxed),
    }
}

#[derive(Debug)]
pub struct NetIntStats {
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub tx_errors: u64,
}

/// Initialize network integration
pub fn init() {
    INITIALIZED.store(true, Ordering::Relaxed);
    serial_println!("[KnoxOS] Network integration initialized (NIC ↔ TCP/IP wired)");
    if has_nic() {
        let mac = get_mac();
        serial_println!(
            "[NETINT] Active NIC MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac[0],
            mac[1],
            mac[2],
            mac[3],
            mac[4],
            mac[5]
        );
    } else {
        serial_println!("[NETINT] No NIC available — loopback only");
    }
}
