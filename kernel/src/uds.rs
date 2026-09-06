/// Unix Domain Sockets - Local IPC via socket interface
/// Compatible with Linux AF_UNIX / AF_LOCAL sockets
/// Provides stream and datagram communication
use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Maximum data in a unix socket buffer
const UNIX_SOCKET_BUF_SIZE: usize = 65536;
/// Maximum pending connections for listen()
const UNIX_SOCKET_MAX_BACKLOG: usize = 128;

/// Unix socket type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnixSocketType {
    Stream,    // SOCK_STREAM (connection-oriented, reliable)
    Dgram,     // SOCK_DGRAM (connectionless, message boundaries)
    SeqPacket, // SOCK_SEQPACKET (connection-oriented, message boundaries)
}

/// Unix socket state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnixSocketState {
    Unbound,
    Bound,
    Listening,
    Connected,
    Connecting,
    Closed,
}

/// A Unix domain socket
struct UnixSocket {
    id: u32,
    socket_type: UnixSocketType,
    state: UnixSocketState,
    bound_path: Option<String>,
    peer_id: Option<u32>,
    recv_buffer: VecDeque<Vec<u8>>,
    pending_connections: VecDeque<u32>,
    backlog: usize,
    nonblocking: bool,
    /// Credentials of the creator
    uid: u32,
    gid: u32,
    pid: u32,
}

impl UnixSocket {
    fn new(id: u32, socket_type: UnixSocketType, pid: u32) -> Self {
        Self {
            id,
            socket_type,
            state: UnixSocketState::Unbound,
            bound_path: None,
            peer_id: None,
            recv_buffer: VecDeque::new(),
            pending_connections: VecDeque::new(),
            backlog: UNIX_SOCKET_MAX_BACKLOG,
            nonblocking: false,
            uid: 1000,
            gid: 1000,
            pid,
        }
    }

    fn recv_buf_size(&self) -> usize {
        self.recv_buffer.iter().map(|v| v.len()).sum()
    }
}

/// Global unix socket registry
lazy_static::lazy_static! {
    static ref UNIX_SOCKETS: Mutex<BTreeMap<u32, UnixSocket>> = Mutex::new(BTreeMap::new());
    /// Path -> socket ID mapping for bound sockets
    static ref BOUND_PATHS: Mutex<BTreeMap<String, u32>> = Mutex::new(BTreeMap::new());
}

static NEXT_UNIX_SOCKET_ID: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(1);

/// Create a new unix domain socket
pub fn socket_create(sock_type: UnixSocketType) -> Result<u32, i32> {
    let id = NEXT_UNIX_SOCKET_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    UNIX_SOCKETS
        .lock()
        .insert(id, UnixSocket::new(id, sock_type, pid));
    crate::serial_println!("[KnoxOS] unix socket created: id={}", id);
    Ok(id)
}

/// Bind a unix socket to a path
pub fn socket_bind(id: u32, path: &str) -> Result<(), i32> {
    let mut sockets = UNIX_SOCKETS.lock();
    let socket = sockets.get_mut(&id).ok_or(-9i32)?; // EBADF

    if socket.state != UnixSocketState::Unbound {
        return Err(-22); // EINVAL
    }

    let mut paths = BOUND_PATHS.lock();
    if paths.contains_key(path) {
        return Err(-98); // EADDRINUSE
    }

    socket.state = UnixSocketState::Bound;
    socket.bound_path = Some(String::from(path));
    paths.insert(String::from(path), id);

    // Create a socket file in the VFS
    crate::vfs::VFS
        .lock()
        .create_file_at_path(path, crate::vfs::FileType::Socket, &[], 0o755);

    crate::serial_println!("[KnoxOS] unix socket {} bound to {}", id, path);
    Ok(())
}

/// Listen on a unix socket
pub fn socket_listen(id: u32, backlog: i32) -> Result<(), i32> {
    let mut sockets = UNIX_SOCKETS.lock();
    let socket = sockets.get_mut(&id).ok_or(-9i32)?;

    if socket.socket_type != UnixSocketType::Stream
        && socket.socket_type != UnixSocketType::SeqPacket
    {
        return Err(-95); // EOPNOTSUPP
    }

    socket.state = UnixSocketState::Listening;
    socket.backlog = (backlog as usize).min(UNIX_SOCKET_MAX_BACKLOG);
    Ok(())
}

/// Connect to a unix socket
pub fn socket_connect(id: u32, path: &str) -> Result<(), i32> {
    let peer_id = {
        let paths = BOUND_PATHS.lock();
        *paths.get(path).ok_or(-111i32)? // ECONNREFUSED
    };

    let mut sockets = UNIX_SOCKETS.lock();

    // Check peer is listening
    let peer = sockets.get(&peer_id).ok_or(-111i32)?;
    if peer.state != UnixSocketState::Listening {
        return Err(-111); // ECONNREFUSED
    }
    if peer.pending_connections.len() >= peer.backlog {
        return Err(-11); // EAGAIN
    }

    // Create a connected pair
    let client = sockets.get_mut(&id).ok_or(-9i32)?;
    client.state = UnixSocketState::Connected;
    client.peer_id = Some(peer_id);

    // Add to peer's pending connections
    let peer = sockets.get_mut(&peer_id).ok_or(-111i32)?;
    peer.pending_connections.push_back(id);

    crate::serial_println!("[KnoxOS] unix socket {} connected to {}", id, path);
    Ok(())
}

