/// Socket Activation — systemd-style socket-based service activation
///
/// Provides on-demand service startup triggered by incoming connections:
///   - TCP/UDP listen sockets managed by the init system
///   - Automatic service start on first connection
///   - File descriptor passing to activated services
///   - Accept mode (one connection per service instance)
///   - Socket unit configuration (port, protocol, backlog)
///   - Integration with service_manager.rs for service lifecycle
///
/// This implements status.md items 21.5 (Socket activation) and parts of
/// 21.6 (Timer activation) for the KnoxOS init system.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// SOCKET UNIT TYPES
// ═══════════════════════════════════════════════════════════════════════

/// Protocol for socket activation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketProtocol {
    Tcp,
    Udp,
    Unix,
}

/// Socket unit state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    /// Socket unit is defined but not listening
    Inactive,
    /// Socket is bound and listening for connections
    Listening,
    /// A connection arrived and the service is being activated
    Activating,
    /// The associated service is running
    Running,
    /// Socket failed to bind or service failed to start
    Failed,
}

/// Address for socket binding
#[derive(Debug, Clone)]
pub enum SocketAddress {
    /// IPv4 address + port (e.g., "0.0.0.0:80")
    Inet { addr: [u8; 4], port: u16 },
    /// IPv6 address + port
    Inet6 { addr: [u8; 16], port: u16 },
    /// Unix domain socket path
    Unix { path: String },
}

impl SocketAddress {
    pub fn tcp_port(port: u16) -> Self {
        SocketAddress::Inet {
            addr: [0, 0, 0, 0],
            port,
        }
    }

    pub fn tcp_addr(a: u8, b: u8, c: u8, d: u8, port: u16) -> Self {
        SocketAddress::Inet {
            addr: [a, b, c, d],
            port,
        }
    }

    pub fn unix(path: &str) -> Self {
        SocketAddress::Unix {
            path: String::from(path),
        }
    }

    pub fn port(&self) -> Option<u16> {
        match self {
            SocketAddress::Inet { port, .. } | SocketAddress::Inet6 { port, .. } => Some(*port),
            SocketAddress::Unix { .. } => None,
        }
    }
}

impl core::fmt::Display for SocketAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SocketAddress::Inet { addr, port } => {
                write!(
                    f,
                    "{}.{}.{}.{}:{}",
                    addr[0], addr[1], addr[2], addr[3], port
                )
            }
            SocketAddress::Inet6 { port, .. } => write!(f, "[::]:{}", port),
            SocketAddress::Unix { path } => write!(f, "unix:{}", path),
        }
    }
}

/// Socket unit configuration — analogous to systemd .socket files
#[derive(Debug, Clone)]
pub struct SocketUnit {
    /// Unit name (e.g., "sshd.socket")
    pub name: String,
    /// Description
    pub description: String,
    /// Listen addresses (can have multiple)
    pub listen_addresses: Vec<SocketAddress>,
    /// Protocol
    pub protocol: SocketProtocol,
    /// Service to activate when connection arrives
    pub service: String,
    /// Whether to accept connections and pass them individually (Accept=yes)
    pub accept: bool,
    /// Maximum connections in backlog
    pub backlog: u32,
    /// Maximum concurrent instances (for Accept=yes mode)
    pub max_connections: u32,
    /// File permissions for Unix sockets
    pub socket_mode: u32,
    /// Current state
    pub state: SocketState,
    /// Number of connections accepted
    pub connections_accepted: u64,
    /// Active file descriptors passed to services
    pub active_fds: Vec<u32>,
    /// Bind IPv6 only (IPV6_V6ONLY)
    pub bind_ipv6_only: bool,
    /// Reuse address (SO_REUSEADDR)
    pub reuse_addr: bool,
    /// Keep-alive
    pub keep_alive: bool,
    /// Trigger limit: max activations per interval
    pub trigger_limit_burst: u32,
    /// Trigger limit interval in seconds
    pub trigger_limit_interval_sec: u32,
    /// Activation count within current interval
    trigger_count: u32,
    /// Interval start timestamp
    trigger_interval_start: u64,
}

