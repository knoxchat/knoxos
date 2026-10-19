use alloc::string::String;
use alloc::vec;

use crate::serial_println;

use super::client::{NfsClient, NfsMountOptions};
use super::compound::{NfsCompoundOp, NfsCompoundResult};
use super::types::{
    NFS4_SHARE_ACCESS_WRITE, NFS4_SHARE_DENY_NONE, NfsAttrs, NfsFileType, NfsStat, NfsStateId,
};

pub const GATE_NFS_MARKER: &str = "GATE_NFS compound";
const GATE_NFS_PAYLOAD: &[u8] = b"knoxos-nfs\n";

pub(super) fn compound_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("gate.txt"), GATE_NFS_PAYLOAD.to_vec());
    let results = client.compound(
        "gate",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gate.txt")),
            NfsCompoundOp::Read {
                state_id: NfsStateId::new(1),
                offset: 0,
                count: 64,
            },
        ],
    );
    match results.get(2) {
        Some(NfsCompoundResult::ReadResult { data, .. }) if data == GATE_NFS_PAYLOAD => {
            serial_println!(
                "[NFS] {} (PUTROOTFH+LOOKUP+READ {} bytes)",
                GATE_NFS_MARKER,
                data.len()
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS FAILED: {:?}", other);
            false
        }
    }
}

pub const GATE_NFS_WRITE_MARKER: &str = "GATE_NFS write";
const GATE_NFS_WRITE_PAYLOAD: &[u8] = b"knoxos-nfs-w\n";

/// WRITE must store bytes that a later READ in the same compound returns.
pub fn write_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let results = client.compound(
        "write",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_WRITE,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-w.txt"),
                create: true,
            },
            NfsCompoundOp::Write {
                state_id: NfsStateId::new(1),
                offset: 0,
                data: GATE_NFS_WRITE_PAYLOAD.to_vec(),
                stable: true,
            },
            NfsCompoundOp::Read {
                state_id: NfsStateId::new(1),
                offset: 0,
                count: 64,
            },
        ],
    );
    let written = matches!(
        results.get(2),
        Some(NfsCompoundResult::WriteResult { count, .. })
            if *count == GATE_NFS_WRITE_PAYLOAD.len() as u32
    );
    match results.get(3) {
        Some(NfsCompoundResult::ReadResult { data, .. })
            if written && data == GATE_NFS_WRITE_PAYLOAD =>
        {
            serial_println!(
                "[NFS] {} (OPEN+WRITE+READ {} bytes)",
                GATE_NFS_WRITE_MARKER,
                data.len()
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS write FAILED: {:?}", other);
            false
        }
    }
}

pub const GATE_NFS_REMOVE_MARKER: &str = "GATE_NFS remove";

/// REMOVE must drop the name so a later LOOKUP is NFS4ERR_NOENT.
pub fn remove_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("gone.txt"), b"x".to_vec());
    let removed = client.compound(
        "remove",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gone.txt")),
            NfsCompoundOp::Remove(String::from("gone.txt")),
        ],
    );
    let drop_ok = matches!(removed.get(2), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let after = client.compound(
        "lookup-gone",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gone.txt")),
        ],
    );
    match after.get(1) {
        Some(NfsCompoundResult::Status(NfsStat::Noent)) if drop_ok => {
            serial_println!(
                "[NFS] {} (REMOVE then LOOKUP NFS4ERR_NOENT)",
                GATE_NFS_REMOVE_MARKER
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS remove FAILED: {:?}", other);
            false
        }
    }
}

pub const GATE_NFS_RENAME_MARKER: &str = "GATE_NFS rename";

/// RENAME must move the name so a later LOOKUP finds the new one.
pub fn rename_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("old.txt"), b"rn".to_vec());
    let renamed = client.compound(
        "rename",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("old.txt")),
            NfsCompoundOp::Rename {
                old_name: String::from("old.txt"),
                new_name: String::from("new.txt"),
            },
        ],
    );
    let drop_ok = matches!(renamed.get(2), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let after = client.compound(
        "lookup-new",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("new.txt")),
        ],
    );
    let found = matches!(after.get(1), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let gone = client.compound(
        "lookup-old",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("old.txt")),
        ],
    );
    match gone.get(1) {
        Some(NfsCompoundResult::Status(NfsStat::Noent)) if drop_ok && found => {
            serial_println!(
                "[NFS] {} (RENAME old→new then LOOKUP NFS4ERR_NOENT)",
                GATE_NFS_RENAME_MARKER
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS rename FAILED: {:?}", other);
            false
        }
    }
}

pub const GATE_NFS_CREATE_MARKER: &str = "GATE_NFS create";

/// CREATE must add a name so a later LOOKUP finds it.
pub fn create_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let created = client.compound(
        "create",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Create {
                name: String::from("gatedir"),
                file_type: NfsFileType::Directory,
                attrs: NfsAttrs::new_dir(1),
                link_text: String::new(),
            },
        ],
    );
    let make_ok = matches!(created.get(1), Some(NfsCompoundResult::Attrs(a)) if a.file_type == NfsFileType::Directory);
    let after = client.compound(
        "lookup-created",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gatedir")),
        ],
    );
    match after.get(1) {
        Some(NfsCompoundResult::Status(NfsStat::Ok)) if make_ok => {
            serial_println!("[NFS] {} (CREATE dir then LOOKUP)", GATE_NFS_CREATE_MARKER);
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS create FAILED: {:?}", other);
            false
        }
    }
}

