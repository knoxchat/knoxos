use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::serial_println;

use super::super::connection::{connect, disconnect};
use super::super::protocol::{DbusValue, Message, MessageType};
use super::super::routing::{receive_message, send_message};
use super::super::state::BusType;

pub const GATE_DBUS_MATCH_MARKER: &str = "GATE_DBUS match";

/// `AddMatch` must reply, then matching signals are delivered and others are not.
pub fn add_match_self_test() -> bool {
    const RULE: &str = "type='signal',interface='org.knoxos.GateMatch',member='Ping'";
    let listener = connect(BusType::System, 0xDB0F, 1000);
    let emitter = connect(BusType::System, 0xDB10, 1000);

    let mut add = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "AddMatch",
    );
    add.body = vec![DbusValue::String(String::from(RULE))];
    add.signature = Some(String::from("s"));
    let add_serial = send_message(BusType::System, listener, add);
    let replied = receive_message(BusType::System, listener)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(add_serial)
        })
        .unwrap_or(false);

    let ping = Message::signal("/org/knoxos/Gate", "org.knoxos.GateMatch", "Ping");
    let _ = send_message(BusType::System, emitter, ping);
    let matched = receive_message(BusType::System, listener)
        .map(|msg| {
            msg.header.message_type == MessageType::Signal
                && msg.interface.as_deref() == Some("org.knoxos.GateMatch")
                && msg.member.as_deref() == Some("Ping")
        })
        .unwrap_or(false);

    let other = Message::signal("/org/knoxos/Gate", "org.knoxos.Other", "Ping");
    let _ = send_message(BusType::System, emitter, other);
    let ignored = receive_message(BusType::System, listener).is_none();

    disconnect(BusType::System, listener);
    disconnect(BusType::System, emitter);
    if replied && matched && ignored {
        serial_println!(
            "[dbus] {} (AddMatch then matching signal)",
            GATE_DBUS_MATCH_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: AddMatch replied={} matched={} ignored={}",
            replied,
            matched,
            ignored
        );
        false
    }
}

pub const GATE_DBUS_REMOVE_MARKER: &str = "GATE_DBUS remove";

/// `RemoveMatch` must reply, then the same signal is no longer delivered.
pub fn remove_match_self_test() -> bool {
    const RULE: &str = "type='signal',interface='org.knoxos.GateUnmatch',member='Ping'";
    let listener = connect(BusType::System, 0xDB11, 1000);
    let emitter = connect(BusType::System, 0xDB12, 1000);

    let mut add = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "AddMatch",
    );
    add.body = vec![DbusValue::String(String::from(RULE))];
    add.signature = Some(String::from("s"));
    let _ = send_message(BusType::System, listener, add);
    let _ = receive_message(BusType::System, listener);

    let ping = Message::signal("/org/knoxos/Gate", "org.knoxos.GateUnmatch", "Ping");
    let _ = send_message(BusType::System, emitter, ping.clone());
    let matched = receive_message(BusType::System, listener)
        .map(|msg| {
            msg.header.message_type == MessageType::Signal
                && msg.interface.as_deref() == Some("org.knoxos.GateUnmatch")
        })
        .unwrap_or(false);

    let mut rem = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RemoveMatch",
    );
    rem.body = vec![DbusValue::String(String::from(RULE))];
    rem.signature = Some(String::from("s"));
    let rem_serial = send_message(BusType::System, listener, rem);
    let replied = receive_message(BusType::System, listener)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(rem_serial)
        })
        .unwrap_or(false);

    let _ = send_message(BusType::System, emitter, ping);
    let ignored = receive_message(BusType::System, listener).is_none();

    disconnect(BusType::System, listener);
    disconnect(BusType::System, emitter);
    if replied && matched && ignored {
        serial_println!(
            "[dbus] {} (RemoveMatch then no matching signal)",
            GATE_DBUS_REMOVE_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: RemoveMatch replied={} matched={} ignored={}",
            replied,
            matched,
            ignored
        );
        false
    }
}

