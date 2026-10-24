/// NFS Client (Network File System v4)
///
/// Implements an NFS4 client for accessing remote filesystems over the network.
/// Compatible with Linux NFS4 protocol for network storage.
///
/// Features:
///   - NFS4 compound operations (LOOKUP, OPEN, READ, WRITE, CLOSE, etc.)
///   - File handle management
///   - Stateful file locking
///   - Attribute caching with configurable timeouts
///   - Mount point integration with VFS
///   - Authentication stubs (AUTH_SYS)
///   - RPC/XDR encoding/decoding
///   - Delegation support
///   - Read-ahead and write-behind caching
///
/// Module layout:
///   types      — NFS4 constants, status, handles, attrs, locks
///   xdr        — RPC message types and XDR encode/decode
///   compound   — NFS4 compound ops and results
///   client     — NfsClient and mount options
///   mount      — mount table, nfs_mount / nfs_umount
///   rpc        — TCP RPC send and real-network mount
///   util       — hash and timestamp helpers
///   gates      — GATE_NFS self-tests (io, fh, verify, state, lock, open, attr, deleg, session)
mod client;
mod compound;
mod gates;
mod mount;
mod rpc;
mod types;
mod util;
mod xdr;

pub use client::{NfsClient, NfsMountOptions};
pub use compound::{NfsCompoundOp, NfsCompoundResult};
pub use gates::{
    GATE_NFS_ACCESS_MARKER, GATE_NFS_CLOSE_MARKER, GATE_NFS_COMMIT_MARKER, GATE_NFS_CONFIRM_MARKER,
    GATE_NFS_CREATE_MARKER, GATE_NFS_DELEGPURGE_MARKER, GATE_NFS_DELEGRETURN_MARKER,
    GATE_NFS_DESTROYCLIENTID_MARKER, GATE_NFS_DESTROYSESSION_MARKER, GATE_NFS_DOWNGRADE_MARKER,
    GATE_NFS_EXCHANGEID_MARKER, GATE_NFS_GETFH_MARKER, GATE_NFS_LINK_MARKER, GATE_NFS_LOCK_MARKER,
    GATE_NFS_LOCKOWNER_MARKER, GATE_NFS_LOCKT_MARKER, GATE_NFS_LOOKUPP_MARKER, GATE_NFS_MARKER,
    GATE_NFS_NVERIFY_MARKER, GATE_NFS_OPENATTR_MARKER, GATE_NFS_OPENCONFIRM_MARKER,
    GATE_NFS_PUTFH_MARKER, GATE_NFS_PUTPUBFH_MARKER, GATE_NFS_READDIR_MARKER,
    GATE_NFS_READLINK_MARKER, GATE_NFS_REMOVE_MARKER, GATE_NFS_RENAME_MARKER,
    GATE_NFS_RENEW_MARKER, GATE_NFS_RESTOREFH_MARKER, GATE_NFS_SECINFO_MARKER,
    GATE_NFS_SESSION_MARKER, GATE_NFS_SETATTR_MARKER, GATE_NFS_UNLOCK_MARKER,
    GATE_NFS_VERIFY_MARKER, GATE_NFS_WRITE_MARKER, access_self_test, close_self_test,
    commit_self_test, confirm_self_test, create_self_test, delegpurge_self_test,
    delegreturn_self_test, destroyclientid_self_test, destroysession_self_test,
    downgrade_self_test, exchangeid_self_test, getfh_self_test, link_self_test, lock_self_test,
    lockowner_self_test, lockt_self_test, lookupp_self_test, nverify_self_test, openattr_self_test,
    openconfirm_self_test, putfh_self_test, putpubfh_self_test, readdir_self_test,
    readlink_self_test, remove_self_test, rename_self_test, renew_self_test, restorefh_self_test,
    secinfo_self_test, session_self_test, setattr_self_test, unlock_self_test, verify_self_test,
    write_self_test,
};
pub use mount::{NfsMount, list_nfs_mounts, nfs_mount, nfs_umount, proc_nfs_info};
pub use rpc::{build_lookup_compound, nfs_mount_real, send_rpc};
pub use types::{
    NFS_PORT, NFS4_ACCESS_DELETE, NFS4_ACCESS_EXECUTE, NFS4_ACCESS_EXTEND, NFS4_ACCESS_LOOKUP,
    NFS4_ACCESS_MODIFY, NFS4_ACCESS_READ, NFS4_OPEN4_CREATE, NFS4_PROGRAM, NFS4_SHARE_ACCESS_BOTH,
    NFS4_SHARE_ACCESS_READ, NFS4_SHARE_ACCESS_WRITE, NFS4_SHARE_DENY_BOTH, NFS4_SHARE_DENY_NONE,
    NFS4_SHARE_DENY_READ, NFS4_SHARE_DENY_WRITE, NFS4_VERSION, NfsAttrs, NfsDirEntry,
    NfsFileHandle, NfsFileType, NfsLock, NfsLockType, NfsOp, NfsStat, NfsStateId,
};
pub use xdr::{AuthFlavor, AuthSys, RpcMsgType, XdrDecoder, XdrEncoder};

use core::sync::atomic::{AtomicBool, Ordering};

use crate::serial_println;

use gates::compound_self_test;

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize NFS subsystem
pub fn init() {
    if INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    INITIALIZED.store(true, Ordering::Relaxed);
    serial_println!("[KnoxOS] NFS v4 client initialized");
    serial_println!("[NFS] Network I/O: TCP socket via net stack");
    let _ = compound_self_test();
    let _ = write_self_test();
    let _ = remove_self_test();
    let _ = rename_self_test();
    let _ = create_self_test();
    let _ = link_self_test();
    let _ = readlink_self_test();
    let _ = setattr_self_test();
    let _ = readdir_self_test();
    let _ = access_self_test();
    let _ = commit_self_test();
    let _ = getfh_self_test();
    let _ = putfh_self_test();
    let _ = lookupp_self_test();
    let _ = restorefh_self_test();
    let _ = putpubfh_self_test();
    let _ = verify_self_test();
    let _ = nverify_self_test();
    let _ = confirm_self_test();
    let _ = close_self_test();
    let _ = renew_self_test();
    let _ = lock_self_test();
    let _ = unlock_self_test();
    let _ = lockt_self_test();
    let _ = lockowner_self_test();
    let _ = openconfirm_self_test();
    let _ = downgrade_self_test();
    let _ = secinfo_self_test();
    let _ = openattr_self_test();
    let _ = delegpurge_self_test();
    let _ = delegreturn_self_test();
    let _ = exchangeid_self_test();
    let _ = session_self_test();
    let _ = destroysession_self_test();
    let _ = destroyclientid_self_test();
}
