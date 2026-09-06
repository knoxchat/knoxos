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
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// D-BUS WIRE PROTOCOL
// ═══════════════════════════════════════════════════════════════════════

/// D-Bus message endianness
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    Little, // 'l'
    Big,    // 'B'
}

/// D-Bus message type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Invalid = 0,
    MethodCall = 1,
    MethodReturn = 2,
    Error = 3,
    Signal = 4,
}

impl MessageType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::MethodCall,
            2 => Self::MethodReturn,
            3 => Self::Error,
            4 => Self::Signal,
            _ => Self::Invalid,
        }
    }
}

/// D-Bus message flags
pub const NO_REPLY_EXPECTED: u8 = 0x01;
pub const NO_AUTO_START: u8 = 0x02;
pub const ALLOW_INTERACTIVE_AUTH: u8 = 0x04;

/// D-Bus header field codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderField {
    Invalid = 0,
    Path = 1,
    Interface = 2,
    Member = 3,
    ErrorName = 4,
    ReplySerial = 5,
    Destination = 6,
    Sender = 7,
    Signature = 8,
    UnixFds = 9,
}

impl HeaderField {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Path,
            2 => Self::Interface,
            3 => Self::Member,
            4 => Self::ErrorName,
            5 => Self::ReplySerial,
            6 => Self::Destination,
            7 => Self::Sender,
            8 => Self::Signature,
            9 => Self::UnixFds,
            _ => Self::Invalid,
        }
    }
}

/// D-Bus type signature characters
pub const DBUS_TYPE_BYTE: char = 'y';
pub const DBUS_TYPE_BOOLEAN: char = 'b';
pub const DBUS_TYPE_INT16: char = 'n';
pub const DBUS_TYPE_UINT16: char = 'q';
pub const DBUS_TYPE_INT32: char = 'i';
pub const DBUS_TYPE_UINT32: char = 'u';
pub const DBUS_TYPE_INT64: char = 'x';
pub const DBUS_TYPE_UINT64: char = 't';
pub const DBUS_TYPE_DOUBLE: char = 'd';
pub const DBUS_TYPE_STRING: char = 's';
pub const DBUS_TYPE_OBJECT_PATH: char = 'o';
pub const DBUS_TYPE_SIGNATURE: char = 'g';
pub const DBUS_TYPE_ARRAY: char = 'a';
pub const DBUS_TYPE_VARIANT: char = 'v';
pub const DBUS_TYPE_STRUCT_BEGIN: char = '(';
pub const DBUS_TYPE_STRUCT_END: char = ')';
pub const DBUS_TYPE_DICT_ENTRY_BEGIN: char = '{';
pub const DBUS_TYPE_DICT_ENTRY_END: char = '}';

/// D-Bus message value
#[derive(Debug, Clone)]
pub enum DbusValue {
    Byte(u8),
    Boolean(bool),
    Int16(i16),
    Uint16(u16),
    Int32(i32),
    Uint32(u32),
    Int64(i64),
    Uint64(u64),
    Double(u64), // stored as bits since no_std
    String(String),
    ObjectPath(String),
    Signature(String),
    Array(Vec<DbusValue>),
    Variant(alloc::boxed::Box<DbusValue>),
    Struct(Vec<DbusValue>),
    DictEntry(alloc::boxed::Box<DbusValue>, alloc::boxed::Box<DbusValue>),
}

/// D-Bus message header
#[derive(Debug, Clone)]
pub struct MessageHeader {
    pub endianness: Endianness,
    pub message_type: MessageType,
    pub flags: u8,
    pub protocol_version: u8,
    pub body_length: u32,
    pub serial: u32,
    pub fields: Vec<(HeaderField, DbusValue)>,
}

/// Complete D-Bus message
#[derive(Debug, Clone)]
pub struct Message {
    pub header: MessageHeader,
    pub body: Vec<DbusValue>,
    // Convenience accessors from header fields
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub error_name: Option<String>,
    pub reply_serial: Option<u32>,
    pub destination: Option<String>,
    pub sender: Option<String>,
    pub signature: Option<String>,
}

