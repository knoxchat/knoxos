use alloc::string::String;

use crate::serial_println;

use super::super::client::{NfsClient, NfsMountOptions};
use super::super::compound::{NfsCompoundOp, NfsCompoundResult};
use super::super::types::{
    NFS4_SHARE_ACCESS_WRITE, NFS4_SHARE_DENY_NONE, NfsLock, NfsLockType, NfsStat, NfsStateId,
};

fn gate_lock(offset: u64, length: u64, owner: &str) -> NfsLock {
    NfsLock {
        lock_type: NfsLockType::WriteLt,
        offset,
        length,
        owner: String::from(owner),
        state_id: NfsStateId::anonymous(),
    }
}

pub const GATE_NFS_LOCK_MARKER: &str = "GATE_NFS lock";

/// LOCK of a byte range is NFS4_OK; overlap is DENIED; missing FH is NOFILEHANDLE.
pub fn lock_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let opened = client.compound(
        "lock-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_WRITE,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-lk.txt"),
                create: true,
            },
        ],
    );
    if !matches!(opened.get(1), Some(NfsCompoundResult::OpenResult { .. })) {
        serial_println!("[NFS] GATE_NFS lock FAILED: open {:?}", opened.get(1));
        return false;
    }
    let ok = client.compound("lock", &[NfsCompoundOp::Lock(gate_lock(0, 100, "knox"))]);
    let locked = matches!(ok.first(), Some(NfsCompoundResult::LockResult { .. }));
    let denied = client.compound(
        "lock-overlap",
        &[NfsCompoundOp::Lock(gate_lock(50, 100, "other"))],
    );
    let denied_ok = matches!(
        denied.first(),
        Some(NfsCompoundResult::Status(NfsStat::Denied))
    );
    let probed = client.compound(
        "lockt",
        &[NfsCompoundOp::LockTest(gate_lock(50, 10, "probe"))],
    );
    let probed_ok = matches!(
        probed.first(),
        Some(NfsCompoundResult::Status(NfsStat::Denied))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "lock-missing",
        &[NfsCompoundOp::Lock(gate_lock(0, 8, "knox"))],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle))
            if locked && denied_ok && probed_ok =>
        {
            serial_println!(
                "[NFS] {} (LOCK then DENIED then NOFILEHANDLE)",
                GATE_NFS_LOCK_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS lock FAILED locked={} denied={} lockt={} missing={:?}",
                locked,
                denied_ok,
                probed_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_UNLOCK_MARKER: &str = "GATE_NFS unlock";

/// LOCKU of a held range is NFS4_OK; a missing range is DENIED; missing FH is NOFILEHANDLE.
pub fn unlock_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let opened = client.compound(
        "unlock-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_WRITE,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-ul.txt"),
                create: true,
            },
        ],
    );
    if !matches!(opened.get(1), Some(NfsCompoundResult::OpenResult { .. })) {
        serial_println!("[NFS] GATE_NFS unlock FAILED: open {:?}", opened.get(1));
        return false;
    }
    let lk = gate_lock(0, 100, "knox");
    let locked = client.compound("unlock-lock", &[NfsCompoundOp::Lock(lk.clone())]);
    let lock_ok = matches!(locked.first(), Some(NfsCompoundResult::LockResult { .. }));
    let unlocked = client.compound("unlock", &[NfsCompoundOp::Unlock(lk.clone())]);
    let unlocked_ok = matches!(
        unlocked.first(),
        Some(NfsCompoundResult::Status(NfsStat::Ok))
    );
    let missing_range = client.compound("unlock-gone", &[NfsCompoundOp::Unlock(lk)]);
    let gone_ok = matches!(
        missing_range.first(),
        Some(NfsCompoundResult::Status(NfsStat::Denied))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "unlock-missing",
        &[NfsCompoundOp::Unlock(gate_lock(0, 8, "knox"))],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle))
            if lock_ok && unlocked_ok && gone_ok =>
        {
            serial_println!(
                "[NFS] {} (LOCKU then DENIED then NOFILEHANDLE)",
                GATE_NFS_UNLOCK_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS unlock FAILED lock={} unlock={} gone={} missing={:?}",
                lock_ok,
                unlocked_ok,
                gone_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_LOCKT_MARKER: &str = "GATE_NFS lockt";

/// LOCKT of a free range is NFS4_OK; overlap is DENIED; missing FH is NOFILEHANDLE.
pub fn lockt_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let opened = client.compound(
        "lockt-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_WRITE,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-lt.txt"),
                create: true,
            },
        ],
    );
    if !matches!(opened.get(1), Some(NfsCompoundResult::OpenResult { .. })) {
        serial_println!("[NFS] GATE_NFS lockt FAILED: open {:?}", opened.get(1));
        return false;
    }
    let free = client.compound(
        "lockt-free",
        &[NfsCompoundOp::LockTest(gate_lock(0, 100, "probe"))],
    );
    let free_ok = matches!(free.first(), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let locked = client.compound(
        "lockt-lock",
        &[NfsCompoundOp::Lock(gate_lock(0, 100, "knox"))],
    );
    let lock_ok = matches!(locked.first(), Some(NfsCompoundResult::LockResult { .. }));
    let denied = client.compound(
        "lockt-overlap",
        &[NfsCompoundOp::LockTest(gate_lock(50, 10, "probe"))],
    );
    let denied_ok = matches!(
        denied.first(),
        Some(NfsCompoundResult::Status(NfsStat::Denied))
    );
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "lockt-missing",
        &[NfsCompoundOp::LockTest(gate_lock(0, 8, "probe"))],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle))
            if free_ok && lock_ok && denied_ok =>
        {
            serial_println!(
                "[NFS] {} (LOCKT then DENIED then NOFILEHANDLE)",
                GATE_NFS_LOCKT_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS lockt FAILED free={} lock={} denied={} missing={:?}",
                free_ok,
                lock_ok,
                denied_ok,
                other
            );
            false
        }
    }
}

