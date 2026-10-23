use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::serial_println;

use super::connection::{connect, disconnect};
use super::protocol::{DbusValue, Message, MessageType};
use super::routing::{receive_message, send_message};
use super::state::{
    BUS_FEATURES, BusType, DBUS_NAME_FLAG_DO_NOT_QUEUE, DBUS_REQUEST_NAME_REPLY_IN_QUEUE,
    DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER, DBUS_START_REPLY_ALREADY_RUNNING,
    DBUS_START_REPLY_SUCCESS, GATE_ACTIVATABLE_NAME, GATE_ADT_SESSION, GATE_APPARMOR_PROFILE,
    GATE_SELINUX_CONTEXT, MACHINE_ID,
};

pub const GATE_DBUS_METHOD_MARKER: &str = "GATE_DBUS method";

/// `Hello` on `org.freedesktop.DBus` must return a `:1.` unique name.
pub fn method_self_test() -> bool {
    let client = connect(BusType::System, 0xDB05, 1000);
    let hello = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "Hello",
    );
    let serial = send_message(BusType::System, client, hello);
    let Some(reply) = receive_message(BusType::System, client) else {
        serial_println!("[dbus] GATE_DBUS FAILED: no Hello reply");
        disconnect(BusType::System, client);
        return false;
    };
    let unique_ok = matches!(
        reply.body.first(),
        Some(DbusValue::String(s)) if s.starts_with(":1.")
    );
    let ok = reply.header.message_type == MessageType::MethodReturn
        && reply.reply_serial == Some(serial)
        && unique_ok;
    disconnect(BusType::System, client);
    if ok {
        serial_println!(
            "[dbus] {} (Hello → MethodReturn unique name)",
            GATE_DBUS_METHOD_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: type={:?} reply_serial={:?} body={:?}",
            reply.header.message_type,
            reply.reply_serial,
            reply.body.first()
        );
        false
    }
}

pub const GATE_DBUS_NAME_MARKER: &str = "GATE_DBUS name";

/// `RequestName` must return PRIMARY_OWNER and deny a second claimant.
pub fn name_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.Gate";
    let owner = connect(BusType::System, 0xDB06, 1000);
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
    let serial = send_message(BusType::System, owner, req);
    let Some(reply) = receive_message(BusType::System, owner) else {
        serial_println!("[dbus] GATE_DBUS FAILED: no RequestName reply");
        disconnect(BusType::System, owner);
        return false;
    };
    let primary = reply.header.message_type == MessageType::MethodReturn
        && reply.reply_serial == Some(serial)
        && matches!(reply.body.first(), Some(DbusValue::Uint32(1)));

    let challenger = connect(BusType::System, 0xDB07, 1000);
    let mut clash = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
    );
    clash.body = vec![
        DbusValue::String(String::from(WELL_KNOWN)),
        DbusValue::Uint32(DBUS_NAME_FLAG_DO_NOT_QUEUE),
    ];
    clash.signature = Some(String::from("su"));
    let clash_serial = send_message(BusType::System, challenger, clash);
    let Some(clash_reply) = receive_message(BusType::System, challenger) else {
        serial_println!("[dbus] GATE_DBUS FAILED: no RequestName EXISTS reply");
        disconnect(BusType::System, owner);
        disconnect(BusType::System, challenger);
        return false;
    };
    let exists = clash_reply.header.message_type == MessageType::MethodReturn
        && clash_reply.reply_serial == Some(clash_serial)
        && matches!(clash_reply.body.first(), Some(DbusValue::Uint32(3)));

    let mut get = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetNameOwner",
    );
    get.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    get.signature = Some(String::from("s"));
    let get_serial = send_message(BusType::System, challenger, get);
    let owner_name = receive_message(BusType::System, challenger).and_then(|msg| {
        if msg.reply_serial == Some(get_serial) {
            match msg.body.first() {
                Some(DbusValue::String(s)) => Some(s.clone()),
                _ => None,
            }
        } else {
            None
        }
    });
    disconnect(BusType::System, owner);
    disconnect(BusType::System, challenger);
    let owned = owner_name
        .as_deref()
        .map(|s| s.starts_with(":1."))
        .unwrap_or(false);
    if primary && exists && owned {
        serial_println!(
            "[dbus] {} (RequestName PRIMARY_OWNER, second EXISTS)",
            GATE_DBUS_NAME_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: primary={} exists={} owner={:?}",
            primary,
            exists,
            owner_name
        );
        false
    }
}

