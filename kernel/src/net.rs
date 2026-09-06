/// Network Stack - TCP/IP networking for Linux compatibility
/// Implements Ethernet, ARP, IP, ICMP, UDP, TCP, and Socket API
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// ETHERNET
// ═══════════════════════════════════════════════════════════════════════

/// MAC address (6 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    pub const BROADCAST: MacAddress = MacAddress([0xFF; 6]);
    pub const ZERO: MacAddress = MacAddress([0; 6]);

    pub fn new(b: [u8; 6]) -> Self {
        MacAddress(b)
    }
}

impl core::fmt::Display for MacAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

/// Ethernet frame types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum EtherType {
    IPv4 = 0x0800,
    ARP = 0x0806,
    IPv6 = 0x86DD,
}

/// Ethernet frame header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct EthernetHeader {
    pub dst: [u8; 6],
    pub src: [u8; 6],
    pub ether_type: u16, // Big-endian
}

impl EthernetHeader {
    pub fn ether_type_value(&self) -> u16 {
        u16::from_be(self.ether_type)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// IP (Internet Protocol v4)
// ═══════════════════════════════════════════════════════════════════════

/// IPv4 address
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ipv4Address(pub [u8; 4]);

impl Ipv4Address {
    pub const BROADCAST: Ipv4Address = Ipv4Address([255, 255, 255, 255]);
    pub const UNSPECIFIED: Ipv4Address = Ipv4Address([0, 0, 0, 0]);
    pub const LOOPBACK: Ipv4Address = Ipv4Address([127, 0, 0, 1]);

    pub fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Ipv4Address([a, b, c, d])
    }

    pub fn to_u32(&self) -> u32 {
        u32::from_be_bytes(self.0)
    }

    pub fn from_u32(v: u32) -> Self {
        Ipv4Address(v.to_be_bytes())
    }

    pub fn is_broadcast(&self) -> bool {
        *self == Self::BROADCAST
    }
    pub fn is_loopback(&self) -> bool {
        self.0[0] == 127
    }
    pub fn is_multicast(&self) -> bool {
        self.0[0] >= 224 && self.0[0] <= 239
    }
}

impl core::fmt::Display for Ipv4Address {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

/// IP protocol numbers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IpProtocol {
    ICMP = 1,
    TCP = 6,
    UDP = 17,
}

/// IPv4 header (simplified, no options)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ipv4Header {
    pub version_ihl: u8,     // Version (4) | IHL (4)
    pub tos: u8,             // Type of Service
    pub total_length: u16,   // Big-endian
    pub identification: u16, // Big-endian
    pub flags_fragment: u16, // Big-endian: Flags (3) | Fragment Offset (13)
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16, // Big-endian
    pub src_addr: [u8; 4],
    pub dst_addr: [u8; 4],
}

impl Ipv4Header {
    pub fn new(src: Ipv4Address, dst: Ipv4Address, protocol: IpProtocol, payload_len: u16) -> Self {
        let total_length = 20 + payload_len;
        let mut hdr = Self {
            version_ihl: 0x45, // IPv4, IHL=5 (20 bytes)
            tos: 0,
            total_length: total_length.to_be(),
            identification: 0,
            flags_fragment: 0x4000u16.to_be(), // Don't fragment
            ttl: 64,
            protocol: protocol as u8,
            checksum: 0,
            src_addr: src.0,
            dst_addr: dst.0,
        };
        hdr.checksum = hdr.compute_checksum().to_be();
        hdr
    }

    pub fn ihl(&self) -> u8 {
        self.version_ihl & 0x0F
    }
    pub fn header_len(&self) -> usize {
        (self.ihl() as usize) * 4
    }

    pub fn compute_checksum(&self) -> u16 {
        let ptr = self as *const Self as *const u16;
        let words = self.header_len() / 2;
        let mut sum: u32 = 0;
        for i in 0..words {
            if i == 5 {
                continue;
            } // Skip checksum field
            sum += unsafe { *ptr.add(i) } as u32;
        }
        while sum >> 16 != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        !(sum as u16)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ARP (Address Resolution Protocol)
// ═══════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════
// ICMP (Internet Control Message Protocol)
// ═══════════════════════════════════════════════════════════════════════

/// ICMP header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IcmpHeader {
    pub icmp_type: u8,
    pub code: u8,
    pub checksum: u16,
    pub rest: [u8; 4], // Varies by type
}

pub const ICMP_ECHO_REQUEST: u8 = 8;
pub const ICMP_ECHO_REPLY: u8 = 0;

// ═══════════════════════════════════════════════════════════════════════
// UDP (User Datagram Protocol)
// ═══════════════════════════════════════════════════════════════════════

/// UDP header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UdpHeader {
    pub src_port: u16, // Big-endian
    pub dst_port: u16, // Big-endian
    pub length: u16,   // Big-endian
    pub checksum: u16, // Big-endian
}

