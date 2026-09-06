use super::{SyscallError, SyscallResult};
/// Syscall implementations — Network socket operations
/// socket, bind, listen, accept, connect, sendto, recvfrom, shutdown,
/// socketpair, sendmsg, recvmsg, getsockname, getpeername
use crate::serial_println;

pub fn sys_socket(domain: i32, sock_type: i32, _protocol: i32) -> SyscallResult {
    // Validate domain
    match domain {
        1 | 2 | 10 | 17 => {} // AF_UNIX, AF_INET, AF_INET6, AF_NETLINK
        _ => return Err(SyscallError::AddressFamilyNotSupported),
    }

    let fd = crate::net::sys_socket(domain as u32, (sock_type & 0xF) as u32, 0)
        .map_err(|_| SyscallError::InvalidArgument)?;
    serial_println!("[KnoxOS] socket({}, {}) = {}", domain, sock_type, fd);
    Ok(fd as u64)
}

pub fn sys_bind(sockfd: i32, addr_ptr: u64, _addrlen: u32) -> SyscallResult {
    crate::net::sys_bind(sockfd as u32, addr_ptr).map_err(|e| match e {
        -98 => SyscallError::AddressInUse,
        -99 => SyscallError::AddressNotAvailable,
        _ => SyscallError::InvalidArgument,
    })?;
    Ok(0)
}

pub fn sys_listen(sockfd: i32, backlog: i32) -> SyscallResult {
    let real_backlog = if backlog < 0 { 128 } else { backlog.min(4096) };
    crate::net::sys_listen(sockfd as u32, real_backlog as u32)
        .map_err(|_| SyscallError::InvalidArgument)?;
    Ok(0)
}

pub fn sys_accept(sockfd: i32, addr_ptr: u64, addrlen_ptr: u64) -> SyscallResult {
    let new_fd = crate::net::sys_accept(sockfd as u32).map_err(|_| SyscallError::WouldBlock)?;
    // Fill in peer address if requested
    if addr_ptr != 0 {
        // Return a zeroed sockaddr_in for now (would need socket tracking for real addr)
        unsafe {
            core::ptr::write_bytes(addr_ptr as *mut u8, 0, 16);
        }
        // sa_family = AF_INET
        unsafe {
            *(addr_ptr as *mut u16) = 2;
        }
    }
    if addrlen_ptr != 0 {
        unsafe {
            *(addrlen_ptr as *mut u32) = 16;
        }
    }
    Ok(new_fd as u64)
}

pub fn sys_connect(sockfd: i32, addr_ptr: u64, _addrlen: u32) -> SyscallResult {
    crate::net::sys_connect(sockfd as u32, addr_ptr).map_err(|e| match e {
        -111 => SyscallError::ConnectionRefused,
        -110 => SyscallError::TimedOut,
        -101 => SyscallError::NetworkUnreachable,
        -113 => SyscallError::HostUnreachable,
        -106 => SyscallError::AlreadyConnected,
        -115 => SyscallError::InProgress,
        _ => SyscallError::IoError,
    })?;
    Ok(0)
}

pub fn sys_sendto(
    sockfd: i32,
    buf_ptr: u64,
    len: usize,
    _flags: i32,
    dest_addr: u64,
    _addrlen: u32,
) -> SyscallResult {
    if buf_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let data = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, len) };
    let sent = crate::net::sys_sendto(sockfd as u32, data, dest_addr).map_err(|e| match e {
        -32 => SyscallError::BrokenPipe,
        -107 => SyscallError::NotConnected,
        _ => SyscallError::IoError,
    })?;
    Ok(sent as u64)
}

pub fn sys_recvfrom(
    sockfd: i32,
    buf_ptr: u64,
    len: usize,
    _flags: i32,
    src_addr: u64,
    addrlen: u64,
) -> SyscallResult {
    if buf_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, len) };
    let received = crate::net::sys_recvfrom(sockfd as u32, buf).map_err(|e| match e {
        -107 => SyscallError::NotConnected,
        -11 => SyscallError::WouldBlock,
        _ => SyscallError::WouldBlock,
    })?;
    // Fill source address if requested
    if src_addr != 0 {
        unsafe {
            core::ptr::write_bytes(src_addr as *mut u8, 0, 16);
        }
        unsafe {
            *(src_addr as *mut u16) = 2;
        } // AF_INET
    }
    if addrlen != 0 {
        unsafe {
            *(addrlen as *mut u32) = 16;
        }
    }
    Ok(received as u64)
}

