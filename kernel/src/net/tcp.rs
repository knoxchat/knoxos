use alloc::vec::Vec;

use super::arp::arp_lookup;
use super::ipv4::Ipv4Address;
use super::socket::{SOCKETS, SocketState};

/// TCP header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct TcpHeader {
    pub src_port: u16,          // Big-endian
    pub dst_port: u16,          // Big-endian
    pub seq_num: u32,           // Big-endian
    pub ack_num: u32,           // Big-endian
    pub data_offset_flags: u16, // Big-endian: Data Offset (4) | Reserved (3) | Flags (9)
    pub window: u16,            // Big-endian
    pub checksum: u16,          // Big-endian
    pub urgent_ptr: u16,        // Big-endian
}

/// TCP flags
pub const TCP_FIN: u16 = 0x001;
pub const TCP_SYN: u16 = 0x002;
pub const TCP_RST: u16 = 0x004;
pub const TCP_PSH: u16 = 0x008;
pub const TCP_ACK: u16 = 0x010;
pub const TCP_URG: u16 = 0x020;

impl TcpHeader {
    pub fn new(src_port: u16, dst_port: u16, seq: u32, ack: u32, flags: u16) -> Self {
        Self {
            src_port: src_port.to_be(),
            dst_port: dst_port.to_be(),
            seq_num: seq.to_be(),
            ack_num: ack.to_be(),
            data_offset_flags: ((5u16 << 12) | flags).to_be(), // 5 words = 20 bytes
            window: 65535u16.to_be(),
            checksum: 0,
            urgent_ptr: 0,
        }
    }

    pub fn flags(&self) -> u16 {
        u16::from_be(self.data_offset_flags) & 0x1FF
    }

    pub fn seq(&self) -> u32 {
        u32::from_be(self.seq_num)
    }
    pub fn ack(&self) -> u32 {
        u32::from_be(self.ack_num)
    }
}

/// TCP connection state (RFC 793)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
}

/// A TCP connection (Transmission Control Block)
pub struct TcpConnection {
    pub state: TcpState,
    pub local_addr: Ipv4Address,
    pub local_port: u16,
    pub remote_addr: Ipv4Address,
    pub remote_port: u16,
    pub snd_nxt: u32, // Next sequence number to send
    pub snd_una: u32, // Oldest unacknowledged sequence number
    pub rcv_nxt: u32, // Next expected sequence number
    pub rcv_wnd: u16, // Receive window we advertise
    pub snd_wnd: u32, // Peer receive window
    pub cc: crate::net_production::CongestionState,
    pub send_buf: Vec<u8>,
    pub recv_buf: Vec<u8>,
    pub last_tx_ticks: u64,
    pub rexmit_count: u32,
    /// False when the last SYN/data TX was dropped (typically ARP miss).
    pub last_tx_ok: bool,
}

impl TcpConnection {
    pub fn new(local_addr: Ipv4Address, local_port: u16) -> Self {
        Self {
            state: TcpState::Closed,
            local_addr,
            local_port,
            remote_addr: Ipv4Address::UNSPECIFIED,
            remote_port: 0,
            snd_nxt: 1000, // Initial sequence number
            snd_una: 1000,
            rcv_nxt: 0,
            rcv_wnd: 65535,
            snd_wnd: 65535,
            cc: crate::net_production::CongestionState::default(),
            send_buf: Vec::new(),
            recv_buf: Vec::new(),
            last_tx_ticks: 0,
            rexmit_count: 0,
            last_tx_ok: true,
        }
    }

    pub(super) fn advertised_window(&self) -> u16 {
        self.rcv_wnd
    }

    pub(super) fn send_limit(&self) -> u32 {
        self.cc.send_window().min(self.snd_wnd.max(1460))
    }
}

/// Retransmit unacked SYNs (Gate D4 RTO).
pub fn tcp_rexmit_pending() -> u32 {
    let now = crate::interrupts::get_ticks();
    let mut pending = Vec::new();
    {
        let mut sockets = SOCKETS.lock();
        for (sock_id, socket) in sockets.iter_mut() {
            if socket.state != SocketState::Connecting {
                continue;
            }
            let Some(tcb) = socket.tcp_conn.as_mut() else {
                continue;
            };
            if tcb.state != TcpState::SynSent {
                continue;
            }
            let rto_due = now.wrapping_sub(tcb.last_tx_ticks) >= 5;
            if tcb.last_tx_ok {
                if !rto_due {
                    continue;
                }
            } else if arp_lookup(tcb.remote_addr).is_none() && !rto_due {
                continue;
            }
            tcb.rexmit_count = tcb.rexmit_count.saturating_add(1);
            tcb.last_tx_ticks = now;
            tcb.cc.on_loss();
            pending.push((
                *sock_id,
                tcb.remote_addr,
                tcb.local_port,
                tcb.remote_port,
                tcb.snd_una,
                tcb.advertised_window(),
            ));
        }
    }
    for (sock_id, dst, src_port, dst_port, seq, window) in &pending {
        let ok = crate::netint::send_tcp_segment(
            *dst,
            *src_port,
            *dst_port,
            *seq,
            0,
            TCP_SYN,
            *window,
            &[],
        );
        if let Some(socket) = SOCKETS.lock().get_mut(sock_id) {
            if let Some(tcb) = socket.tcp_conn.as_mut() {
                tcb.last_tx_ok = ok;
            }
        }
    }
    pending.len() as u32
}
