use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::serial_println;

use super::super::connection::{connect, disconnect};
use super::super::protocol::{DbusValue, Message};
use super::super::routing::{receive_message, send_message};
use super::super::state::{
    BusType, GATE_ADT_SESSION, GATE_APPARMOR_PROFILE, GATE_LINUX_SECURITY_LABEL,
    GATE_SELINUX_CONTEXT,
};

pub const GATE_DBUS_PID_MARKER: &str = "GATE_DBUS pid";

/// `GetConnectionUnixProcessID` must return the connection pid for unique and well-known names.
pub fn pid_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GatePid";
    const PID: u32 = 0xDB0B;
    let owner = connect(BusType::System, PID, 1000);
    let unique = format!(":1.{}", owner);

    let mut get_unique = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConnectionUnixProcessID",
    );
    get_unique.body = vec![DbusValue::String(unique)];
    get_unique.signature = Some(String::from("s"));
    let uniq_serial = send_message(BusType::System, owner, get_unique);
    let unique_ok = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(uniq_serial)
                && matches!(msg.body.first(), Some(DbusValue::Uint32(p)) if *p == PID)
        })
        .unwrap_or(false);

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

    let mut get_wk = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConnectionUnixProcessID",
    );
    get_wk.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    get_wk.signature = Some(String::from("s"));
    let wk_serial = send_message(BusType::System, owner, get_wk);
    let well_known_ok = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(wk_serial)
                && matches!(msg.body.first(), Some(DbusValue::Uint32(p)) if *p == PID)
        })
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    if unique_ok && well_known_ok {
        serial_println!(
            "[dbus] {} (GetConnectionUnixProcessID unique+well-known)",
            GATE_DBUS_PID_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: unique={} well_known={}",
            unique_ok,
            well_known_ok
        );
        false
    }
}

pub const GATE_DBUS_UID_MARKER: &str = "GATE_DBUS uid";

/// `GetConnectionUnixUser` must return the connection uid for unique and well-known names.
pub fn uid_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateUid";
    const PID: u32 = 0xDB0E;
    const UID: u32 = 1000;
    let owner = connect(BusType::System, PID, UID);
    let unique = format!(":1.{}", owner);

    let mut get_unique = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConnectionUnixUser",
    );
    get_unique.body = vec![DbusValue::String(unique)];
    get_unique.signature = Some(String::from("s"));
    let uniq_serial = send_message(BusType::System, owner, get_unique);
    let unique_ok = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(uniq_serial)
                && matches!(msg.body.first(), Some(DbusValue::Uint32(u)) if *u == UID)
        })
        .unwrap_or(false);

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

    let mut get_wk = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConnectionUnixUser",
    );
    get_wk.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    get_wk.signature = Some(String::from("s"));
    let wk_serial = send_message(BusType::System, owner, get_wk);
    let well_known_ok = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(wk_serial)
                && matches!(msg.body.first(), Some(DbusValue::Uint32(u)) if *u == UID)
        })
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    if unique_ok && well_known_ok {
        serial_println!(
            "[dbus] {} (GetConnectionUnixUser unique+well-known)",
            GATE_DBUS_UID_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: uid unique={} well_known={}",
            unique_ok,
            well_known_ok
        );
        false
    }
}

pub const GATE_DBUS_CREDS_MARKER: &str = "GATE_DBUS creds";

fn creds_match(body: &[DbusValue], pid: u32, uid: u32) -> bool {
    let Some(DbusValue::Array(entries)) = body.first() else {
        return false;
    };
    let mut got_pid = false;
    let mut got_uid = false;
    for entry in entries {
        let DbusValue::DictEntry(key, value) = entry else {
            continue;
        };
        let DbusValue::String(k) = key.as_ref() else {
            continue;
        };
        let DbusValue::Variant(inner) = value.as_ref() else {
            continue;
        };
        match (k.as_str(), inner.as_ref()) {
            ("ProcessID", DbusValue::Uint32(p)) if *p == pid => got_pid = true,
            ("UnixUserID", DbusValue::Uint32(u)) if *u == uid => got_uid = true,
            _ => {}
        }
    }
    got_pid && got_uid
}