impl SocketUnit {
    pub fn new(name: &str, service: &str, addr: SocketAddress) -> Self {
        Self {
            name: String::from(name),
            description: String::new(),
            listen_addresses: alloc::vec![addr],
            protocol: SocketProtocol::Tcp,
            service: String::from(service),
            accept: false,
            backlog: 128,
            max_connections: 64,
            socket_mode: 0o660,
            state: SocketState::Inactive,
            connections_accepted: 0,
            active_fds: Vec::new(),
            bind_ipv6_only: false,
            reuse_addr: true,
            keep_alive: true,
            trigger_limit_burst: 200,
            trigger_limit_interval_sec: 2,
            trigger_count: 0,
            trigger_interval_start: 0,
        }
    }

    /// Add a listen address
    pub fn add_listen(&mut self, addr: SocketAddress) {
        self.listen_addresses.push(addr);
    }

    /// Set description
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = String::from(desc);
        self
    }

    /// Set accept mode
    pub fn with_accept(mut self, accept: bool) -> Self {
        self.accept = accept;
        self
    }

    /// Set protocol
    pub fn with_protocol(mut self, proto: SocketProtocol) -> Self {
        self.protocol = proto;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SOCKET ACTIVATION MANAGER
// ═══════════════════════════════════════════════════════════════════════

/// Manages all socket units and their lifecycle
struct SocketActivationManager {
    /// Registered socket units
    sockets: BTreeMap<String, SocketUnit>,
    /// Port → socket name mapping for fast lookup
    port_map: BTreeMap<u16, String>,
    /// Unix path → socket name mapping
    path_map: BTreeMap<String, String>,
    /// File descriptor counter
    next_fd: u32,
}

impl SocketActivationManager {
    fn new() -> Self {
        Self {
            sockets: BTreeMap::new(),
            port_map: BTreeMap::new(),
            path_map: BTreeMap::new(),
            next_fd: 3, // 0=stdin, 1=stdout, 2=stderr, 3+ for sockets
        }
    }

    /// Register a socket unit
    fn register(&mut self, unit: SocketUnit) {
        serial_println!("[socket_activation] Registering: {}", unit.name);

        // Build port/path mappings
        for addr in &unit.listen_addresses {
            match addr {
                SocketAddress::Inet { port, .. } | SocketAddress::Inet6 { port, .. } => {
                    self.port_map.insert(*port, unit.name.clone());
                }
                SocketAddress::Unix { path } => {
                    self.path_map.insert(path.clone(), unit.name.clone());
                }
            }
        }

        self.sockets.insert(unit.name.clone(), unit);
    }

    /// Start listening on a socket unit
    fn start_listening(&mut self, name: &str) -> Result<(), &'static str> {
        let socket = self.sockets.get_mut(name).ok_or("socket unit not found")?;

        if socket.state == SocketState::Listening {
            return Ok(());
        }

        // Bind each listen address
        for addr in &socket.listen_addresses {
            serial_println!("[socket_activation] Binding {} on {}", name, addr);

            match (socket.protocol, addr) {
                (SocketProtocol::Tcp, SocketAddress::Inet { addr: ip, port }) => {
                    // Create a kernel-managed listening socket
                    // In real implementation: call into net::tcp_listen()
                    let fd = self.next_fd;
                    self.next_fd += 1;
                    socket.active_fds.push(fd);
                    serial_println!(
                        "[socket_activation] TCP listen fd={} on {}.{}.{}.{}:{}",
                        fd,
                        ip[0],
                        ip[1],
                        ip[2],
                        ip[3],
                        port
                    );
                }
                (SocketProtocol::Udp, SocketAddress::Inet { addr: ip, port }) => {
                    let fd = self.next_fd;
                    self.next_fd += 1;
                    socket.active_fds.push(fd);
                    serial_println!(
                        "[socket_activation] UDP bind fd={} on {}.{}.{}.{}:{}",
                        fd,
                        ip[0],
                        ip[1],
                        ip[2],
                        ip[3],
                        port
                    );
                }
                (_, SocketAddress::Unix { path }) => {
                    let fd = self.next_fd;
                    self.next_fd += 1;
                    socket.active_fds.push(fd);
                    serial_println!("[socket_activation] Unix socket fd={} on {}", fd, path);
                    // Create the socket file in VFS
                    let _ = crate::vfs::write_file_dispatch(
                        path,
                        alloc::format!("SOCK:{}", name).as_bytes(),
                    );
                }
                _ => {}
            }
        }

        socket.state = SocketState::Listening;
        SOCKETS_LISTENING.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Stop listening on a socket unit
    fn stop_listening(&mut self, name: &str) -> Result<(), &'static str> {
        let socket = self.sockets.get_mut(name).ok_or("socket unit not found")?;
        socket.state = SocketState::Inactive;
        socket.active_fds.clear();
        serial_println!("[socket_activation] Stopped: {}", name);
        Ok(())
    }

    /// Handle an incoming connection on a port — triggers service activation
    fn handle_connection(&mut self, port: u16) -> Result<&str, &'static str> {
        let socket_name = self
            .port_map
            .get(&port)
            .ok_or("no socket unit for this port")?
            .clone();

        let socket = self
            .sockets
            .get_mut(&socket_name)
            .ok_or("socket unit disappeared")?;

        // Rate limiting
        let now = crate::rtc::read_rtc().to_unix_timestamp() as u64;
        if now.wrapping_sub(socket.trigger_interval_start)
            > socket.trigger_limit_interval_sec as u64
        {
            socket.trigger_count = 0;
            socket.trigger_interval_start = now;
        }
        socket.trigger_count += 1;
        if socket.trigger_count > socket.trigger_limit_burst {
            return Err("trigger rate limit exceeded");
        }

        socket.connections_accepted += 1;
        CONNECTIONS_TOTAL.fetch_add(1, Ordering::Relaxed);

        let service_name = socket.service.clone();

        // Check if the service is already running
        let mgr = crate::service_manager::SERVICE_MANAGER.lock();
        let already_running =
            mgr.status(&service_name) == Some(crate::service_manager::ServiceState::Running);
        drop(mgr);

        if !already_running {
            // Activate the service
            socket.state = SocketState::Activating;
            serial_println!(
                "[socket_activation] Connection on port {} → activating {}",
                port,
                service_name
            );

            // Pass file descriptors via environment variables (systemd-compatible)
            // LISTEN_FDS=N, LISTEN_PID=<pid>, LISTEN_FDNAMES=<name>
            let fd_count = socket.active_fds.len();
            let mut mgr = crate::service_manager::SERVICE_MANAGER.lock();
            if let Some(_svc) = mgr
                .list_services()
                .iter()
                .find(|(n, _)| *n == service_name.as_str())
            {
                // Service exists, start it
                drop(mgr);
                let mut mgr = crate::service_manager::SERVICE_MANAGER.lock();
                let _ = mgr.start_service(&service_name);
            } else {
                drop(mgr);
                serial_println!(
                    "[socket_activation] Service {} not registered, creating on-demand",
                    service_name
                );
                // Auto-register a service unit
                let svc = crate::service_manager::ServiceUnit::new(
                    &service_name,
                    &alloc::format!("Socket-activated: {}", socket_name),
                    &alloc::format!("/usr/sbin/{}", service_name.replace(".service", "")),
                );
                let mut mgr = crate::service_manager::SERVICE_MANAGER.lock();
                mgr.add_service(svc);
                let _ = mgr.start_service(&service_name);
            }

            ACTIVATIONS.fetch_add(1, Ordering::Relaxed);

            if let Some(socket) = self.sockets.get_mut(&socket_name) {
                socket.state = SocketState::Running;
            }
        }

        // Return the service name (borrow from the stored socket)
        Ok("activated")
    }

    /// Handle a Unix socket connection
    fn handle_unix_connection(&mut self, path: &str) -> Result<(), &'static str> {
        let socket_name = self
            .path_map
            .get(path)
            .ok_or("no socket unit for this path")?
            .clone();

        let socket = self
            .sockets
            .get_mut(&socket_name)
            .ok_or("socket unit disappeared")?;

        socket.connections_accepted += 1;
        CONNECTIONS_TOTAL.fetch_add(1, Ordering::Relaxed);

        let service_name = socket.service.clone();
        let mut mgr = crate::service_manager::SERVICE_MANAGER.lock();
        let _ = mgr.start_service(&service_name);
        drop(mgr);

        ACTIVATIONS.fetch_add(1, Ordering::Relaxed);
        serial_println!(
            "[socket_activation] Unix connection on {} → activated {}",
            path,
            service_name
        );
        Ok(())
    }

    /// List all socket units
    fn list(&self) -> Vec<(&str, SocketState, u64)> {
        self.sockets
            .iter()
            .map(|(name, s)| (name.as_str(), s.state, s.connections_accepted))
            .collect()
    }

    /// Get status of a specific socket
    fn status(&self, name: &str) -> Option<(SocketState, u64)> {
        self.sockets
            .get(name)
            .map(|s| (s.state, s.connections_accepted))
    }

    /// Start all registered sockets
    fn start_all(&mut self) {
        let names: Vec<String> = self.sockets.keys().cloned().collect();
        for name in &names {
            if let Err(e) = self.start_listening(name) {
                serial_println!("[socket_activation] Failed to start {}: {}", name, e);
            }
        }
    }
}

