//! Linux socket ABI: address families, socket types, and `sockaddr_*`.

pub const AF_UNIX: u32 = 1;
pub const AF_LOCAL: u32 = AF_UNIX;
pub const AF_INET: u32 = 2;
pub const AF_INET6: u32 = 10;
pub const AF_NETLINK: u32 = 16;

pub const SOCK_STREAM: u32 = 1;
pub const SOCK_DGRAM: u32 = 2;
pub const SOCK_RAW: u32 = 3;

/// Socket domain (address family).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum AddressFamily {
    Unix = AF_UNIX,
    Inet = AF_INET,
    Inet6 = AF_INET6,
    Netlink = AF_NETLINK,
}

impl AddressFamily {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            AF_UNIX => Some(Self::Unix),
            AF_INET => Some(Self::Inet),
            AF_INET6 => Some(Self::Inet6),
            AF_NETLINK => Some(Self::Netlink),
            _ => None,
        }
    }
}

/// Socket type (`SOCK_*`, without `SOCK_CLOEXEC` / `SOCK_NONBLOCK` flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SocketType {
    Stream = SOCK_STREAM,
    Dgram = SOCK_DGRAM,
    Raw = SOCK_RAW,
}

impl SocketType {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            SOCK_STREAM => Some(Self::Stream),
            SOCK_DGRAM => Some(Self::Dgram),
            SOCK_RAW => Some(Self::Raw),
            _ => None,
        }
    }
}

/// `struct sockaddr_in` (IPv4).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockAddrIn {
    pub sin_family: u16,
    pub sin_port: u16,
    pub sin_addr: u32,
    pub sin_zero: [u8; 8],
}

/// `struct sockaddr_un` (pathname Unix-domain).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockAddrUn {
    pub sun_family: u16,
    pub sun_path: [u8; 108],
}
