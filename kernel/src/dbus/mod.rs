/// D-Bus — Desktop Bus IPC Implementation
///
/// Provides Linux-compatible D-Bus message bus for desktop integration.
/// Vivaldi browser requires D-Bus for:
///   - Desktop notifications (org.freedesktop.Notifications)
///   - File chooser portals (org.freedesktop.portal.FileChooser)
///   - Screen capture portals (org.freedesktop.portal.ScreenCast)
///   - Settings/appearance (org.freedesktop.portal.Settings)
///   - Secret service (org.freedesktop.secrets)
///   - Power management (org.freedesktop.UPower)
///   - NetworkManager status
///   - MPRIS2 media control
///
/// Implementation:
///   - System bus (/run/dbus/system_bus_socket)
///   - Session bus (/run/user/1000/bus)
///   - Wire protocol (header + body marshalling)
///   - Name ownership (RequestName, ReleaseName)
///   - Signal broadcasting
///   - Method call dispatch
///   - Property access (Get/Set/GetAll)
///   - Introspection support
///
/// Module layout:
///   protocol       — wire protocol types and message constructors
///   state          — connections, name flags, bus globals
///   connection     — connect/disconnect and name ownership
///   routing        — method dispatch, signals, match rules
///   notifications  — org.freedesktop.Notifications
///   portal         — XDG desktop portal
///   upower         — org.freedesktop.UPower
///   unix           — AF_UNIX system bus socket
///   gates          — GATE_DBUS self-tests
mod connection;
mod gates;
mod protocol;
mod routing;
mod state;
mod unix;

pub mod notifications;
pub mod portal;
pub mod upower;

pub use connection::{connect, disconnect, release_name, request_name};
pub use gates::{
    GATE_DBUS_CREDS_MARKER, GATE_DBUS_HAS_OWNER_MARKER, GATE_DBUS_ID_MARKER, GATE_DBUS_LIST_MARKER,
    GATE_DBUS_MATCH_MARKER, GATE_DBUS_METHOD_MARKER, GATE_DBUS_NAME_MARKER, GATE_DBUS_OWNER_MARKER,
    GATE_DBUS_PID_MARKER, GATE_DBUS_RELEASE_MARKER, GATE_DBUS_REMOVE_MARKER,
    GATE_DBUS_START_MARKER, GATE_DBUS_UID_MARKER, add_match_self_test, creds_self_test,
    get_id_self_test, has_owner_self_test, list_self_test, method_self_test, name_self_test,
    owner_self_test, pid_self_test, release_self_test, remove_match_self_test, start_self_test,
    uid_self_test,
};
pub use protocol::{
    ALLOW_INTERACTIVE_AUTH, DBUS_TYPE_ARRAY, DBUS_TYPE_BOOLEAN, DBUS_TYPE_BYTE,
    DBUS_TYPE_DICT_ENTRY_BEGIN, DBUS_TYPE_DICT_ENTRY_END, DBUS_TYPE_DOUBLE, DBUS_TYPE_INT16,
    DBUS_TYPE_INT32, DBUS_TYPE_INT64, DBUS_TYPE_OBJECT_PATH, DBUS_TYPE_SIGNATURE, DBUS_TYPE_STRING,
    DBUS_TYPE_STRUCT_BEGIN, DBUS_TYPE_STRUCT_END, DBUS_TYPE_UINT16, DBUS_TYPE_UINT32,
    DBUS_TYPE_UINT64, DBUS_TYPE_VARIANT, DbusValue, Endianness, HeaderField, Message,
    MessageHeader, MessageType, NO_AUTO_START, NO_REPLY_EXPECTED,
};
pub use routing::{add_match, receive_message, remove_match, send_message};
pub use state::{
    BusType, Connection, ConnectionId, DBUS_NAME_FLAG_ALLOW_REPLACEMENT,
    DBUS_NAME_FLAG_DO_NOT_QUEUE, DBUS_NAME_FLAG_REPLACE_EXISTING,
    DBUS_RELEASE_NAME_REPLY_NON_EXISTENT, DBUS_RELEASE_NAME_REPLY_NOT_OWNER,
    DBUS_RELEASE_NAME_REPLY_RELEASED, DBUS_REQUEST_NAME_REPLY_ALREADY_OWNER,
    DBUS_REQUEST_NAME_REPLY_EXISTS, DBUS_REQUEST_NAME_REPLY_IN_QUEUE,
    DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER, DBUS_START_REPLY_ALREADY_RUNNING,
    DBUS_START_REPLY_SUCCESS, MACHINE_ID, MatchRule,
};
pub use unix::{GATE_L3_MARKER, SYSTEM_BUS_PATH, unix_bus_self_test};

use crate::serial_println;

use state::{SESSION_BUS, SYSTEM_BUS};
use unix::ensure_system_bus_listen;

// ═══════════════════════════════════════════════════════════════════════
// CONVENIENCE FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════

/// Send a desktop notification via D-Bus (convenience wrapper)
pub fn send_notification(summary: &str, body: &str, icon: &str) {
    serial_println!("[dbus] Notification: [{}] {} — {}", icon, summary, body);
    // Forward to KnoxOS GUI notification system
    crate::gui::notifications::system(summary, body);
}

/// Connect a client to the D-Bus session bus (simplified convenience wrapper)
/// Returns 0 if session bus not yet started, otherwise the connection ID.
pub fn connect_session(app_name: &str) -> ConnectionId {
    serial_println!("[dbus] Client connecting: {}", app_name);
    connect(BusType::Session, 0, 1000)
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the D-Bus subsystem
pub fn init() {
    serial_println!("[dbus] D-Bus message bus initializing...");

    crate::vfs::ensure_directory("/run/dbus");
    if let Some(id) = ensure_system_bus_listen() {
        serial_println!("[dbus] System bus: {} (id={})", SYSTEM_BUS_PATH, id);
    } else {
        serial_println!("[dbus] System bus bind/listen failed");
    }
    serial_println!("[dbus] Session bus: /run/user/1000/bus");

    // Register built-in services
    notifications::init();
    portal::init();
    upower::init();

    // Register well-known names for built-in services
    let system_conn = connect(BusType::System, 0, 0); // kernel PID 0
    request_name(BusType::System, system_conn, "org.freedesktop.DBus", 0);
    request_name(BusType::System, system_conn, "org.freedesktop.UPower", 0);

    let session_conn = connect(BusType::Session, 0, 1000);
    request_name(BusType::Session, session_conn, "org.freedesktop.DBus", 0);
    request_name(
        BusType::Session,
        session_conn,
        "org.freedesktop.Notifications",
        0,
    );
    request_name(
        BusType::Session,
        session_conn,
        "org.freedesktop.portal.Desktop",
        0,
    );
    request_name(
        BusType::Session,
        session_conn,
        "org.freedesktop.portal.FileChooser",
        0,
    );
    request_name(
        BusType::Session,
        session_conn,
        "org.freedesktop.portal.Settings",
        0,
    );

    serial_println!(
        "[dbus] D-Bus initialized ({} system names, {} session names)",
        SYSTEM_BUS.lock().names.len(),
        SESSION_BUS.lock().names.len()
    );
    let _ = unix_bus_self_test();
    let _ = method_self_test();
    let _ = name_self_test();
    let _ = release_self_test();
    let _ = list_self_test();
    let _ = pid_self_test();
    let _ = owner_self_test();
    let _ = has_owner_self_test();
    let _ = uid_self_test();
    let _ = add_match_self_test();
    let _ = remove_match_self_test();
    let _ = get_id_self_test();
    let _ = start_self_test();
    let _ = creds_self_test();
}