/// `GetConnectionCredentials` must return ProcessID and UnixUserID for unique and well-known names.
pub fn creds_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateCreds";
    const PID: u32 = 0xDB15;
    const UID: u32 = 1000;
    let owner = connect(BusType::System, PID, UID);
    let unique = format!(":1.{}", owner);

    let mut get_unique = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConnectionCredentials",
    );
    get_unique.body = vec![DbusValue::String(unique)];
    get_unique.signature = Some(String::from("s"));
    let uniq_serial = send_message(BusType::System, owner, get_unique);
    let unique_ok = receive_message(BusType::System, owner)
        .map(|msg| msg.reply_serial == Some(uniq_serial) && creds_match(&msg.body, PID, UID))
        .unwrap_or(false);

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

    let mut get_wk = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConnectionCredentials",
    );
    get_wk.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    get_wk.signature = Some(String::from("s"));
    let wk_serial = send_message(BusType::System, owner, get_wk);
    let well_known_ok = receive_message(BusType::System, owner)
        .map(|msg| msg.reply_serial == Some(wk_serial) && creds_match(&msg.body, PID, UID))
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    if unique_ok && well_known_ok {
        serial_println!(
            "[dbus] {} (GetConnectionCredentials unique+well-known)",
            GATE_DBUS_CREDS_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: creds unique={} well_known={}",
            unique_ok,
            well_known_ok
        );
        false
    }
}

fn ay_utf8(body: &[DbusValue]) -> Option<String> {
    let DbusValue::Array(items) = body.first()? else {
        return None;
    };
    let mut bytes = Vec::new();
    for v in items {
        match v {
            DbusValue::Byte(b) => bytes.push(*b),
            _ => return None,
        }
    }
    String::from_utf8(bytes).ok()
}

pub const GATE_DBUS_SELINUX_MARKER: &str = "GATE_DBUS selinux";

/// `GetConnectionSELinuxSecurityContext` must return the context for unique and well-known names.
pub fn selinux_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateSelinux";
    let owner = connect(BusType::System, 0xDB18, 1000);
    let unique = format!(":1.{}", owner);

    let mut get_unique = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConnectionSELinuxSecurityContext",
    );
    get_unique.body = vec![DbusValue::String(unique)];
    get_unique.signature = Some(String::from("s"));
    let uniq_serial = send_message(BusType::System, owner, get_unique);
    let unique_ok = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(uniq_serial)
                && ay_utf8(&msg.body).as_deref() == Some(GATE_SELINUX_CONTEXT)
        })
        .unwrap_or(false);

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

    let mut get_wk = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConnectionSELinuxSecurityContext",
    );
    get_wk.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    get_wk.signature = Some(String::from("s"));
    let wk_serial = send_message(BusType::System, owner, get_wk);
    let well_known_ok = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(wk_serial)
                && ay_utf8(&msg.body).as_deref() == Some(GATE_SELINUX_CONTEXT)
        })
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    if unique_ok && well_known_ok {
        serial_println!(
            "[dbus] {} (GetConnectionSELinuxSecurityContext unique+well-known)",
            GATE_DBUS_SELINUX_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: selinux unique={} well_known={}",
            unique_ok,
            well_known_ok
        );
        false
    }
}

pub const GATE_DBUS_APPARMOR_MARKER: &str = "GATE_DBUS apparmor";