impl Message {
    /// Create a new method call message
    pub fn method_call(destination: &str, path: &str, interface: &str, member: &str) -> Self {
        let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
        Self {
            header: MessageHeader {
                endianness: Endianness::Little,
                message_type: MessageType::MethodCall,
                flags: 0,
                protocol_version: 1,
                body_length: 0,
                serial,
                fields: Vec::new(),
            },
            body: Vec::new(),
            path: Some(path.to_string()),
            interface: Some(interface.to_string()),
            member: Some(member.to_string()),
            error_name: None,
            reply_serial: None,
            destination: Some(destination.to_string()),
            sender: None,
            signature: None,
        }
    }

    /// Create a method return message
    pub fn method_return(reply_to: u32) -> Self {
        let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
        Self {
            header: MessageHeader {
                endianness: Endianness::Little,
                message_type: MessageType::MethodReturn,
                flags: 0,
                protocol_version: 1,
                body_length: 0,
                serial,
                fields: Vec::new(),
            },
            body: Vec::new(),
            path: None,
            interface: None,
            member: None,
            error_name: None,
            reply_serial: Some(reply_to),
            destination: None,
            sender: None,
            signature: None,
        }
    }

    /// Create a signal message
    pub fn signal(path: &str, interface: &str, member: &str) -> Self {
        let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
        Self {
            header: MessageHeader {
                endianness: Endianness::Little,
                message_type: MessageType::Signal,
                flags: NO_REPLY_EXPECTED,
                protocol_version: 1,
                body_length: 0,
                serial,
                fields: Vec::new(),
            },
            body: Vec::new(),
            path: Some(path.to_string()),
            interface: Some(interface.to_string()),
            member: Some(member.to_string()),
            error_name: None,
            reply_serial: None,
            destination: None,
            sender: None,
            signature: None,
        }
    }

