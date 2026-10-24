use alloc::format;
use alloc::string::String;
use alloc::vec;

use crate::serial_println;

use super::super::connection::{connect, disconnect};
use super::super::protocol::{DbusValue, Message, MessageType};
use super::super::routing::{receive_message, send_message};
use super::super::state::{
    BusType, DBUS_NAME_FLAG_DO_NOT_QUEUE, DBUS_REQUEST_NAME_REPLY_IN_QUEUE,
    DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER, DBUS_START_REPLY_ALREADY_RUNNING,
    DBUS_START_REPLY_SUCCESS, GATE_ACTIVATABLE_NAME,
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
