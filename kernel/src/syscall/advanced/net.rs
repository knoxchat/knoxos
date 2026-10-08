/// accept4, sendmsg/recvmsg, getsockname/getpeername
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

// ── getsockname / getpeername ───────────────────────────────────────

pub fn sys_getsockname(sockfd: i32, addr_ptr: u64, addrlen_ptr: u64) -> SyscallResult {
    // Look up the socket's local address from the fd table
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
    let file = fd_table
        .get(sockfd)
        .ok_or(SyscallError::BadFileDescriptor)?;
    let sock_id = file.inode;
    drop(tables);

    let sockets = crate::net::SOCKETS.lock();
    if let Some(sock) = sockets.get(&(sock_id as u32)) {
        if addr_ptr != 0 {
            match &sock.local_addr {
                Some(crate::net::SocketAddress::Inet(ip, port)) => {
                    // sockaddr_in: sa_family(2) + port(2) + addr(4) + zero(8)
                    unsafe {
                        core::ptr::write_bytes(addr_ptr as *mut u8, 0, 16);
                        *(addr_ptr as *mut u16) = 2; // AF_INET
                        *((addr_ptr + 2) as *mut u16) = port.to_be();
                        *((addr_ptr + 4) as *mut u32) = u32::from_be_bytes(ip.0);
                    }
                    if addrlen_ptr != 0 {
                        unsafe {
                            *(addrlen_ptr as *mut u32) = 16;
                        }
                    }
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
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
    let file = fd_table
        .get(sockfd)
        .ok_or(SyscallError::BadFileDescriptor)?;
    let sock_id = file.inode;
    drop(tables);

    let sockets = crate::net::SOCKETS.lock();
    if let Some(sock) = sockets.get(&(sock_id as u32)) {
        if sock.state != crate::net::SocketState::Connected {
            return Err(SyscallError::NotConnected);
        }
        if addr_ptr != 0 {
            match &sock.remote_addr {
                Some(crate::net::SocketAddress::Inet(ip, port)) => {
                    unsafe {
                        core::ptr::write_bytes(addr_ptr as *mut u8, 0, 16);
                        *(addr_ptr as *mut u16) = 2; // AF_INET
                        *((addr_ptr + 2) as *mut u16) = port.to_be();
                        *((addr_ptr + 4) as *mut u32) = u32::from_be_bytes(ip.0);
                    }
                    if addrlen_ptr != 0 {
                        unsafe {
                            *(addrlen_ptr as *mut u32) = 16;
                        }
                    }
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
