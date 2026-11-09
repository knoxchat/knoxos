use alloc::string::String;

use crate::serial_println;

use super::super::client::{NfsClient, NfsMountOptions};
use super::super::compound::{NfsCompoundOp, NfsCompoundResult};
use super::super::types::{NFS4_SHARE_ACCESS_READ, NFS4_SHARE_DENY_NONE, NfsStat, NfsStateId};

pub const GATE_NFS_EXCHANGEID_MARKER: &str = "GATE_NFS exchangeid";

/// EXCHANGE_ID of a client_name returns a client_id; an empty name is INVAL.
pub fn exchangeid_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let issued = client.compound(
        "exchangeid",
        &[NfsCompoundOp::ExchangeId {
            client_name: String::from("knoxos-exid"),
        }],
    );
    let client_id = match issued.first() {
        Some(NfsCompoundResult::ClientId { client_id, .. }) if *client_id != 0 => *client_id,
        other => {
            serial_println!("[NFS] GATE_NFS exchangeid FAILED: {:?}", other);
            return false;
        }
    };
    let empty = client.compound(
        "exchangeid-empty",
        &[NfsCompoundOp::ExchangeId {
            client_name: String::new(),
        }],
    );
    match empty.first() {
        Some(NfsCompoundResult::Status(NfsStat::Inval)) => {
            serial_println!(
                "[NFS] {} (EXCHANGE_ID {:#x} then INVAL)",
                GATE_NFS_EXCHANGEID_MARKER,
                client_id
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS exchangeid FAILED empty={:?}", other);
            false
        }
    }
}

pub const GATE_NFS_SESSION_MARKER: &str = "GATE_NFS session";

