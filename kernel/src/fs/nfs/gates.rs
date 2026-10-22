use alloc::string::String;
use alloc::vec;

use crate::serial_println;

use super::client::{NfsClient, NfsMountOptions};
use super::compound::{NfsCompoundOp, NfsCompoundResult};
use super::types::{
    NFS4_ACCESS_DELETE, NFS4_ACCESS_EXECUTE, NFS4_ACCESS_EXTEND, NFS4_ACCESS_LOOKUP,
    NFS4_ACCESS_MODIFY, NFS4_ACCESS_READ, NFS4_SHARE_ACCESS_WRITE, NFS4_SHARE_DENY_NONE, NfsAttrs,
    NfsFileHandle, NfsFileType, NfsStat, NfsStateId,
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

pub const GATE_NFS_ACCESS_MARKER: &str = "GATE_NFS access";

/// ACCESS must grant file bits from mode, not echo the requested mask.
pub fn access_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    client
        .export_files
        .insert(String::from("gate-a.txt"), b"a".to_vec());
    let requested = NFS4_ACCESS_READ
        | NFS4_ACCESS_LOOKUP
        | NFS4_ACCESS_MODIFY
        | NFS4_ACCESS_EXTEND
        | NFS4_ACCESS_DELETE
        | NFS4_ACCESS_EXECUTE;
    let file_bits = NFS4_ACCESS_READ | NFS4_ACCESS_MODIFY | NFS4_ACCESS_EXTEND;
    let looked = client.compound(
        "access-file",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Lookup(String::from("gate-a.txt")),
            NfsCompoundOp::Access(requested),
        ],
    );
    let file_ok = matches!(
        looked.get(2),
        Some(NfsCompoundResult::AccessResult { supported, access })
            if *supported == requested && *access == file_bits && *access != requested
    );
    let created = client.compound(
        "access-dir",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Create {
                name: String::from("gatedir"),
                file_type: NfsFileType::Directory,
                attrs: NfsAttrs::new_dir(1),
                link_text: String::new(),
            },
            NfsCompoundOp::Access(requested),
        ],
    );
    match created.get(2) {
        Some(NfsCompoundResult::AccessResult { supported, access })
            if file_ok
                && *supported == requested
                && *access & NFS4_ACCESS_LOOKUP != 0
                && *access & NFS4_ACCESS_EXECUTE != 0 =>
        {
            serial_println!(
                "[NFS] {} (ACCESS file={:#x} dir={:#x})",
                GATE_NFS_ACCESS_MARKER,
                file_bits,
                access
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS access FAILED: {:?}", other);
            false
        }
    }
}

pub const GATE_NFS_COMMIT_MARKER: &str = "GATE_NFS commit";
const GATE_NFS_COMMIT_PAYLOAD: &[u8] = b"knoxos-nfs-c\n";

/// Unstable WRITE must not persist until COMMIT; then READ returns the bytes.
pub fn commit_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let written = client.compound(
        "commit-write",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_WRITE,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-c.txt"),
                create: true,
            },
            NfsCompoundOp::Write {
                state_id: NfsStateId::new(1),
                offset: 0,
                data: GATE_NFS_COMMIT_PAYLOAD.to_vec(),
                stable: false,
            },
        ],
    );
    let write_ok = matches!(
        written.get(2),
        Some(NfsCompoundResult::WriteResult {
            count,
            committed: false
        }) if *count == GATE_NFS_COMMIT_PAYLOAD.len() as u32
    );
    let before = client.compound(
        "commit-read-before",
        &[NfsCompoundOp::Read {
            state_id: NfsStateId::anonymous(),
            offset: 0,
            count: 64,
        }],
    );
    let unseen = matches!(
        before.first(),
        Some(NfsCompoundResult::ReadResult { data, .. }) if data.is_empty()
    );
    let committed = client.compound(
        "commit",
        &[NfsCompoundOp::Commit {
            offset: 0,
            count: 0,
        }],
    );
    let commit_ok = matches!(
        committed.first(),
        Some(NfsCompoundResult::Status(NfsStat::Ok))
    );
    let after = client.compound(
        "commit-read-after",
        &[NfsCompoundOp::Read {
            state_id: NfsStateId::anonymous(),
            offset: 0,
            count: 64,
        }],
    );
    match after.first() {
        Some(NfsCompoundResult::ReadResult { data, .. })
            if write_ok && unseen && commit_ok && data == GATE_NFS_COMMIT_PAYLOAD =>
        {
            serial_println!(
                "[NFS] {} (unstable WRITE then COMMIT {} bytes)",
                GATE_NFS_COMMIT_MARKER,
                data.len()
            );
            true
        }
        other => {
            serial_println!("[NFS] GATE_NFS commit FAILED: {:?}", other);
            false
        }
    }
}

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