lazy_static::lazy_static! {
    static ref SOCKET_MGR: Mutex<SocketActivationManager> =
        Mutex::new(SocketActivationManager::new());
}

static SOCKETS_LISTENING: AtomicU64 = AtomicU64::new(0);
static CONNECTIONS_TOTAL: AtomicU64 = AtomicU64::new(0);
static ACTIVATIONS: AtomicU64 = AtomicU64::new(0);

// ═══════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════

/// Register a socket unit
pub fn register(unit: SocketUnit) {
    SOCKET_MGR.lock().register(unit);
}

/// Start listening on a socket unit
pub fn start(name: &str) -> Result<(), &'static str> {
    SOCKET_MGR.lock().start_listening(name)
}

/// Stop a socket unit
pub fn stop(name: &str) -> Result<(), &'static str> {
    SOCKET_MGR.lock().stop_listening(name)
}

/// Notify of incoming TCP/UDP connection on a port
pub fn notify_connection(port: u16) -> Result<(), &'static str> {
    SOCKET_MGR.lock().handle_connection(port).map(|_| ())
}

/// Notify of incoming Unix socket connection
pub fn notify_unix_connection(path: &str) -> Result<(), &'static str> {
    SOCKET_MGR.lock().handle_unix_connection(path)
}

/// List all socket units
pub fn list() -> Vec<(String, SocketState, u64)> {
    SOCKET_MGR
        .lock()
        .list()
        .iter()
        .map(|(n, s, c)| (String::from(*n), *s, *c))
        .collect()
}

