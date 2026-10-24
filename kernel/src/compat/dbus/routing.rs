use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use crate::serial_println;

use super::connection::{release_name_on_bus, request_name_on_bus};
use super::protocol::{DbusValue, Message, MessageType};
use super::state::{
    BUS_FEATURES, BusState, BusType, Connection, ConnectionId, DBUS_START_REPLY_ALREADY_RUNNING,
    DBUS_START_REPLY_SUCCESS, GATE_ADT_SESSION, GATE_APPARMOR_PROFILE, GATE_SELINUX_CONTEXT,
    MACHINE_ID, MatchRule, get_bus,
};

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
            let dest = msg.destination.as_deref().unwrap_or("");
            let iface = msg.interface.as_deref().unwrap_or("");
            // Bus daemon methods must not be queued to the owner of the
            // well-known name `org.freedesktop.DBus` — they are handled here.
            if dest == "org.freedesktop.DBus" || iface == "org.freedesktop.DBus" {
                handle_bus_method(&mut bus, from, &msg);
            } else if let Some(ref dest) = msg.destination {
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
    if conn.monitoring && conn.match_rules.is_empty() {
        return true;
    }
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
        if let Some(ref t) = rule.rule_type {
            if msg.header.message_type != *t {
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
                let name = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.clone(),
                    _ => return,
                };
                let flags = match msg.body.get(1) {
                    Some(DbusValue::Uint32(f)) => *f,
                    _ => 0,
                };
                let code = request_name_on_bus(bus, from, &name, flags);
                let mut reply = Message::method_return(msg.header.serial);
                reply.body = vec![DbusValue::Uint32(code)];
                reply.signature = Some(String::from("u"));
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(reply);
                }
            }
            "ReleaseName" => {
                let name = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.clone(),
                    _ => return,
                };
                let code = release_name_on_bus(bus, from, &name);
                let mut reply = Message::method_return(msg.header.serial);
                reply.body = vec![DbusValue::Uint32(code)];
                reply.signature = Some(String::from("u"));
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(reply);
                }
            }
            "ListNames" => {
                let mut names: Vec<DbusValue> = bus
                    .names
                    .keys()
                    .map(|n| DbusValue::String(n.clone()))
                    .collect();
                for conn in bus.connections.values() {
                    names.push(DbusValue::String(conn.unique_name.clone()));
                }
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
                if let Some(conn) = bus.connections.get_mut(&from) {
                    if let Some(DbusValue::String(rule_str)) = msg.body.first() {
                        conn.match_rules.push(parse_match_rule(rule_str));
                    }
                    conn.message_queue
                        .push(Message::method_return(msg.header.serial));
                }
            }
            "RemoveMatch" => {
                if let Some(conn) = bus.connections.get_mut(&from) {
                    if let Some(DbusValue::String(rule_str)) = msg.body.first() {
                        let parsed = parse_match_rule(rule_str);
                        if let Some(idx) = conn.match_rules.iter().position(|r| r == &parsed) {
                            conn.match_rules.remove(idx);
                        }
                    }
                    conn.message_queue
                        .push(Message::method_return(msg.header.serial));
                }
            }
            "GetId" => {
                let id = bus.machine_id.clone();
                let mut reply = Message::method_return(msg.header.serial);
                reply.body = vec![DbusValue::String(id)];
                reply.signature = Some(String::from("s"));
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(reply);
                }
            }
            "StartServiceByName" => {
                let name = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.clone(),
                    _ => return,
                };
                let _flags = match msg.body.get(1) {
                    Some(DbusValue::Uint32(f)) => *f,
                    _ => 0,
                };
                let code = if bus.names.contains_key(&name) {
                    DBUS_START_REPLY_ALREADY_RUNNING
                } else {
                    let _ = request_name_on_bus(bus, from, &name, 0);
                    DBUS_START_REPLY_SUCCESS
                };
                let mut reply = Message::method_return(msg.header.serial);
                reply.body = vec![DbusValue::Uint32(code)];
                reply.signature = Some(String::from("u"));
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(reply);
                }
            }
            "GetConnectionCredentials" => {
                let name = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.as_str(),
                    _ => return,
                };
                let owner_id = bus.names.get(name).copied().or_else(|| {
                    bus.connections
                        .values()
                        .find(|c| c.unique_name == name)
                        .map(|c| c.id)
                });
                if let Some(id) = owner_id {
                    let creds = bus.connections.get(&id).map(|c| (c.pid, c.uid));
                    if let Some((pid, uid)) = creds {
                        let entries = vec![
                            DbusValue::DictEntry(
                                Box::new(DbusValue::String(String::from("UnixUserID"))),
                                Box::new(DbusValue::Variant(Box::new(DbusValue::Uint32(uid)))),
                            ),
                            DbusValue::DictEntry(
                                Box::new(DbusValue::String(String::from("ProcessID"))),
                                Box::new(DbusValue::Variant(Box::new(DbusValue::Uint32(pid)))),
                            ),
                        ];
                        let mut reply = Message::method_return(msg.header.serial);
                        reply.body = vec![DbusValue::Array(entries)];
                        reply.signature = Some(String::from("a{sv}"));
                        if let Some(c) = bus.connections.get_mut(&from) {
                            c.message_queue.push(reply);
                        }
                    }
                }
            }
            "GetConnectionUnixUser" => {
                let name = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.as_str(),
                    _ => return,
                };
                let owner_id = bus.names.get(name).copied().or_else(|| {
                    bus.connections
                        .values()
                        .find(|c| c.unique_name == name)
                        .map(|c| c.id)
                });
                if let Some(id) = owner_id {
                    let uid = bus.connections.get(&id).map(|c| c.uid);
                    if let Some(uid) = uid {
                        let mut reply = Message::method_return(msg.header.serial);
                        reply.body = vec![DbusValue::Uint32(uid)];
                        reply.signature = Some(String::from("u"));
                        if let Some(c) = bus.connections.get_mut(&from) {
                            c.message_queue.push(reply);
                        }
                    }
                }
            }
            "GetConnectionUnixProcessID" => {
                let name = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.as_str(),
                    _ => return,
                };
                let owner_id = bus.names.get(name).copied().or_else(|| {
                    bus.connections
                        .values()
                        .find(|c| c.unique_name == name)
                        .map(|c| c.id)
                });
                if let Some(id) = owner_id {
                    let pid = bus.connections.get(&id).map(|c| c.pid);
                    if let Some(pid) = pid {
                        let mut reply = Message::method_return(msg.header.serial);
                        reply.body = vec![DbusValue::Uint32(pid)];
                        reply.signature = Some(String::from("u"));
                        if let Some(c) = bus.connections.get_mut(&from) {
                            c.message_queue.push(reply);
                        }
                    }
                }
            }
            "ListActivatableNames" => {
                let names: Vec<DbusValue> = bus
                    .activatable_names
                    .iter()
                    .map(|n| DbusValue::String(n.clone()))
                    .collect();
                let mut reply = Message::method_return(msg.header.serial);
                reply.body = vec![DbusValue::Array(names)];
                reply.signature = Some(String::from("as"));
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(reply);
                }
            }
            "ListQueuedOwners" => {
                let name = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.as_str(),
                    _ => return,
                };
                let ids = bus.name_queues.get(name).cloned().unwrap_or_default();
                let mut names = Vec::new();
                for id in ids {
                    if let Some(c) = bus.connections.get(&id) {
                        names.push(DbusValue::String(c.unique_name.clone()));
                    }
                }
                let mut reply = Message::method_return(msg.header.serial);
                reply.body = vec![DbusValue::Array(names)];
                reply.signature = Some(String::from("as"));
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(reply);
                }
            }
            "GetConnectionSELinuxSecurityContext" => {
                let name = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.as_str(),
                    _ => return,
                };
                let owner_id = bus.names.get(name).copied().or_else(|| {
                    bus.connections
                        .values()
                        .find(|c| c.unique_name == name)
                        .map(|c| c.id)
                });
                if owner_id.is_some() {
                    let bytes: Vec<DbusValue> = GATE_SELINUX_CONTEXT
                        .as_bytes()
                        .iter()
                        .copied()
                        .map(DbusValue::Byte)
                        .collect();
                    let mut reply = Message::method_return(msg.header.serial);
                    reply.body = vec![DbusValue::Array(bytes)];
                    reply.signature = Some(String::from("ay"));
                    if let Some(c) = bus.connections.get_mut(&from) {
                        c.message_queue.push(reply);
                    }
                }
            }
            "GetConnectionAppArmorSecurityContext" => {
                let name = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.as_str(),
                    _ => return,
                };
                let owner_id = bus.names.get(name).copied().or_else(|| {
                    bus.connections
                        .values()
                        .find(|c| c.unique_name == name)
                        .map(|c| c.id)
                });
                if owner_id.is_some() {
                    let mut reply = Message::method_return(msg.header.serial);
                    reply.body = vec![DbusValue::String(String::from(GATE_APPARMOR_PROFILE))];
                    reply.signature = Some(String::from("s"));
                    if let Some(c) = bus.connections.get_mut(&from) {
                        c.message_queue.push(reply);
                    }
                }
            }
            "GetAdtAuditSessionData" => {
                let name = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.as_str(),
                    _ => return,
                };
                let owner_id = bus.names.get(name).copied().or_else(|| {
                    bus.connections
                        .values()
                        .find(|c| c.unique_name == name)
                        .map(|c| c.id)
                });
                if owner_id.is_some() {
                    let bytes: Vec<DbusValue> = GATE_ADT_SESSION
                        .as_bytes()
                        .iter()
                        .copied()
                        .map(DbusValue::Byte)
                        .collect();
                    let mut reply = Message::method_return(msg.header.serial);
                    reply.body = vec![DbusValue::Array(bytes)];
                    reply.signature = Some(String::from("ay"));
                    if let Some(c) = bus.connections.get_mut(&from) {
                        c.message_queue.push(reply);
                    }
                }
            }
            "UpdateActivationEnvironment" => {
                if let Some(DbusValue::Array(items)) = msg.body.first() {
                    for item in items {
                        if let DbusValue::DictEntry(k, v) = item {
                            if let (DbusValue::String(key), DbusValue::String(val)) =
                                (k.as_ref(), v.as_ref())
                            {
                                bus.activation_env.insert(key.clone(), val.clone());
                            }
                        }
                    }
                }
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue
                        .push(Message::method_return(msg.header.serial));
                }
            }
            "ReloadConfig" => {
                bus.config_generation = bus.config_generation.wrapping_add(1);
                serial_println!("[dbus] ReloadConfig generation={}", bus.config_generation);
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue
                        .push(Message::method_return(msg.header.serial));
                }
            }
            _ => {
                serial_println!("[dbus] Unknown bus method: {}.{}", interface, member);
            }
        }
    } else if interface == "org.freedesktop.DBus.Monitoring" {
        match member {
            "BecomeMonitor" => {
                if let Some(conn) = bus.connections.get_mut(&from) {
                    conn.monitoring = true;
                    if let Some(DbusValue::Array(rules)) = msg.body.first() {
                        for rule in rules {
                            if let DbusValue::String(s) = rule {
                                conn.match_rules.push(parse_match_rule(s));
                            }
                        }
                    }
                    conn.message_queue
                        .push(Message::method_return(msg.header.serial));
                }
            }
            _ => {
                serial_println!("[dbus] Unknown bus method: {}.{}", interface, member);
            }
        }
    } else if interface == "org.freedesktop.DBus.Peer" {
        match member {
            "Ping" => {
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue
                        .push(Message::method_return(msg.header.serial));
                }
            }
            "GetMachineId" => {
                let mut reply = Message::method_return(msg.header.serial);
                reply.body = vec![DbusValue::String(String::from(MACHINE_ID))];
                reply.signature = Some(String::from("s"));
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(reply);
                }
            }
            _ => {
                serial_println!("[dbus] Unknown bus method: {}.{}", interface, member);
            }
        }
    } else if interface == "org.freedesktop.DBus.Introspectable" {
        match member {
            "Introspect" => {
                let mut reply = Message::method_return(msg.header.serial);
                reply.body = vec![DbusValue::String(String::from(BUS_INTROSPECT_XML))];
                reply.signature = Some(String::from("s"));
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(reply);
                }
            }
            _ => {
                serial_println!("[dbus] Unknown bus method: {}.{}", interface, member);
            }
        }
    } else if interface == "org.freedesktop.DBus.Properties" {
        match member {
            "Get" => {
                let iface = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.as_str(),
                    _ => return,
                };
                let name = match msg.body.get(1) {
                    Some(DbusValue::String(s)) => s.as_str(),
                    _ => return,
                };
                if iface == "org.freedesktop.DBus" && name == "Features" {
                    let items: Vec<DbusValue> = BUS_FEATURES
                        .iter()
                        .map(|f| DbusValue::String(String::from(*f)))
                        .collect();
                    let mut reply = Message::method_return(msg.header.serial);
                    reply.body = vec![DbusValue::Variant(Box::new(DbusValue::Array(items)))];
                    reply.signature = Some(String::from("v"));
                    if let Some(c) = bus.connections.get_mut(&from) {
                        c.message_queue.push(reply);
                    }
                } else if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(Message::error(
                        msg.header.serial,
                        "org.freedesktop.DBus.Error.UnknownProperty",
                        name,
                    ));
                }
            }
            "GetAll" => {
                let iface = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.as_str(),
                    _ => return,
                };
                if iface == "org.freedesktop.DBus" {
                    let items: Vec<DbusValue> = BUS_FEATURES
                        .iter()
                        .map(|f| DbusValue::String(String::from(*f)))
                        .collect();
                    let entries = vec![DbusValue::DictEntry(
                        Box::new(DbusValue::String(String::from("Features"))),
                        Box::new(DbusValue::Variant(Box::new(DbusValue::Array(items)))),
                    )];
                    let mut reply = Message::method_return(msg.header.serial);
                    reply.body = vec![DbusValue::Array(entries)];
                    reply.signature = Some(String::from("a{sv}"));
                    if let Some(c) = bus.connections.get_mut(&from) {
                        c.message_queue.push(reply);
                    }
                } else if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(Message::error(
                        msg.header.serial,
                        "org.freedesktop.DBus.Error.UnknownInterface",
                        iface,
                    ));
                }
            }
            "Set" => {
                let iface = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.as_str(),
                    _ => return,
                };
                let name = match msg.body.get(1) {
                    Some(DbusValue::String(s)) => s.as_str(),
                    _ => return,
                };
                let err = if iface == "org.freedesktop.DBus" && name == "Features" {
                    ("org.freedesktop.DBus.Error.PropertyReadOnly", name)
                } else {
                    ("org.freedesktop.DBus.Error.UnknownProperty", name)
                };
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue
                        .push(Message::error(msg.header.serial, err.0, err.1));
                }
            }
            _ => {
                serial_println!("[dbus] Unknown bus method: {}.{}", interface, member);
            }
        }
    } else if interface == "org.freedesktop.DBus.Debug.Stats" {
        match member {
            "GetStats" => {
                let connections = bus.connections.len() as u32;
                let match_rules: u32 = bus
                    .connections
                    .values()
                    .map(|c| c.match_rules.len() as u32)
                    .sum();
                let entries = vec![
                    DbusValue::DictEntry(
                        Box::new(DbusValue::String(String::from("Connections"))),
                        Box::new(DbusValue::Variant(Box::new(DbusValue::Uint32(connections)))),
                    ),
                    DbusValue::DictEntry(
                        Box::new(DbusValue::String(String::from("MatchRules"))),
                        Box::new(DbusValue::Variant(Box::new(DbusValue::Uint32(match_rules)))),
                    ),
                ];
                let mut reply = Message::method_return(msg.header.serial);
                reply.body = vec![DbusValue::Array(entries)];
                reply.signature = Some(String::from("a{sv}"));
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(reply);
                }
            }
            "GetAllMatchRules" => {
                let mut entries = Vec::new();
                for conn in bus.connections.values() {
                    if conn.match_rules.is_empty() {
                        continue;
                    }
                    let rules: Vec<DbusValue> = conn
                        .match_rules
                        .iter()
                        .map(|r| DbusValue::String(match_rule_to_string(r)))
                        .collect();
                    entries.push(DbusValue::DictEntry(
                        Box::new(DbusValue::String(conn.unique_name.clone())),
                        Box::new(DbusValue::Array(rules)),
                    ));
                }
                let mut reply = Message::method_return(msg.header.serial);
                reply.body = vec![DbusValue::Array(entries)];
                reply.signature = Some(String::from("a{sas}"));
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(reply);
                }
            }
            "GetConnectionStats" => {
                let name = match msg.body.first() {
                    Some(DbusValue::String(s)) => s.as_str(),
                    _ => return,
                };
                let owner_id = bus.names.get(name).copied().or_else(|| {
                    bus.connections
                        .values()
                        .find(|c| c.unique_name == name)
                        .map(|c| c.id)
                });
                if let Some(id) = owner_id {
                    let stats = bus.connections.get(&id).map(|c| {
                        (
                            c.unique_name.clone(),
                            c.pid,
                            c.uid,
                            c.match_rules.len() as u32,
                        )
                    });
                    if let Some((unique, pid, uid, rules)) = stats {
                        let entries = vec![
                            DbusValue::DictEntry(
                                Box::new(DbusValue::String(String::from("UnixUser"))),
                                Box::new(DbusValue::Variant(Box::new(DbusValue::Uint32(uid)))),
                            ),
                            DbusValue::DictEntry(
                                Box::new(DbusValue::String(String::from("UnixProcessID"))),
                                Box::new(DbusValue::Variant(Box::new(DbusValue::Uint32(pid)))),
                            ),
                            DbusValue::DictEntry(
                                Box::new(DbusValue::String(String::from("UniqueName"))),
                                Box::new(DbusValue::Variant(Box::new(DbusValue::String(unique)))),
                            ),
                            DbusValue::DictEntry(
                                Box::new(DbusValue::String(String::from("MatchRules"))),
                                Box::new(DbusValue::Variant(Box::new(DbusValue::Uint32(rules)))),
                            ),
                        ];
                        let mut reply = Message::method_return(msg.header.serial);
                        reply.body = vec![DbusValue::Array(entries)];
                        reply.signature = Some(String::from("a{sv}"));
                        if let Some(c) = bus.connections.get_mut(&from) {
                            c.message_queue.push(reply);
                        }
                    }
                } else if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(Message::error(
                        msg.header.serial,
                        "org.freedesktop.DBus.Error.NameHasNoOwner",
                        name,
                    ));
                }
            }
            _ => {
                serial_println!("[dbus] Unknown bus method: {}.{}", interface, member);
            }
        }
    } else if interface == "org.freedesktop.DBus.ObjectManager" {
        match member {
            "GetManagedObjects" => {
                let features: Vec<DbusValue> = BUS_FEATURES
                    .iter()
                    .map(|f| DbusValue::String(String::from(*f)))
                    .collect();
                let props = vec![DbusValue::DictEntry(
                    Box::new(DbusValue::String(String::from("Features"))),
                    Box::new(DbusValue::Variant(Box::new(DbusValue::Array(features)))),
                )];
                let ifaces = vec![DbusValue::DictEntry(
                    Box::new(DbusValue::String(String::from("org.freedesktop.DBus"))),
                    Box::new(DbusValue::Array(props)),
                )];
                let objects = vec![DbusValue::DictEntry(
                    Box::new(DbusValue::ObjectPath(String::from("/org/freedesktop/DBus"))),
                    Box::new(DbusValue::Array(ifaces)),
                )];
                let mut reply = Message::method_return(msg.header.serial);
                reply.body = vec![DbusValue::Array(objects)];
                reply.signature = Some(String::from("a{oa{sa{sv}}}"));
                if let Some(c) = bus.connections.get_mut(&from) {
                    c.message_queue.push(reply);
                }
            }
            _ => {
                serial_println!("[dbus] Unknown bus method: {}.{}", interface, member);
            }
        }
    }
}