pub const GATE_DBUS_RELEASE_MARKER: &str = "GATE_DBUS release";

/// `ReleaseName` must drop ownership so a later claimant can take PRIMARY_OWNER.
pub fn release_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateRel";
    let owner = connect(BusType::System, 0xDB08, 1000);
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
    let Some(owned) = receive_message(BusType::System, owner) else {
        serial_println!("[dbus] GATE_DBUS FAILED: no RequestName before release");
        disconnect(BusType::System, owner);
        return false;
    };
    let got_name = matches!(owned.body.first(), Some(DbusValue::Uint32(1)));

    let mut rel = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ReleaseName",
    );
    rel.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    rel.signature = Some(String::from("s"));
    let rel_serial = send_message(BusType::System, owner, rel);
    let Some(rel_reply) = receive_message(BusType::System, owner) else {
        serial_println!("[dbus] GATE_DBUS FAILED: no ReleaseName reply");
        disconnect(BusType::System, owner);
        return false;
    };
    let released = rel_reply.header.message_type == MessageType::MethodReturn
        && rel_reply.reply_serial == Some(rel_serial)
        && matches!(rel_reply.body.first(), Some(DbusValue::Uint32(1)));

    let mut has = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
    );
    has.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    has.signature = Some(String::from("s"));
    let has_serial = send_message(BusType::System, owner, has);
    let vacant = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(has_serial)
                && matches!(msg.body.first(), Some(DbusValue::Boolean(false)))
        })
        .unwrap_or(false);

    let challenger = connect(BusType::System, 0xDB09, 1000);
    let mut again = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
    );
    again.body = vec![
        DbusValue::String(String::from(WELL_KNOWN)),
        DbusValue::Uint32(0),
    ];
    again.signature = Some(String::from("su"));
    let _ = send_message(BusType::System, challenger, again);
    let retaken = receive_message(BusType::System, challenger)
        .map(|msg| matches!(msg.body.first(), Some(DbusValue::Uint32(1))))
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    disconnect(BusType::System, challenger);
    if got_name && released && vacant && retaken {
        serial_println!(
            "[dbus] {} (ReleaseName then PRIMARY_OWNER)",
            GATE_DBUS_RELEASE_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: got={} released={} vacant={} retaken={}",
            got_name,
            released,
            vacant,
            retaken
        );
        false
    }
}

pub const GATE_DBUS_LIST_MARKER: &str = "GATE_DBUS list";

/// `ListNames` must include a well-known name after `RequestName`.
pub fn list_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateList";
    let owner = connect(BusType::System, 0xDB0A, 1000);
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
    let Some(owned) = receive_message(BusType::System, owner) else {
        serial_println!("[dbus] GATE_DBUS FAILED: no RequestName before ListNames");
        disconnect(BusType::System, owner);
        return false;
    };
    let got_name = matches!(owned.body.first(), Some(DbusValue::Uint32(1)));

    let list = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ListNames",
    );
    let list_serial = send_message(BusType::System, owner, list);
    let Some(reply) = receive_message(BusType::System, owner) else {
        serial_println!("[dbus] GATE_DBUS FAILED: no ListNames reply");
        disconnect(BusType::System, owner);
        return false;
    };
    let listed = reply.header.message_type == MessageType::MethodReturn
        && reply.reply_serial == Some(list_serial)
        && match reply.body.first() {
            Some(DbusValue::Array(names)) => names.iter().any(|v| match v {
                DbusValue::String(s) => s == WELL_KNOWN,
                _ => false,
            }),
            _ => false,
        };

    disconnect(BusType::System, owner);
    if got_name && listed {
        serial_println!(
            "[dbus] {} (ListNames contains well-known name)",
            GATE_DBUS_LIST_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: got={} listed={}",
            got_name,
            listed
        );
        false
    }
}

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

