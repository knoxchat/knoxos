use alloc::string::String;

use crate::serial_println;

use super::super::client::{NfsClient, NfsMountOptions};
use super::super::compound::{NfsCompoundOp, NfsCompoundResult};
use super::super::types::NfsStat;

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
