use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::Ordering;

use crate::serial_println;

use super::iface::NETWORK_INTERFACES;
use super::ipv4::Ipv4Address;
use super::socket::{
    AddressFamily, NEXT_EPHEMERAL_PORT, SOCKETS, SockAddrIn, SockAddrUn, Socket, SocketAddress,
    SocketState, SocketType,
};
use super::tcp::{TCP_ACK, TCP_PSH};

fn is_loopback_ip(ip: Ipv4Address) -> bool {
    ip.is_loopback()
}

fn is_local_or_loopback(ip: Ipv4Address) -> bool {
    if ip.is_loopback() || ip == Ipv4Address::UNSPECIFIED {
        return true;
    }
    NETWORK_INTERFACES.lock().iter().any(|i| i.ip == ip)
}

fn inet_parts(addr: &SocketAddress) -> Option<(Ipv4Address, u16)> {
    match addr {
        SocketAddress::Inet(ip, port) => Some((*ip, *port)),
        SocketAddress::Unix(_) => None,
    }
}

fn bind_conflict(existing: &SocketAddress, new_ip: Ipv4Address, new_port: u16) -> bool {
    match existing {
        SocketAddress::Inet(ip, port) if *port == new_port => {
            *ip == Ipv4Address::UNSPECIFIED || new_ip == Ipv4Address::UNSPECIFIED || *ip == new_ip
        }
        _ => false,
    }
}

fn dgram_matches(local: &SocketAddress, dest: &SocketAddress) -> bool {
    match (local, dest) {
        (SocketAddress::Inet(lip, lport), SocketAddress::Inet(dip, dport)) => {
            *lport == *dport
                && (*lip == Ipv4Address::UNSPECIFIED
                    || *lip == *dip
                    || (lip.is_loopback() && dip.is_loopback()))
        }
        (SocketAddress::Unix(a), SocketAddress::Unix(b)) => a == b,
        _ => false,
    }
}

fn account_loopback(nbytes: usize) {
    let mut interfaces = NETWORK_INTERFACES.lock();
    if let Some(lo) = interfaces.iter_mut().find(|i| i.name == "lo") {
        lo.tx_bytes += nbytes as u64;
        lo.tx_packets += 1;
        lo.rx_bytes += nbytes as u64;
        lo.rx_packets += 1;
    }
}

fn loopback_connect(
    sockets: &mut BTreeMap<u32, Socket>,
    sockfd: u32,
    addr: SocketAddress,
) -> Result<(), i32> {
    let client_type = sockets.get(&sockfd).ok_or(-9i32)?.sock_type;

    if client_type != SocketType::Stream {
        let client = sockets.get_mut(&sockfd).ok_or(-9i32)?;
        if client.state == SocketState::Unbound {
            if let SocketAddress::Inet(_, _) = &addr {
                let port = NEXT_EPHEMERAL_PORT.fetch_add(1, Ordering::Relaxed);
                client.local_addr = Some(SocketAddress::Inet(Ipv4Address::LOOPBACK, port));
            }
        }
        client.remote_addr = Some(addr);
        client.state = SocketState::Connected;
        return Ok(());
    }

    let dest = inet_parts(&addr).ok_or(-97i32)?;
    let listener_id = sockets.iter().find_map(|(id, s)| {
        if s.sock_type != SocketType::Stream || s.state != SocketState::Listening {
            return None;
        }
        match &s.local_addr {
            Some(SocketAddress::Inet(lip, lport)) if *lport == dest.1 => {
                if *lip == Ipv4Address::UNSPECIFIED
                    || *lip == dest.0
                    || (lip.is_loopback() && dest.0.is_loopback())
                {
                    Some(*id)
                } else {
                    None
                }
            }
            _ => None,
        }
    });
    let listener_id = listener_id.ok_or(-111i32)?; // ECONNREFUSED

    {
        let listener = sockets.get(&listener_id).ok_or(-111i32)?;
        if listener.backlog.len() >= listener.max_backlog {
            return Err(-11); // EAGAIN
        }
    }

    let client_local = {
        let client = sockets.get_mut(&sockfd).ok_or(-9i32)?;
        if client.state == SocketState::Unbound {
            let port = NEXT_EPHEMERAL_PORT.fetch_add(1, Ordering::Relaxed);
            client.local_addr = Some(SocketAddress::Inet(Ipv4Address::LOOPBACK, port));
        }
        client.local_addr.clone()
    };
    let listener_local = sockets.get(&listener_id).and_then(|l| l.local_addr.clone());

    let mut accepted = Socket::new(AddressFamily::Inet, SocketType::Stream, 6);
    accepted.state = SocketState::Connected;
    accepted.local_addr = listener_local;
    accepted.remote_addr = client_local;
    accepted.peer_id = Some(sockfd);
    let accepted_id = accepted.id;

    {
        let client = sockets.get_mut(&sockfd).ok_or(-9i32)?;
        client.remote_addr = Some(addr);
        client.peer_id = Some(accepted_id);
        client.state = SocketState::Connected;
    }

    sockets.insert(accepted_id, accepted);
    sockets
        .get_mut(&listener_id)
        .ok_or(-111i32)?
        .backlog
        .push(accepted_id);
    Ok(())
}

