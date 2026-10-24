use alloc::string::String;

use crate::serial_println;

use super::super::client::{NfsClient, NfsMountOptions};
use super::super::compound::{NfsCompoundOp, NfsCompoundResult};
use super::super::types::{
    NFS4_SHARE_ACCESS_BOTH, NFS4_SHARE_ACCESS_READ, NFS4_SHARE_ACCESS_WRITE, NFS4_SHARE_DENY_NONE,
    NfsStat, NfsStateId,
};

pub const GATE_NFS_OPENCONFIRM_MARKER: &str = "GATE_NFS openconfirm";

/// OPEN_CONFIRM of the issued OPEN state_id is OK; a wrong id is BAD_STATEID; missing FH is NOFILEHANDLE.
pub fn openconfirm_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let opened = client.compound(
        "openconfirm-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_WRITE,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-oc.txt"),
                create: true,
            },
        ],
    );
    let state_id = match opened.get(1) {
        Some(NfsCompoundResult::OpenResult { state_id, .. }) => *state_id,
        other => {
            serial_println!("[NFS] GATE_NFS openconfirm FAILED: open {:?}", other);
            return false;
        }
    };
    let ok = client.compound("openconfirm", &[NfsCompoundOp::OpenConfirm { state_id }]);
    let confirmed = matches!(ok.first(), Some(NfsCompoundResult::OpenResult { .. }));
    let bad = client.compound(
        "openconfirm-bad",
        &[NfsCompoundOp::OpenConfirm {
            state_id: NfsStateId::anonymous(),
        }],
    );
    let bad_ok = matches!(
        bad.first(),
        Some(NfsCompoundResult::Status(NfsStat::BadStateId))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "openconfirm-missing",
        &[NfsCompoundOp::OpenConfirm {
            state_id: NfsStateId::anonymous(),
        }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle)) if confirmed && bad_ok => {
            serial_println!(
                "[NFS] {} (OPEN_CONFIRM then BAD_STATEID then NOFILEHANDLE)",
                GATE_NFS_OPENCONFIRM_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS openconfirm FAILED confirmed={} bad={} missing={:?}",
                confirmed,
                bad_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_DOWNGRADE_MARKER: &str = "GATE_NFS downgrade";

/// OPEN_DOWNGRADE to a subset is OK; a superset is INVAL; a wrong id is BAD_STATEID; missing FH is NOFILEHANDLE.
pub fn downgrade_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let opened = client.compound(
        "downgrade-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_BOTH,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-od.txt"),
                create: true,
            },
        ],
    );
    let state_id = match opened.get(1) {
        Some(NfsCompoundResult::OpenResult { state_id, .. }) => *state_id,
        other => {
            serial_println!("[NFS] GATE_NFS downgrade FAILED: open {:?}", other);
            return false;
        }
    };
    let down = client.compound(
        "downgrade",
        &[NfsCompoundOp::OpenDowngrade {
            state_id,
            share_access: NFS4_SHARE_ACCESS_READ,
            share_deny: NFS4_SHARE_DENY_NONE,
        }],
    );
    let down_ok = matches!(down.first(), Some(NfsCompoundResult::OpenResult { .. }));
    let wider = client.compound(
        "downgrade-wider",
        &[NfsCompoundOp::OpenDowngrade {
            state_id,
            share_access: NFS4_SHARE_ACCESS_BOTH,
            share_deny: NFS4_SHARE_DENY_NONE,
        }],
    );
    let wider_ok = matches!(
        wider.first(),
        Some(NfsCompoundResult::Status(NfsStat::Inval))
    );
    let bad = client.compound(
        "downgrade-bad",
        &[NfsCompoundOp::OpenDowngrade {
            state_id: NfsStateId::anonymous(),
            share_access: NFS4_SHARE_ACCESS_READ,
            share_deny: NFS4_SHARE_DENY_NONE,
        }],
    );
    let bad_ok = matches!(
        bad.first(),
        Some(NfsCompoundResult::Status(NfsStat::BadStateId))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "downgrade-missing",
        &[NfsCompoundOp::OpenDowngrade {
            state_id: NfsStateId::anonymous(),
            share_access: NFS4_SHARE_ACCESS_READ,
            share_deny: NFS4_SHARE_DENY_NONE,
        }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle)) if down_ok && wider_ok && bad_ok => {
            serial_println!(
                "[NFS] {} (OPEN_DOWNGRADE then INVAL then BAD_STATEID then NOFILEHANDLE)",
                GATE_NFS_DOWNGRADE_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS downgrade FAILED down={} wider={} bad={} missing={:?}",
                down_ok,
                wider_ok,
                bad_ok,
                other
            );
            false
        }
    }
}
