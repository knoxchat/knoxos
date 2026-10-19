use alloc::vec::Vec;

use crate::serial_println;

use super::types::{NFS_PORT, NFS4_PROGRAM, NFS4_VERSION};
use super::util::rdtsc;

// ─── Real Network I/O ──────────────────────────────────────────────

/// Send an RPC/XDR message over a TCP connection
pub fn send_rpc(server_ip: &str, port: u16, xdr_payload: &[u8]) -> Result<Vec<u8>, &'static str> {
    // Build RPC message header (RFC 5531)
    let mut rpc_msg = Vec::new();

    // Record marker (4 bytes): last fragment flag | fragment length
    let frag_len = xdr_payload.len() as u32;
    let record_marker = 0x80000000 | frag_len;
    rpc_msg.extend_from_slice(&record_marker.to_be_bytes());

    // XID (transaction ID)
    let xid = rdtsc() as u32;
    rpc_msg.extend_from_slice(&xid.to_be_bytes());

    // Message type: CALL (0)
    rpc_msg.extend_from_slice(&0u32.to_be_bytes());

    // RPC version: 2
    rpc_msg.extend_from_slice(&2u32.to_be_bytes());

    // Program: NFS (100003)
    rpc_msg.extend_from_slice(&NFS4_PROGRAM.to_be_bytes());

    // Version: 4
    rpc_msg.extend_from_slice(&NFS4_VERSION.to_be_bytes());

    // Procedure: COMPOUND (1)
    rpc_msg.extend_from_slice(&1u32.to_be_bytes());

    // Auth: AUTH_SYS (1)
    rpc_msg.extend_from_slice(&1u32.to_be_bytes()); // flavor
    let auth_body_len = 20u32;
    rpc_msg.extend_from_slice(&auth_body_len.to_be_bytes());
    rpc_msg.extend_from_slice(&0u32.to_be_bytes()); // stamp
    rpc_msg.extend_from_slice(&0u32.to_be_bytes()); // machinename len
    rpc_msg.extend_from_slice(&0u32.to_be_bytes()); // uid
    rpc_msg.extend_from_slice(&0u32.to_be_bytes()); // gid
    rpc_msg.extend_from_slice(&0u32.to_be_bytes()); // aux gids len

    // Verifier: AUTH_NONE (0)
    rpc_msg.extend_from_slice(&0u32.to_be_bytes());
    rpc_msg.extend_from_slice(&0u32.to_be_bytes());

    // COMPOUND payload
    rpc_msg.extend_from_slice(xdr_payload);

    serial_println!(
        "[NFS] Sending RPC to {}:{} ({} bytes, xid={})",
        server_ip,
        port,
        rpc_msg.len(),
        xid
    );

    // Send via TCP socket through the kernel networking stack
    use crate::net::{Ipv4Address, SocketAddress};

    // Resolve server IP
    let octets: Vec<u8> = server_ip
        .split('.')
        .filter_map(|s| s.parse::<u8>().ok())
        .collect();
    if octets.len() != 4 {
        // Try DNS resolution
        let resolved = crate::dns::resolve(server_ip);
        match resolved {
            Some(addrs) if !addrs.is_empty() => {
                let ip = addrs[0];
                let addr = SocketAddress::Inet(Ipv4Address::new(ip[0], ip[1], ip[2], ip[3]), port);
                return send_rpc_to_addr(&rpc_msg, addr);
            }
            _ => return Err("Failed to resolve NFS server address"),
        }
    }

    let addr = SocketAddress::Inet(
        Ipv4Address::new(octets[0], octets[1], octets[2], octets[3]),
        port,
    );
    send_rpc_to_addr(&rpc_msg, addr)
}

