use alloc::string::String;

use crate::serial_println;

use super::super::client::{NfsClient, NfsMountOptions};
use super::super::compound::{NfsCompoundOp, NfsCompoundResult};
use super::super::types::{NFS4_SHARE_ACCESS_WRITE, NFS4_SHARE_DENY_NONE, NfsStat, NfsStateId};

pub const GATE_NFS_CONFIRM_MARKER: &str = "GATE_NFS confirm";

/// SETCLIENTID_CONFIRM of the issued client_id is NFS4_OK; a wrong id is STALE_CLIENTID.
pub fn confirm_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let issued = client.compound(
        "setclientid",
        &[NfsCompoundOp::SetClientId {
            client_name: String::from("knoxos-gate"),
        }],
    );
    let client_id = match issued.first() {
        Some(NfsCompoundResult::ClientId { client_id, .. }) => *client_id,
        other => {
            serial_println!("[NFS] GATE_NFS confirm FAILED: setclientid {:?}", other);
            return false;
        }
    };
    let ok = client.compound(
        "setclientid-confirm",
        &[NfsCompoundOp::SetClientIdConfirm { client_id }],
    );
    let confirmed = matches!(ok.first(), Some(NfsCompoundResult::Status(NfsStat::Ok)))
        && client.client_id_confirmed;
    let stale = client.compound(
        "setclientid-stale",
        &[NfsCompoundOp::SetClientIdConfirm { client_id: 0xdead }],
    );
    match stale.first() {
        Some(NfsCompoundResult::Status(NfsStat::StaleClientId)) if confirmed => {
            serial_println!(
                "[NFS] {} (SETCLIENTID_CONFIRM then STALE_CLIENTID)",
                GATE_NFS_CONFIRM_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS confirm FAILED confirmed={} stale={:?}",
                confirmed,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_CLOSE_MARKER: &str = "GATE_NFS close";

/// CLOSE of the issued OPEN state_id is NFS4_OK; a wrong id is BAD_STATEID; missing FH is NOFILEHANDLE.
pub fn close_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let opened = client.compound(
        "close-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_WRITE,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-cl.txt"),
                create: true,
            },
        ],
    );
    let state_id = match opened.get(1) {
        Some(NfsCompoundResult::OpenResult { state_id, .. }) => *state_id,
        other => {
            serial_println!("[NFS] GATE_NFS close FAILED: open {:?}", other);
            return false;
        }
    };
    let ok = client.compound("close", &[NfsCompoundOp::Close(state_id)]);
    let closed = matches!(ok.first(), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let bad = client.compound(
        "close-bad",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gate-cl.txt")),
            NfsCompoundOp::Close(NfsStateId::anonymous()),
        ],
    );
    let bad_ok = matches!(
        bad.get(2),
        Some(NfsCompoundResult::Status(NfsStat::BadStateId))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "close-missing",
        &[NfsCompoundOp::Close(NfsStateId::anonymous())],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle)) if closed && bad_ok => {
            serial_println!(
                "[NFS] {} (CLOSE then BAD_STATEID then NOFILEHANDLE)",
                GATE_NFS_CLOSE_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS close FAILED closed={} bad={} missing={:?}",
                closed,
                bad_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_RENEW_MARKER: &str = "GATE_NFS renew";

/// RENEW of a confirmed client_id is NFS4_OK; a wrong id is STALE_CLIENTID.
pub fn renew_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let issued = client.compound(
        "renew-setclientid",
        &[NfsCompoundOp::SetClientId {
            client_name: String::from("knoxos-renew"),
        }],
    );
    let client_id = match issued.first() {
        Some(NfsCompoundResult::ClientId { client_id, .. }) => *client_id,
        other => {
            serial_println!("[NFS] GATE_NFS renew FAILED: setclientid {:?}", other);
            return false;
        }
    };
    let confirmed = client.compound(
        "renew-confirm",
        &[NfsCompoundOp::SetClientIdConfirm { client_id }],
    );
    let confirm_ok = matches!(
        confirmed.first(),
        Some(NfsCompoundResult::Status(NfsStat::Ok))
    ) && client.client_id_confirmed;
    let ok = client.compound("renew", &[NfsCompoundOp::Renew(client_id)]);
    let renewed = matches!(ok.first(), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let stale = client.compound("renew-stale", &[NfsCompoundOp::Renew(0xdead)]);
    match stale.first() {
        Some(NfsCompoundResult::Status(NfsStat::StaleClientId)) if confirm_ok && renewed => {
            serial_println!(
                "[NFS] {} (RENEW then STALE_CLIENTID)",
                GATE_NFS_RENEW_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS renew FAILED confirm={} renewed={} stale={:?}",
                confirm_ok,
                renewed,
                other
            );
            false
        }
    }
}