/// CREATE_SESSION of an EXCHANGE_ID client_id is OK; a wrong id is STALE_CLIENTID.
pub fn session_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let issued = client.compound(
        "session-exchangeid",
        &[NfsCompoundOp::ExchangeId {
            client_name: String::from("knoxos-session"),
        }],
    );
    let client_id = match issued.first() {
        Some(NfsCompoundResult::ClientId { client_id, .. }) => *client_id,
        other => {
            serial_println!("[NFS] GATE_NFS session FAILED: exchangeid {:?}", other);
            return false;
        }
    };
    let created = client.compound("session", &[NfsCompoundOp::CreateSession { client_id }]);
    let session_ok = matches!(
        created.first(),
        Some(NfsCompoundResult::Session { session_id }) if *session_id != 0
    );
    let stale = client.compound(
        "session-stale",
        &[NfsCompoundOp::CreateSession { client_id: 0xdead }],
    );
    let stale_ok = matches!(
        stale.first(),
        Some(NfsCompoundResult::Status(NfsStat::StaleClientId))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "session-missing",
        &[NfsCompoundOp::CreateSession { client_id }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::StaleClientId)) if session_ok && stale_ok => {
            serial_println!(
                "[NFS] {} (CREATE_SESSION then STALE_CLIENTID)",
                GATE_NFS_SESSION_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS session FAILED created={} stale={} missing={:?}",
                session_ok,
                stale_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_DESTROYSESSION_MARKER: &str = "GATE_NFS destroysession";

/// DESTROY_SESSION of a CREATE_SESSION id is OK; a second or unknown id is BADSESSION.
pub fn destroysession_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let issued = client.compound(
        "destroysession-exchangeid",
        &[NfsCompoundOp::ExchangeId {
            client_name: String::from("knoxos-dsess"),
        }],
    );
    let client_id = match issued.first() {
        Some(NfsCompoundResult::ClientId { client_id, .. }) => *client_id,
        other => {
            serial_println!(
                "[NFS] GATE_NFS destroysession FAILED: exchangeid {:?}",
                other
            );
            return false;
        }
    };
    let created = client.compound(
        "destroysession-create",
        &[NfsCompoundOp::CreateSession { client_id }],
    );
    let session_id = match created.first() {
        Some(NfsCompoundResult::Session { session_id }) => *session_id,
        other => {
            serial_println!("[NFS] GATE_NFS destroysession FAILED: create {:?}", other);
            return false;
        }
    };
    let destroyed = client.compound(
        "destroysession",
        &[NfsCompoundOp::DestroySession { session_id }],
    );
    let destroyed_ok = matches!(
        destroyed.first(),
        Some(NfsCompoundResult::Status(NfsStat::Ok))
    );
    let second = client.compound(
        "destroysession-again",
        &[NfsCompoundOp::DestroySession { session_id }],
    );
    let second_ok = matches!(
        second.first(),
        Some(NfsCompoundResult::Status(NfsStat::BadSession))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "destroysession-missing",
        &[NfsCompoundOp::DestroySession { session_id }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::BadSession)) if destroyed_ok && second_ok => {
            serial_println!(
                "[NFS] {} (DESTROY_SESSION then BADSESSION)",
                GATE_NFS_DESTROYSESSION_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS destroysession FAILED destroyed={} second={} missing={:?}",
                destroyed_ok,
                second_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_DESTROYCLIENTID_MARKER: &str = "GATE_NFS destroyclientid";

/// DESTROY_CLIENTID of a session-free client_id is OK; a live session is CLIENTID_BUSY; a wrong id is STALE_CLIENTID.
pub fn destroyclientid_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let issued = client.compound(
        "destroyclientid-exchangeid",
        &[NfsCompoundOp::ExchangeId {
            client_name: String::from("knoxos-dcid"),
        }],
    );
    let client_id = match issued.first() {
        Some(NfsCompoundResult::ClientId { client_id, .. }) => *client_id,
        other => {
            serial_println!(
                "[NFS] GATE_NFS destroyclientid FAILED: exchangeid {:?}",
                other
            );
            return false;
        }
    };
    let created = client.compound(
        "destroyclientid-create",
        &[NfsCompoundOp::CreateSession { client_id }],
    );
    let session_id = match created.first() {
        Some(NfsCompoundResult::Session { session_id }) => *session_id,
        other => {
            serial_println!("[NFS] GATE_NFS destroyclientid FAILED: create {:?}", other);
            return false;
        }
    };
    let busy = client.compound(
        "destroyclientid-busy",
        &[NfsCompoundOp::DestroyClientId { client_id }],
    );
    let busy_ok = matches!(
        busy.first(),
        Some(NfsCompoundResult::Status(NfsStat::ClientIdBusy))
    );
    let _ = client.compound(
        "destroyclientid-session",
        &[NfsCompoundOp::DestroySession { session_id }],
    );
    let destroyed = client.compound(
        "destroyclientid",
        &[NfsCompoundOp::DestroyClientId { client_id }],
    );
    let destroyed_ok = matches!(
        destroyed.first(),
        Some(NfsCompoundResult::Status(NfsStat::Ok))
    );
    let stale = client.compound(
        "destroyclientid-stale",
        &[NfsCompoundOp::DestroyClientId { client_id: 0xdead }],
    );
    match stale.first() {
        Some(NfsCompoundResult::Status(NfsStat::StaleClientId)) if busy_ok && destroyed_ok => {
            serial_println!(
                "[NFS] {} (DESTROY_CLIENTID busy then OK then STALE_CLIENTID)",
                GATE_NFS_DESTROYCLIENTID_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS destroyclientid FAILED busy={} destroyed={} stale={:?}",
                busy_ok,
                destroyed_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_SEQUENCE_MARKER: &str = "GATE_NFS sequence";

/// SEQUENCE of a CREATE_SESSION id returns that session; sequenceid 0 is INVAL; unknown is BADSESSION.
pub fn sequence_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let issued = client.compound(
        "sequence-exchangeid",
        &[NfsCompoundOp::ExchangeId {
            client_name: String::from("knoxos-seq"),
        }],
    );
    let client_id = match issued.first() {
        Some(NfsCompoundResult::ClientId { client_id, .. }) => *client_id,
        other => {
            serial_println!("[NFS] GATE_NFS sequence FAILED: exchangeid {:?}", other);
            return false;
        }
    };
    let created = client.compound(
        "sequence-create",
        &[NfsCompoundOp::CreateSession { client_id }],
    );
    let session_id = match created.first() {
        Some(NfsCompoundResult::Session { session_id }) => *session_id,
        other => {
            serial_println!("[NFS] GATE_NFS sequence FAILED: create {:?}", other);
            return false;
        }
    };
    let sequenced = client.compound(
        "sequence",
        &[NfsCompoundOp::Sequence {
            session_id,
            sequenceid: 1,
        }],
    );
    let seq_ok = matches!(
        sequenced.first(),
        Some(NfsCompoundResult::Sequence {
            session_id: sid,
            sequenceid: 1
        }) if *sid == session_id
    );
    let inval = client.compound(
        "sequence-zero",
        &[NfsCompoundOp::Sequence {
            session_id,
            sequenceid: 0,
        }],
    );
    let inval_ok = matches!(
        inval.first(),
        Some(NfsCompoundResult::Status(NfsStat::Inval))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "sequence-missing",
        &[NfsCompoundOp::Sequence {
            session_id,
            sequenceid: 1,
        }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::BadSession)) if seq_ok && inval_ok => {
            serial_println!(
                "[NFS] {} (SEQUENCE then INVAL then BADSESSION)",
                GATE_NFS_SEQUENCE_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS sequence FAILED seq={} inval={} missing={:?}",
                seq_ok,
                inval_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_RECLAIM_MARKER: &str = "GATE_NFS reclaim";

/// RECLAIM_COMPLETE of a live session is OK; a second is COMPLETE_ALREADY; no session is BADSESSION.
pub fn reclaim_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let issued = client.compound(
        "reclaim-exchangeid",
        &[NfsCompoundOp::ExchangeId {
            client_name: String::from("knoxos-reclaim"),
        }],
    );
    let client_id = match issued.first() {
        Some(NfsCompoundResult::ClientId { client_id, .. }) => *client_id,
        other => {
            serial_println!("[NFS] GATE_NFS reclaim FAILED: exchangeid {:?}", other);
            return false;
        }
    };
    let created = client.compound(
        "reclaim-create",
        &[NfsCompoundOp::CreateSession { client_id }],
    );
    if !matches!(created.first(), Some(NfsCompoundResult::Session { .. })) {
        serial_println!(
            "[NFS] GATE_NFS reclaim FAILED: create {:?}",
            created.first()
        );
        return false;
    }
    let first = client.compound(
        "reclaim",
        &[NfsCompoundOp::ReclaimComplete { one_fs: false }],
    );
    let first_ok = matches!(first.first(), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let second = client.compound(
        "reclaim-again",
        &[NfsCompoundOp::ReclaimComplete { one_fs: false }],
    );
    let second_ok = matches!(
        second.first(),
        Some(NfsCompoundResult::Status(NfsStat::CompleteAlready))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "reclaim-missing",
        &[NfsCompoundOp::ReclaimComplete { one_fs: false }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::BadSession)) if first_ok && second_ok => {
            serial_println!(
                "[NFS] {} (RECLAIM_COMPLETE then COMPLETE_ALREADY then BADSESSION)",
                GATE_NFS_RECLAIM_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS reclaim FAILED first={} second={} missing={:?}",
                first_ok,
                second_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_BINDCONN_MARKER: &str = "GATE_NFS bindconn";

/// BIND_CONN_TO_SESSION of a CREATE_SESSION id is OK; unknown is BADSESSION.
pub fn bindconn_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let issued = client.compound(
        "bindconn-exchangeid",
        &[NfsCompoundOp::ExchangeId {
            client_name: String::from("knoxos-bindconn"),
        }],
    );
    let client_id = match issued.first() {
        Some(NfsCompoundResult::ClientId { client_id, .. }) => *client_id,
        other => {
            serial_println!("[NFS] GATE_NFS bindconn FAILED: exchangeid {:?}", other);
            return false;
        }
    };
    let created = client.compound(
        "bindconn-create",
        &[NfsCompoundOp::CreateSession { client_id }],
    );
    let session_id = match created.first() {
        Some(NfsCompoundResult::Session { session_id }) => *session_id,
        other => {
            serial_println!("[NFS] GATE_NFS bindconn FAILED: create {:?}", other);
            return false;
        }
    };
    let bound = client.compound(
        "bindconn",
        &[NfsCompoundOp::BindConnToSession { session_id }],
    );
    let bound_ok = matches!(
        bound.first(),
        Some(NfsCompoundResult::Session { session_id: sid }) if *sid == session_id
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "bindconn-missing",
        &[NfsCompoundOp::BindConnToSession { session_id }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::BadSession)) if bound_ok => {
            serial_println!(
                "[NFS] {} (BIND_CONN_TO_SESSION then BADSESSION)",
                GATE_NFS_BINDCONN_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS bindconn FAILED bound={} missing={:?}",
                bound_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_FREESTATEID_MARKER: &str = "GATE_NFS freestateid";

/// FREE_STATEID of a CLOSEd OPEN state_id is OK; a second or unknown id is BAD_STATEID.
pub fn freestateid_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let opened = client.compound(
        "freestateid-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_READ,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-free.txt"),
                create: true,
            },
        ],
    );
    let state_id = match opened.get(1) {
        Some(NfsCompoundResult::OpenResult { state_id, .. }) => *state_id,
        other => {
            serial_println!("[NFS] GATE_NFS freestateid FAILED: open {:?}", other);
            return false;
        }
    };
    let closed = client.compound("freestateid-close", &[NfsCompoundOp::Close(state_id)]);
    if !matches!(closed.first(), Some(NfsCompoundResult::Status(NfsStat::Ok))) {
        serial_println!(
            "[NFS] GATE_NFS freestateid FAILED: close {:?}",
            closed.first()
        );
        return false;
    }
    let freed = client.compound("freestateid", &[NfsCompoundOp::FreeStateId { state_id }]);
    let free_ok = matches!(freed.first(), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let again = client.compound(
        "freestateid-again",
        &[NfsCompoundOp::FreeStateId { state_id }],
    );
    let again_ok = matches!(
        again.first(),
        Some(NfsCompoundResult::Status(NfsStat::BadStateId))
    );
    let missing = client.compound(
        "freestateid-missing",
        &[NfsCompoundOp::FreeStateId {
            state_id: NfsStateId::anonymous(),
        }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::BadStateId)) if free_ok && again_ok => {
            serial_println!(
                "[NFS] {} (FREE_STATEID then BAD_STATEID)",
                GATE_NFS_FREESTATEID_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS freestateid FAILED free={} again={} missing={:?}",
                free_ok,
                again_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_BACKCHANNEL_MARKER: &str = "GATE_NFS backchannel";

/// BACKCHANNEL_CTL of a CREATE_SESSION id is OK; unknown is BADSESSION.
pub fn backchannel_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let issued = client.compound(
        "backchannel-exchangeid",
        &[NfsCompoundOp::ExchangeId {
            client_name: String::from("knoxos-backchannel"),
        }],
    );
    let client_id = match issued.first() {
        Some(NfsCompoundResult::ClientId { client_id, .. }) => *client_id,
        other => {
            serial_println!("[NFS] GATE_NFS backchannel FAILED: exchangeid {:?}", other);
            return false;
        }
    };
    let created = client.compound(
        "backchannel-create",
        &[NfsCompoundOp::CreateSession { client_id }],
    );
    let session_id = match created.first() {
        Some(NfsCompoundResult::Session { session_id }) => *session_id,
        other => {
            serial_println!("[NFS] GATE_NFS backchannel FAILED: create {:?}", other);
            return false;
        }
    };
    let bound = client.compound(
        "backchannel",
        &[NfsCompoundOp::BackchannelCtl { session_id }],
    );
    let bound_ok = matches!(bound.first(), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "backchannel-missing",
        &[NfsCompoundOp::BackchannelCtl { session_id }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::BadSession)) if bound_ok => {
            serial_println!(
                "[NFS] {} (BACKCHANNEL_CTL then BADSESSION)",
                GATE_NFS_BACKCHANNEL_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS backchannel FAILED bound={} missing={:?}",
                bound_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_TESTSTATEID_MARKER: &str = "GATE_NFS teststateid";

/// TEST_STATEID of a live OPEN is OK; a CLOSEd or unknown id is BAD_STATEID.
pub fn teststateid_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let opened = client.compound(
        "teststateid-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_READ,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-test.txt"),
                create: true,
            },
        ],
    );
    let state_id = match opened.get(1) {
        Some(NfsCompoundResult::OpenResult { state_id, .. }) => *state_id,
        other => {
            serial_println!("[NFS] GATE_NFS teststateid FAILED: open {:?}", other);
            return false;
        }
    };
    let live = client.compound("teststateid", &[NfsCompoundOp::TestStateId { state_id }]);
    let live_ok = matches!(live.first(), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let closed = client.compound("teststateid-close", &[NfsCompoundOp::Close(state_id)]);
    if !matches!(closed.first(), Some(NfsCompoundResult::Status(NfsStat::Ok))) {
        serial_println!(
            "[NFS] GATE_NFS teststateid FAILED: close {:?}",
            closed.first()
        );
        return false;
    }
    let after = client.compound(
        "teststateid-closed",
        &[NfsCompoundOp::TestStateId { state_id }],
    );
    let closed_ok = matches!(
        after.first(),
        Some(NfsCompoundResult::Status(NfsStat::BadStateId))
    );
    let missing = client.compound(
        "teststateid-missing",
        &[NfsCompoundOp::TestStateId {
            state_id: NfsStateId::anonymous(),
        }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::BadStateId)) if live_ok && closed_ok => {
            serial_println!(
                "[NFS] {} (TEST_STATEID then BAD_STATEID)",
                GATE_NFS_TESTSTATEID_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS teststateid FAILED live={} closed={} missing={:?}",
                live_ok,
                closed_ok,
                other
            );
            false
        }
    }
}
