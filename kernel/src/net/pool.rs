use alloc::vec::Vec;
use spin::Mutex;

use super::ipv4::Ipv4Address;
use super::socket::{SOCKETS, SocketAddress};
use super::syscall::sys_close_socket;

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
