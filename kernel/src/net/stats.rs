use alloc::vec::Vec;

use super::iface::NETWORK_INTERFACES;
use super::socket::{SOCKETS, SocketAddress, SocketState, SocketType};

/// Get network statistics
pub fn get_net_stats() -> NetStats {
    let sockets = SOCKETS.lock();
    let interfaces = NETWORK_INTERFACES.lock();
    let mut rx_bytes = 0u64;
    let mut tx_bytes = 0u64;
    let mut rx_packets = 0u64;
    let mut tx_packets = 0u64;
    for iface in interfaces.iter() {
        rx_bytes += iface.rx_bytes;
        tx_bytes += iface.tx_bytes;
        rx_packets += iface.rx_packets;
        tx_packets += iface.tx_packets;
    }
    NetStats {
        interfaces_up: interfaces.iter().filter(|i| i.is_up).count(),
        total_sockets: sockets.len(),
        rx_bytes,
        tx_bytes,
        rx_packets,
        tx_packets,
    }
}

pub struct NetStats {
    pub interfaces_up: usize,
    pub total_sockets: usize,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

/// Socket info for /proc/net/* reporting
pub struct ProcSocketInfo {
    pub local_addr: u32,
    pub local_port: u16,
    pub remote_addr: u32,
    pub remote_port: u16,
    pub state: u8,
    pub uid: u32,
    pub inode: u64,
}

/// List TCP sockets for /proc/net/tcp
pub fn list_tcp_sockets() -> Vec<ProcSocketInfo> {
    let sockets = SOCKETS.lock();
    let mut result = Vec::new();
    for (_, sock) in sockets.iter() {
        if sock.sock_type != SocketType::Stream {
            continue;
        }
        let (la, lp) = match &sock.local_addr {
            Some(SocketAddress::Inet(ip, port)) => {
                let addr = u32::from_be_bytes(ip.0);
                (addr, *port)
            }
            _ => (0u32, 0u16),
        };
        let (ra, rp) = match &sock.remote_addr {
            Some(SocketAddress::Inet(ip, port)) => {
                let addr = u32::from_be_bytes(ip.0);
                (addr, *port)
            }
            _ => (0u32, 0u16),
        };
        let state = match sock.state {
            SocketState::Connected => 1,  // ESTABLISHED
            SocketState::Listening => 10, // LISTEN
            SocketState::Connecting => 2, // SYN_SENT
            SocketState::Closing => 8,    // CLOSE_WAIT
            SocketState::Closed => 7,     // CLOSE
            _ => 0,
        };
        result.push(ProcSocketInfo {
            local_addr: la,
            local_port: lp,
            remote_addr: ra,
            remote_port: rp,
            state,
            uid: 0,
            inode: sock.id as u64,
        });
    }
    result
}

/// List UDP sockets for /proc/net/udp
pub fn list_udp_sockets() -> Vec<ProcSocketInfo> {
    let sockets = SOCKETS.lock();
    let mut result = Vec::new();
    for (_, sock) in sockets.iter() {
        if sock.sock_type != SocketType::Dgram {
            continue;
        }
        let (la, lp) = match &sock.local_addr {
            Some(SocketAddress::Inet(ip, port)) => {
                let addr = u32::from_be_bytes(ip.0);
                (addr, *port)
            }
            _ => (0u32, 0u16),
        };
        result.push(ProcSocketInfo {
            local_addr: la,
            local_port: lp,
            remote_addr: 0,
            remote_port: 0,
            state: 7,
            uid: 0,
            inode: sock.id as u64,
        });
    }
    result
}
