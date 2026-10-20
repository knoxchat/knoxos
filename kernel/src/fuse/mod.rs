/// FUSE — Filesystem in Userspace
///
/// Allows userspace processes to provide filesystem implementations
/// via a well-defined kernel↔userspace protocol. The kernel handles
/// VFS integration while the userspace daemon handles the actual storage.
///
/// Module layout:
///   protocol  — opcodes, headers, attrs, packed encode/decode
///   mount     — mount table, loopback store, kernel↔daemon channel
///   loopback  — in-kernel FUSE daemon handlers
///   ops       — VFS lookup/read/write/... through FUSE
///   gates     — GATE_FUSE serial markers and self-tests
mod gates;
mod loopback;
mod mount;
mod ops;
mod protocol;

pub use gates::{
    GATE_FUSE_CREATE_MARKER, GATE_FUSE_GETATTR_MARKER, GATE_FUSE_GETXATTR_MARKER,
    GATE_FUSE_LINK_MARKER, GATE_FUSE_MARKER, GATE_FUSE_MKDIR_MARKER, GATE_FUSE_MKNOD_MARKER,
    GATE_FUSE_READDIR_MARKER, GATE_FUSE_RENAME_MARKER, GATE_FUSE_RMDIR_MARKER,
    GATE_FUSE_SETATTR_MARKER, GATE_FUSE_SETXATTR_MARKER, GATE_FUSE_STATFS_MARKER,
    GATE_FUSE_SYMLINK_MARKER, GATE_FUSE_UNLINK_MARKER, GATE_FUSE_WRITE_MARKER, create_self_test,
    getattr_self_test, getxattr_self_test, link_self_test, mkdir_self_test, mknod_self_test,
    readdir_self_test, rename_self_test, rmdir_self_test, setattr_self_test, setxattr_self_test,
    statfs_self_test, symlink_self_test, unlink_self_test, write_self_test,
};
pub use mount::{
    FuseMount, FuseRequest, list_mounts, loopback_put, mount, read_request, unmount, write_response,
};
pub use ops::{
    fuse_create, fuse_getattr, fuse_getxattr, fuse_link, fuse_lookup, fuse_mkdir, fuse_mknod,
    fuse_read, fuse_readdir, fuse_readlink, fuse_rename, fuse_rmdir, fuse_setattr, fuse_setxattr,
    fuse_statfs, fuse_symlink, fuse_unlink, fuse_write,
};
pub use protocol::{
    FuseAttr, FuseEntry, FuseInHeader, FuseOpcode, FuseOutHeader, FuseStatfs, parse_fuse_dirents,
    parse_fuse_entry, parse_fuse_statfs,
};

use crate::serial_println;

use gates::lookup_self_test;

/// Initialize FUSE subsystem
pub fn init() {
    serial_println!("[fuse] Filesystem in Userspace (FUSE) initialized");
    let _ = lookup_self_test();
    let _ = write_self_test();
    let _ = unlink_self_test();
    let _ = mkdir_self_test();
    let _ = rmdir_self_test();
    let _ = rename_self_test();
    let _ = symlink_self_test();
    let _ = link_self_test();
    let _ = create_self_test();
    let _ = getattr_self_test();
    let _ = setattr_self_test();
    let _ = readdir_self_test();
    let _ = mknod_self_test();
    let _ = statfs_self_test();
    let _ = setxattr_self_test();
    let _ = getxattr_self_test();
}