impl UdpHeader {
    pub fn new(src_port: u16, dst_port: u16, payload_len: u16) -> Self {
        let length = 8 + payload_len;
        Self {
            src_port: src_port.to_be(),
            dst_port: dst_port.to_be(),
            length: length.to_be(),
            checksum: 0, // Optional in IPv4
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TCP (Transmission Control Protocol)
// ═══════════════════════════════════════════════════════════════════════

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
    pub rcv_wnd: u16, // Receive window size
    pub send_buf: Vec<u8>,
    pub recv_buf: Vec<u8>,
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
            send_buf: Vec::new(),
            recv_buf: Vec::new(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SOCKET API (Linux-compatible)
// ═══════════════════════════════════════════════════════════════════════

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
    pub backlog: Vec<Socket>, // For listening sockets
    pub max_backlog: usize,
    pub nonblocking: bool,
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
#[derive(Debug, Clone)]
pub enum SocketAddress {
    Inet(Ipv4Address, u16),
    Unix(String),
}

static NEXT_SOCKET_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_EPHEMERAL_PORT: AtomicU16 = AtomicU16::new(49152);

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

        self.remote_addr = Some(addr);
        if self.sock_type == SocketType::Stream {
            // TCP: initiate 3-way handshake
            self.state = SocketState::Connecting;
            // In a real implementation, we'd send SYN here
            // For now, simulate immediate connection
            self.state = SocketState::Connected;
        } else {
            self.state = SocketState::Connected;
        }
        Ok(())
    }

    /// Accept an incoming connection (TCP only)
    pub fn accept(&mut self) -> Result<Socket, i32> {
        if self.state != SocketState::Listening {
            return Err(-22); // EINVAL
        }
        if self.backlog.is_empty() {
            return Err(-11); // EAGAIN
        }
        Ok(self.backlog.remove(0))
    }

    /// Send data
    pub fn send(&mut self, data: &[u8]) -> Result<usize, i32> {
        if self.state != SocketState::Connected {
            return Err(-107); // ENOTCONN
        }
        self.send_buf.extend_from_slice(data);
        // Account for transmitted bytes on the primary interface
        {
            let mut interfaces = NETWORK_INTERFACES.lock();
            if let Some(iface) = interfaces.iter_mut().find(|i| i.is_up && i.name != "lo") {
                iface.tx_bytes += data.len() as u64;
                iface.tx_packets += 1;
            }
        }
        Ok(data.len())
    }

    /// Receive data
    pub fn recv(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        if self.state != SocketState::Connected && self.state != SocketState::Closing {
            return Err(-107); // ENOTCONN
        }
        if self.recv_buf.is_empty() {
            if self.nonblocking {
                return Err(-11); // EAGAIN
            }
            return Ok(0); // EOF for blocking
        }
        let to_read = buf.len().min(self.recv_buf.len());
        buf[..to_read].copy_from_slice(&self.recv_buf[..to_read]);
        self.recv_buf.drain(..to_read);
        Ok(to_read)
    }

    /// Send data to a specific address (UDP)
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
                ip: Ipv4Address::new(10, 0, 2, 15),   // QEMU user-mode default
                netmask: Ipv4Address::new(255, 255, 255, 0),
                gateway: Ipv4Address::new(10, 0, 2, 2),
                dns: Ipv4Address::new(10, 0, 2, 3),
                mtu: 1500,
                is_up: false, // Will be activated when driver loads
                rx_bytes: 0,
                tx_bytes: 0,
                rx_packets: 0,
                tx_packets: 0,
            },
        ];

        Mutex::new(interfaces)
    };
}

// ═══════════════════════════════════════════════════════════════════════
// DNS Resolution (stub)
// ═══════════════════════════════════════════════════════════════════════