pub fn sys_shutdown(sockfd: i32, how: i32) -> SyscallResult {
    // Validate how: 0=SHUT_RD, 1=SHUT_WR, 2=SHUT_RDWR
    if !(0..=2).contains(&how) {
        return Err(SyscallError::InvalidArgument);
    }
    crate::net::sys_close_socket(sockfd as u32).map_err(|_| SyscallError::BadFileDescriptor)?;
    Ok(0)
}

pub fn sys_socketpair(domain: i32, sock_type: i32, _protocol: i32, sv: u64) -> SyscallResult {
    if sv == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    if domain != 1 {
        // AF_UNIX only
        return Err(SyscallError::AddressFamilyNotSupported);
    }
    let uds_type = match sock_type & 0xF {
        1 => crate::uds::UnixSocketType::Stream,
        2 => crate::uds::UnixSocketType::Dgram,
        5 => crate::uds::UnixSocketType::SeqPacket,
        _ => return Err(SyscallError::InvalidArgument),
    };
    let (fd1, fd2) = crate::uds::socketpair(uds_type).map_err(|_| SyscallError::TooManyFiles)?;
    let fds = unsafe { &mut *(sv as *mut [u32; 2]) };
    fds[0] = fd1;
    fds[1] = fd2;
    Ok(0)
}

/// sendmsg(sockfd, msg, flags) — send structured message
pub fn sys_sendmsg(sockfd: i32, msg_ptr: u64, flags: i32) -> SyscallResult {
    if msg_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    // Linux msghdr structure
    #[repr(C)]
    struct Msghdr {
        msg_name: u64, // optional address
        msg_namelen: u32,
        _pad1: u32,
        msg_iov: u64,        // scatter/gather array
        msg_iovlen: u64,     // # elements in msg_iov
        msg_control: u64,    // ancillary data
        msg_controllen: u64, // ancillary data length
        msg_flags: i32,      // flags on received message
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Iovec {
        iov_base: u64,
        iov_len: usize,
    }

    let msg = unsafe { &*(msg_ptr as *const Msghdr) };
    let mut total_sent = 0usize;

    if msg.msg_iov != 0 && msg.msg_iovlen > 0 {
        let iovecs = unsafe {
            core::slice::from_raw_parts(msg.msg_iov as *const Iovec, msg.msg_iovlen as usize)
        };
        for iov in iovecs {
            if iov.iov_base != 0 && iov.iov_len > 0 {
                let data =
                    unsafe { core::slice::from_raw_parts(iov.iov_base as *const u8, iov.iov_len) };
                let sent = crate::net::sys_sendto(sockfd as u32, data, msg.msg_name)
                    .map_err(|_| SyscallError::IoError)?;
                total_sent += sent;
            }
        }
    }

    let _ = flags;
    Ok(total_sent as u64)
}

/// recvmsg(sockfd, msg, flags) — receive structured message
pub fn sys_recvmsg(sockfd: i32, msg_ptr: u64, flags: i32) -> SyscallResult {
    if msg_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    #[repr(C)]
    struct Msghdr {
        msg_name: u64,
        msg_namelen: u32,
        _pad1: u32,
        msg_iov: u64,
        msg_iovlen: u64,
        msg_control: u64,
        msg_controllen: u64,
        msg_flags: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Iovec {
        iov_base: u64,
        iov_len: usize,
    }

    let msg = unsafe { &mut *(msg_ptr as *mut Msghdr) };
    let mut total_recv = 0usize;

    if msg.msg_iov != 0 && msg.msg_iovlen > 0 {
        let iovecs = unsafe {
            core::slice::from_raw_parts(msg.msg_iov as *const Iovec, msg.msg_iovlen as usize)
        };
        for iov in iovecs {
            if iov.iov_base != 0 && iov.iov_len > 0 {
                let buf = unsafe {
                    core::slice::from_raw_parts_mut(iov.iov_base as *mut u8, iov.iov_len)
                };
                match crate::net::sys_recvfrom(sockfd as u32, buf) {
                    Ok(n) => {
                        total_recv += n;
                        break; // One recv per call typically
                    }
                    Err(_) => {
                        if total_recv > 0 {
                            break;
                        }
                        return Err(SyscallError::WouldBlock);
                    }
                }
            }
        }
    }

    // Set msg_flags
    msg.msg_flags = 0;
    msg.msg_controllen = 0;

    let _ = flags;
    Ok(total_recv as u64)
}
