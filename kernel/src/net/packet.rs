use crate::serial_println;

use super::arp::{ArpPacket, arp_insert};
use super::ethernet::{EthernetHeader, MacAddress};
use super::icmp::{ICMP_ECHO_REQUEST, IcmpHeader};
use super::iface::NETWORK_INTERFACES;
use super::ipv4::{Ipv4Address, Ipv4Header};
use super::socket::{SOCKETS, SocketAddress, SocketState, SocketType};
use super::tcp::{TCP_ACK, TCP_RST, TCP_SYN, TcpState};
use super::udp::UdpHeader;

/// Process incoming network packets
pub fn process_packet(data: &[u8]) {
    if data.len() < 14 {
        return;
    } // Minimum Ethernet frame

    // Account for received bytes/packets on the primary interface
    {
        let mut interfaces = NETWORK_INTERFACES.lock();
        if let Some(iface) = interfaces.iter_mut().find(|i| i.is_up && i.name != "lo") {
            iface.rx_bytes += data.len() as u64;
            iface.rx_packets += 1;
        }
    }

    let eth_hdr = unsafe { &*(data.as_ptr() as *const EthernetHeader) };

    match eth_hdr.ether_type_value() {
        0x0806 => process_arp(&data[14..]),
        0x0800 => process_ipv4(&data[14..]),
        _ => {}
    }
}

fn process_arp(data: &[u8]) {
    if data.len() < core::mem::size_of::<ArpPacket>() {
        return;
    }
    let arp = unsafe { &*(data.as_ptr() as *const ArpPacket) };

    // Cache sender's MAC/IP
    let sender_ip = Ipv4Address(arp.spa);
    let sender_mac = MacAddress(arp.sha);
    arp_insert(sender_ip, sender_mac);

    serial_println!("[NET] ARP: {} is at {}", sender_ip, sender_mac);
}

fn process_ipv4(data: &[u8]) {
    if data.len() < 20 {
        return;
    }
    let ip_hdr = unsafe { &*(data.as_ptr() as *const Ipv4Header) };

    let payload_offset = ip_hdr.header_len();
    if payload_offset > data.len() {
        return;
    }
    let payload = &data[payload_offset..];

    // ── Firewall: filter inbound packets on the INPUT chain ─────────
    let src = Ipv4Address(ip_hdr.src_addr);
    let dst = Ipv4Address(ip_hdr.dst_addr);
    let proto = ip_hdr.protocol;
    let (src_port, dst_port) = extract_ports(proto, payload);

    let action = crate::firewall::filter_packet(
        crate::firewall::Chain::Input,
        src,
        dst,
        proto,
        src_port,
        dst_port,
        data.len(),
    );
    match action {
        crate::firewall::Target::Drop | crate::firewall::Target::Reject => {
            serial_println!("[NET] Firewall dropped packet from {} proto={}", src, proto);
            return;
        }
        _ => {} // Accept or Log — continue processing
    }

    match ip_hdr.protocol {
        1 => process_icmp(ip_hdr, payload),
        6 => process_tcp(ip_hdr, payload),
        17 => process_udp(ip_hdr, payload),
        _ => {}
    }
}

/// Extract src/dst ports from transport-layer payload for firewall checks
fn extract_ports(proto: u8, payload: &[u8]) -> (u16, u16) {
    match proto {
        6 | 17
            // TCP and UDP both have src_port and dst_port as first 4 bytes
            if payload.len() >= 4 => {
                let src = u16::from_be(((payload[0] as u16) << 8) | payload[1] as u16);
                let dst = u16::from_be(((payload[2] as u16) << 8) | payload[3] as u16);
                (src, dst)
            }
        _ => (0, 0),
    }
}

fn process_icmp(ip_hdr: &Ipv4Header, data: &[u8]) {
    if data.len() < 8 {
        return;
    }
    let icmp = unsafe { &*(data.as_ptr() as *const IcmpHeader) };

    if icmp.icmp_type == ICMP_ECHO_REQUEST {
        serial_println!(
            "[NET] ICMP Echo Request from {}.{}.{}.{}",
            ip_hdr.src_addr[0],
            ip_hdr.src_addr[1],
            ip_hdr.src_addr[2],
            ip_hdr.src_addr[3]
        );
        // Would send ICMP Echo Reply
    }
}

