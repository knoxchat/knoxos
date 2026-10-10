use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use spin::Mutex;

use super::ipv4::Ipv4Address;
use super::tcp::{TCP_SYN, TcpConnection, TcpState};

/// Socket domain (address family)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum AddressFamily {
    Unix = 1,     // AF_UNIX / AF_LOCAL
    Inet = 2,     // AF_INET (IPv4)
    Inet6 = 10,   // AF_INET6 (IPv6)
    Netlink = 16, // AF_NETLINK
}

impl AddressFamily {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            1 => Some(Self::Unix),
            2 => Some(Self::Inet),
            10 => Some(Self::Inet6),
            16 => Some(Self::Netlink),
            _ => None,
        }
    }
}

/// Socket type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SocketType {
    Stream = 1, // SOCK_STREAM (TCP)
    Dgram = 2,  // SOCK_DGRAM (UDP)
    Raw = 3,    // SOCK_RAW
}

impl SocketType {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            1 => Some(Self::Stream),
            2 => Some(Self::Dgram),
            3 => Some(Self::Raw),
            _ => None,
        }
    }
}

/// Socket address for IPv4 (struct sockaddr_in)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockAddrIn {
    pub sin_family: u16,
    pub sin_port: u16, // Big-endian
    pub sin_addr: u32, // Big-endian
    pub sin_zero: [u8; 8],
}

/// Socket address for Unix domain (struct sockaddr_un)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct SockAddrUn {
    pub sun_family: u16,
    pub sun_path: [u8; 108],
}

/// A network socket
pub struct Socket {
    pub id: u32,
    pub family: AddressFamily,
    pub sock_type: SocketType,
    pub protocol: u32,
    pub state: SocketState,
    pub local_addr: Option<SocketAddress>,
    pub remote_addr: Option<SocketAddress>,
    pub recv_buf: Vec<u8>,
    pub send_buf: Vec<u8>,
    pub tcp_conn: Option<TcpConnection>,
    /// Pending accepted connection IDs (already inserted into `SOCKETS`).
    pub backlog: Vec<u32>,
    pub max_backlog: usize,
    pub nonblocking: bool,
    /// Connected peer socket id (TCP loopback / paired sockets).
    pub peer_id: Option<u32>,
    /// `SO_REUSEADDR` — allow bind when the local address is already in use.
    pub reuseaddr: bool,
}

/// Socket state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    Unbound,
    Bound,
    Listening,
    Connecting,
    Connected,
    Closing,
    Closed,
}

/// Generic socket address
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketAddress {
    Inet(Ipv4Address, u16),
    Unix(String),
}

static NEXT_SOCKET_ID: AtomicU32 = AtomicU32::new(1);
pub(super) static NEXT_EPHEMERAL_PORT: AtomicU16 = AtomicU16::new(49152);

impl Socket {
    pub fn new(family: AddressFamily, sock_type: SocketType, protocol: u32) -> Self {
        Self {
            id: NEXT_SOCKET_ID.fetch_add(1, Ordering::Relaxed),
            family,
            sock_type,
            protocol,
            state: SocketState::Unbound,
            local_addr: None,
            remote_addr: None,
            recv_buf: Vec::new(),
            send_buf: Vec::new(),
            tcp_conn: None,
            backlog: Vec::new(),
            max_backlog: 128,
            nonblocking: false,
            peer_id: None,
            reuseaddr: false,
        }
    }

    /// Bind socket to an address
    pub fn bind(&mut self, addr: SocketAddress) -> Result<(), i32> {
        if self.state != SocketState::Unbound {
            return Err(-22); // EINVAL
        }
        self.local_addr = Some(addr);
        self.state = SocketState::Bound;
        Ok(())
    }

    /// Listen for incoming connections (TCP only)
    pub fn listen(&mut self, backlog: u32) -> Result<(), i32> {
        if self.sock_type != SocketType::Stream {
            return Err(-95); // EOPNOTSUPP
        }
        if self.state != SocketState::Bound {
            return Err(-22); // EINVAL
        }
        self.max_backlog = backlog as usize;
        self.state = SocketState::Listening;
        Ok(())
    }

