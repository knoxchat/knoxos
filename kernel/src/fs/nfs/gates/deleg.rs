use alloc::string::String;

use crate::serial_println;

use super::super::client::{NfsClient, NfsMountOptions};
use super::super::compound::{NfsCompoundOp, NfsCompoundResult};
use super::super::types::{NFS4_SHARE_ACCESS_READ, NFS4_SHARE_DENY_NONE, NfsStat, NfsStateId};

pub const GATE_NFS_DELEGPURGE_MARKER: &str = "GATE_NFS delegpurge";

/// DELEGPURGE of a confirmed client_id is OK; a wrong id is STALE_CLIENTID.
pub fn delegpurge_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let issued = client.compound(
        "delegpurge-setclientid",
        &[NfsCompoundOp::SetClientId {
            client_name: String::from("knoxos-delegpurge"),
        }],
    );
    let client_id = match issued.first() {
        Some(NfsCompoundResult::ClientId { client_id, .. }) => *client_id,
        other => {
            serial_println!("[NFS] GATE_NFS delegpurge FAILED: setclientid {:?}", other);
            return false;
        }
    };
    let confirmed = client.compound(
        "delegpurge-confirm",
        &[NfsCompoundOp::SetClientIdConfirm { client_id }],
    );
    if !matches!(
        confirmed.first(),
        Some(NfsCompoundResult::Status(NfsStat::Ok))
    ) {
        serial_println!(
            "[NFS] GATE_NFS delegpurge FAILED: confirm {:?}",
            confirmed.first()
        );
        return false;
    }
    let opened = client.compound(
        "delegpurge-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_READ,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-dp.txt"),
                create: true,
            },
        ],
    );
    if !matches!(opened.get(1), Some(NfsCompoundResult::OpenResult { .. })) {
        serial_println!("[NFS] GATE_NFS delegpurge FAILED: open {:?}", opened.get(1));
        return false;
    }
    let ok = client.compound("delegpurge", &[NfsCompoundOp::DelegPurge { client_id }]);
    let purged = matches!(ok.first(), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let stale = client.compound(
        "delegpurge-stale",
        &[NfsCompoundOp::DelegPurge { client_id: 0xdead }],
    );
    match stale.first() {
        Some(NfsCompoundResult::Status(NfsStat::StaleClientId)) if purged => {
            serial_println!(
                "[NFS] {} (DELEGPURGE then STALE_CLIENTID)",
                GATE_NFS_DELEGPURGE_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS delegpurge FAILED purged={} stale={:?}",
                purged,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_DELEGRETURN_MARKER: &str = "GATE_NFS delegreturn";

/// DELEGRETURN of an OPEN state_id is OK; a second return is BAD_STATEID; missing FH is NOFILEHANDLE.
pub fn delegreturn_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let opened = client.compound(
        "delegreturn-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_READ,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-dr.txt"),
                create: true,
            },
        ],
    );
    let state_id = match opened.get(1) {
        Some(NfsCompoundResult::OpenResult { state_id, .. }) => *state_id,
        other => {
            serial_println!("[NFS] GATE_NFS delegreturn FAILED: open {:?}", other);
            return false;
        }
    };
    let ok = client.compound("delegreturn", &[NfsCompoundOp::DelegReturn { state_id }]);
    let returned = matches!(ok.first(), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let again = client.compound(
        "delegreturn-again",
        &[NfsCompoundOp::DelegReturn { state_id }],
    );
    let bad_ok = matches!(
        again.first(),
        Some(NfsCompoundResult::Status(NfsStat::BadStateId))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "delegreturn-missing",
        &[NfsCompoundOp::DelegReturn { state_id }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle)) if returned && bad_ok => {
            serial_println!(
                "[NFS] {} (DELEGRETURN then BAD_STATEID then NOFILEHANDLE)",
                GATE_NFS_DELEGRETURN_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS delegreturn FAILED returned={} bad={} missing={:?}",
                returned,
                bad_ok,
                other
            );
            false
        }
    }
}