pub const GATE_DBUS_MONITOR_MARKER: &str = "GATE_DBUS monitor";

/// `BecomeMonitor` must reply, then signals are delivered without AddMatch.
pub fn monitor_self_test() -> bool {
    let listener = connect(BusType::System, 0xDB1D, 1000);
    let emitter = connect(BusType::System, 0xDB1E, 1000);

    let mut monitor_call = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Monitoring",
        "BecomeMonitor",
    );
    monitor_call.body = vec![DbusValue::Array(Vec::new()), DbusValue::Uint32(0)];
    monitor_call.signature = Some(String::from("asu"));
    let serial = send_message(BusType::System, listener, monitor_call);
    let replied = receive_message(BusType::System, listener)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn && msg.reply_serial == Some(serial)
        })
        .unwrap_or(false);

    let ping = Message::signal("/org/knoxos/Gate", "org.knoxos.GateMonitor", "Ping");
    let _ = send_message(BusType::System, emitter, ping);
    let matched = receive_message(BusType::System, listener)
        .map(|msg| {
            msg.header.message_type == MessageType::Signal
                && msg.interface.as_deref() == Some("org.knoxos.GateMonitor")
                && msg.member.as_deref() == Some("Ping")
        })
        .unwrap_or(false);

    disconnect(BusType::System, listener);
    disconnect(BusType::System, emitter);
    if replied && matched {
        serial_println!(
            "[dbus] {} (BecomeMonitor then signal without AddMatch)",
            GATE_DBUS_MONITOR_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: BecomeMonitor replied={} matched={}",
            replied,
            matched
        );
        false
    }
}

pub const GATE_DBUS_OWNERCHANGED_MARKER: &str = "GATE_DBUS ownerchanged";

fn owner_changed_names(body: &[DbusValue]) -> Option<(&str, &str, &str)> {
    match body {
        [
            DbusValue::String(name),
            DbusValue::String(old),
            DbusValue::String(new),
        ] => Some((name.as_str(), old.as_str(), new.as_str())),
        _ => None,
    }
}

/// `NameOwnerChanged` is delivered to AddMatch listeners after `RequestName`.
pub fn ownerchanged_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateOwnerChanged";
    const RULE: &str = "type='signal',interface='org.freedesktop.DBus',member='NameOwnerChanged'";
    let listener = connect(BusType::System, 0xDB29, 1000);
    let owner = connect(BusType::System, 0xDB2A, 1000);
    let unique = format!(":1.{}", owner);

    let mut add = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "AddMatch",
    );
    add.body = vec![DbusValue::String(String::from(RULE))];
    add.signature = Some(String::from("s"));
    let _ = send_message(BusType::System, listener, add);
    let _ = receive_message(BusType::System, listener);

    let mut req = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
    );
    req.body = vec![
        DbusValue::String(String::from(WELL_KNOWN)),
        DbusValue::Uint32(0),
    ];
    req.signature = Some(String::from("su"));
    let _ = send_message(BusType::System, owner, req);
    let _ = receive_message(BusType::System, owner);

    let changed = receive_message(BusType::System, listener)
        .map(|msg| {
            msg.header.message_type == MessageType::Signal
                && msg.member.as_deref() == Some("NameOwnerChanged")
                && owner_changed_names(&msg.body) == Some((WELL_KNOWN, "", unique.as_str()))
        })
        .unwrap_or(false);

    disconnect(BusType::System, listener);
    disconnect(BusType::System, owner);
    if changed {
        serial_println!(
            "[dbus] {} (NameOwnerChanged after RequestName)",
            GATE_DBUS_OWNERCHANGED_MARKER
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: NameOwnerChanged");
        false
    }
}

pub const GATE_DBUS_NAMELOST_MARKER: &str = "GATE_DBUS namelost";