/// Simple DNS cache
lazy_static::lazy_static! {
    pub static ref DNS_CACHE: Mutex<BTreeMap<String, Ipv4Address>> = {
        let mut cache = BTreeMap::new();
        cache.insert(String::from("localhost"), Ipv4Address::LOOPBACK);
        cache.insert(String::from("knoxos"), Ipv4Address::LOOPBACK);
        Mutex::new(cache)
    };
}

/// Resolve a hostname to an IP address
pub fn dns_resolve(hostname: &str) -> Option<Ipv4Address> {
    DNS_CACHE.lock().get(hostname).copied()
}

// ═══════════════════════════════════════════════════════════════════════
// SOCKET SYSCALLS
// ═══════════════════════════════════════════════════════════════════════

/// Create a socket
pub fn sys_socket(domain: u32, sock_type: u32, protocol: u32) -> Result<u32, i32> {
    let family = AddressFamily::from_u32(domain).ok_or(-97i32)?; // EAFNOSUPPORT
    let stype = SocketType::from_u32(sock_type & 0xFF).ok_or(-94i32)?; // ESOCKTNOSUPPORT
    let socket = Socket::new(family, stype, protocol);
    let id = socket.id;
    SOCKETS.lock().insert(id, socket);
    serial_println!("[KnoxOS] socket({:?}, {:?}) = {}", family, stype, id);
    Ok(id)
}

/// Bind a socket
pub fn sys_bind(sockfd: u32, addr_ptr: u64) -> Result<(), i32> {
    let addr = unsafe { parse_sockaddr(addr_ptr)? };
    let mut sockets = SOCKETS.lock();
    let socket = sockets.get_mut(&sockfd).ok_or(-9i32)?; // EBADF
    socket.bind(addr)
}

/// Listen on a socket
pub fn sys_listen(sockfd: u32, backlog: u32) -> Result<(), i32> {
    let mut sockets = SOCKETS.lock();
    let socket = sockets.get_mut(&sockfd).ok_or(-9i32)?;
    socket.listen(backlog)
}

/// Accept a connection
pub fn sys_accept(sockfd: u32) -> Result<u32, i32> {
    let mut sockets = SOCKETS.lock();
    let socket = sockets.get_mut(&sockfd).ok_or(-9i32)?;
    let new_socket = socket.accept()?;
    let new_id = new_socket.id;
    sockets.insert(new_id, new_socket);
    Ok(new_id)
}

/// Connect to a remote address
pub fn sys_connect(sockfd: u32, addr_ptr: u64) -> Result<(), i32> {
    let addr = unsafe { parse_sockaddr(addr_ptr)? };
    let mut sockets = SOCKETS.lock();
    let socket = sockets.get_mut(&sockfd).ok_or(-9i32)?;
    socket.connect(addr)
}

/// Send data on a socket
pub fn sys_sendto(sockfd: u32, buf: &[u8], addr_ptr: u64) -> Result<usize, i32> {
    let mut sockets = SOCKETS.lock();
    let socket = sockets.get_mut(&sockfd).ok_or(-9i32)?;
    if addr_ptr != 0 {
        let addr = unsafe { parse_sockaddr(addr_ptr)? };
        socket.sendto(buf, &addr)
    } else {
        socket.send(buf)
    }
}

/// Receive data from a socket
pub fn sys_recvfrom(sockfd: u32, buf: &mut [u8]) -> Result<usize, i32> {
    let mut sockets = SOCKETS.lock();
    let socket = sockets.get_mut(&sockfd).ok_or(-9i32)?;
    socket.recv(buf)
}

/// Close a socket
pub fn sys_close_socket(sockfd: u32) -> Result<(), i32> {
    let mut sockets = SOCKETS.lock();
    if let Some(mut socket) = sockets.remove(&sockfd) {
        socket.close();
        Ok(())
    } else {
        Err(-9) // EBADF
    }
}

