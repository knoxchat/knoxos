use alloc::string::String;
use alloc::vec;

use crate::serial_println;

use super::super::client::{NfsClient, NfsMountOptions};
use super::super::compound::{NfsCompoundOp, NfsCompoundResult};
use super::super::types::{NfsFileHandle, NfsStat};

pub const GATE_NFS_GETFH_MARKER: &str = "GATE_NFS getfh";

/// GETFH after LOOKUP must return a child handle distinct from the export root.
pub fn getfh_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("gate-fh.txt"), b"fh".to_vec());
    let root = client.compound(
        "getfh-root",
        &[NfsCompoundOp::PutRootFh, NfsCompoundOp::GetFh],
    );
    let child = client.compound(
        "getfh-child",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gate-fh.txt")),
            NfsCompoundOp::GetFh,
        ],
    );
    let root_fh = match root.get(1) {
        Some(NfsCompoundResult::FileHandle(fh)) => fh.clone(),
        other => {
            serial_println!("[NFS] GATE_NFS getfh FAILED: root {:?}", other);
            return false;
        }
    };
    match child.get(2) {
        Some(NfsCompoundResult::FileHandle(fh)) if *fh != root_fh => {
            serial_println!(
                "[NFS] {} (GETFH child {} bytes)",
                GATE_NFS_GETFH_MARKER,
                fh.0.len()
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS getfh FAILED: child {:?}", other);
            false
        }
    }
}

pub const GATE_NFS_PUTFH_MARKER: &str = "GATE_NFS putfh";

/// PUTFH restores a GETFH handle; an unknown handle is STALE.
pub fn putfh_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("gate-pf.txt"), b"pf".to_vec());
    let looked = client.compound(
        "putfh-lookup",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gate-pf.txt")),
            NfsCompoundOp::GetFh,
        ],
    );
    let child = match looked.get(2) {
        Some(NfsCompoundResult::FileHandle(fh)) => fh.clone(),
        other => {
            serial_println!("[NFS] GATE_NFS putfh FAILED: lookup {:?}", other);
            return false;
        }
    };
    let switched = client.compound(
        "putfh-root",
        &[NfsCompoundOp::PutRootFh, NfsCompoundOp::GetFh],
    );
    let root_ok = matches!(
        switched.get(1),
        Some(NfsCompoundResult::FileHandle(fh)) if *fh != child
    );
    let restored = client.compound(
        "putfh-restore",
        &[NfsCompoundOp::PutFh(child.clone()), NfsCompoundOp::GetFh],
    );
    let restored_ok = matches!(
        restored.get(1),
        Some(NfsCompoundResult::FileHandle(fh)) if *fh == child
    );
    let stale = client.compound(
        "putfh-stale",
        &[NfsCompoundOp::PutFh(NfsFileHandle::new(vec![0xff; 8]))],
    );
    match stale.first() {
        Some(NfsCompoundResult::Status(NfsStat::Stale)) if root_ok && restored_ok => {
            serial_println!("[NFS] {} (PUTFH restore then STALE)", GATE_NFS_PUTFH_MARKER);
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS putfh FAILED: {:?}", other);
            false
        }
    }
}

pub const GATE_NFS_LOOKUPP_MARKER: &str = "GATE_NFS lookupp";