pub const GATE_DBUS_OWNER_MARKER: &str = "GATE_DBUS owner";

/// `GetNameOwner` must return the unique name of a well-known name's owner.
pub fn owner_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateOwner";
    let owner = connect(BusType::System, 0xDB0C, 1000);
    let unique = format!(":1.{}", owner);

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

    let mut get_owner = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetNameOwner",
    );
    get_owner.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    get_owner.signature = Some(String::from("s"));
    let serial = send_message(BusType::System, owner, get_owner);
    let owned = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(serial)
                && matches!(msg.body.first(), Some(DbusValue::String(s)) if s == &unique)
        })
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    if owned {
        serial_println!(
            "[dbus] {} (GetNameOwner well-known → unique)",
            GATE_DBUS_OWNER_MARKER
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: GetNameOwner unique={}", unique);
        false
    }
}

pub const GATE_DBUS_HAS_OWNER_MARKER: &str = "GATE_DBUS hasowner";

/// `NameHasOwner` is false before `RequestName` and true after.
pub fn has_owner_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateHasOwner";
    let owner = connect(BusType::System, 0xDB0D, 1000);

    let mut missing = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
    );
    missing.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    missing.signature = Some(String::from("s"));
    let miss_serial = send_message(BusType::System, owner, missing);
    let vacant = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(miss_serial)
                && matches!(msg.body.first(), Some(DbusValue::Boolean(false)))
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

    let mut present = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
    );
    present.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    present.signature = Some(String::from("s"));
    let hit_serial = send_message(BusType::System, owner, present);
    let owned = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.reply_serial == Some(hit_serial)
                && matches!(msg.body.first(), Some(DbusValue::Boolean(true)))
        })
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    if vacant && owned {
        serial_println!(
            "[dbus] {} (NameHasOwner false then true)",
            GATE_DBUS_HAS_OWNER_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: NameHasOwner vacant={} owned={}",
            vacant,
            owned
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

pub const GATE_DBUS_START_MARKER: &str = "GATE_DBUS start";

/// `StartServiceByName` is SUCCESS then ALREADY_RUNNING for the same well-known name.
pub fn start_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateStart";
    let conn = connect(BusType::System, 0xDB14, 1000);

    let mut start = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "StartServiceByName",
    );
    start.body = vec![
        DbusValue::String(String::from(WELL_KNOWN)),
        DbusValue::Uint32(0),
    ];
    start.signature = Some(String::from("su"));
    let first_serial = send_message(BusType::System, conn, start);
    let first_ok = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(first_serial)
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::Uint32(c)) if *c == DBUS_START_REPLY_SUCCESS
                )
        })
        .unwrap_or(false);

    let mut has = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
    );
    has.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    has.signature = Some(String::from("s"));
    let has_serial = send_message(BusType::System, conn, has);
    let owned = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.reply_serial == Some(has_serial)
                && matches!(msg.body.first(), Some(DbusValue::Boolean(true)))
        })
        .unwrap_or(false);

    let mut again = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "StartServiceByName",
    );
    again.body = vec![
        DbusValue::String(String::from(WELL_KNOWN)),
        DbusValue::Uint32(0),
    ];
    again.signature = Some(String::from("su"));
    let again_serial = send_message(BusType::System, conn, again);
    let already = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.reply_serial == Some(again_serial)
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::Uint32(c)) if *c == DBUS_START_REPLY_ALREADY_RUNNING
                )
        })
        .unwrap_or(false);

    disconnect(BusType::System, conn);
    if first_ok && owned && already {
        serial_println!(
            "[dbus] {} (StartServiceByName SUCCESS then ALREADY_RUNNING)",
            GATE_DBUS_START_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: StartServiceByName first={} owned={} already={}",
            first_ok,
            owned,
            already
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

pub const GATE_DBUS_ACTIVATE_MARKER: &str = "GATE_DBUS activate";

/// `ListActivatableNames` must include the seeded well-known name.
pub fn activate_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB15, 1000);
    let list = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ListActivatableNames",
    );
    let list_serial = send_message(BusType::System, conn, list);
    let Some(reply) = receive_message(BusType::System, conn) else {
        serial_println!("[dbus] GATE_DBUS FAILED: no ListActivatableNames reply");
        disconnect(BusType::System, conn);
        return false;
    };
    let listed = reply.header.message_type == MessageType::MethodReturn
        && reply.reply_serial == Some(list_serial)
        && match reply.body.first() {
            Some(DbusValue::Array(names)) => names.iter().any(|v| match v {
                DbusValue::String(s) => s == GATE_ACTIVATABLE_NAME,
                _ => false,
            }),
            _ => false,
        };
    disconnect(BusType::System, conn);
    if listed {
        serial_println!(
            "[dbus] {} (ListActivatableNames contains {})",
            GATE_DBUS_ACTIVATE_MARKER,
            GATE_ACTIVATABLE_NAME
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: ListActivatableNames");
        false
    }
}

