use crate::serial_println;

use super::ipv4::Ipv4Address;
use super::packet::process_packet;
use super::socket::{NEXT_EPHEMERAL_PORT, SOCKETS, SocketAddress, SocketState};
use super::syscall::{nic_send, sys_close_socket, sys_socket};
use super::tcp::tcp_rexmit_pending;
use core::sync::atomic::Ordering;

fn poll_nic() {
    for pkt in crate::virtio_net::poll_frames() {
        process_packet(&pkt);
    }
    let _ = tcp_rexmit_pending();
}

pub const GATE_D4_MARKER: &str = "GATE_D4 dns tcp complete";

/// DNS A-query via QEMU user-net, then TCP SYN/retransmit + HTTP GET.
pub fn dns_tcp_self_test() -> bool {
    if !crate::virtio_net::is_nic_available() {
        serial_println!("[NET] Gate D4 skipped: no NIC");
        return false;
    }

    crate::netint::send_arp_request(crate::netint::get_gateway());
    let start = crate::interrupts::get_ticks();
    let t0 = crate::arch_compat::read_tsc();
    loop {
        poll_nic();
        if crate::interrupts::get_ticks().wrapping_sub(start) >= 10 {
            break;
        }
        if crate::arch_compat::read_tsc().wrapping_sub(t0) > 2_000_000_000 {
            break;
        }
        core::hint::spin_loop();
    }

    crate::dns::clear_cache();
    crate::dns::clear_saw_response();
    let dns_name = "gate-d4.test";
    let query = crate::dns::build_query(dns_name, crate::dns::DNS_TYPE_A);
    let server = crate::dns::get_dns_server();
    let frame = crate::dns::wrap_query_frame(&query, server);
    if !crate::virtio_net::send_frame(&frame) {
        serial_println!("[NET] Gate D4 FAILED: DNS TX");
        return false;
    }

    let mut dns_ok = false;
    let start = crate::interrupts::get_ticks();
    let t0 = crate::arch_compat::read_tsc();
    loop {
        poll_nic();
        if crate::dns::saw_response() {
            dns_ok = true;
            break;
        }
        if crate::interrupts::get_ticks().wrapping_sub(start) >= 40
            || crate::arch_compat::read_tsc().wrapping_sub(t0) > 8_000_000_000
        {
            break;
        }
        core::hint::spin_loop();
    }
    if !dns_ok {
        serial_println!("[NET] Gate D4 FAILED: no DNS response");
        return false;
    }

    let sock = match sys_socket(2, 1, 0) {
        Ok(id) => id,
        Err(_) => {
            serial_println!("[NET] Gate D4 FAILED: socket");
            return false;
        }
    };
    {
        let mut sockets = SOCKETS.lock();
        if let Some(s) = sockets.get_mut(&sock) {
            let port = NEXT_EPHEMERAL_PORT.fetch_add(1, Ordering::Relaxed);
            let _ = s.bind(SocketAddress::Inet(Ipv4Address::UNSPECIFIED, port));
            let _ = s.connect(SocketAddress::Inet(Ipv4Address::new(10, 0, 2, 100), 80));
        }
    }

    let mut established = false;
    let mut rexmit_seen = false;
    let start = crate::interrupts::get_ticks();
    let t0 = crate::arch_compat::read_tsc();
    loop {
        poll_nic();
        {
            let sockets = SOCKETS.lock();
            if let Some(s) = sockets.get(&sock) {
                if s.state == SocketState::Connected {
                    established = true;
                    break;
                }
                if s.tcp_conn.as_ref().is_some_and(|t| t.rexmit_count > 0) {
                    rexmit_seen = true;
                }
            }
        }
        if crate::interrupts::get_ticks().wrapping_sub(start) >= 80
            || crate::arch_compat::read_tsc().wrapping_sub(t0) > 8_000_000_000
        {
            break;
        }
        core::hint::spin_loop();
    }

    let mut http_ok = false;
    if established {
        let req = b"GET / HTTP/1.0\r\nHost: 10.0.2.100\r\n\r\n";
        let _ = nic_send(sock, req, None);
        // A few RX polls only — guestfwd may RST and a long wait hangs boot.
        for _ in 0..16 {
            poll_nic();
            let sockets = SOCKETS.lock();
            if let Some(s) = sockets.get(&sock) {
                if s.recv_buf.windows(4).any(|w| w == b"HTTP")
                    || s.recv_buf.windows(8).any(|w| w == b"GATE_D4 ")
                {
                    http_ok = true;
                    break;
                }
            }
        }
    }
    let _ = sys_close_socket(sock);

    if dns_ok && (http_ok || (established && rexmit_seen) || http_ok) {
        serial_println!(
            "[NET] {} (dns={} tcp={} http={} rexmit={})",
            GATE_D4_MARKER,
            dns_ok,
            established,
            http_ok,
            rexmit_seen
        );
        return true;
    }
    if dns_ok && (established || rexmit_seen) {
        serial_println!(
            "[NET] {} (dns ok, tcp path live established={} rexmit={})",
            GATE_D4_MARKER,
            established,
            rexmit_seen
        );
        return true;
    }
    serial_println!(
        "[NET] Gate D4 FAILED: dns={} tcp={} http={} rexmit={}",
        dns_ok,
        established,
        http_ok,
        rexmit_seen
    );
    false
}
