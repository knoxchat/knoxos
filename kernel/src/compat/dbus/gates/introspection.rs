use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::serial_println;

use super::super::connection::{connect, disconnect};
use super::super::protocol::{DbusValue, Message, MessageType};
use super::super::routing::{receive_message, send_message};
use super::super::state::{BUS_FEATURES, BusType, MACHINE_ID};

pub const GATE_DBUS_PING_MARKER: &str = "GATE_DBUS ping";

/// `org.freedesktop.DBus.Peer.Ping` must return a method reply.
pub fn ping_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB1F, 1000);
    let ping = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Peer",
        "Ping",
    );
    let serial = send_message(BusType::System, conn, ping);
    let ok = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(serial)
                && msg.body.is_empty()
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if ok {
        serial_println!("[dbus] {} (Peer.Ping method return)", GATE_DBUS_PING_MARKER);
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: Peer.Ping");
        false
    }
}

pub const GATE_DBUS_MACHINEID_MARKER: &str = "GATE_DBUS machineid";

/// `org.freedesktop.DBus.Peer.GetMachineId` must return the 32-hex machine UUID.
pub fn machineid_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB20, 1000);
    let get = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Peer",
        "GetMachineId",
    );
    let serial = send_message(BusType::System, conn, get);
    let ok = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(serial)
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::String(id)) if id == MACHINE_ID && id.len() == 32
                )
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if ok {
        serial_println!(
            "[dbus] {} (Peer.GetMachineId {})",
            GATE_DBUS_MACHINEID_MARKER,
            MACHINE_ID
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: Peer.GetMachineId");
        false
    }
}

pub const GATE_DBUS_INTROSPECT_MARKER: &str = "GATE_DBUS introspect";

/// `org.freedesktop.DBus.Introspectable.Introspect` must return XML naming the bus interfaces.
pub fn introspect_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB21, 1000);
    let intro = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Introspectable",
        "Introspect",
    );
    let serial = send_message(BusType::System, conn, intro);
    let ok = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(serial)
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::String(xml))
                        if xml.contains("org.freedesktop.DBus.Introspectable")
                            && xml.contains("org.freedesktop.DBus.Peer")
                )
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if ok {
        serial_println!(
            "[dbus] {} (Introspectable.Introspect XML)",
            GATE_DBUS_INTROSPECT_MARKER
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: Introspect");
        false
    }
}

pub const GATE_DBUS_PROPS_MARKER: &str = "GATE_DBUS props";

/// `Properties.Get(org.freedesktop.DBus, Features)` must return the advertised features.
pub fn props_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB22, 1000);
    let mut get = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Properties",
        "Get",
    );
    get.body = vec![
        DbusValue::String(String::from("org.freedesktop.DBus")),
        DbusValue::String(String::from("Features")),
    ];
    get.signature = Some(String::from("ss"));
    let serial = send_message(BusType::System, conn, get);
    let ok = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(serial)
                && match msg.body.first() {
                    Some(DbusValue::Variant(inner)) => match inner.as_ref() {
                        DbusValue::Array(items) => BUS_FEATURES.iter().all(|want| {
                            items
                                .iter()
                                .any(|v| matches!(v, DbusValue::String(s) if s == *want))
                        }),
                        _ => false,
                    },
                    _ => false,
                }
        })
        .unwrap_or(false);
    let mut unknown = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Properties",
        "Get",
    );
    unknown.body = vec![
        DbusValue::String(String::from("org.freedesktop.DBus")),
        DbusValue::String(String::from("NoSuchProperty")),
    ];
    unknown.signature = Some(String::from("ss"));
    let bad_serial = send_message(BusType::System, conn, unknown);
    let rejected = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::Error
                && msg.reply_serial == Some(bad_serial)
                && msg.error_name.as_deref() == Some("org.freedesktop.DBus.Error.UnknownProperty")
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if ok && rejected {
        serial_println!(
            "[dbus] {} (Properties.Get Features then UnknownProperty)",
            GATE_DBUS_PROPS_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: Properties.Get ok={} rejected={}",
            ok,
            rejected
        );
        false
    }
}

pub const GATE_DBUS_GETALL_MARKER: &str = "GATE_DBUS getall";

fn features_in_prop_dict(body: &[DbusValue]) -> bool {
    let Some(DbusValue::Array(entries)) = body.first() else {
        return false;
    };
    for entry in entries {
        let DbusValue::DictEntry(key, value) = entry else {
            continue;
        };
        if !matches!(key.as_ref(), DbusValue::String(s) if s == "Features") {
            continue;
        }
        let DbusValue::Variant(inner) = value.as_ref() else {
            continue;
        };
        let DbusValue::Array(items) = inner.as_ref() else {
            continue;
        };
        return BUS_FEATURES.iter().all(|want| {
            items
                .iter()
                .any(|v| matches!(v, DbusValue::String(s) if s == *want))
        });
    }
    false
}