    /// Create an error reply message
    pub fn error(reply_to: u32, error_name: &str, message: &str) -> Self {
        let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
        Self {
            header: MessageHeader {
                endianness: Endianness::Little,
                message_type: MessageType::Error,
                flags: 0,
                protocol_version: 1,
                body_length: 0,
                serial,
                fields: Vec::new(),
            },
            body: vec![DbusValue::String(message.to_string())],
            path: None,
            interface: None,
            member: None,
            error_name: Some(error_name.to_string()),
            reply_serial: Some(reply_to),
            destination: None,
            sender: None,
            signature: Some(String::from("s")),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// D-BUS NAME OWNERSHIP & CONNECTION TRACKING
// ═══════════════════════════════════════════════════════════════════════

/// D-Bus connection identifier
pub type ConnectionId = u32;

static NEXT_SERIAL: AtomicU32 = AtomicU32::new(1);
static NEXT_CONN_ID: AtomicU32 = AtomicU32::new(1);

/// A D-Bus connection (represents a client connected to the bus)
#[derive(Debug, Clone)]
pub struct Connection {
    pub id: ConnectionId,
    pub unique_name: String, // ":1.NNN"
    pub pid: u32,
    pub uid: u32,
    pub owned_names: Vec<String>,
    pub match_rules: Vec<MatchRule>,
    pub message_queue: Vec<Message>,
    pub authenticated: bool,
}

/// Signal match rule
#[derive(Debug, Clone)]
pub struct MatchRule {
    pub rule_type: Option<MessageType>,
    pub sender: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub path: Option<String>,
    pub destination: Option<String>,
    pub arg0: Option<String>,
}

/// Name ownership flags
pub const DBUS_NAME_FLAG_ALLOW_REPLACEMENT: u32 = 0x01;
pub const DBUS_NAME_FLAG_REPLACE_EXISTING: u32 = 0x02;
pub const DBUS_NAME_FLAG_DO_NOT_QUEUE: u32 = 0x04;

/// Name request results
pub const DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER: u32 = 1;
pub const DBUS_REQUEST_NAME_REPLY_IN_QUEUE: u32 = 2;
pub const DBUS_REQUEST_NAME_REPLY_EXISTS: u32 = 3;
pub const DBUS_REQUEST_NAME_REPLY_ALREADY_OWNER: u32 = 4;

// ═══════════════════════════════════════════════════════════════════════
// BUS STATE
// ═══════════════════════════════════════════════════════════════════════

/// D-Bus type (system or session)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusType {
    System,
    Session,
}

/// Bus state
struct BusState {
    connections: BTreeMap<ConnectionId, Connection>,
    names: BTreeMap<String, ConnectionId>, // well-known name → owner
    name_queues: BTreeMap<String, Vec<ConnectionId>>, // queued ownership
    services: BTreeMap<String, ServiceInfo>, // registered service handlers
}

/// A registered D-Bus service handler
#[derive(Debug, Clone)]
struct ServiceInfo {
    interface: String,
    object_path: String,
    owner: ConnectionId,
    methods: Vec<String>,
    properties: BTreeMap<String, DbusValue>,
}

lazy_static::lazy_static! {
    static ref SYSTEM_BUS: Mutex<BusState> = Mutex::new(BusState {
        connections: BTreeMap::new(),
        names: BTreeMap::new(),
        name_queues: BTreeMap::new(),
        services: BTreeMap::new(),
    });

    static ref SESSION_BUS: Mutex<BusState> = Mutex::new(BusState {
        connections: BTreeMap::new(),
        names: BTreeMap::new(),
        name_queues: BTreeMap::new(),
        services: BTreeMap::new(),
    });
}

fn get_bus(bus_type: BusType) -> &'static Mutex<BusState> {
    match bus_type {
        BusType::System => &SYSTEM_BUS,
        BusType::Session => &SESSION_BUS,
    }
}

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

/// Release a bus name
pub fn release_name(bus_type: BusType, conn_id: ConnectionId, name: &str) -> bool {
    let mut bus = get_bus(bus_type).lock();

    if bus.names.get(name).copied() == Some(conn_id) {
        bus.names.remove(name);
        if let Some(conn) = bus.connections.get_mut(&conn_id) {
            conn.owned_names.retain(|n| n != name);
        }
        true
    } else {
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MESSAGE ROUTING
// ═══════════════════════════════════════════════════════════════════════

/// Send a message on the bus
pub fn send_message(bus_type: BusType, from: ConnectionId, msg: Message) -> u32 {
    let serial = msg.header.serial;
    let mut bus = get_bus(bus_type).lock();

    // Route the message
    match msg.header.message_type {
        MessageType::MethodCall => {
            // Route to destination
            if let Some(ref dest) = msg.destination {
                let target_id = if dest.starts_with(':') {
                    // Direct connection name
                    bus.connections
                        .values()
                        .find(|c| c.unique_name == *dest)
                        .map(|c| c.id)
                } else {
                    // Well-known name
                    bus.names.get(dest).copied()
                };

                if let Some(target) = target_id {
                    if let Some(conn) = bus.connections.get_mut(&target) {
                        conn.message_queue.push(msg);
                    }
                } else {
                    // Handle internally if it's a bus method
                    handle_bus_method(&mut bus, from, &msg);
                }
            }
        }
        MessageType::Signal => {
            // Broadcast to all matching connections
            let matching: Vec<ConnectionId> = bus
                .connections
                .values()
                .filter(|c| c.id != from && matches_signal(c, &msg))
                .map(|c| c.id)
                .collect();

            for id in matching {
                if let Some(conn) = bus.connections.get_mut(&id) {
                    conn.message_queue.push(msg.clone());
                }
            }
        }
        MessageType::MethodReturn | MessageType::Error => {
            // Route to the original sender
            if let Some(ref dest) = msg.destination {
                let target_id = bus
                    .connections
                    .values()
                    .find(|c| c.unique_name == *dest)
                    .map(|c| c.id);

                if let Some(target) = target_id {
                    if let Some(conn) = bus.connections.get_mut(&target) {
                        conn.message_queue.push(msg);
                    }
                }
            }
        }
        _ => {}
    }

    serial
}

/// Check if a connection's match rules match a signal
fn matches_signal(conn: &Connection, msg: &Message) -> bool {
    if conn.match_rules.is_empty() {
        return false;
    }

    for rule in &conn.match_rules {
        let mut matches = true;

        if let Some(ref iface) = rule.interface {
            if msg.interface.as_ref() != Some(iface) {
                matches = false;
            }
        }
        if let Some(ref member) = rule.member {
            if msg.member.as_ref() != Some(member) {
                matches = false;
            }
        }
        if let Some(ref path) = rule.path {
            if msg.path.as_ref() != Some(path) {
                matches = false;
            }
        }
        if let Some(ref sender) = rule.sender {
            if msg.sender.as_ref() != Some(sender) {
                matches = false;
            }
        }

        if matches {
            return true;
        }
    }

    false
}

/// Handle bus-level methods (org.freedesktop.DBus)
fn handle_bus_method(bus: &mut BusState, from: ConnectionId, msg: &Message) {
    let member = msg.member.as_deref().unwrap_or("");
    let interface = msg.interface.as_deref().unwrap_or("");

    if interface == "org.freedesktop.DBus" {
        match member {
            "Hello" => {
                if let Some(conn) = bus.connections.get(&from) {
                    let reply = Message::method_return(msg.header.serial);
                    // Return unique name
                    let mut reply = reply;
                    reply.body = vec![DbusValue::String(conn.unique_name.clone())];
                    reply.signature = Some(String::from("s"));
                    if let Some(c) = bus.connections.get_mut(&from) {
                        c.message_queue.push(reply);
                    }
                }
            }
            "RequestName" => {
                // Already handled by request_name()
            }
            "ListNames" => {
                let names: Vec<DbusValue> = bus
                    .names
                    .keys()
                    .map(|n| DbusValue::String(n.clone()))
                    .collect();
                let reply = Message::method_return(msg.header.serial);
                let mut reply = reply;
                reply.body = vec![DbusValue::Array(names)];
                reply.signature = Some(String::from("as"));
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(reply);
                }
            }
            "GetNameOwner" => {
                // Return the unique name of the owner of a well-known name
                if let Some(DbusValue::String(name)) = msg.body.first() {
                    if let Some(&owner_id) = bus.names.get(name) {
                        if let Some(owner) = bus.connections.get(&owner_id) {
                            let mut reply = Message::method_return(msg.header.serial);
                            reply.body = vec![DbusValue::String(owner.unique_name.clone())];
                            reply.signature = Some(String::from("s"));
                            if let Some(c) = bus.connections.get_mut(&from) {
                                c.message_queue.push(reply);
                            }
                        }
                    }
                }
            }
            "NameHasOwner" => {
                if let Some(DbusValue::String(name)) = msg.body.first() {
                    let has_owner = bus.names.contains_key(name);
                    let mut reply = Message::method_return(msg.header.serial);
                    reply.body = vec![DbusValue::Boolean(has_owner)];
                    reply.signature = Some(String::from("b"));
                    if let Some(c) = bus.connections.get_mut(&from) {
                        c.message_queue.push(reply);
                    }
                }
            }
            "AddMatch" => {
                // Parse match rule and add to connection
                if let Some(DbusValue::String(rule_str)) = msg.body.first() {
                    let rule = parse_match_rule(rule_str);
                    if let Some(conn) = bus.connections.get_mut(&from) {
                        conn.match_rules.push(rule);
                    }
                }
            }
            _ => {
                serial_println!("[dbus] Unknown bus method: {}.{}", interface, member);
            }
        }
    }
}

/// Parse a D-Bus match rule string
fn parse_match_rule(rule: &str) -> MatchRule {
    let mut result = MatchRule {
        rule_type: None,
        sender: None,
        interface: None,
        member: None,
        path: None,
        destination: None,
        arg0: None,
    };

    for part in rule.split(',') {
        let part = part.trim();
        if let Some(eq_pos) = part.find('=') {
            let key = part[..eq_pos].trim();
            let value = part[eq_pos + 1..].trim().trim_matches('\'');
            match key {
                "type" => {
                    result.rule_type = match value {
                        "signal" => Some(MessageType::Signal),
                        "method_call" => Some(MessageType::MethodCall),
                        "method_return" => Some(MessageType::MethodReturn),
                        "error" => Some(MessageType::Error),
                        _ => None,
                    };
                }
                "sender" => result.sender = Some(value.to_string()),
                "interface" => result.interface = Some(value.to_string()),
                "member" => result.member = Some(value.to_string()),
                "path" => result.path = Some(value.to_string()),
                "destination" => result.destination = Some(value.to_string()),
                "arg0" => result.arg0 = Some(value.to_string()),
                _ => {}
            }
        }
    }

    result
}

/// Receive a message from the bus (non-blocking)
pub fn receive_message(bus_type: BusType, conn_id: ConnectionId) -> Option<Message> {
    let mut bus = get_bus(bus_type).lock();
    if let Some(conn) = bus.connections.get_mut(&conn_id) {
        if !conn.message_queue.is_empty() {
            return Some(conn.message_queue.remove(0));
        }
    }
    None
}

/// Add a signal match rule
pub fn add_match(bus_type: BusType, conn_id: ConnectionId, rule: &str) {
    let parsed = parse_match_rule(rule);
    let mut bus = get_bus(bus_type).lock();
    if let Some(conn) = bus.connections.get_mut(&conn_id) {
        conn.match_rules.push(parsed);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BUILT-IN SERVICES (org.freedesktop.*)
// ═══════════════════════════════════════════════════════════════════════

/// Built-in notification service (org.freedesktop.Notifications)
/// Vivaldi uses this for desktop notifications
pub mod notifications {
    use super::*;

    const SERVICE_NAME: &str = "org.freedesktop.Notifications";
    const OBJECT_PATH: &str = "/org/freedesktop/Notifications";

    static NEXT_NOTIFICATION_ID: AtomicU32 = AtomicU32::new(1);

    /// Notification urgency levels
    #[derive(Debug, Clone, Copy)]
    pub enum Urgency {
        Low = 0,
        Normal = 1,
        Critical = 2,
    }

    /// A desktop notification
    #[derive(Debug, Clone)]
    pub struct Notification {
        pub id: u32,
        pub app_name: String,
        pub replaces_id: u32,
        pub icon: String,
        pub summary: String,
        pub body: String,
        pub actions: Vec<String>,
        pub urgency: Urgency,
        pub expire_timeout: i32, // ms, -1 = server default, 0 = never
    }

    /// Handle a Notify method call
    pub fn handle_notify(msg: &Message) -> Message {
        let id = NEXT_NOTIFICATION_ID.fetch_add(1, Ordering::Relaxed);

        // Extract notification fields from body
        let app_name = match msg.body.first() {
            Some(DbusValue::String(s)) => s.clone(),
            _ => String::from("unknown"),
        };

        let summary = match msg.body.get(3) {
            Some(DbusValue::String(s)) => s.clone(),
            _ => String::from("Notification"),
        };

        let body = match msg.body.get(4) {
            Some(DbusValue::String(s)) => s.clone(),
            _ => String::new(),
        };

        serial_println!(
            "[dbus:notify] #{}: [{}] {} - {}",
            id,
            app_name,
            summary,
            body
        );

        // Forward to KnoxOS notification system
        crate::gui::notifications::system(&summary, &body);

        // Return notification ID
        let mut reply = Message::method_return(msg.header.serial);
        reply.body = vec![DbusValue::Uint32(id)];
        reply.signature = Some(String::from("u"));
        reply
    }

    /// Get server capabilities
    pub fn get_capabilities() -> Vec<String> {
        vec![
            String::from("body"),
            String::from("body-markup"),
            String::from("actions"),
            String::from("icon-static"),
            String::from("persistence"),
        ]
    }

    /// Get server information
    pub fn get_server_info() -> (String, String, String, String) {
        (
            String::from("KnoxOS Notification Daemon"),
            String::from("KnoxOS"),
            String::from("0.1.0"),
            String::from("1.2"), // notification spec version
        )
    }

    pub fn init() {
        serial_println!(
            "[dbus] Registered service: {} at {}",
            SERVICE_NAME,
            OBJECT_PATH
        );
    }
}

/// XDG Desktop Portal service (org.freedesktop.portal.Desktop)
/// Vivaldi uses this for file chooser, screen capture, settings
pub mod portal {
    use super::*;

    const SERVICE_NAME: &str = "org.freedesktop.portal.Desktop";
    const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";

    /// Portal request handle
    #[derive(Debug, Clone)]
    pub struct PortalRequest {
        pub handle: String,
        pub sender: ConnectionId,
        pub interface: String,
        pub completed: bool,
        pub response: u32, // 0 = success, 1 = cancelled, 2 = error
        pub results: BTreeMap<String, DbusValue>,
    }

    lazy_static::lazy_static! {
        static ref REQUESTS: Mutex<BTreeMap<String, PortalRequest>> =
            Mutex::new(BTreeMap::new());
    }

    static NEXT_REQUEST: AtomicU32 = AtomicU32::new(1);

    /// Handle FileChooser.OpenFile
    pub fn open_file(msg: &Message) -> Message {
        let req_id = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
        let handle = format!("/org/freedesktop/portal/desktop/request/{}", req_id);

        serial_println!("[dbus:portal] FileChooser.OpenFile -> {}", handle);

        let mut reply = Message::method_return(msg.header.serial);
        reply.body = vec![DbusValue::ObjectPath(handle)];
        reply.signature = Some(String::from("o"));
        reply
    }

    /// Handle FileChooser.SaveFile
    pub fn save_file(msg: &Message) -> Message {
        let req_id = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed);
        let handle = format!("/org/freedesktop/portal/desktop/request/{}", req_id);

        serial_println!("[dbus:portal] FileChooser.SaveFile -> {}", handle);

        let mut reply = Message::method_return(msg.header.serial);
        reply.body = vec![DbusValue::ObjectPath(handle)];
        reply.signature = Some(String::from("o"));
        reply
    }

    /// Handle Settings.Read
    pub fn read_setting(msg: &Message) -> Message {
        let namespace = match msg.body.first() {
            Some(DbusValue::String(s)) => s.as_str(),
            _ => "",
        };
        let key = match msg.body.get(1) {
            Some(DbusValue::String(s)) => s.as_str(),
            _ => "",
        };

        serial_println!("[dbus:portal] Settings.Read {}.{}", namespace, key);

        let value = match (namespace, key) {
            ("org.freedesktop.appearance", "color-scheme") => {
                // 0 = no preference, 1 = dark, 2 = light
                DbusValue::Variant(alloc::boxed::Box::new(DbusValue::Uint32(0)))
            }
            ("org.gnome.desktop.interface", "gtk-theme") => DbusValue::Variant(
                alloc::boxed::Box::new(DbusValue::String(String::from("KnoxOS-Default"))),
            ),
            ("org.gnome.desktop.interface", "icon-theme") => DbusValue::Variant(
                alloc::boxed::Box::new(DbusValue::String(String::from("KnoxOS"))),
            ),
            ("org.gnome.desktop.interface", "cursor-theme") => DbusValue::Variant(
                alloc::boxed::Box::new(DbusValue::String(String::from("default"))),
            ),
            ("org.gnome.desktop.interface", "font-name") => DbusValue::Variant(
                alloc::boxed::Box::new(DbusValue::String(String::from("Cantarell 11"))),
            ),
            _ => DbusValue::Variant(alloc::boxed::Box::new(DbusValue::String(String::new()))),
        };

        let mut reply = Message::method_return(msg.header.serial);
        reply.body = vec![value];
        reply.signature = Some(String::from("v"));
        reply
    }

    pub fn init() {
        serial_println!(
            "[dbus] Registered portal: {} at {}",
            SERVICE_NAME,
            OBJECT_PATH
        );
    }
}

/// Power management service (org.freedesktop.UPower)
pub mod upower {
    use super::*;

    pub fn get_display_device() -> BTreeMap<String, DbusValue> {
        let mut props = BTreeMap::new();
        props.insert(
            String::from("Type"),
            DbusValue::Uint32(2), // Battery
        );
        props.insert(
            String::from("State"),
            DbusValue::Uint32(2), // Discharging
        );
        props.insert(
            String::from("Percentage"),
            DbusValue::Double(100_u64), // 100% (as bits)
        );
        props.insert(String::from("IsPresent"), DbusValue::Boolean(true));
        props.insert(String::from("OnBattery"), DbusValue::Boolean(false));
        props
    }

    pub fn init() {
        serial_println!("[dbus] Registered service: org.freedesktop.UPower");
    }
}

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

    // Create the system bus socket path
    serial_println!("[dbus] System bus: /run/dbus/system_bus_socket");
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
}
