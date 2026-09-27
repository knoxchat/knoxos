/// Network Stack - TCP/IP networking for Linux compatibility
/// Implements Ethernet, ARP, IP, ICMP, UDP, TCP, and Socket API
///
/// Split into submodules for maintainability:
///   ethernet  — MAC addresses and Ethernet headers
///   ipv4      — IPv4 addresses and headers
///   arp       — ARP packets and cache
///   icmp      — ICMP header and echo constants
///   udp       — UDP header
///   tcp       — TCP header, flags, TCB, retransmit
///   socket    — Socket API types and global table
///   iface     — NetworkInterface and IPv4 config
///   resolver  — hostname cache
///   syscall   — socket syscalls and loopback path
///   packet    — inbound Ethernet/IP dispatch
///   stats     — NetStats and /proc/net listings
///   pool      — TCP connection pooling
///   selftest  — Gate D4 DNS/TCP self-test
use crate::serial_println;

mod arp;
mod ethernet;
mod icmp;
mod iface;
mod ipv4;
mod packet;
mod pool;
mod resolver;
mod selftest;
mod socket;
mod stats;
mod syscall;
mod tcp;
mod udp;

pub use arp::*;
pub use ethernet::*;
pub use icmp::*;
pub use iface::*;
pub use ipv4::*;
pub use packet::*;
pub use pool::*;
pub use resolver::*;
pub use selftest::*;
pub use socket::*;
pub use stats::*;
pub use syscall::*;
pub use tcp::*;
pub use udp::*;

/// Initialize the network stack
pub fn init() {
    serial_println!("[KnoxOS] Network stack initialized");
    let interfaces = NETWORK_INTERFACES.lock();
    for iface in interfaces.iter() {
        serial_println!(
            "[KnoxOS]   {}: {} ({})",
            iface.name,
            iface.ip,
            if iface.is_up { "UP" } else { "DOWN" }
        );
    }
}