/// `Properties.GetAll(org.freedesktop.DBus)` must include Features; unknown interface fails.
pub fn getall_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB23, 1000);
    let mut getall = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Properties",
        "GetAll",
    );
    getall.body = vec![DbusValue::String(String::from("org.freedesktop.DBus"))];
    getall.signature = Some(String::from("s"));
    let serial = send_message(BusType::System, conn, getall);
    let ok = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(serial)
                && features_in_prop_dict(&msg.body)
        })
        .unwrap_or(false);
    let mut unknown = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Properties",
        "GetAll",
    );
    unknown.body = vec![DbusValue::String(String::from("org.knoxos.NoSuch"))];
    unknown.signature = Some(String::from("s"));
    let bad_serial = send_message(BusType::System, conn, unknown);
    let rejected = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::Error
                && msg.reply_serial == Some(bad_serial)
                && msg.error_name.as_deref() == Some("org.freedesktop.DBus.Error.UnknownInterface")
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if ok && rejected {
        serial_println!(
            "[dbus] {} (Properties.GetAll Features then UnknownInterface)",
            GATE_DBUS_GETALL_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: Properties.GetAll ok={} rejected={}",
            ok,
            rejected
        );
        false
    }
}

pub const GATE_DBUS_SETPROP_MARKER: &str = "GATE_DBUS setprop";

/// `Properties.Set(Features)` is PropertyReadOnly; an unknown name is UnknownProperty.
pub fn setprop_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB24, 1000);
    let mut set = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Properties",
        "Set",
    );
    set.body = vec![
        DbusValue::String(String::from("org.freedesktop.DBus")),
        DbusValue::String(String::from("Features")),
        DbusValue::Variant(Box::new(DbusValue::Array(Vec::new()))),
    ];
    set.signature = Some(String::from("ssv"));
    let serial = send_message(BusType::System, conn, set);
    let readonly = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::Error
                && msg.reply_serial == Some(serial)
                && msg.error_name.as_deref() == Some("org.freedesktop.DBus.Error.PropertyReadOnly")
        })
        .unwrap_or(false);
    let mut unknown = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Properties",
        "Set",
    );
    unknown.body = vec![
        DbusValue::String(String::from("org.freedesktop.DBus")),
        DbusValue::String(String::from("NoSuchProperty")),
        DbusValue::Variant(Box::new(DbusValue::String(String::from("x")))),
    ];
    unknown.signature = Some(String::from("ssv"));
    let bad_serial = send_message(BusType::System, conn, unknown);
    let rejected = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::Error
                && msg.reply_serial == Some(bad_serial)
                && msg.error_name.as_deref() == Some("org.freedesktop.DBus.Error.UnknownProperty")
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if readonly && rejected {
        serial_println!(
            "[dbus] {} (Properties.Set Features PropertyReadOnly then UnknownProperty)",
            GATE_DBUS_SETPROP_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: Properties.Set readonly={} rejected={}",
            readonly,
            rejected
        );
        false
    }
}

pub const GATE_DBUS_OBJECTS_MARKER: &str = "GATE_DBUS objects";

fn managed_has_bus(body: &[DbusValue]) -> bool {
    let Some(DbusValue::Array(objects)) = body.first() else {
        return false;
    };
    for object in objects {
        let DbusValue::DictEntry(path, ifaces) = object else {
            continue;
        };
        if !matches!(path.as_ref(), DbusValue::ObjectPath(p) if p == "/org/freedesktop/DBus") {
            continue;
        }
        let DbusValue::Array(entries) = ifaces.as_ref() else {
            continue;
        };
        return entries.iter().any(|e| {
            matches!(
                e,
                DbusValue::DictEntry(k, _)
                    if matches!(k.as_ref(), DbusValue::String(s) if s == "org.freedesktop.DBus")
            )
        });
    }
    false
}

/// `ObjectManager.GetManagedObjects` includes `/org/freedesktop/DBus`.
pub fn objects_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB28, 1000);
    let call = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
    );
    let serial = send_message(BusType::System, conn, call);
    let ok = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(serial)
                && managed_has_bus(&msg.body)
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if ok {
        serial_println!(
            "[dbus] {} (ObjectManager.GetManagedObjects)",
            GATE_DBUS_OBJECTS_MARKER
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: GetManagedObjects");
        false
    }
}