pub const GATE_NFS_LINK_MARKER: &str = "GATE_NFS link";

/// LINK must add a name so a later LOOKUP finds it while the old name remains.
pub fn link_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("old.txt"), b"ln".to_vec());
    let linked = client.compound(
        "link",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("old.txt")),
            NfsCompoundOp::SaveFh,
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Link {
                new_name: String::from("new.txt"),
            },
        ],
    );
    let make_ok = matches!(
        linked.get(4),
        Some(NfsCompoundResult::LinkResult { name }) if name == "new.txt"
    );
    let after = client.compound(
        "lookup-new",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("new.txt")),
        ],
    );
    let found = matches!(after.get(1), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let still = client.compound(
        "lookup-old",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("old.txt")),
        ],
    );
    match still.get(1) {
        Some(NfsCompoundResult::Status(NfsStat::Ok)) if make_ok && found => {
            serial_println!(
                "[NFS] {} (SAVEFH+LINK then LOOKUP old+new)",
                GATE_NFS_LINK_MARKER
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS link FAILED: {:?}", other);
            false
        }
    }
}

pub const GATE_NFS_READLINK_MARKER: &str = "GATE_NFS readlink";
const GATE_NFS_READLINK_TARGET: &str = "/tmp/gate-target";

/// CREATE NF4LNK then READLINK must return the target.
pub fn readlink_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let created = client.compound(
        "symlink",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Create {
                name: String::from("gate.link"),
                file_type: NfsFileType::SymLink,
                attrs: NfsAttrs::new_file(1, GATE_NFS_READLINK_TARGET.len() as u64),
                link_text: String::from(GATE_NFS_READLINK_TARGET),
            },
        ],
    );
    let make_ok = matches!(created.get(1), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let after = client.compound(
        "readlink",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gate.link")),
            NfsCompoundOp::ReadLink,
        ],
    );
    match after.get(2) {
        Some(NfsCompoundResult::ReadLinkResult { target })
            if make_ok && target == GATE_NFS_READLINK_TARGET =>
        {
            serial_println!(
                "[NFS] {} (CREATE NF4LNK then READLINK)",
                GATE_NFS_READLINK_MARKER
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS readlink FAILED: {:?}", other);
            false
        }
    }
}

pub const GATE_NFS_SETATTR_MARKER: &str = "GATE_NFS setattr";
const GATE_NFS_SETATTR_PAYLOAD: &[u8] = b"knoxos-nfs-sa\n";
const GATE_NFS_SETATTR_SIZE: u64 = 5;

/// SETATTR must change size so a later GETATTR/READ sees the new length.
pub fn setattr_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("sattr.txt"), GATE_NFS_SETATTR_PAYLOAD.to_vec());
    let looked = client.compound(
        "lookup",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("sattr.txt")),
        ],
    );
    let found = matches!(looked.get(1), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let mut attrs = NfsAttrs::new_file(1, GATE_NFS_SETATTR_SIZE);
    attrs.size = GATE_NFS_SETATTR_SIZE;
    let set = client.compound(
        "setattr",
        &[NfsCompoundOp::SetAttr(NfsStateId::anonymous(), attrs)],
    );
    let set_ok = matches!(set.first(), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let after = client.compound("getattr", &[NfsCompoundOp::GetAttr(vec![0, 1])]);
    let size_ok = matches!(
        after.first(),
        Some(NfsCompoundResult::Attrs(a)) if a.size == GATE_NFS_SETATTR_SIZE
    );
    let read = client.compound(
        "read",
        &[NfsCompoundOp::Read {
            state_id: NfsStateId::anonymous(),
            offset: 0,
            count: 64,
        }],
    );
    match read.first() {
        Some(NfsCompoundResult::ReadResult { data, .. })
            if found
                && set_ok
                && size_ok
                && data == &GATE_NFS_SETATTR_PAYLOAD[..GATE_NFS_SETATTR_SIZE as usize] =>
        {
            serial_println!(
                "[NFS] {} (SETATTR size={} then GETATTR/READ)",
                GATE_NFS_SETATTR_MARKER,
                GATE_NFS_SETATTR_SIZE
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS setattr FAILED: {:?}", other);
            false
        }
    }
}

pub const GATE_NFS_READDIR_MARKER: &str = "GATE_NFS readdir";

/// READDIR of the export root must list created files and directories.
pub fn readdir_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("a.txt"), b"a".to_vec());
    let created = client.compound(
        "create-dir",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Create {
                name: String::from("bdir"),
                file_type: NfsFileType::Directory,
                attrs: NfsAttrs::new_dir(1),
                link_text: String::new(),
            },
        ],
    );
    let make_ok = matches!(
        created.get(1),
        Some(NfsCompoundResult::Attrs(a)) if a.file_type == NfsFileType::Directory
    );
    let listed = client.compound(
        "readdir",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::ReadDir {
                cookie: 0,
                dircount: 8192,
                maxcount: 32768,
            },
        ],
    );
    match listed.get(1) {
        Some(NfsCompoundResult::DirEntries { entries, eof: true })
            if make_ok
                && entries.iter().any(|e| e.name == "a.txt")
                && entries.iter().any(|e| e.name == "bdir") =>
        {
            serial_println!(
                "[NFS] {} (READDIR {} names)",
                GATE_NFS_READDIR_MARKER,
                entries.len()
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS readdir FAILED: {:?}", other);
            false
        }
    }
}