pub const GATE_DBUS_QUEUE_MARKER: &str = "GATE_DBUS queue";

/// `ListQueuedOwners` must return the unique name of a queued claimant.
pub fn queue_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateQueue";
    let owner = connect(BusType::System, 0xDB16, 1000);
    let waiter = connect(BusType::System, 0xDB17, 1000);
    let waiter_unique = format!(":1.{}", waiter);

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
    let owner_ok = receive_message(BusType::System, owner)
        .map(|msg| {
            matches!(
                msg.body.first(),
                Some(DbusValue::Uint32(c)) if *c == DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER
            )
        })
        .unwrap_or(false);

    let mut wait = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
    );
    wait.body = vec![
        DbusValue::String(String::from(WELL_KNOWN)),
        DbusValue::Uint32(0),
    ];
    wait.signature = Some(String::from("su"));
    let _ = send_message(BusType::System, waiter, wait);
    let queued_ok = receive_message(BusType::System, waiter)
        .map(|msg| {
            matches!(
                msg.body.first(),
                Some(DbusValue::Uint32(c)) if *c == DBUS_REQUEST_NAME_REPLY_IN_QUEUE
            )
        })
        .unwrap_or(false);

    let mut list = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ListQueuedOwners",
    );
    list.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    list.signature = Some(String::from("s"));
    let list_serial = send_message(BusType::System, owner, list);
    let Some(reply) = receive_message(BusType::System, owner) else {
        serial_println!("[dbus] GATE_DBUS FAILED: no ListQueuedOwners reply");
        disconnect(BusType::System, owner);
        disconnect(BusType::System, waiter);
        return false;
    };
    let listed = reply.header.message_type == MessageType::MethodReturn
        && reply.reply_serial == Some(list_serial)
        && match reply.body.first() {
            Some(DbusValue::Array(names)) => {
                names.iter().any(|v| match v {
                    DbusValue::String(s) => s == &waiter_unique,
                    _ => false,
                }) && !names.iter().any(|v| match v {
                    DbusValue::String(s) => s == &format!(":1.{}", owner),
                    _ => false,
                })
            }
            _ => false,
        };

    disconnect(BusType::System, owner);
    disconnect(BusType::System, waiter);
    if owner_ok && queued_ok && listed {
        serial_println!(
            "[dbus] {} (ListQueuedOwners contains queued unique name)",
            GATE_DBUS_QUEUE_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: owner={} queued={} listed={}",
            owner_ok,
            queued_ok,
            listed
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