fn loopback_send(
    sockets: &mut BTreeMap<u32, Socket>,
    sockfd: u32,
    buf: &[u8],
    dest: Option<SocketAddress>,
) -> Result<usize, i32> {
    let sock = sockets.get(&sockfd).ok_or(-9i32)?;
    let sock_type = sock.sock_type;
    let state = sock.state;
    let peer_id = sock.peer_id;
    let local = sock.local_addr.clone();
    let remote = dest.clone().or_else(|| sock.remote_addr.clone());

    if sock_type == SocketType::Stream {
        if state != SocketState::Connected {
            return Err(-107); // ENOTCONN
        }
        let peer = peer_id.ok_or(-32i32)?; // EPIPE
        let peer_sock = sockets.get_mut(&peer).ok_or(-32i32)?;
        peer_sock.recv_buf.extend_from_slice(buf);
        account_loopback(buf.len());
        return Ok(buf.len());
    }

    let dest = remote.ok_or(-89i32)?; // EDESTADDRREQ
    if let SocketAddress::Inet(ip, _) = &dest {
        if is_local_or_loopback(*ip) {
            let sender = local.unwrap_or(SocketAddress::Inet(Ipv4Address::LOOPBACK, 0));
            let target = sockets.iter().find_map(|(id, s)| {
                if *id == sockfd || s.sock_type != SocketType::Dgram {
                    return None;
                }
                s.local_addr
                    .as_ref()
                    .filter(|la| dgram_matches(la, &dest))
                    .map(|_| *id)
            });
            if let Some(tid) = target {
                if let Some(t) = sockets.get_mut(&tid) {
                    t.recv_buf.extend_from_slice(buf);
                    t.remote_addr = Some(sender);
                }
            }
            account_loopback(buf.len());
            return Ok(buf.len());
        }
    }

    // Not loopback: keep the bytes so a later NIC path can drain send_buf.
    let socket = sockets.get_mut(&sockfd).ok_or(-9i32)?;
    if dest_is_set(&dest) {
        socket.sendto(buf, &dest)
    } else {
        socket.send(buf)
    }
}

fn dest_is_set(addr: &SocketAddress) -> bool {
    !matches!(addr, SocketAddress::Inet(ip, 0) if *ip == Ipv4Address::UNSPECIFIED)
}