fn process_tcp(ip_hdr: &Ipv4Header, data: &[u8]) {
    if data.len() < 20 {
        return;
    }
    let src_port = u16::from_be_bytes([data[0], data[1]]);
    let dst_port = u16::from_be_bytes([data[2], data[3]]);
    let seq = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let ack = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    let flags = u16::from_be_bytes([data[12], data[13]]) & 0x1FF;
    let data_offset = ((data[12] >> 4) as usize) * 4;
    let payload = if data_offset < data.len() {
        &data[data_offset..]
    } else {
        &[]
    };

    let window = if data.len() >= 16 {
        u16::from_be_bytes([data[14], data[15]])
    } else {
        0
    };

    let mut sockets = SOCKETS.lock();
    for (_, socket) in sockets.iter_mut() {
        if socket.sock_type != SocketType::Stream {
            continue;
        }
        let Some(SocketAddress::Inet(_, port)) = &socket.local_addr else {
            continue;
        };
        if *port != dst_port {
            continue;
        }

        if flags & TCP_RST != 0 {
            socket.state = SocketState::Closed;
            if let Some(tcb) = socket.tcp_conn.as_mut() {
                tcb.state = TcpState::Closed;
            }
            break;
        }

        if socket.state == SocketState::Connecting && flags & TCP_SYN != 0 && flags & TCP_ACK != 0 {
            if let Some(tcb) = socket.tcp_conn.as_mut() {
                tcb.rcv_nxt = seq.wrapping_add(1);
                let newly = ack.wrapping_sub(tcb.snd_una);
                tcb.snd_una = ack;
                if window != 0 {
                    tcb.snd_wnd = window as u32;
                }
                tcb.cc.on_acked_inflight(newly);
                tcb.cc.on_ack(newly.max(1), 1_000);
                tcb.state = TcpState::Established;
                socket.state = SocketState::Connected;
                let src = tcb.local_port;
                let dst_ip = tcb.remote_addr;
                let dst_p = tcb.remote_port;
                let snd = tcb.snd_nxt;
                let rcv = tcb.rcv_nxt;
                let adv = tcb.advertised_window();
                drop(sockets);
                let _ = crate::netint::send_tcp_segment(
                    dst_ip,
                    src,
                    dst_p,
                    snd,
                    rcv,
                    TCP_ACK,
                    adv,
                    &[],
                );
                return;
            }
        }

        if (socket.state == SocketState::Connected || socket.state == SocketState::Connecting)
            && flags & TCP_ACK != 0
        {
            if let Some(tcb) = socket.tcp_conn.as_mut() {
                let newly = ack.wrapping_sub(tcb.snd_una);
                tcb.snd_una = ack;
                if window != 0 {
                    tcb.snd_wnd = window as u32;
                }
                if newly > 0 && newly < 16 * 1024 * 1024 {
                    tcb.cc.on_acked_inflight(newly);
                    tcb.cc.on_ack(newly, 1_000);
                }
                if !payload.is_empty() {
                    tcb.rcv_nxt = seq.wrapping_add(payload.len() as u32);
                    socket.recv_buf.extend_from_slice(payload);
                }
            } else if !payload.is_empty() {
                socket.recv_buf.extend_from_slice(payload);
            }
        }
        break;
    }
}

fn process_udp(ip_hdr: &Ipv4Header, data: &[u8]) {
    if data.len() < 8 {
        return;
    }
    let udp = unsafe { &*(data.as_ptr() as *const UdpHeader) };
    let src_port = u16::from_be(udp.src_port);
    let dst_port = u16::from_be(udp.dst_port);
    let payload_len = u16::from_be(udp.length).saturating_sub(8) as usize;

    if (src_port == 67 || dst_port == 68) && data.len() >= 8 {
        let end = (8 + payload_len).min(data.len());
        if end > 8 {
            crate::dhcp::process_dhcp_response(&data[8..end]);
        }
    }

    // DNS response: source port 53 from the DNS server
    if src_port == 53 && data.len() >= 8 + payload_len && payload_len > 0 {
        crate::dns::process_dns_response(&data[8..8 + payload_len]);
    }

    // Deliver to matching socket
    let mut sockets = SOCKETS.lock();
    for (_, socket) in sockets.iter_mut() {
        if socket.sock_type == SocketType::Dgram {
            if let Some(SocketAddress::Inet(_, port)) = &socket.local_addr {
                if *port == dst_port {
                    if data.len() >= 8 + payload_len {
                        socket.recv_buf.extend_from_slice(&data[8..8 + payload_len]);
                    }
                    break;
                }
            }
        }
    }
}
