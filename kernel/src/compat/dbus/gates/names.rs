use alloc::format;
use alloc::string::String;
use alloc::vec;

use crate::serial_println;

use super::super::connection::{connect, disconnect};
use super::super::protocol::{DbusValue, Message, MessageType};
use super::super::routing::{receive_message, send_message};
use super::super::state::{
    BusType, DBUS_NAME_FLAG_ALLOW_REPLACEMENT, DBUS_NAME_FLAG_DO_NOT_QUEUE,
    DBUS_NAME_FLAG_REPLACE_EXISTING, DBUS_RELEASE_NAME_REPLY_NON_EXISTENT,
    DBUS_RELEASE_NAME_REPLY_NOT_OWNER, DBUS_REQUEST_NAME_REPLY_EXISTS,
    DBUS_REQUEST_NAME_REPLY_IN_QUEUE, DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER,
    DBUS_START_REPLY_ALREADY_RUNNING, DBUS_START_REPLY_SUCCESS, GATE_ACTIVATABLE_NAME,
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

pub const GATE_DBUS_REPLACE_MARKER: &str = "GATE_DBUS replace";

/// `RequestName` REPLACE_EXISTING takes over ALLOW_REPLACEMENT; without it the name EXISTS.
pub fn replace_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateReplace";
    const HELD: &str = "org.knoxos.GateReplaceHeld";
    let owner = connect(BusType::System, 0xDB33, 1000);
    let mut req = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
    );
    req.body = vec![
        DbusValue::String(String::from(WELL_KNOWN)),
        DbusValue::Uint32(DBUS_NAME_FLAG_ALLOW_REPLACEMENT),
    ];
    req.signature = Some(String::from("su"));
    let serial = send_message(BusType::System, owner, req);
    let primary = receive_message(BusType::System, owner)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(serial)
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::Uint32(n)) if *n == DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER
                )
        })
        .unwrap_or(false);

    let challenger = connect(BusType::System, 0xDB34, 1000);
    let mut take = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
    );
    take.body = vec![
        DbusValue::String(String::from(WELL_KNOWN)),
        DbusValue::Uint32(DBUS_NAME_FLAG_REPLACE_EXISTING),
    ];
    take.signature = Some(String::from("su"));
    let take_serial = send_message(BusType::System, challenger, take);
    let replaced = receive_message(BusType::System, challenger)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(take_serial)
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::Uint32(n)) if *n == DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER
                )
        })
        .unwrap_or(false);

    let mut hold = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
    );
    hold.body = vec![DbusValue::String(String::from(HELD)), DbusValue::Uint32(0)];
    hold.signature = Some(String::from("su"));
    let _ = send_message(BusType::System, owner, hold);
    let _ = receive_message(BusType::System, owner);

    let mut clash = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
    );
    clash.body = vec![
        DbusValue::String(String::from(HELD)),
        DbusValue::Uint32(DBUS_NAME_FLAG_REPLACE_EXISTING | DBUS_NAME_FLAG_DO_NOT_QUEUE),
    ];
    clash.signature = Some(String::from("su"));
    let clash_serial = send_message(BusType::System, challenger, clash);
    let exists = receive_message(BusType::System, challenger)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(clash_serial)
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::Uint32(n)) if *n == DBUS_REQUEST_NAME_REPLY_EXISTS
                )
        })
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    disconnect(BusType::System, challenger);
    if primary && replaced && exists {
        serial_println!(
            "[dbus] {} (REPLACE_EXISTING then EXISTS without ALLOW_REPLACEMENT)",
            GATE_DBUS_REPLACE_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: replace primary={} replaced={} exists={}",
            primary,
            replaced,
            exists
        );
        false
    }
}

pub const GATE_DBUS_UNIQUEOWNER_MARKER: &str = "GATE_DBUS uniqueowner";

/// `GetNameOwner` of a unique name returns that unique name.
pub fn uniqueowner_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB35, 1000);
    let unique = format!(":1.{}", conn);
    let mut get_owner = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetNameOwner",
    );
    get_owner.body = vec![DbusValue::String(unique.clone())];
    get_owner.signature = Some(String::from("s"));
    let serial = send_message(BusType::System, conn, get_owner);
    let owned = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(serial)
                && matches!(msg.body.first(), Some(DbusValue::String(s)) if s == &unique)
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if owned {
        serial_println!(
            "[dbus] {} (GetNameOwner of unique name)",
            GATE_DBUS_UNIQUEOWNER_MARKER
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: unique GetNameOwner");
        false
    }
}

pub const GATE_DBUS_NOOWNER_MARKER: &str = "GATE_DBUS noowner";

/// `GetNameOwner` of a missing name is `NameHasNoOwner`.
pub fn noowner_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB36, 1000);
    let mut get_owner = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetNameOwner",
    );
    get_owner.body = vec![DbusValue::String(String::from("org.knoxos.NoSuchName"))];
    get_owner.signature = Some(String::from("s"));
    let serial = send_message(BusType::System, conn, get_owner);
    let missing = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::Error
                && msg.reply_serial == Some(serial)
                && msg.error_name.as_deref() == Some("org.freedesktop.DBus.Error.NameHasNoOwner")
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if missing {
        serial_println!(
            "[dbus] {} (GetNameOwner NameHasNoOwner)",
            GATE_DBUS_NOOWNER_MARKER
        );
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: missing GetNameOwner");
        false
    }
}

