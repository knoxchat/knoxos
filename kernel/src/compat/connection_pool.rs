use crate::serial_println;
/// Network Connection Pooling
///
/// Reusable TCP connection pools for HTTP/NFS/SMB, keep-alive,
/// idle timeout, per-host limits.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone)]
pub struct PooledConnection {
    pub host: String,
    pub port: u16,
    pub socket_id: u32,
    pub created_at: u64,
    pub last_used: u64,
    pub in_use: bool,
}

pub struct ConnectionPool {
    pub connections: Vec<PooledConnection>,
    pub max_per_host: usize,
    pub max_total: usize,
    pub idle_timeout_secs: u64,
    pub next_socket_id: u32,
}

lazy_static::lazy_static! {
    static ref POOL: Mutex<ConnectionPool> = Mutex::new(ConnectionPool {
        connections: Vec::new(),
        max_per_host: 6,
        max_total: 64,
        idle_timeout_secs: 120,
        next_socket_id: 1,
    });
}

impl ConnectionPool {
    pub fn acquire(&mut self, host: &str, port: u16, now: u64) -> Option<u32> {
        // Try reuse idle connection
        if let Some(conn) = self
            .connections
            .iter_mut()
            .find(|c| c.host == host && c.port == port && !c.in_use)
        {
            conn.in_use = true;
            conn.last_used = now;
            serial_println!("[CONNPOOL] Reused connection to {}:{}", host, port);
            return Some(conn.socket_id);
        }
        // Check limits
        let host_count = self.connections.iter().filter(|c| c.host == host).count();
        if host_count >= self.max_per_host || self.connections.len() >= self.max_total {
            return None;
        }
        // Create new
        let id = self.next_socket_id;
        self.next_socket_id += 1;
        self.connections.push(PooledConnection {
            host: String::from(host),
            port,
            socket_id: id,
            created_at: now,
            last_used: now,
            in_use: true,
        });
        serial_println!("[CONNPOOL] New connection to {}:{} (id={})", host, port, id);
        Some(id)
    }

    pub fn release(&mut self, socket_id: u32) {
        if let Some(conn) = self
            .connections
            .iter_mut()
            .find(|c| c.socket_id == socket_id)
        {
            conn.in_use = false;
        }
    }

    pub fn evict_idle(&mut self, now: u64) {
        let timeout = self.idle_timeout_secs;
        self.connections
            .retain(|c| c.in_use || (now - c.last_used) < timeout);
    }

    pub fn stats(&self) -> (usize, usize) {
        let active = self.connections.iter().filter(|c| c.in_use).count();
        (active, self.connections.len())
    }
}

pub fn init() {
    serial_println!("[CONNPOOL] Connection pool initialized");
}
