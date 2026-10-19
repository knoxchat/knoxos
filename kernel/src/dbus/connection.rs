use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

use crate::serial_println;

use super::state::{
    BusState, BusType, Connection, ConnectionId, DBUS_NAME_FLAG_DO_NOT_QUEUE,
    DBUS_NAME_FLAG_REPLACE_EXISTING, DBUS_RELEASE_NAME_REPLY_NON_EXISTENT,
    DBUS_RELEASE_NAME_REPLY_NOT_OWNER, DBUS_RELEASE_NAME_REPLY_RELEASED,
    DBUS_REQUEST_NAME_REPLY_ALREADY_OWNER, DBUS_REQUEST_NAME_REPLY_EXISTS,
    DBUS_REQUEST_NAME_REPLY_IN_QUEUE, DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER, NEXT_CONN_ID, get_bus,
};

// ═══════════════════════════════════════════════════════════════════════
// CONNECTION MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════

/// Connect to a D-Bus bus, returns a connection ID
pub fn connect(bus_type: BusType, pid: u32, uid: u32) -> ConnectionId {
    let id = NEXT_CONN_ID.fetch_add(1, Ordering::Relaxed);
    let unique_name = format!(":1.{}", id);

    let conn = Connection {
        id,
        unique_name: unique_name.clone(),
        pid,
        uid,
        owned_names: Vec::new(),
        match_rules: Vec::new(),
        message_queue: Vec::new(),
        authenticated: true, // auto-authenticate in kernel
    };

    let mut bus = get_bus(bus_type).lock();
    bus.connections.insert(id, conn);

    serial_println!(
        "[dbus] New connection {} on {:?} bus (pid={})",
        unique_name,
        bus_type,
        pid
    );

    id
}

/// Disconnect from the bus
pub fn disconnect(bus_type: BusType, conn_id: ConnectionId) {
    let mut bus = get_bus(bus_type).lock();

    if let Some(conn) = bus.connections.remove(&conn_id) {
        // Release all owned names
        for name in &conn.owned_names {
            bus.names.remove(name);
            // Activate next in queue
            let next = bus.name_queues.get_mut(name).and_then(|queue| {
                let next_id = queue.first().copied();
                if next_id.is_some() {
                    queue.remove(0);
                }
                next_id
            });
            if let Some(next_id) = next {
                bus.names.insert(name.clone(), next_id);
            }
        }

        serial_println!("[dbus] Disconnected {}", conn.unique_name);
    }
}

/// Request a well-known bus name
pub fn request_name(bus_type: BusType, conn_id: ConnectionId, name: &str, flags: u32) -> u32 {
    let mut bus = get_bus(bus_type).lock();
    request_name_on_bus(&mut bus, conn_id, name, flags)
}

pub(super) fn request_name_on_bus(
    bus: &mut BusState,
    conn_id: ConnectionId,
    name: &str,
    flags: u32,
) -> u32 {
    // Check if name is already owned
    if let Some(&current_owner) = bus.names.get(name) {
        if current_owner == conn_id {
            return DBUS_REQUEST_NAME_REPLY_ALREADY_OWNER;
        }

        if flags & DBUS_NAME_FLAG_REPLACE_EXISTING != 0 {
            // Try to replace
            bus.names.insert(name.to_string(), conn_id);
            if let Some(conn) = bus.connections.get_mut(&conn_id) {
                conn.owned_names.push(name.to_string());
            }
            serial_println!("[dbus] {} took ownership of {}", conn_id, name);
            return DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER;
        }

        if flags & DBUS_NAME_FLAG_DO_NOT_QUEUE != 0 {
            return DBUS_REQUEST_NAME_REPLY_EXISTS;
        }

        // Queue
        bus.name_queues
            .entry(name.to_string())
            .or_default()
            .push(conn_id);
        return DBUS_REQUEST_NAME_REPLY_IN_QUEUE;
    }

    // Name is free, take it
    bus.names.insert(name.to_string(), conn_id);
    if let Some(conn) = bus.connections.get_mut(&conn_id) {
        conn.owned_names.push(name.to_string());
    }

    serial_println!("[dbus] {} owns {}", conn_id, name);
    DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER
}

/// Release a bus name. True when this connection was the owner.
pub fn release_name(bus_type: BusType, conn_id: ConnectionId, name: &str) -> bool {
    let mut bus = get_bus(bus_type).lock();
    release_name_on_bus(&mut bus, conn_id, name) == DBUS_RELEASE_NAME_REPLY_RELEASED
}

pub(super) fn release_name_on_bus(bus: &mut BusState, conn_id: ConnectionId, name: &str) -> u32 {
    match bus.names.get(name).copied() {
        Some(owner) if owner == conn_id => {
            bus.names.remove(name);
            if let Some(conn) = bus.connections.get_mut(&conn_id) {
                conn.owned_names.retain(|n| n != name);
            }
            DBUS_RELEASE_NAME_REPLY_RELEASED
        }
        Some(_) => DBUS_RELEASE_NAME_REPLY_NOT_OWNER,
        None => DBUS_RELEASE_NAME_REPLY_NON_EXISTENT,
    }
}