pub const GATE_DBUS_UNKNOWNMETHOD_MARKER: &str = "GATE_DBUS unknownmethod";

/// An unknown `org.freedesktop.DBus` member is `UnknownMethod`.
pub fn unknownmethod_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB37, 1000);
    let call = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NotARealMethod",
    );
    let serial = send_message(BusType::System, conn, call);
    let unknown = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::Error
                && msg.reply_serial == Some(serial)
                && msg.error_name.as_deref() == Some("org.freedesktop.DBus.Error.UnknownMethod")
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if unknown {
        serial_println!("[dbus] {} (UnknownMethod)", GATE_DBUS_UNKNOWNMETHOD_MARKER);
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: unknown method");
        false
    }
}

pub const GATE_DBUS_NOTOWNER_MARKER: &str = "GATE_DBUS notowner";

/// `ReleaseName` of a missing name is NON_EXISTENT; another connection is NOT_OWNER.
pub fn notowner_self_test() -> bool {
    const WELL_KNOWN: &str = "org.knoxos.GateNotOwner";
    let missing_conn = connect(BusType::System, 0xDB38, 1000);
    let mut missing = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ReleaseName",
    );
    missing.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    missing.signature = Some(String::from("s"));
    let missing_serial = send_message(BusType::System, missing_conn, missing);
    let nonexistent = receive_message(BusType::System, missing_conn)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(missing_serial)
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::Uint32(code))
                        if *code == DBUS_RELEASE_NAME_REPLY_NON_EXISTENT
                )
        })
        .unwrap_or(false);
    disconnect(BusType::System, missing_conn);

    let owner = connect(BusType::System, 0xDB39, 1000);
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
        serial_println!("[dbus] GATE_DBUS FAILED: no RequestName before notowner");
        disconnect(BusType::System, owner);
        return false;
    };
    let got_name = matches!(owned.body.first(), Some(DbusValue::Uint32(1)));

    let other = connect(BusType::System, 0xDB3A, 1000);
    let mut rel = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ReleaseName",
    );
    rel.body = vec![DbusValue::String(String::from(WELL_KNOWN))];
    rel.signature = Some(String::from("s"));
    let rel_serial = send_message(BusType::System, other, rel);
    let not_owner = receive_message(BusType::System, other)
        .map(|msg| {
            msg.header.message_type == MessageType::MethodReturn
                && msg.reply_serial == Some(rel_serial)
                && matches!(
                    msg.body.first(),
                    Some(DbusValue::Uint32(code)) if *code == DBUS_RELEASE_NAME_REPLY_NOT_OWNER
                )
        })
        .unwrap_or(false);

    disconnect(BusType::System, owner);
    disconnect(BusType::System, other);
    if nonexistent && got_name && not_owner {
        serial_println!(
            "[dbus] {} (NON_EXISTENT then NOT_OWNER)",
            GATE_DBUS_NOTOWNER_MARKER
        );
        true
    } else {
        serial_println!(
            "[dbus] GATE_DBUS FAILED: nonexistent={} got={} not_owner={}",
            nonexistent,
            got_name,
            not_owner
        );
        false
    }
}

pub const GATE_DBUS_UNKNOWNOBJECT_MARKER: &str = "GATE_DBUS unknownobject";

/// A bus method on the wrong object path is `UnknownObject`.
pub fn unknownobject_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB3B, 1000);
    let call = Message::method_call(
        "org.freedesktop.DBus",
        "/org/knoxos/NotDBus",
        "org.freedesktop.DBus",
        "Hello",
    );
    let serial = send_message(BusType::System, conn, call);
    let unknown = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::Error
                && msg.reply_serial == Some(serial)
                && msg.error_name.as_deref() == Some("org.freedesktop.DBus.Error.UnknownObject")
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if unknown {
        serial_println!("[dbus] {} (UnknownObject)", GATE_DBUS_UNKNOWNOBJECT_MARKER);
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: unknown object");
        false
    }
}

pub const GATE_DBUS_INVALIDARGS_MARKER: &str = "GATE_DBUS invalidargs";

/// `RequestName` without a string body is `InvalidArgs`.
pub fn invalidargs_self_test() -> bool {
    let conn = connect(BusType::System, 0xDB3C, 1000);
    let call = Message::method_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
    );
    let serial = send_message(BusType::System, conn, call);
    let invalid = receive_message(BusType::System, conn)
        .map(|msg| {
            msg.header.message_type == MessageType::Error
                && msg.reply_serial == Some(serial)
                && msg.error_name.as_deref() == Some("org.freedesktop.DBus.Error.InvalidArgs")
        })
        .unwrap_or(false);
    disconnect(BusType::System, conn);
    if invalid {
        serial_println!("[dbus] {} (InvalidArgs)", GATE_DBUS_INVALIDARGS_MARKER);
        true
    } else {
        serial_println!("[dbus] GATE_DBUS FAILED: invalid args");
        false
    }
}