pub const GATE_NFS_LOCKOWNER_MARKER: &str = "GATE_NFS lockowner";

/// RELEASE_LOCKOWNER drops that owner's locks; missing FH is NOFILEHANDLE.
pub fn lockowner_self_test() -> bool {
    let mut client = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let opened = client.compound(
        "lockowner-open",
        &[
            NfsCompoundOp::PutRootFh,
            NfsCompoundOp::Open {
                share_access: NFS4_SHARE_ACCESS_WRITE,
                share_deny: NFS4_SHARE_DENY_NONE,
                name: String::from("gate-lo.txt"),
                create: true,
            },
        ],
    );
    if !matches!(opened.get(1), Some(NfsCompoundResult::OpenResult { .. })) {
        serial_println!("[NFS] GATE_NFS lockowner FAILED: open {:?}", opened.get(1));
        return false;
    }
    let locked = client.compound(
        "lockowner-lock",
        &[NfsCompoundOp::Lock(gate_lock(0, 100, "knox"))],
    );
    let lock_ok = matches!(locked.first(), Some(NfsCompoundResult::LockResult { .. }));
    let released = client.compound(
        "lockowner",
        &[NfsCompoundOp::ReleaseLockOwner {
            owner: String::from("knox"),
        }],
    );
    let released_ok = matches!(
        released.first(),
        Some(NfsCompoundResult::Status(NfsStat::Ok))
    );
    let probed = client.compound(
        "lockowner-lockt",
        &[NfsCompoundOp::LockTest(gate_lock(0, 100, "probe"))],
    );
    let probed_ok = matches!(probed.first(), Some(NfsCompoundResult::Status(NfsStat::Ok)));
    let mut bare = NfsClient::new(NfsMountOptions::default(
        [127, 0, 0, 1],
        "/export",
        "/mnt/nfs",
    ));
    let missing = bare.compound(
        "lockowner-missing",
        &[NfsCompoundOp::ReleaseLockOwner {
            owner: String::from("knox"),
        }],
    );
    match missing.first() {
        Some(NfsCompoundResult::Status(NfsStat::NoFileHandle))
            if lock_ok && released_ok && probed_ok =>
        {
            serial_println!(
                "[NFS] {} (RELEASE_LOCKOWNER then LOCKT OK then NOFILEHANDLE)",
                GATE_NFS_LOCKOWNER_MARKER
            );
            true
        }
        other => {
            serial_println!(
                "[NFS] GATE_NFS lockowner FAILED lock={} release={} lockt={} missing={:?}",
                lock_ok,
                released_ok,
                probed_ok,
                other
            );
            false
        }
    }
}
