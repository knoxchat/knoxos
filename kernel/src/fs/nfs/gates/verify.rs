use alloc::string::String;

use crate::serial_println;

use super::super::client::{NfsClient, NfsMountOptions};
use super::super::compound::{NfsCompoundOp, NfsCompoundResult};
use super::super::types::{NfsAttrs, NfsStat};

pub const GATE_NFS_VERIFY_MARKER: &str = "GATE_NFS verify";

/// VERIFY of matching size is NFS4_OK; mismatch is NFS4ERR_NOT_SAME; missing FH is NOFILEHANDLE.
pub fn verify_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("gate-vf.txt"), b"vf".to_vec());
    let ok = client.compound(
        "verify-match",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gate-vf.txt")),
            NfsCompoundOp::Verify { size: 2 },
        ],
    );
    let match_ok = matches!(ok.get(2), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let mismatch = client.compound(
        "verify-mismatch",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gate-vf.txt")),
            NfsCompoundOp::Verify { size: 99 },
        ],
    );
    let mismatch_ok = matches!(
        mismatch.get(2),
        Some(NfsCompoundResult::Status(NfsStat::NotSameAttr))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound("verify-missing", &[NfsCompoundOp::Verify { size: 0 }]);
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle)) if match_ok && mismatch_ok => {
            serial_println!(
                "[NFS] {} (VERIFY match then NOT_SAME then NOFILEHANDLE)",
                GATE_NFS_VERIFY_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS verify FAILED match={} mismatch={} missing={:?}",
                match_ok,
                mismatch_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_NVERIFY_MARKER: &str = "GATE_NFS nverify";

/// NVERIFY of a mismatch is NFS4_OK; matching size is NFS4ERR_SAME; missing FH is NOFILEHANDLE.
pub fn nverify_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("gate-nv.txt"), b"nv".to_vec());
    let mismatch = client.compound(
        "nverify-mismatch",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gate-nv.txt")),
            NfsCompoundOp::Nverify { size: 99 },
        ],
    );
    let mismatch_ok = matches!(
        mismatch.get(2),
        Some(NfsCompoundResult::Status(NfsStat::Ok))
    );
    let same = client.compound(
        "nverify-same",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gate-nv.txt")),
            NfsCompoundOp::Nverify { size: 2 },
        ],
    );
    let same_ok = matches!(
        same.get(2),
        Some(NfsCompoundResult::Status(NfsStat::NotSame))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound("nverify-missing", &[NfsCompoundOp::Nverify { size: 0 }]);
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle)) if mismatch_ok && same_ok => {
            serial_println!(
                "[NFS] {} (NVERIFY mismatch then SAME then NOFILEHANDLE)",
                GATE_NFS_NVERIFY_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS nverify FAILED mismatch={} same={} missing={:?}",
                mismatch_ok,
                same_ok,
                other
            );
            false
        }
    }
}