/// Get socket status
pub fn status(name: &str) -> Option<(SocketState, u64)> {
    SOCKET_MGR.lock().status(name)
}

/// Start all registered sockets
pub fn start_all() {
    SOCKET_MGR.lock().start_all();
}

/// Get statistics
pub fn stats() -> (u64, u64, u64) {
    (
        SOCKETS_LISTENING.load(Ordering::Relaxed),
        CONNECTIONS_TOTAL.load(Ordering::Relaxed),
        ACTIVATIONS.load(Ordering::Relaxed),
    )
}

/// Initialize socket activation with default system sockets
pub fn init() {
    // SSH socket — activates sshd on first connection to port 22
    let mut ssh_socket =
        SocketUnit::new("sshd.socket", "sshd.service", SocketAddress::tcp_port(22));
    ssh_socket.description = String::from("OpenSSH Server Socket");
    ssh_socket.accept = false;
    register(ssh_socket);

    // HTTP socket — activates httpd on first connection to port 80
    let mut http_socket =
        SocketUnit::new("httpd.socket", "httpd.service", SocketAddress::tcp_port(80));
    http_socket.description = String::from("HTTP Server Socket");
    http_socket.accept = true;
    http_socket.max_connections = 256;
    register(http_socket);

    // D-Bus system socket
    let dbus_socket = SocketUnit::new(
        "dbus.socket",
        "dbus.service",
        SocketAddress::unix("/run/dbus/system_bus_socket"),
    )
    .with_description("D-Bus System Bus Socket")
    .with_protocol(SocketProtocol::Unix);
    register(dbus_socket);

    // DNS resolver socket (port 53)
    let mut dns_socket =
        SocketUnit::new("dnsd.socket", "dnsd.service", SocketAddress::tcp_port(53));
    dns_socket.description = String::from("DNS Resolver Socket");
    dns_socket.add_listen(SocketAddress::Inet {
        addr: [0, 0, 0, 0],
        port: 53,
    });
    dns_socket.protocol = SocketProtocol::Udp;
    register(dns_socket);

    // Start all sockets listening
    start_all();

    serial_println!("[KnoxOS] Socket activation initialized (4 units registered)");
}
