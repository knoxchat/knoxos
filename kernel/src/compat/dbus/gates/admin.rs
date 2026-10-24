use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;

use crate::serial_println;

use super::super::connection::{connect, disconnect};
use super::super::protocol::{DbusValue, Message, MessageType};
use super::super::routing::{receive_message, send_message};
use super::super::state::{BusType, GATE_CONFIG_FILE, MACHINE_ID};

pub const GATE_DBUS_ID_MARKER: &str = "GATE_DBUS id";

/// `GetId` must return the 32-hex machine UUID.
pub fn get_id_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB13, 1000);
    let get = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetId",
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
        serial_println!("[dbus] {} (GetId {})", GATE_DBUS_ID_MARKER, MACHINE_ID);
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: GetId");
        false
    }
}

pub const GATE_DBUS_ENV_MARKER: &str = "GATE_DBUS env";

/// `UpdateActivationEnvironment` must return a method reply for `a{ss}`.
pub fn env_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB1B, 1000);
    let mut upd = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "UpdateActivationEnvironment",
    );
    upd.body = vec![DbusValue::Array(vec![DbusValue::DictEntry(
        Box::new(DbusValue::String(String::from("KNOXOS_GATE"))),
        Box::new(DbusValue::String(String::from("1"))),
    )])];
    upd.signature = Some(String::from("a{ss}"));
    let serial = send_message(BusType::System, conn, upd);
    let ok = receive_message(BusType::System, conn)
        .map(|msg| msg.reply_serial == Some(serial) && msg.body.is_empty())
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if ok {
        serial_println!(
            "[dbus] {} (UpdateActivationEnvironment method return)",
            GATE_DBUS_ENV_MARKER
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: UpdateActivationEnvironment");
        false
    }
}

pub const GATE_DBUS_RELOAD_MARKER: &str = "GATE_DBUS reload";

/// `ReloadConfig` must return a method reply.
pub fn reload_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB1C, 1000);
    let reload = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ReloadConfig",
    );
    let serial = send_message(BusType::System, conn, reload);
    let ok = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(serial)
                && msg.body.is_empty()
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if ok {
        serial_println!(
            "[dbus] {} (ReloadConfig method return)",
            GATE_DBUS_RELOAD_MARKER
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: ReloadConfig");
        false
    }
}

pub const GATE_DBUS_CONFIG_MARKER: &str = "GATE_DBUS config";

/// `GetConfigFile` must return the system bus config path.
pub fn config_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB2E, 1000);
    let get = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConfigFile",
    );
    let serial = send_message(BusType::System, conn, get);
    let ok = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.reply_serial == Some(serial)
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::String(s)) if s == GATE_CONFIG_FILE
                )
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if ok {
        serial_println!(
            "[dbus] {} (GetConfigFile {})",
            GATE_DBUS_CONFIG_MARKER,
            GATE_CONFIG_FILE
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: GetConfigFile");
        false
    }
}