/// LOOKUPP of a child returns the export root; LOOKUPP of the root is NOENT.
pub fn lookupp_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("gate-pp.txt"), b"pp".to_vec());
    let root = client.compound(
        "lookupp-rootfh",
        &[NfsCompoundOp::PutRootFh, NfsCompoundOp::GetFh],
    );
    let root_fh = match root.get(1) {
        Some(NfsCompoundResult::FileHandle(fh)) => fh.clone(),
        other => {
            serial_println!("[NFS] GATE_NFS lookupp FAILED: root {:?}", other);
            return false;
        }
    };
    let parent = client.compound(
        "lookupp-child",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gate-pp.txt")),
            NfsCompoundOp::LookupP,
            NfsCompoundOp::GetFh,
        ],
    );
    let parent_ok = matches!(
        parent.get(3),
        Some(NfsCompoundResult::FileHandle(fh)) if *fh == root_fh
    );
    let at_root = client.compound(
        "lookupp-root",
        &[NfsCompoundOp::PutRootFh, NfsCompoundOp::LookupP],
    );
    match at_root.get(1) {
        Some(NfsCompoundResult::Status(NfsStat::Noent)) if parent_ok => {
            serial_println!(
                "[NFS] {} (LOOKUPP child→root then root NOENT)",
                GATE_NFS_LOOKUPP_MARKER
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS lookupp FAILED: {:?}", other);
            false
        }
    }
}

pub const GATE_NFS_RESTOREFH_MARKER: &str = "GATE_NFS restorefh";

/// RESTOREFH restores SAVEFH after PUTROOTFH; RESTOREFH without SAVEFH is NOFILEHANDLE.
pub fn restorefh_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("gate-rf.txt"), b"rf".to_vec());
    let looked = client.compound(
        "restorefh-lookup",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gate-rf.txt")),
            NfsCompoundOp::GetFh,
        ],
    );
    let child = match looked.get(2) {
        Some(NfsCompoundResult::FileHandle(fh)) => fh.clone(),
        other => {
            serial_println!("[NFS] GATE_NFS restorefh FAILED: lookup {:?}", other);
            return false;
        }
    };
    let restored = client.compound(
        "restorefh-roundtrip",
        &[
            NfsCompoundOp::PutFh(child.clone()),
            NfsCompoundOp::SaveFh,
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::RestoreFh,
            NfsCompoundOp::GetFh,
        ],
    );
    let restored_ok = matches!(
        restored.get(4),
        Some(NfsCompoundResult::FileHandle(fh)) if *fh == child
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound("restorefh-missing", &[NfsCompoundOp::RestoreFh]);
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle)) if restored_ok => {
            serial_println!(
                "[NFS] {} (RESTOREFH restore then NOFILEHANDLE)",
                GATE_NFS_RESTOREFH_MARKER
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS restorefh FAILED: {:?}", other);
            false
        }
    }
}

pub const GATE_NFS_PUTPUBFH_MARKER: &str = "GATE_NFS putpubfh";

/// PUTPUBFH sets the public (root) handle after LOOKUP of a child.
pub fn putpubfh_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("gate-pb.txt"), b"pb".to_vec());
    let root = client.compound(
        "putpubfh-root",
        &[NfsCompoundOp::PutRootFh, NfsCompoundOp::GetFh],
    );
    let root_fh = match root.get(1) {
        Some(NfsCompoundResult::FileHandle(fh)) => fh.clone(),
        other => {
            serial_println!("[NFS] GATE_NFS putpubfh FAILED: root {:?}", other);
            return false;
        }
    };
    let child = client.compound(
        "putpubfh-lookup",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gate-pb.txt")),
            NfsCompoundOp::GetFh,
        ],
    );
    let child_fh = match child.get(2) {
        Some(NfsCompoundResult::FileHandle(fh)) => fh.clone(),
        other => {
            serial_println!("[NFS] GATE_NFS putpubfh FAILED: lookup {:?}", other);
            return false;
        }
    };
    if child_fh == root_fh {
        serial_println!("[NFS] GATE_NFS putpubfh FAILED: child equals root");
        return false;
    }
    let pubfh = client.compound(
        "putpubfh-switch",
        &[NfsCompoundOp::PutPubFh, NfsCompoundOp::GetFh],
    );
    match pubfh.get(1) {
        Some(NfsCompoundResult::FileHandle(fh)) if *fh == root_fh => {
            serial_println!(
                "[NFS] {} (PUTPUBFH after LOOKUP is root)",
                GATE_NFS_PUTPUBFH_MARKER
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS putpubfh FAILED: {:?}", other);
            false
        }
    }
}
