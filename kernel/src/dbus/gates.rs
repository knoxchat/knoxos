use alloc::format;
use alloc::string::String;
use alloc::vec;

use crate::serial_println;

use super::connection::{connect, disconnect};
use super::protocol::{DbusValue, Message, MessageType};
use super::routing::{receive_message, send_message};
use super::state::{BusType, DBUS_NAME_FLAG_DO_NOT_QUEUE, MACHINE_ID};

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