/// Bind + connect + send/recv on 127.0.0.1. Used by Gate D1 and tests.
pub fn loopback_self_test() -> bool {
    const PORT: u16 = 42424;
    const PAYLOAD: &[u8] = b"ping";

    let udp_ok = (|| -> Result<(), i32> {
        let a = sys_socket(2, 2, 0)?;
        let b = sys_socket(2, 2, 0)?;
        {
            let mut sockets = SOCKETS.lock();
            let sock = sockets.get_mut(&b).ok_or(-9i32)?;
            sock.bind(SocketAddress::Inet(Ipv4Address::LOOPBACK, PORT))?;
        }
        {
            let mut sockets = SOCKETS.lock();
            loopback_send(
                &mut sockets,
                a,
                PAYLOAD,
                Some(SocketAddress::Inet(Ipv4Address::LOOPBACK, PORT)),
            )?;
        }
        let mut buf = [0u8; 8];
        let n = sys_recvfrom(b, &mut buf)?;
        let _ = sys_close_socket(a);
        let _ = sys_close_socket(b);
        if n == PAYLOAD.len() && &buf[..n] == PAYLOAD {
            Ok(())
        } else {
            Err(-1)
        }
    })()
    .is_ok();

    let tcp_ok = (|| -> Result<(), i32> {
        let listener = sys_socket(2, 1, 0)?;
        let client = sys_socket(2, 1, 0)?;
        {
            let mut sockets = SOCKETS.lock();
            let sock = sockets.get_mut(&listener).ok_or(-9i32)?;
            sock.bind(SocketAddress::Inet(Ipv4Address::LOOPBACK, PORT + 1))?;
            sock.listen(1)?;
        }
        {
            let mut sockets = SOCKETS.lock();
            loopback_connect(
                &mut sockets,
                client,
                SocketAddress::Inet(Ipv4Address::LOOPBACK, PORT + 1),
            )?;
        }
        let accepted = sys_accept(listener)?;
        {
            let mut sockets = SOCKETS.lock();
            loopback_send(&mut sockets, client, PAYLOAD, None)?;
        }
        let mut buf = [0u8; 8];
        let n = sys_recvfrom(accepted, &mut buf)?;
        let _ = sys_close_socket(client);
        let _ = sys_close_socket(accepted);
        let _ = sys_close_socket(listener);
        if n == PAYLOAD.len() && &buf[..n] == PAYLOAD {
            Ok(())
        } else {
            Err(-1)
        }
    })()
    .is_ok();

    if udp_ok && tcp_ok {
        serial_println!("[NET] loopback self-test: UDP+TCP send/recv ok");
    } else {
        serial_println!(
            "[NET] loopback self-test FAILED udp={} tcp={}",
            udp_ok,
            tcp_ok
        );
    }
    udp_ok && tcp_ok
}

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
    let mut addr = unsafe { parse_sockaddr(addr_ptr)? };
    if let SocketAddress::Inet(ip, port) = addr {
        let port = if port == 0 {
            NEXT_EPHEMERAL_PORT.fetch_add(1, Ordering::Relaxed)
        } else {
            port
        };
        if port > 0 && port < 1024 {
            let pid = crate::scheduler::current_pid().unwrap_or(1);
            crate::capabilities::check_net_bind(pid, port)?;
        }
        addr = SocketAddress::Inet(ip, port);
        let mut sockets = SOCKETS.lock();
        let sock_type = sockets.get(&sockfd).ok_or(-9i32)?.sock_type;
        let conflict = sockets.iter().any(|(id, s)| {
            *id != sockfd
                && s.sock_type == sock_type
                && s.local_addr
                    .as_ref()
                    .is_some_and(|existing| bind_conflict(existing, ip, port))
        });
        if conflict {
            return Err(-98); // EADDRINUSE
        }
        let socket = sockets.get_mut(&sockfd).ok_or(-9i32)?;
        socket.bind(addr)
    } else {
        let mut sockets = SOCKETS.lock();
        let socket = sockets.get_mut(&sockfd).ok_or(-9i32)?;
        socket.bind(addr)
    }
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
    socket.accept()
}

