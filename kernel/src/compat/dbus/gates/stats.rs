use alloc::format;
use alloc::string::String;
use alloc::vec;

use crate::serial_println;

use super::super::connection::{connect, disconnect};
use super::super::protocol::{DbusValue, Message, MessageType};
use super::super::routing::{receive_message, send_message};
use super::super::state::BusType;

pub const GATE_DBUS_STATS_MARKER: &str = "GATE_DBUS stats";

fn stats_connections(body: &[DbusValue]) -> Option<u32> {
    let DbusValue::Array(entries) = body.first()? else {
        return None;
    };
    for entry in entries {
        let DbusValue::DictEntry(key, value) = entry else {
            continue;
        };
        if !matches!(key.as_ref(), DbusValue::String(s) if s == "Connections") {
            continue;
        }
        let DbusValue::Variant(inner) = value.as_ref() else {
            continue;
        };
        if let DbusValue::Uint32(n) = inner.as_ref() {
            return Some(*n);
        }
    }
    None
}

/// `Debug.Stats.GetStats` must include a non-zero Connections count.
pub fn stats_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB25, 1000);
    let stats = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Debug.Stats",
        "GetStats",
    );
    let serial = send_message(BusType::System, conn, stats);
    let ok = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(serial)
                && stats_connections(&msg.body).unwrap_or(0) >= 1
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if ok {
        serial_println!(
            "[dbus] {} (Debug.Stats.GetStats Connections)",
            GATE_DBUS_STATS_MARKER
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: GetStats");
        false
    }
}

pub const GATE_DBUS_MATCHRULES_MARKER: &str = "GATE_DBUS matchrules";

fn matchrules_has(body: &[DbusValue], unique: &str, needle: &str) -> bool {
    let Some(DbusValue::Array(entries)) = body.first() else {
        return false;
    };
    for entry in entries {
        let DbusValue::DictEntry(key, value) = entry else {
            continue;
        };
        if !matches!(key.as_ref(), DbusValue::String(s) if s == unique) {
            continue;
        }
        let DbusValue::Array(rules) = value.as_ref() else {
            continue;
        };
        return rules
            .iter()
            .any(|v| matches!(v, DbusValue::String(s) if s.contains(needle)));
    }
    false
}

/// `GetAllMatchRules` lists a unique name after AddMatch; missing before AddMatch.
pub fn matchrules_self_test() -> bool {
    const RULE: &str = "type='signal',interface='org.knoxos.GateStats',member='Ping'";
    let conn = connect(BusType::System, 0xDB26, 1000);
    let hello = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "Hello",
    );
    let _ = send_message(BusType::System, conn, hello);
    let unique = match receive_message(BusType::System, conn) {
        Some(msg) => match msg.body.first() {
            Some(DbusValue::String(s)) => s.clone(),
            _ => {
                serial_println!("[dbus] GATE_DBUS FAILED: GetAllMatchRules hello body");
                disconnect(BusType::System, conn);
                return false;
            }
        },
        None => {
            serial_println!("[dbus] GATE_DBUS FAILED: GetAllMatchRules hello");
            disconnect(BusType::System, conn);
            return false;
        }
    };
    let empty = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Debug.Stats",
        "GetAllMatchRules",
    );
    let empty_serial = send_message(BusType::System, conn, empty);
    let absent = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(empty_serial)
                && !matchrules_has(&msg.body, &unique, "org.knoxos.GateStats")
        })
        .unwrap_or(false);
    let mut add = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "AddMatch",
    );
    add.body = vec![DbusValue::String(String::from(RULE))];
    add.signature = Some(String::from("s"));
    let add_serial = send_message(BusType::System, conn, add);
    let added = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(add_serial)
        })
        .unwrap_or(false);
    let listed = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Debug.Stats",
        "GetAllMatchRules",
    );
    let list_serial = send_message(BusType::System, conn, listed);
    let present = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(list_serial)
                && matchrules_has(&msg.body, &unique, "org.knoxos.GateStats")
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if absent && added && present {
        serial_println!(
            "[dbus] {} (GetAllMatchRules after AddMatch)",
            GATE_DBUS_MATCHRULES_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: GetAllMatchRules absent={} added={} present={}",
            absent,
            added,
            present
        );
        false
    }
}

pub const GATE_DBUS_CONNSTATS_MARKER: &str = "GATE_DBUS connstats";

fn connstats_u32(body: &[DbusValue], key: &str) -> Option<u32> {
    let Some(DbusValue::Array(entries)) = body.first() else {
        return None;
    };
    for entry in entries {
        let DbusValue::DictEntry(k, value) = entry else {
            continue;
        };
        if !matches!(k.as_ref(), DbusValue::String(s) if s == key) {
            continue;
        }
        let DbusValue::Variant(inner) = value.as_ref() else {
            continue;
        };
        if let DbusValue::Uint32(n) = inner.as_ref() {
            return Some(*n);
        }
    }
    None
}

/// `GetConnectionStats` returns UnixProcessID for a unique name; unknown is NameHasNoOwner.
pub fn connstats_self_test() -> bool {
    const PID: u32 = 0xDB27;
    let conn = connect(BusType::System, PID, 1000);
    let unique = format!(":1.{}", conn);
    let mut stats = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Debug.Stats",
        "GetConnectionStats",
    );
    stats.body = vec![DbusValue::String(unique.clone())];
    stats.signature = Some(String::from("s"));
    let serial = send_message(BusType::System, conn, stats);
    let ok = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(serial)
                && connstats_u32(&msg.body, "UnixProcessID") == Some(PID)
        })
        .unwrap_or(false);
    let mut missing = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Debug.Stats",
        "GetConnectionStats",
    );
    missing.body = vec![DbusValue::String(String::from(":1.missing-stats"))];
    missing.signature = Some(String::from("s"));
    let miss_serial = send_message(BusType::System, conn, missing);
    let no_owner = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::Error
                && msg.reply_serial == Some(miss_serial)
                && msg.error_name.as_deref() == Some("org.freedesktop.DBus.Error.NameHasNoOwner")
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if ok && no_owner {
        serial_println!(
            "[dbus] {} (GetConnectionStats UnixProcessID)",
            GATE_DBUS_CONNSTATS_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: GetConnectionStats ok={} no_owner={}",
            ok,
            no_owner
        );
        false
    }
}
