use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::AtomicU32;
use spin::Mutex;

use super::protocol::{DbusValue, Message, MessageType};

// ═══════════════════════════════════════════════════════════════════════
// D-BUS NAME OWNERSHIP & CONNECTION TRACKING
// ═══════════════════════════════════════════════════════════════════════

/// D-Bus connection identifier
pub type ConnectionId = u32;

pub(super) static NEXT_CONN_ID: AtomicU32 = AtomicU32::new(1);

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
    pub monitoring: bool,
}

/// Signal match rule
#[derive(Debug, Clone, PartialEq, Eq)]
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
pub const DBUS_RELEASE_NAME_REPLY_RELEASED: u32 = 1;
pub const DBUS_RELEASE_NAME_REPLY_NON_EXISTENT: u32 = 2;
pub const DBUS_RELEASE_NAME_REPLY_NOT_OWNER: u32 = 3;
pub const DBUS_START_REPLY_SUCCESS: u32 = 1;
pub const DBUS_START_REPLY_ALREADY_RUNNING: u32 = 2;

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
pub(super) struct BusState {
    pub(super) connections: BTreeMap<ConnectionId, Connection>,
    pub(super) names: BTreeMap<String, ConnectionId>, // well-known name → owner
    pub(super) name_queues: BTreeMap<String, Vec<ConnectionId>>, // queued ownership
    pub(super) activatable_names: BTreeSet<String>,
    pub(super) services: BTreeMap<String, ServiceInfo>, // registered service handlers
    pub(super) machine_id: String,
    pub(super) activation_env: BTreeMap<String, String>,
    pub(super) config_generation: u32,
}

/// A registered D-Bus service handler
#[derive(Debug, Clone)]
pub(super) struct ServiceInfo {
    interface: String,
    object_path: String,
    owner: ConnectionId,
    methods: Vec<String>,
    properties: BTreeMap<String, DbusValue>,
}

/// Stable 32-hex machine UUID returned by `org.freedesktop.DBus.GetId`.
pub const MACHINE_ID: &str = "0123456789abcdef0123456789abcdef";

/// Well-known name advertised by `ListActivatableNames`.
pub const GATE_ACTIVATABLE_NAME: &str = "org.knoxos.GateActivatable";

/// SELinux context returned by `GetConnectionSELinuxSecurityContext`.
pub const GATE_SELINUX_CONTEXT: &str = "system_u:system_r:unconfined_t:s0";

/// AppArmor profile returned by `GetConnectionAppArmorSecurityContext`.
pub const GATE_APPARMOR_PROFILE: &str = "unconfined";

/// ADT audit session bytes returned by `GetAdtAuditSessionData`.
pub const GATE_ADT_SESSION: &str = "knoxos-adt-session";

fn default_activatable_names() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    names.insert(String::from(GATE_ACTIVATABLE_NAME));
    names
}

lazy_static::lazy_static! {
    pub(super) static ref SYSTEM_BUS: Mutex<BusState> = Mutex::new(BusState {
        connections: BTreeMap::new(),
        names: BTreeMap::new(),
        name_queues: BTreeMap::new(),
        activatable_names: default_activatable_names(),
        services: BTreeMap::new(),
        machine_id: String::from(MACHINE_ID),
        activation_env: BTreeMap::new(),
        config_generation: 0,
    });

    pub(super) static ref SESSION_BUS: Mutex<BusState> = Mutex::new(BusState {
        connections: BTreeMap::new(),
        names: BTreeMap::new(),
        name_queues: BTreeMap::new(),
        activatable_names: default_activatable_names(),
        services: BTreeMap::new(),
        machine_id: String::from(MACHINE_ID),
        activation_env: BTreeMap::new(),
        config_generation: 0,
    });
}

pub(super) fn get_bus(bus_type: BusType) -> &'static Mutex<BusState> {
    match bus_type {
        BusType::System => &SYSTEM_BUS,
        BusType::Session => &SESSION_BUS,
    }
}
