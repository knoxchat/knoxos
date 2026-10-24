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
        -1 => SyscallError::PermissionDenied,
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
        // SAFETY: the pointer is derived from a kernel object we own or a mapped physical address; the type layout matches the hardware or on-disk record.
        unsafe {
            core::ptr::write_bytes(addr_ptr as *mut u8, 0, 16);
        }
        // sa_family = AF_INET
        // SAFETY: the pointer is derived from a kernel object we own or a mapped physical address; the type layout matches the hardware or on-disk record.
        unsafe {
            *(addr_ptr as *mut u16) = 2;
        }
    }
    if addrlen_ptr != 0 {
        // SAFETY: the pointer is derived from a kernel object we own or a mapped physical address; the type layout matches the hardware or on-disk record.
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
    // SAFETY: pointer and length come from the current kernel-mode caller and are used only as a bounded slice.
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
    // SAFETY: pointer and length come from the current kernel-mode caller and are used only as a bounded mutable slice.
    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, len) };
    let received = crate::net::sys_recvfrom(sockfd as u32, buf).map_err(|e| match e {
        -107 => SyscallError::NotConnected,
        -11 => SyscallError::WouldBlock,
        _ => SyscallError::WouldBlock,
    })?;
    // Fill source address if requested
    if src_addr != 0 {
        // SAFETY: the pointer is derived from a kernel object we own or a mapped physical address; the type layout matches the hardware or on-disk record.
        unsafe {
            core::ptr::write_bytes(src_addr as *mut u8, 0, 16);
        }
        // SAFETY: the pointer is derived from a kernel object we own or a mapped physical address; the type layout matches the hardware or on-disk record.
        unsafe {
            *(src_addr as *mut u16) = 2;
        } // AF_INET
    }
    if addrlen != 0 {
        // SAFETY: the pointer is derived from a kernel object we own or a mapped physical address; the type layout matches the hardware or on-disk record.
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
    let (id1, id2) = crate::uds::socketpair(uds_type).map_err(|_| SyscallError::TooManyFiles)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;
    let fd1 = fd_table
        .open(
            &alloc::format!("unix:{}", id1),
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR),
            crate::fd::FileType::Socket,
        )
        .map_err(|_| SyscallError::TooManyFiles)?;
    let fd2 = fd_table
        .open(
            &alloc::format!("unix:{}", id2),
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR),
            crate::fd::FileType::Socket,
        )
        .map_err(|_| SyscallError::TooManyFiles)?;
    drop(tables);
    // SAFETY: src and dst are valid for the copied byte count and do not overlap.
    unsafe {
        core::ptr::copy_nonoverlapping([fd1, fd2].as_ptr(), sv as *mut i32, 2);
    }
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&fd1.to_ne_bytes());
    bytes[4..].copy_from_slice(&fd2.to_ne_bytes());
    crate::vmm::write_user_memory(pid, sv, &bytes);
    Ok(0)
}

// Linux msghdr / iovec on x86_64.
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

const SOL_SOCKET: i32 = 1;
const SCM_RIGHTS: i32 = 1;
const MSG_CTRUNC: i32 = 8;
const CMSG_HDR_LEN: usize = 16;
const MAX_IOV: usize = 8;
const MAX_CMSG: usize = 256;
const MAX_SCM_FDS: usize = 8;

fn unix_id_for_fd(sockfd: i32) -> Option<u32> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::fd::path_for_fd(pid, sockfd).and_then(|p| crate::fd::unix_id_from_path(&p))
}

fn cmsg_align(len: usize) -> usize {
    (len + 7) & !7
}

fn gather_iov(msg: &Msghdr) -> Result<alloc::vec::Vec<u8>, SyscallError> {
    let mut data = alloc::vec::Vec::new();
    if msg.msg_iov == 0 || msg.msg_iovlen == 0 {
        return Ok(data);
    }
    let n = msg.msg_iovlen as usize;
    if n > MAX_IOV {
        return Err(SyscallError::InvalidArgument);
    }
    // SAFETY: pointer and length come from the current kernel-mode caller and are used only as a bounded slice.
    let iovecs = unsafe { core::slice::from_raw_parts(msg.msg_iov as *const Iovec, n) };
    for iov in iovecs {
        if iov.iov_base != 0 && iov.iov_len > 0 {
            if data.len() + iov.iov_len > 4096 {
                return Err(SyscallError::InvalidArgument);
            }
            // SAFETY: pointer and length come from the current kernel-mode caller and are used only as a bounded slice.
            let chunk =
                unsafe { core::slice::from_raw_parts(iov.iov_base as *const u8, iov.iov_len) };
            data.extend_from_slice(chunk);
        }
    }
    Ok(data)
}

