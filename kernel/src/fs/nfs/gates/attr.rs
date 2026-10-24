use alloc::string::String;

use crate::serial_println;

use super::super::client::{NfsClient, NfsMountOptions};
use super::super::compound::{NfsCompoundOp, NfsCompoundResult};
use super::super::types::{NFS4_SHARE_ACCESS_READ, NFS4_SHARE_DENY_NONE, NfsStat};
use super::super::xdr::AuthFlavor;

pub const GATE_NFS_SECINFO_MARKER: &str = "GATE_NFS secinfo";

/// SECINFO of a known name is AUTH_SYS; missing name is NOENT; missing FH is NOFILEHANDLE.
pub fn secinfo_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let opened = client.compound(
        "secinfo-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_READ,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-si.txt"),
                create: true,
            },
        ],
    );
    if !matches!(opened.get(1), Some(NfsCompoundResult::OpenResult { .. })) {
        serial_println!("[NFS] GATE_NFS secinfo FAILED: open {:?}", opened.get(1));
        return false;
    }
    let ok = client.compound(
        "secinfo",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::SecInfo {
                name: String::from("gate-si.txt"),
            },
        ],
    );
    let flavors_ok = matches!(
        ok.get(1),
        Some(NfsCompoundResult::SecInfo { flavors })
            if matches!(flavors.as_slice(), [f] if *f == AuthFlavor::Sys as u32)
    );
    let missing_name = client.compound(
        "secinfo-noent",
        &[NfsCompoundOp::SecInfo {
            name: String::from("missing-si.txt"),
        }],
    );
    let noent_ok = matches!(
        missing_name.first(),
        Some(NfsCompoundResult::Status(NfsStat::Noent))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "secinfo-missing",
        &[NfsCompoundOp::SecInfo {
            name: String::from("gate-si.txt"),
        }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle)) if flavors_ok && noent_ok => {
            serial_println!(
                "[NFS] {} (SECINFO AUTH_SYS then NOENT then NOFILEHANDLE)",
                GATE_NFS_SECINFO_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS secinfo FAILED flavors={} noent={} missing={:?}",
                flavors_ok,
                noent_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_OPENATTR_MARKER: &str = "GATE_NFS openattr";

/// OPENATTR createdir creates a named-attr directory; without createdir first is NOENT; missing FH is NOFILEHANDLE.
pub fn openattr_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let opened = client.compound(
        "openattr-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_READ,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-oa.txt"),
                create: true,
            },
        ],
    );
    let file_fh = match opened.get(1) {
        Some(NfsCompoundResult::OpenResult { fh, .. }) => fh.clone(),
        other => {
            serial_println!("[NFS] GATE_NFS openattr FAILED: open {:?}", other);
            return false;
        }
    };
    let absent = client.compound(
        "openattr-absent",
        &[NfsCompoundOp::OpenAttr { createdir: false }],
    );
    let absent_ok = matches!(
        absent.first(),
        Some(NfsCompoundResult::Status(NfsStat::Noent))
    );
    let created = client.compound("openattr", &[NfsCompoundOp::OpenAttr { createdir: true }]);
    let created_ok = match created.first() {
        Some(NfsCompoundResult::FileHandle(fh)) => *fh != file_fh,
        _ => false,
    };
    let again = client.compound(
        "openattr-again",
        &[
            NfsCompoundOp::PutFh(file_fh.clone()),
            NfsCompoundOp::OpenAttr { createdir: false },
        ],
    );
    let again_ok = matches!(again.get(1), Some(NfsCompoundResult::FileHandle(_)));
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "openattr-missing",
        &[NfsCompoundOp::OpenAttr { createdir: true }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle))
            if absent_ok && created_ok && again_ok =>
        {
            serial_println!(
                "[NFS] {} (OPENATTR NOENT then createdir then GETFH)",
                GATE_NFS_OPENATTR_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS openattr FAILED absent={} created={} again={} missing={:?}",
                absent_ok,
                created_ok,
                again_ok,
                other
            );
            false
        }
    }
}
