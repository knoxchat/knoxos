/// accept4, sendmsg/recvmsg/sendmmsg/recvmmsg, getsockname/getpeername, getsockopt
use crate::syscall::{SyscallError, SyscallResult};

// ── accept4 ─────────────────────────────────────────────────────────

pub fn sys_accept4(sockfd: i32, addr_ptr: u64, addrlen_ptr: u64, flags: i32) -> SyscallResult {
    const SOCK_CLOEXEC: i32 = 0x80000;
    const SOCK_NONBLOCK: i32 = 0x800;
    let fd = crate::syscall::net::sys_accept(sockfd, addr_ptr, addrlen_ptr)?;
    if flags & (SOCK_CLOEXEC | SOCK_NONBLOCK) != 0 {
        let pid = crate::scheduler::current_pid().unwrap_or(1);
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(fd_table) = tables.get_mut(&pid) {
            if flags & SOCK_CLOEXEC != 0 {
                fd_table.set_cloexec(fd as i32, true);
            }
            if flags & SOCK_NONBLOCK != 0 {
                fd_table.set_nonblock(fd as i32, true);
            }
        }
    }
    if flags & SOCK_NONBLOCK != 0 {
        if let Some(socket) = crate::net::SOCKETS.lock().get_mut(&(fd as u32)) {
            socket.nonblocking = true;
        }
    }
    Ok(fd)
}

// ── recvmsg / sendmsg ───────────────────────────────────────────────

pub fn sys_sendmsg(sockfd: i32, msg_ptr: u64, flags: i32) -> SyscallResult {
    crate::syscall::net::sys_sendmsg(sockfd, msg_ptr, flags)
}

pub fn sys_recvmsg(sockfd: i32, msg_ptr: u64, flags: i32) -> SyscallResult {
    crate::syscall::net::sys_recvmsg(sockfd, msg_ptr, flags)
}

/// Linux `mmsghdr` is a 56-byte `msghdr` plus `msg_len` at offset 56 (64 bytes total).
const MMSGHDR_SIZE: u64 = 64;
const MMSGHDR_LEN_OFF: u64 = 56;
const SENDMMSG_MAX: u32 = 1024;

pub fn sys_sendmmsg(sockfd: i32, msgvec_ptr: u64, vlen: u32, flags: i32) -> SyscallResult {
    if msgvec_ptr == 0 && vlen != 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let n = vlen.min(SENDMMSG_MAX);
    let mut sent = 0u64;
    for i in 0..n {
        let hdr = msgvec_ptr + i as u64 * MMSGHDR_SIZE;
        match crate::syscall::net::sys_sendmsg(sockfd, hdr, flags) {
            Ok(nbytes) => {
                unsafe {
                    *((hdr + MMSGHDR_LEN_OFF) as *mut u32) = nbytes as u32;
                }
                sent += 1;
            }
            Err(e) => {
                if sent == 0 {
                    return Err(e);
                }
                break;
            }
        }
    }
    Ok(sent)
}

pub fn sys_recvmmsg(
    sockfd: i32,
    msgvec_ptr: u64,
    vlen: u32,
    flags: i32,
    timeout_ptr: u64,
) -> SyscallResult {
    let _ = timeout_ptr;
    if msgvec_ptr == 0 && vlen != 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let n = vlen.min(SENDMMSG_MAX);
    let mut got = 0u64;
    for i in 0..n {
        let hdr = msgvec_ptr + i as u64 * MMSGHDR_SIZE;
        match crate::syscall::net::sys_recvmsg(sockfd, hdr, flags) {
            Ok(nbytes) => {
                unsafe {
                    *((hdr + MMSGHDR_LEN_OFF) as *mut u32) = nbytes as u32;
                }
                got += 1;
            }
            Err(e) => {
                if got == 0 {
                    return Err(e);
                }
                break;
            }
        }
    }
    Ok(got)
}

pub fn sys_getsockopt(
    sockfd: i32,
    level: i32,
    optname: i32,
    optval: u64,
    optlen_ptr: u64,
) -> SyscallResult {
    const SOL_SOCKET: i32 = 1;
    const SO_TYPE: i32 = 3;
    const SO_ERROR: i32 = 4;
    const SO_ACCEPTCONN: i32 = 30;

    let sock_id = socket_id_from_fd(sockfd)?;
    let value = {
        let sockets = crate::net::SOCKETS.lock();
        let sock = sockets.get(&sock_id);
        match (level, optname) {
            (SOL_SOCKET, SO_TYPE) => sock.map(|s| s.sock_type as i32).unwrap_or(0),
            (SOL_SOCKET, SO_ERROR) => 0,
            (SOL_SOCKET, SO_ACCEPTCONN) => {
                if sock.is_some_and(|s| s.state == crate::net::SocketState::Listening) {
                    1
                } else {
                    0
                }
            }
            _ => 0,
        }
    };
    if optval != 0 {
        unsafe {
            *(optval as *mut i32) = value;
        }
    }
    if optlen_ptr != 0 {
        unsafe {
            *(optlen_ptr as *mut u32) = 4;
        }
    }
    Ok(0)
}