/// `GetConnectionAppArmorSecurityContext` must return the profile for unique and well-known names.
pub fn apparmor_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateApparmor";
    let owner = connect(BusType::System, 0xDB19, 1000);
    let unique = format!(":1.{}", owner);

    let mut get_unique = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConnectionAppArmorSecurityContext",
    );
    get_unique.body = vec![DbusValue::String(unique)];
    get_unique.signature = Some(String::from("s"));
    let uniq_serial = send_message(BusType::System, owner, get_unique);
    let unique_ok = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(uniq_serial)
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::String(s)) if s == GATE_APPARMOR_PROFILE
                )
        })
        .unwrap_or(false);

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

    let mut get_wk = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConnectionAppArmorSecurityContext",
    );
    get_wk.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    get_wk.signature = Some(String::from("s"));
    let wk_serial = send_message(BusType::System, owner, get_wk);
    let well_known_ok = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(wk_serial)
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::String(s)) if s == GATE_APPARMOR_PROFILE
                )
        })
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    if unique_ok && well_known_ok {
        serial_println!(
            "[dbus] {} (GetConnectionAppArmorSecurityContext unique+well-known)",
            GATE_DBUS_APPARMOR_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: apparmor unique={} well_known={}",
            unique_ok,
            well_known_ok
        );
        false
    }
}

pub const GATE_DBUS_ADT_MARKER: &str = "GATE_DBUS adt";

/// `GetAdtAuditSessionData` must return the session bytes for unique and well-known names.
pub fn adt_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateAdt";
    let owner = connect(BusType::System, 0xDB1A, 1000);
    let unique = format!(":1.{}", owner);

    let mut get_unique = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetAdtAuditSessionData",
    );
    get_unique.body = vec![DbusValue::String(unique)];
    get_unique.signature = Some(String::from("s"));
    let uniq_serial = send_message(BusType::System, owner, get_unique);
    let unique_ok = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(uniq_serial)
                && ay_utf8(&msg.body).as_deref() == Some(GATE_ADT_SESSION)
        })
        .unwrap_or(false);

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

    let mut get_wk = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetAdtAuditSessionData",
    );
    get_wk.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    get_wk.signature = Some(String::from("s"));
    let wk_serial = send_message(BusType::System, owner, get_wk);
    let well_known_ok = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(wk_serial)
                && ay_utf8(&msg.body).as_deref() == Some(GATE_ADT_SESSION)
        })
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    if unique_ok && well_known_ok {
        serial_println!(
            "[dbus] {} (GetAdtAuditSessionData unique+well-known)",
            GATE_DBUS_ADT_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: adt unique={} well_known={}",
            unique_ok,
            well_known_ok
        );
        false
    }
}

pub const GATE_DBUS_LABEL_MARKER: &str = "GATE_DBUS label";

/// `GetConnectionLinuxSecurityLabel` must return the LSM label for unique and well-known names.
pub fn label_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateLinuxLabel";
    let owner = connect(BusType::System, 0xDB2D, 1000);
    let unique = format!(":1.{}", owner);

    let mut get_unique = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConnectionLinuxSecurityLabel",
    );
    get_unique.body = vec![DbusValue::String(unique)];
    get_unique.signature = Some(String::from("s"));
    let uniq_serial = send_message(BusType::System, owner, get_unique);
    let unique_ok = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(uniq_serial)
                && ay_utf8(&msg.body).as_deref() == Some(GATE_LINUX_SECURITY_LABEL)
        })
        .unwrap_or(false);

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

    let mut get_wk = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConnectionLinuxSecurityLabel",
    );
    get_wk.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    get_wk.signature = Some(String::from("s"));
    let wk_serial = send_message(BusType::System, owner, get_wk);
    let well_known_ok = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(wk_serial)
                && ay_utf8(&msg.body).as_deref() == Some(GATE_LINUX_SECURITY_LABEL)
        })
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    if unique_ok && well_known_ok {
        serial_println!(
            "[dbus] {} (GetConnectionLinuxSecurityLabel unique+well-known)",
            GATE_DBUS_LABEL_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: label unique={} well_known={}",
            unique_ok,
            well_known_ok
        );
        false
    }
}