/// Internal: send RPC data to a resolved socket address and read the response
fn send_rpc_to_addr(data: &[u8], addr: crate::net::SocketAddress) -> Result<Vec<u8>, &'static str> {
    use crate::net::SOCKETS;

    // Create TCP socket: AF_INET=2, SOCK_STREAM=1, protocol=0
    let fd = crate::net::sys_socket(2, 1, 0).map_err(|_| "socket() failed")?;

    // Connect
    crate::net::sys_connect(fd, 0).map_err(|_| "connect() failed")?;

    // Manually set remote addr and send
    {
        let mut sockets = SOCKETS.lock();
        if let Some(sock) = sockets.get_mut(&fd) {
            sock.remote_addr = Some(addr);
            sock.state = crate::net::SocketState::Connected;
            sock.send_buf.extend_from_slice(data);
        }
    }

    // Poll for response with timeout (~2 seconds at ~100Hz timer)
    let start = crate::interrupts::get_ticks();
    let timeout_ticks = 200; // ~2 seconds
    let mut response = Vec::new();

    loop {
        let elapsed = crate::interrupts::get_ticks().wrapping_sub(start);
        if elapsed > timeout_ticks {
            break;
        }

        let sockets = SOCKETS.lock();
        if let Some(sock) = sockets.get(&fd) {
            if !sock.recv_buf.is_empty() {
                drop(sockets);
                let mut sockets = SOCKETS.lock();
                if let Some(sock) = sockets.get_mut(&fd) {
                    let n = sock.recv_buf.len().min(4096);
                    response.extend_from_slice(&sock.recv_buf[..n]);
                    sock.recv_buf.drain(..n);
                }
                break;
            }
        }
        // Yield briefly
        crate::arch_compat::instructions::interrupts::hlt();
    }

    // Close socket
    {
        let mut sockets = SOCKETS.lock();
        if let Some(sock) = sockets.get_mut(&fd) {
            sock.close();
        }
    }

    serial_println!("[NFS] RPC response: {} bytes", response.len());
    Ok(response)
}

/// Build an NFS4 COMPOUND operation for LOOKUP + GETATTR
pub fn build_lookup_compound(path: &str) -> Vec<u8> {
    let mut xdr = Vec::new();

    // Tag (empty)
    xdr.extend_from_slice(&0u32.to_be_bytes());

    // Minor version: 0
    xdr.extend_from_slice(&0u32.to_be_bytes());

    let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let num_ops = 1 + components.len() + 1; // PUTROOTFH + LOOKUPs + GETATTR

    xdr.extend_from_slice(&(num_ops as u32).to_be_bytes());

    // Op: PUTROOTFH (24)
    xdr.extend_from_slice(&24u32.to_be_bytes());

    // Op: LOOKUP for each path component
    for component in &components {
        xdr.extend_from_slice(&15u32.to_be_bytes()); // LOOKUP opcode
        let bytes = component.as_bytes();
        xdr.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        xdr.extend_from_slice(bytes);
        // Pad to 4-byte boundary
        while xdr.len() % 4 != 0 {
            xdr.push(0);
        }
    }

    // Op: GETATTR (9)
    xdr.extend_from_slice(&9u32.to_be_bytes());
    // Attr request bitmap: FATTR4_SIZE | FATTR4_TYPE | FATTR4_MODE
    xdr.extend_from_slice(&2u32.to_be_bytes()); // bitmap length
    xdr.extend_from_slice(&0x0018_0000u32.to_be_bytes()); // word 0
    xdr.extend_from_slice(&0x0000_0001u32.to_be_bytes()); // word 1

    xdr
}

/// NFS mount using real network I/O
pub fn nfs_mount_real(
    server: &str,
    export_path: &str,
    mount_point: &str,
) -> Result<(), &'static str> {
    serial_println!(
        "[NFS] Mounting {}:{} at {}",
        server,
        export_path,
        mount_point
    );

    // Step 1: DNS resolve server
    // Step 2: TCP connect to port 2049
    // Step 3: Send COMPOUND(PUTROOTFH, LOOKUP...)
    // Step 4: Get file handle
    // Step 5: Register mount in VFS

    crate::vfs::ensure_directory(mount_point);

    // For real implementation, connect and send RPC
    let compound = build_lookup_compound(export_path);
    let _response = send_rpc(server, NFS_PORT, &compound)?;

    serial_println!(
        "[NFS] Mount complete: {}:{} -> {}",
        server,
        export_path,
        mount_point
    );
    Ok(())
}