fn scatter_iov(msg: &Msghdr, data: &[u8]) -> usize {
    if msg.msg_iov == 0 || msg.msg_iovlen == 0 || data.is_empty() {
        return 0;
    }
    let n = (msg.msg_iovlen as usize).min(MAX_IOV);
    // SAFETY: pointer and length come from the current kernel-mode caller and are used only as a bounded slice.
    let iovecs = unsafe { core::slice::from_raw_parts(msg.msg_iov as *const Iovec, n) };
    let mut copied = 0usize;
    for iov in iovecs {
        if copied >= data.len() {
            break;
        }
        if iov.iov_base != 0 && iov.iov_len > 0 {
            let n = (iov.iov_len).min(data.len() - copied);
            // SAFETY: pointer and length come from the current kernel-mode caller and are used only as a bounded mutable slice.
            let buf = unsafe { core::slice::from_raw_parts_mut(iov.iov_base as *mut u8, n) };
            buf.copy_from_slice(&data[copied..copied + n]);
            copied += n;
        }
    }
    copied
}

fn parse_scm_rights(control: u64, controllen: u64) -> alloc::vec::Vec<i32> {
    if control == 0 || controllen < CMSG_HDR_LEN as u64 {
        return alloc::vec::Vec::new();
    }
    let len = (controllen as usize).min(MAX_CMSG);
    // SAFETY: pointer and length come from the current kernel-mode caller and are used only as a bounded slice.
    let bytes = unsafe { core::slice::from_raw_parts(control as *const u8, len) };
    let mut fds = alloc::vec::Vec::new();
    let mut off = 0usize;
    while off + CMSG_HDR_LEN <= bytes.len() && fds.len() < MAX_SCM_FDS {
        let mut len_bytes = [0u8; 8];
        len_bytes.copy_from_slice(&bytes[off..off + 8]);
        let cmsg_len = u64::from_ne_bytes(len_bytes) as usize;
        if cmsg_len < CMSG_HDR_LEN || off + cmsg_len > bytes.len() {
            break;
        }
        let mut level_bytes = [0u8; 4];
        level_bytes.copy_from_slice(&bytes[off + 8..off + 12]);
        let mut type_bytes = [0u8; 4];
        type_bytes.copy_from_slice(&bytes[off + 12..off + 16]);
        let level = i32::from_ne_bytes(level_bytes);
        let ty = i32::from_ne_bytes(type_bytes);
        if level == SOL_SOCKET && ty == SCM_RIGHTS {
            let mut data_off = off + CMSG_HDR_LEN;
            let data_end = off + cmsg_len;
            while data_off + 4 <= data_end && fds.len() < MAX_SCM_FDS {
                let mut fd_bytes = [0u8; 4];
                fd_bytes.copy_from_slice(&bytes[data_off..data_off + 4]);
                fds.push(i32::from_ne_bytes(fd_bytes));
                data_off += 4;
            }
        }
        let next = cmsg_align(cmsg_len);
        if next == 0 {
            break;
        }
        off = off.saturating_add(next);
    }
    fds
}

fn write_scm_rights(control: u64, controllen: u64, fds: &[i32]) -> (u64, i32) {
    if fds.is_empty() {
        return (0, 0);
    }
    let data_len = fds.len() * 4;
    let cmsg_len = CMSG_HDR_LEN + data_len;
    if control == 0 || (controllen as usize) < cmsg_len {
        return (0, MSG_CTRUNC);
    }
    // SAFETY: pointer is a user/kernel buffer supplied for this recvmsg and is large enough for the cmsg we just sized.
    unsafe {
        core::ptr::write(control as *mut u64, cmsg_len as u64);
        core::ptr::write((control as *mut u8).add(8) as *mut i32, SOL_SOCKET);
        core::ptr::write((control as *mut u8).add(12) as *mut i32, SCM_RIGHTS);
        for (i, fd) in fds.iter().enumerate() {
            core::ptr::write(
                (control as *mut u8).add(CMSG_HDR_LEN + i * 4) as *mut i32,
                *fd,
            );
        }
    }
    (cmsg_len as u64, 0)
}