// ── getsockname / getpeername ───────────────────────────────────────

/// `socket()` returns the `SOCKETS` id as the userspace fd. Fall back to the
/// process fd table (Unix sockets / accept4 CLOEXEC wrappers) when needed.
fn socket_id_from_fd(sockfd: i32) -> Result<u32, SyscallError> {
    {
        let sockets = crate::net::SOCKETS.lock();
        if sockets.contains_key(&(sockfd as u32)) {
            return Ok(sockfd as u32);
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
    let file = fd_table
        .get(sockfd)
        .ok_or(SyscallError::BadFileDescriptor)?;
    Ok(file.inode as u32)
}

fn write_inet_addr(addr_ptr: u64, addrlen_ptr: u64, ip: crate::net::Ipv4Address, port: u16) {
    unsafe {
        core::ptr::write_bytes(addr_ptr as *mut u8, 0, 16);
        *(addr_ptr as *mut u16) = 2; // AF_INET
        *((addr_ptr + 2) as *mut u16) = port.to_be();
        core::ptr::copy_nonoverlapping(ip.0.as_ptr(), (addr_ptr + 4) as *mut u8, 4);
    }
    if addrlen_ptr != 0 {
        unsafe {
            *(addrlen_ptr as *mut u32) = 16;
        }
    }
}

pub fn sys_getsockname(sockfd: i32, addr_ptr: u64, addrlen_ptr: u64) -> SyscallResult {
    let sock_id = socket_id_from_fd(sockfd)?;

    let sockets = crate::net::SOCKETS.lock();
    if let Some(sock) = sockets.get(&sock_id) {
        if addr_ptr != 0 {
            match &sock.local_addr {
                Some(crate::net::SocketAddress::Inet(ip, port)) => {
                    write_inet_addr(addr_ptr, addrlen_ptr, *ip, *port);
                }
                Some(crate::net::SocketAddress::Unix(path)) => {
                    let path_bytes = path.as_bytes();
                    let copy_len = path_bytes.len().min(107);
                    unsafe {
                        core::ptr::write_bytes(addr_ptr as *mut u8, 0, 110);
                        *(addr_ptr as *mut u16) = 1; // AF_UNIX
                        core::ptr::copy_nonoverlapping(
                            path_bytes.as_ptr(),
                            (addr_ptr + 2) as *mut u8,
                            copy_len,
                        );
                    }
                    if addrlen_ptr != 0 {
                        unsafe {
                            *(addrlen_ptr as *mut u32) = (2 + copy_len + 1) as u32;
                        }
                    }
                }
                None => {
                    // Unbound socket: return zeroed address
                    if addr_ptr != 0 {
                        unsafe {
                            core::ptr::write_bytes(addr_ptr as *mut u8, 0, 16);
                        }
                    }
                    if addrlen_ptr != 0 {
                        unsafe {
                            *(addrlen_ptr as *mut u32) = 0;
                        }
                    }
                }
            }
        }
        Ok(0)
    } else {
        // Fallback: return zeroed for sockets not in SOCKETS table
        if addr_ptr != 0 {
            unsafe {
                core::ptr::write_bytes(addr_ptr as *mut u8, 0, 16);
            }
        }
        if addrlen_ptr != 0 {
            unsafe {
                *(addrlen_ptr as *mut u32) = 16;
            }
        }
        Ok(0)
    }
}

pub fn sys_getpeername(sockfd: i32, addr_ptr: u64, addrlen_ptr: u64) -> SyscallResult {
    let sock_id = socket_id_from_fd(sockfd)?;

    let sockets = crate::net::SOCKETS.lock();
    if let Some(sock) = sockets.get(&sock_id) {
        if sock.state != crate::net::SocketState::Connected {
            return Err(SyscallError::NotConnected);
        }
        if addr_ptr != 0 {
            match &sock.remote_addr {
                Some(crate::net::SocketAddress::Inet(ip, port)) => {
                    write_inet_addr(addr_ptr, addrlen_ptr, *ip, *port);
                }
                Some(crate::net::SocketAddress::Unix(path)) => {
                    let path_bytes = path.as_bytes();
                    let copy_len = path_bytes.len().min(107);
                    unsafe {
                        core::ptr::write_bytes(addr_ptr as *mut u8, 0, 110);
                        *(addr_ptr as *mut u16) = 1; // AF_UNIX
                        core::ptr::copy_nonoverlapping(
                            path_bytes.as_ptr(),
                            (addr_ptr + 2) as *mut u8,
                            copy_len,
                        );
                    }
                    if addrlen_ptr != 0 {
                        unsafe {
                            *(addrlen_ptr as *mut u32) = (2 + copy_len + 1) as u32;
                        }
                    }
                }
                None => {
                    return Err(SyscallError::NotConnected);
                }
            }
        }
        Ok(0)
    } else {
        Err(SyscallError::BadFileDescriptor)
    }
}
