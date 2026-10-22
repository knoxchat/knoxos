use core::sync::atomic::{AtomicU32, Ordering};

use crate::serial_println;

static SYSTEM_LISTEN_ID: AtomicU32 = AtomicU32::new(0);

/// Path of the live system bus AF_UNIX socket.
pub const SYSTEM_BUS_PATH: &str = "/run/dbus/system_bus_socket";
pub const GATE_L3_MARKER: &str = "GATE_L3 dbus unix";

/// Bind + listen `/run/dbus/system_bus_socket` once.
pub(super) fn ensure_system_bus_listen() -> Option<u32> {
    let existing = SYSTEM_LISTEN_ID.load(Ordering::Relaxed);
    if existing != 0 {
        return Some(existing);
    }
    if let Some(id) = crate::uds::socket_id_for_path(SYSTEM_BUS_PATH) {
        SYSTEM_LISTEN_ID.store(id, Ordering::Relaxed);
        return Some(id);
    }
    crate::vfs::ensure_directory("/run/dbus");
    let id = crate::uds::socket_create(crate::uds::UnixSocketType::Stream).ok()?;
    crate::uds::socket_bind(id, SYSTEM_BUS_PATH).ok()?;
    crate::uds::socket_listen(id, 16).ok()?;
    SYSTEM_LISTEN_ID.store(id, Ordering::Relaxed);
    Some(id)
}

/// Connect, send, and receive on `/run/dbus/system_bus_socket`.
pub fn unix_bus_self_test() -> bool {
    let Some(listen) = ensure_system_bus_listen() else {
        serial_println!("[dbus] Gate L3 FAILED: no listen socket");
        return false;
    };
    let Ok(client) = crate::uds::socket_create(crate::uds::UnixSocketType::Stream) else {
        serial_println!("[dbus] Gate L3 FAILED: client socket");
        return false;
    };
    if crate::uds::socket_connect(client, SYSTEM_BUS_PATH).is_err() {
        serial_println!("[dbus] Gate L3 FAILED: connect");
        return false;
    }
    let Ok(server) = crate::uds::socket_accept(listen) else {
        serial_println!("[dbus] Gate L3 FAILED: accept");
        return false;
    };
    let hello = b"Hello";
    if crate::uds::socket_send(client, hello) != Ok(hello.len()) {
        serial_println!("[dbus] Gate L3 FAILED: send");
        return false;
    }
    let mut buf = [0u8; 8];
    let n = crate::uds::socket_recv(server, &mut buf).unwrap_or(0);
    let _ = crate::uds::socket_close(client);
    let _ = crate::uds::socket_close(server);
    if n != hello.len() || &buf[..hello.len()] != hello {
        serial_println!("[dbus] Gate L3 FAILED: recv n={}", n);
        return false;
    }
    serial_println!("[dbus] {}", GATE_L3_MARKER);
    true
}