/// Connect to a remote address
pub fn sys_connect(sockfd: u32, addr_ptr: u64) -> Result<(), i32> {
    let addr = unsafe { parse_sockaddr(addr_ptr)? };
    let mut sockets = SOCKETS.lock();
    if !sockets.contains_key(&sockfd) {
        return Err(-9);
    }
    if let SocketAddress::Inet(ip, _) = &addr {
        if is_loopback_ip(*ip) || is_local_or_loopback(*ip) {
            return loopback_connect(&mut sockets, sockfd, addr);
        }
    }
    let socket = sockets.get_mut(&sockfd).ok_or(-9i32)?;
    socket.connect(addr)
}

/// Send data on a socket
pub fn sys_sendto(sockfd: u32, buf: &[u8], addr_ptr: u64) -> Result<usize, i32> {
    let dest = if addr_ptr != 0 {
        Some(unsafe { parse_sockaddr(addr_ptr)? })
    } else {
        None
    };
    if let Some(SocketAddress::Inet(ip, _)) = &dest {
        if is_loopback_ip(*ip) || is_local_or_loopback(*ip) {
            let mut sockets = SOCKETS.lock();
            return loopback_send(&mut sockets, sockfd, buf, dest);
        }
    } else if dest.is_none() {
        let sockets = SOCKETS.lock();
        if let Some(sock) = sockets.get(&sockfd) {
            if let Some(SocketAddress::Inet(ip, _)) = &sock.remote_addr {
                if is_loopback_ip(*ip) || is_local_or_loopback(*ip) {
                    drop(sockets);
                    let mut sockets = SOCKETS.lock();
                    return loopback_send(&mut sockets, sockfd, buf, dest);
                }
            }
        }
    }
    nic_send(sockfd, buf, dest)
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
        if let Some(peer) = socket.peer_id {
            if let Some(p) = sockets.get_mut(&peer) {
                p.peer_id = None;
                if p.state == SocketState::Connected {
                    p.state = SocketState::Closing;
                }
            }
        }
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

pub(super) fn nic_send(sockfd: u32, buf: &[u8], dest: Option<SocketAddress>) -> Result<usize, i32> {
    let mut sockets = SOCKETS.lock();
    let socket = sockets.get_mut(&sockfd).ok_or(-9i32)?;
    let sock_type = socket.sock_type;
    let remote = dest.or_else(|| socket.remote_addr.clone()).ok_or(-89i32)?;
    let SocketAddress::Inet(dst_ip, dst_port) = remote else {
        return Err(-97);
    };
    let src_port = match socket.local_addr {
        Some(SocketAddress::Inet(_, p)) => p,
        _ => {
            let p = NEXT_EPHEMERAL_PORT.fetch_add(1, Ordering::Relaxed);
            socket.local_addr = Some(SocketAddress::Inet(Ipv4Address::UNSPECIFIED, p));
            p
        }
    };
    if sock_type == SocketType::Stream {
        if socket.state != SocketState::Connected {
            return Err(-107);
        }
        let (seq, ack, window, send_len) = if let Some(tcb) = socket.tcp_conn.as_mut() {
            let limit = tcb.send_limit() as usize;
            if limit == 0 {
                return Err(-11);
            }
            let n = buf.len().min(limit);
            let seq = tcb.snd_nxt;
            tcb.snd_nxt = tcb.snd_nxt.wrapping_add(n as u32);
            tcb.cc.on_send(n as u32);
            tcb.last_tx_ticks = crate::interrupts::get_ticks();
            (seq, tcb.rcv_nxt, tcb.advertised_window(), n)
        } else {
            (0, 0, 65535, buf.len())
        };
        let payload = &buf[..send_len];
        drop(sockets);
        if crate::netint::send_tcp_segment(
            dst_ip,
            src_port,
            dst_port,
            seq,
            ack,
            TCP_PSH | TCP_ACK,
            window,
            payload,
        ) {
            Ok(send_len)
        } else {
            Err(-101)
        }
    } else {
        drop(sockets);
        if crate::netint::send_udp(dst_ip, src_port, dst_port, buf) {
            Ok(buf.len())
        } else {
            Err(-101)
        }
    }
}