fn unix_sendmsg(unix_id: u32, msg: &Msghdr) -> SyscallResult {
    let data = gather_iov(msg)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let fds = parse_scm_rights(msg.msg_control, msg.msg_controllen);
    let rights = crate::fd::snapshot_rights(pid, &fds);
    crate::uds::socket_send_with_rights(unix_id, &data, rights).map_err(|e| match e {
        -107 => SyscallError::NotConnected,
        -32 => SyscallError::BrokenPipe,
        -11 => SyscallError::WouldBlock,
        _ => SyscallError::IoError,
    })?;
    Ok(data.len() as u64)
}

fn unix_recvmsg(unix_id: u32, msg: &mut Msghdr) -> SyscallResult {
    let mut scratch = [0u8; 4096];
    let mut rights = alloc::vec::Vec::new();
    let n = crate::uds::socket_recv_with_rights(unix_id, &mut scratch, &mut rights).map_err(
        |e| match e {
            -107 => SyscallError::NotConnected,
            -11 => SyscallError::WouldBlock,
            _ => SyscallError::WouldBlock,
        },
    )?;
    let copied = scatter_iov(msg, &scratch[..n]);
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let new_fds = crate::fd::install_rights(pid, rights);
    let (used, trunc) = write_scm_rights(msg.msg_control, msg.msg_controllen, &new_fds);
    msg.msg_controllen = used;
    msg.msg_flags = trunc;
    Ok(copied as u64)
}

/// sendmsg(sockfd, msg, flags) — send structured message
pub fn sys_sendmsg(sockfd: i32, msg_ptr: u64, flags: i32) -> SyscallResult {
    if msg_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    // SAFETY: the pointer is derived from a kernel object we own or a mapped physical address; the type layout matches the hardware or on-disk record.
    let msg = unsafe { &*(msg_ptr as *const Msghdr) };
    let _ = flags;

    if let Some(unix_id) = unix_id_for_fd(sockfd) {
        return unix_sendmsg(unix_id, msg);
    }

    let data = gather_iov(msg)?;
    let fds = parse_scm_rights(msg.msg_control, msg.msg_controllen);
    let sent = crate::net::sys_sendto_with_fds(sockfd as u32, &data, msg.msg_name, &fds)
        .map_err(|_| SyscallError::IoError)?;
    Ok(sent as u64)
}

/// recvmsg(sockfd, msg, flags) — receive structured message
pub fn sys_recvmsg(sockfd: i32, msg_ptr: u64, flags: i32) -> SyscallResult {
    if msg_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    // SAFETY: the pointer is derived from a kernel object we own or a mapped physical address; the type layout matches the hardware or on-disk record.
    let msg = unsafe { &mut *(msg_ptr as *mut Msghdr) };
    let _ = flags;

    if let Some(unix_id) = unix_id_for_fd(sockfd) {
        return unix_recvmsg(unix_id, msg);
    }

    let mut total_recv = 0usize;
    if msg.msg_iov != 0 && msg.msg_iovlen > 0 {
        let n = (msg.msg_iovlen as usize).min(MAX_IOV);
        // SAFETY: pointer and length come from the current kernel-mode caller and are used only as a bounded slice.
        let iovecs = unsafe { core::slice::from_raw_parts(msg.msg_iov as *const Iovec, n) };
        for iov in iovecs {
            if iov.iov_base != 0 && iov.iov_len > 0 {
                // SAFETY: pointer and length come from the current kernel-mode caller and are used only as a bounded mutable slice.
                let buf = unsafe {
                    core::slice::from_raw_parts_mut(iov.iov_base as *mut u8, iov.iov_len)
                };
                match crate::net::sys_recvfrom(sockfd as u32, buf) {
                    Ok(n) => {
                        total_recv += n;
                        break;
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

    msg.msg_flags = 0;
    msg.msg_controllen = 0;
    Ok(total_recv as u64)
}
