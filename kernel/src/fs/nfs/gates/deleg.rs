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

pub const GATE_NFS_DIRDELEG_MARKER: &str = "GATE_NFS dirdeleg";

/// GET_DIR_DELEGATION of root/dir is a state_id; a file is NOTDIR; missing FH is NOFILEHANDLE.
pub fn dirdeleg_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let root = client.compound(
        "dirdeleg-root",
        &[NfsCompoundOp::PutRootFh, NfsCompoundOp::GetDirDelegation],
    );
    let root_ok = matches!(root.get(1), Some(NfsCompoundResult::DirDeleg { .. }));
    let opened = client.compound(
        "dirdeleg-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_READ,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-dd.txt"),
                create: true,
            },
        ],
    );
    if !matches!(opened.get(1), Some(NfsCompoundResult::OpenResult { .. })) {
        serial_println!("[NFS] GATE_NFS dirdeleg FAILED: open {:?}", opened.get(1));
        return false;
    }
    let file = client.compound("dirdeleg-file", &[NfsCompoundOp::GetDirDelegation]);
    let notdir_ok = matches!(
        file.first(),
        Some(NfsCompoundResult::Status(NfsStat::Notdir))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound("dirdeleg-missing", &[NfsCompoundOp::GetDirDelegation]);
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle)) if root_ok && notdir_ok => {
            serial_println!(
                "[NFS] {} (GET_DIR_DELEGATION then NOTDIR then NOFILEHANDLE)",
                GATE_NFS_DIRDELEG_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS dirdeleg FAILED root={} notdir={} missing={:?}",
                root_ok,
                notdir_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_WANTDELEG_MARKER: &str = "GATE_NFS wantdeleg";

/// WANT_DELEGATION of a file is a state_id; a directory is NOTDIR; missing FH is NOFILEHANDLE.
pub fn wantdeleg_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let opened = client.compound(
        "wantdeleg-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_READ,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-wd.txt"),
                create: true,
            },
        ],
    );
    if !matches!(opened.get(1), Some(NfsCompoundResult::OpenResult { .. })) {
        serial_println!("[NFS] GATE_NFS wantdeleg FAILED: open {:?}", opened.get(1));
        return false;
    }
    let file = client.compound("wantdeleg", &[NfsCompoundOp::WantDelegation]);
    let file_ok = matches!(file.first(), Some(NfsCompoundResult::WantDeleg { .. }));
    let root = client.compound(
        "wantdeleg-root",
        &[NfsCompoundOp::PutRootFh, NfsCompoundOp::WantDelegation],
    );
    let notdir_ok = matches!(
        root.get(1),
        Some(NfsCompoundResult::Status(NfsStat::Notdir))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound("wantdeleg-missing", &[NfsCompoundOp::WantDelegation]);
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle)) if file_ok && notdir_ok => {
            serial_println!(
                "[NFS] {} (WANT_DELEGATION then NOTDIR then NOFILEHANDLE)",
                GATE_NFS_WANTDELEG_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS wantdeleg FAILED file={} notdir={} missing={:?}",
                file_ok,
                notdir_ok,
                other
            );
            false
        }
    }
}