    /// Connect to a remote address
    pub fn connect(&mut self, addr: SocketAddress) -> Result<(), i32> {
        // Auto-bind if not bound
        if self.state == SocketState::Unbound {
            let port = NEXT_EPHEMERAL_PORT.fetch_add(1, Ordering::Relaxed);
            if self.family == AddressFamily::Inet {
                self.local_addr = Some(SocketAddress::Inet(Ipv4Address::UNSPECIFIED, port));
            }
        }

        self.remote_addr = Some(addr.clone());
        if self.sock_type == SocketType::Stream {
            if let SocketAddress::Inet(dst_ip, dst_port) = addr {
                let src_port = match self.local_addr {
                    Some(SocketAddress::Inet(_, p)) => p,
                    _ => NEXT_EPHEMERAL_PORT.fetch_add(1, Ordering::Relaxed),
                };
                let isn = crate::random::random_u32();
                let mut tcb = TcpConnection::new(crate::netint::get_local_ip(), src_port);
                tcb.remote_addr = dst_ip;
                tcb.remote_port = dst_port;
                tcb.state = TcpState::SynSent;
                tcb.snd_nxt = isn.wrapping_add(1);
                tcb.snd_una = isn;
                tcb.last_tx_ticks = crate::interrupts::get_ticks();
                tcb.rexmit_count = 0;
                tcb.last_tx_ok = crate::netint::send_tcp_segment(
                    dst_ip,
                    src_port,
                    dst_port,
                    isn,
                    0,
                    TCP_SYN,
                    tcb.advertised_window(),
                    &[],
                );
                self.tcp_conn = Some(tcb);
                self.state = SocketState::Connecting;
            } else {
                self.state = SocketState::Connected;
            }
        } else {
            self.state = SocketState::Connected;
        }
        Ok(())
    }

    /// Accept an incoming connection (TCP only). Returns the new socket id.
    pub fn accept(&mut self) -> Result<u32, i32> {
        if self.state != SocketState::Listening {
            return Err(-22); // EINVAL
        }
        if self.backlog.is_empty() {
            return Err(-11); // EAGAIN
        }
        Ok(self.backlog.remove(0))
    }

    fn can_recv(&self) -> bool {
        match self.sock_type {
            SocketType::Stream => {
                self.state == SocketState::Connected || self.state == SocketState::Closing
            }
            SocketType::Dgram | SocketType::Raw => {
                self.state == SocketState::Bound
                    || self.state == SocketState::Connected
                    || self.state == SocketState::Closing
            }
        }
    }

    /// Send data (connected socket). Delivery is handled by `sys_sendto`.
    pub fn send(&mut self, data: &[u8]) -> Result<usize, i32> {
        if self.sock_type == SocketType::Stream && self.state != SocketState::Connected {
            return Err(-107); // ENOTCONN
        }
        self.send_buf.extend_from_slice(data);
        Ok(data.len())
    }

    /// Receive data
    pub fn recv(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        if !self.can_recv() {
            return Err(-107); // ENOTCONN
        }
        if self.recv_buf.is_empty() {
            if self.nonblocking {
                return Err(-11); // EAGAIN
            }
            return Ok(0); // EOF / no datagram yet
        }
        let to_read = buf.len().min(self.recv_buf.len());
        buf[..to_read].copy_from_slice(&self.recv_buf[..to_read]);
        self.recv_buf.drain(..to_read);
        Ok(to_read)
    }

    /// Send data to a specific address (UDP). Delivery is handled by `sys_sendto`.
    pub fn sendto(&mut self, data: &[u8], addr: &SocketAddress) -> Result<usize, i32> {
        self.remote_addr = Some(addr.clone());
        self.send_buf.extend_from_slice(data);
        Ok(data.len())
    }

    /// Receive data with sender address (UDP)
    pub fn recvfrom(&mut self, buf: &mut [u8]) -> Result<(usize, Option<SocketAddress>), i32> {
        let n = self.recv(buf)?;
        Ok((n, self.remote_addr.clone()))
    }

    /// Close the socket
    pub fn close(&mut self) {
        self.state = SocketState::Closed;
    }

    /// Set socket to non-blocking mode
    pub fn set_nonblocking(&mut self, nonblocking: bool) {
        self.nonblocking = nonblocking;
    }
}

/// Global socket table
lazy_static::lazy_static! {
    pub static ref SOCKETS: Mutex<BTreeMap<u32, Socket>> = Mutex::new(BTreeMap::new());
}