/// Parse a sockaddr structure from a pointer
unsafe fn parse_sockaddr(ptr: u64) -> Result<SocketAddress, i32> {
    if ptr == 0 {
        return Err(-14);
    } // EFAULT
    let family = *(ptr as *const u16);
    match family {
        2 => {
            // AF_INET
            let addr = &*(ptr as *const SockAddrIn);
            Ok(SocketAddress::Inet(
                Ipv4Address::from_u32(u32::from_be(addr.sin_addr)),
                u16::from_be(addr.sin_port),
            ))
        }
        1 => {
            // AF_UNIX
            let addr = &*(ptr as *const SockAddrUn);
            let len = addr.sun_path.iter().position(|&b| b == 0).unwrap_or(108);
            let path = core::str::from_utf8(&addr.sun_path[..len]).map_err(|_| -22i32)?;
            Ok(SocketAddress::Unix(String::from(path)))
        }
        _ => Err(-97), // EAFNOSUPPORT
    }
}

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
    let tcp = unsafe { &*(data.as_ptr() as *const TcpHeader) };
    let dst_port = u16::from_be(tcp.dst_port);
    let src_port = u16::from_be(tcp.src_port);

    serial_println!(
        "[NET] TCP {}:{} -> {}:{}",
        Ipv4Address(ip_hdr.src_addr),
        src_port,
        Ipv4Address(ip_hdr.dst_addr),
        dst_port
    );

    // Deliver to matching socket
    let mut sockets = SOCKETS.lock();
    for (_, socket) in sockets.iter_mut() {
        if socket.sock_type == SocketType::Stream {
            if let Some(SocketAddress::Inet(_, port)) = &socket.local_addr {
                if *port == dst_port {
                    let data_offset = ((u16::from_be(tcp.data_offset_flags) >> 12) as usize) * 4;
                    if data_offset < data.len() {
                        socket.recv_buf.extend_from_slice(&data[data_offset..]);
                    }
                    break;
                }
            }
        }
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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Connection Pooling   (31.8)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// A pooled TCP connection that can be reused for the same host:port
#[derive(Clone)]
pub struct PooledConnection {
    pub socket_fd: u32,
    pub remote_ip: Ipv4Address,
    pub remote_port: u16,
    /// Tick when the connection was last used
    pub last_used: u64,
    /// Whether this connection is currently in use
    pub in_use: bool,
}

/// Connection pool configuration
pub struct ConnectionPool {
    pub connections: Vec<PooledConnection>,
    pub max_idle: usize,
    pub idle_timeout_ticks: u64,
}

lazy_static::lazy_static! {
    static ref CONN_POOL: Mutex<ConnectionPool> = Mutex::new(ConnectionPool {
        connections: Vec::new(),
        max_idle: 32,
        idle_timeout_ticks: 300_000_000, // ~5 minutes at typical HPET freq
    });
}

/// Get a pooled connection to a remote host, or None if no idle connection exists
pub fn pool_get(ip: Ipv4Address, port: u16) -> Option<u32> {
    let mut pool = CONN_POOL.lock();
    for conn in pool.connections.iter_mut() {
        if !conn.in_use && conn.remote_ip == ip && conn.remote_port == port {
            conn.in_use = true;
            conn.last_used = crate::hpet::read_counter();
            return Some(conn.socket_fd);
        }
    }
    None
}

/// Return a connection to the pool for reuse
pub fn pool_release(socket_fd: u32) {
    let mut pool = CONN_POOL.lock();
    if let Some(conn) = pool
        .connections
        .iter_mut()
        .find(|c| c.socket_fd == socket_fd)
    {
        conn.in_use = false;
        conn.last_used = crate::hpet::read_counter();
    } else {
        // If the socket isn't tracked, check if we should add it
        let sockets = SOCKETS.lock();
        if let Some(sock) = sockets.get(&socket_fd) {
            if let Some(SocketAddress::Inet(ip, port)) = sock.remote_addr.as_ref() {
                if pool.connections.len() < pool.max_idle * 2 {
                    pool.connections.push(PooledConnection {
                        socket_fd,
                        remote_ip: *ip,
                        remote_port: *port,
                        last_used: crate::hpet::read_counter(),
                        in_use: false,
                    });
                }
            }
        }
    }
}

/// Evict idle connections that have exceeded the timeout
pub fn pool_evict_idle() {
    let now = crate::hpet::read_counter();
    let mut pool = CONN_POOL.lock();
    let timeout = pool.idle_timeout_ticks;
    pool.connections.retain(|c| {
        if c.in_use {
            return true;
        }
        if now.saturating_sub(c.last_used) > timeout {
            // Close the socket
            let _ = sys_close_socket(c.socket_fd);
            false
        } else {
            true
        }
    });
}

/// Get pool statistics
pub fn pool_stats() -> (usize, usize) {
    let pool = CONN_POOL.lock();
    let total = pool.connections.len();
    let idle = pool.connections.iter().filter(|c| !c.in_use).count();
    (total, idle)
}