/// `NameLost` is delivered after `ReleaseName` to an AddMatch listener.
pub fn namelost_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateNameLost";
    const RULE: &str = "type='signal',interface='org.freedesktop.DBus',member='NameLost'";
    let owner = connect(BusType::System, 0xDB2B, 1000);

    let mut add = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "AddMatch",
    );
    add.body = vec![DbusValue::String(String::from(RULE))];
    add.signature = Some(String::from("s"));
    let _ = send_message(BusType::System, owner, add);
    let _ = receive_message(BusType::System, owner);

    let mut req = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
    );
    req.body = vec![
        DbusValue::String(String::from(WELL_KNOWN)),
        DbusValue::Uint32(0),
    ];
    req.signature = Some(String::from("su"));
    let _ = send_message(BusType::System, owner, req);
    let _ = receive_message(BusType::System, owner);

    let mut rel = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ReleaseName",
    );
    rel.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    rel.signature = Some(String::from("s"));
    let _ = send_message(BusType::System, owner, rel);
    let _ = receive_message(BusType::System, owner);

    let lost = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.header.message_type == MessageType::Signal
                && msg.member.as_deref() == Some("NameLost")
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::String(s)) if s == WELL_KNOWN
                )
        })
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    if lost {
        serial_println!(
            "[dbus] {} (NameLost after ReleaseName)",
            GATE_DBUS_NAMELOST_MARKER
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: NameLost");
        false
    }
}

pub const GATE_DBUS_NAMEACQUIRED_MARKER: &str = "GATE_DBUS nameacquired";

/// `NameAcquired` is delivered after `RequestName` to an AddMatch listener.
pub fn nameacquired_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateNameAcquired";
    const RULE: &str = "type='signal',interface='org.freedesktop.DBus',member='NameAcquired'";
    let owner = connect(BusType::System, 0xDB2C, 1000);

    let mut add = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "AddMatch",
    );
    add.body = vec![DbusValue::String(String::from(RULE))];
    add.signature = Some(String::from("s"));
    let _ = send_message(BusType::System, owner, add);
    let _ = receive_message(BusType::System, owner);

    let mut req = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
    );
    req.body = vec![
        DbusValue::String(String::from(WELL_KNOWN)),
        DbusValue::Uint32(0),
    ];
    req.signature = Some(String::from("su"));
    let _ = send_message(BusType::System, owner, req);
    let _ = receive_message(BusType::System, owner);

    let acquired = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.header.message_type == MessageType::Signal
                && msg.member.as_deref() == Some("NameAcquired")
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::String(s)) if s == WELL_KNOWN
                )
        })
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    if acquired {
        serial_println!(
            "[dbus] {} (NameAcquired after RequestName)",
            GATE_DBUS_NAMEACQUIRED_MARKER
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: NameAcquired");
        false
    }
}

pub const GATE_DBUS_SVCCHANGED_MARKER: &str = "GATE_DBUS svcchanged";

/// `ActivatableServicesChanged` is delivered after `ReloadConfig` to an AddMatch listener.
pub fn svcchanged_self_test() -> bool {
    const RULE: &str =
        "type='signal',interface='org.freedesktop.DBus',member='ActivatableServicesChanged'";
    let listener = connect(BusType::System, 0xDB2F, 1000);

    let mut add = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "AddMatch",
    );
    add.body = vec![DbusValue::String(String::from(RULE))];
    add.signature = Some(String::from("s"));
    let _ = send_message(BusType::System, listener, add);
    let _ = receive_message(BusType::System, listener);

    let reload = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ReloadConfig",
    );
    let _ = send_message(BusType::System, listener, reload);
    let _ = receive_message(BusType::System, listener);

    let changed = receive_message(BusType::System, listener)
        .map(|msg| {
            msg.header.message_type == MessageType::Signal
                && msg.member.as_deref() == Some("ActivatableServicesChanged")
        })
        .unwrap_or(false);

    disconnect(BusType::System, listener);
    if changed {
        serial_println!(
            "[dbus] {} (ActivatableServicesChanged after ReloadConfig)",
            GATE_DBUS_SVCCHANGED_MARKER
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: ActivatableServicesChanged");
        false
    }
}