const BUS_INTROSPECT_XML: &str = concat!(
    "<!DOCTYPE node PUBLIC \"-//freedesktop//DTD D-BUS Object Introspection 1.0//EN\" ",
    "\"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd\">",
    "<node>",
    "<interface name=\"org.freedesktop.DBus\">",
    "<method name=\"Hello\"><arg direction=\"out\" type=\"s\"/></method>",
    "</interface>",
    "<interface name=\"org.freedesktop.DBus.Introspectable\">",
    "<method name=\"Introspect\"><arg direction=\"out\" type=\"s\"/></method>",
    "</interface>",
    "<interface name=\"org.freedesktop.DBus.Peer\">",
    "<method name=\"Ping\"/>",
    "<method name=\"GetMachineId\"><arg direction=\"out\" type=\"s\"/></method>",
    "</interface>",
    "<interface name=\"org.freedesktop.DBus.Properties\">",
    "<method name=\"Get\"><arg direction=\"in\" type=\"s\"/><arg direction=\"in\" type=\"s\"/>",
    "<arg direction=\"out\" type=\"v\"/></method>",
    "</interface>",
    "</node>",
);

fn match_rule_to_string(rule: &MatchRule) -> String {
    let mut parts = Vec::new();
    if let Some(t) = rule.rule_type {
        let s = match t {
            MessageType::Signal => "signal",
            MessageType::MethodCall => "method_call",
            MessageType::MethodReturn => "method_return",
            MessageType::Error => "error",
            MessageType::Invalid => "invalid",
        };
        parts.push(format!("type='{}'", s));
    }
    if let Some(ref iface) = rule.interface {
        parts.push(format!("interface='{}'", iface));
    }
    if let Some(ref member) = rule.member {
        parts.push(format!("member='{}'", member));
    }
    if let Some(ref path) = rule.path {
        parts.push(format!("path='{}'", path));
    }
    parts.join(",")
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

/// Remove a signal match rule
pub fn remove_match(bus_type: BusType, conn_id: ConnectionId, rule: &str) {
    let parsed = parse_match_rule(rule);
    let mut bus = get_bus(bus_type).lock();
    if let Some(conn) = bus.connections.get_mut(&conn_id) {
        if let Some(idx) = conn.match_rules.iter().position(|r| r == &parsed) {
            conn.match_rules.remove(idx);
        }
    }
}