/// Accept a connection on a listening unix socket
pub fn socket_accept(id: u32) -> Result<u32, i32> {
    let mut sockets = UNIX_SOCKETS.lock();
    let socket = sockets.get_mut(&id).ok_or(-9i32)?;

    if socket.state != UnixSocketState::Listening {
        return Err(-22); // EINVAL
    }

    let client_id = socket.pending_connections.pop_front().ok_or(-11i32)?; // EAGAIN

    // Create a new socket for the accepted connection
    let new_id = NEXT_UNIX_SOCKET_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let pid = socket.pid;
    let mut new_socket = UnixSocket::new(new_id, socket.socket_type, pid);
    new_socket.state = UnixSocketState::Connected;
    new_socket.peer_id = Some(client_id);

    // Update client's peer to point to new socket
    if let Some(client) = sockets.get_mut(&client_id) {
        client.peer_id = Some(new_id);
    }

    sockets.insert(new_id, new_socket);

    crate::serial_println!("[KnoxOS] unix socket {} accepted -> {}", id, new_id);
    Ok(new_id)
}

/// Send data through a unix socket
pub fn socket_send(id: u32, data: &[u8]) -> Result<usize, i32> {
    let sockets = UNIX_SOCKETS.lock();
    let socket = sockets.get(&id).ok_or(-9i32)?;

    if socket.state != UnixSocketState::Connected {
        return Err(-107); // ENOTCONN
    }

    let peer_id = socket.peer_id.ok_or(-107i32)?;
    drop(sockets);

    // Write to peer's receive buffer
    let mut sockets = UNIX_SOCKETS.lock();
    let peer = sockets.get_mut(&peer_id).ok_or(-32i32)?; // EPIPE (peer closed)

    if peer.recv_buf_size() + data.len() > UNIX_SOCKET_BUF_SIZE {
        return Err(-11); // EAGAIN
    }

    peer.recv_buffer.push_back(data.to_vec());
    Ok(data.len())
}

/// Receive data from a unix socket
pub fn socket_recv(id: u32, buf: &mut [u8]) -> Result<usize, i32> {
    let mut sockets = UNIX_SOCKETS.lock();
    let socket = sockets.get_mut(&id).ok_or(-9i32)?;

    if socket.state != UnixSocketState::Connected {
        return Err(-107); // ENOTCONN
    }

    if let Some(msg) = socket.recv_buffer.pop_front() {
        let copy_len = msg.len().min(buf.len());
        buf[..copy_len].copy_from_slice(&msg[..copy_len]);
        Ok(copy_len)
    } else {
        if socket.nonblocking {
            Err(-11) // EAGAIN
        } else {
            Ok(0) // EOF / no data
        }
    }
}

/// Send a datagram to a path (for DGRAM sockets)
pub fn socket_sendto(id: u32, data: &[u8], path: &str) -> Result<usize, i32> {
    let peer_id = {
        let paths = BOUND_PATHS.lock();
        *paths.get(path).ok_or(-2i32)? // ENOENT
    };

    let mut sockets = UNIX_SOCKETS.lock();
    let peer = sockets.get_mut(&peer_id).ok_or(-111i32)?;

    if peer.recv_buf_size() + data.len() > UNIX_SOCKET_BUF_SIZE {
        return Err(-11); // EAGAIN
    }

    peer.recv_buffer.push_back(data.to_vec());
    Ok(data.len())
}

/// Receive a datagram (for DGRAM sockets)
pub fn socket_recvfrom(id: u32, buf: &mut [u8]) -> Result<(usize, Option<String>), i32> {
    let mut sockets = UNIX_SOCKETS.lock();
    let socket = sockets.get_mut(&id).ok_or(-9i32)?;

    if let Some(msg) = socket.recv_buffer.pop_front() {
        let copy_len = msg.len().min(buf.len());
        buf[..copy_len].copy_from_slice(&msg[..copy_len]);
        Ok((copy_len, socket.bound_path.clone()))
    } else {
        if socket.nonblocking {
            Err(-11) // EAGAIN
        } else {
            Ok((0, None))
        }
    }
}

/// Close a unix socket
pub fn socket_close(id: u32) -> Result<(), i32> {
    let mut sockets = UNIX_SOCKETS.lock();

    if let Some(socket) = sockets.remove(&id) {
        // Remove from bound paths
        if let Some(ref path) = socket.bound_path {
            BOUND_PATHS.lock().remove(path);
        }

        // Notify peer
        if let Some(peer_id) = socket.peer_id {
            if let Some(peer) = sockets.get_mut(&peer_id) {
                peer.peer_id = None;
                peer.state = UnixSocketState::Closed;
            }
        }
    }

    Ok(())
}

/// Get socket credentials (SO_PEERCRED)
pub fn socket_get_credentials(id: u32) -> Result<(u32, u32, u32), i32> {
    let sockets = UNIX_SOCKETS.lock();
    let socket = sockets.get(&id).ok_or(-9i32)?;
    Ok((socket.pid, socket.uid, socket.gid))
}

/// Create a connected pair of unix sockets (socketpair)
pub fn socketpair(sock_type: UnixSocketType) -> Result<(u32, u32), i32> {
    let id1 = NEXT_UNIX_SOCKET_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let id2 = NEXT_UNIX_SOCKET_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let pid = crate::scheduler::current_pid().unwrap_or(1);

    let mut sock1 = UnixSocket::new(id1, sock_type, pid);
    let mut sock2 = UnixSocket::new(id2, sock_type, pid);
    sock1.state = UnixSocketState::Connected;
    sock1.peer_id = Some(id2);
    sock2.state = UnixSocketState::Connected;
    sock2.peer_id = Some(id1);

    let mut sockets = UNIX_SOCKETS.lock();
    sockets.insert(id1, sock1);
    sockets.insert(id2, sock2);

    crate::serial_println!("[KnoxOS] socketpair() = ({}, {})", id1, id2);
    Ok((id1, id2))
}

/// Initialize unix socket subsystem
pub fn init() {
    crate::serial_println!("[KnoxOS] Unix domain sockets initialized");
}
