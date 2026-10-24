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
///   gates     — GATE_FUSE self-tests (io, names, dir, attr, xattr, file, lock, misc)
mod gates;
mod loopback;
mod mount;
mod ops;
mod protocol;

pub use gates::{
    GATE_FUSE_ACCESS_MARKER, GATE_FUSE_BMAP_MARKER, GATE_FUSE_COPY_FILE_RANGE_MARKER,
    GATE_FUSE_CREATE_MARKER, GATE_FUSE_DESTROY_MARKER, GATE_FUSE_FALLOCATE_MARKER,
    GATE_FUSE_FLUSH_MARKER, GATE_FUSE_FORGET_MARKER, GATE_FUSE_FSYNC_MARKER,
    GATE_FUSE_FSYNCDIR_MARKER, GATE_FUSE_GETATTR_MARKER, GATE_FUSE_GETLK_MARKER,
    GATE_FUSE_GETXATTR_MARKER, GATE_FUSE_INIT_MARKER, GATE_FUSE_IOCTL_MARKER,
    GATE_FUSE_LINK_MARKER, GATE_FUSE_LISTXATTR_MARKER, GATE_FUSE_LSEEK_MARKER, GATE_FUSE_MARKER,
    GATE_FUSE_MKDIR_MARKER, GATE_FUSE_MKNOD_MARKER, GATE_FUSE_OPEN_MARKER,
    GATE_FUSE_OPENDIR_MARKER, GATE_FUSE_POLL_MARKER, GATE_FUSE_READDIR_MARKER,
    GATE_FUSE_READDIRPLUS_MARKER, GATE_FUSE_RELEASE_MARKER, GATE_FUSE_RELEASEDIR_MARKER,
    GATE_FUSE_REMOVEXATTR_MARKER, GATE_FUSE_RENAME_MARKER, GATE_FUSE_RENAME2_MARKER,
    GATE_FUSE_RMDIR_MARKER, GATE_FUSE_SETATTR_MARKER, GATE_FUSE_SETLK_MARKER,
    GATE_FUSE_SETLKW_MARKER, GATE_FUSE_SETXATTR_MARKER, GATE_FUSE_STATFS_MARKER,
    GATE_FUSE_STATX_MARKER, GATE_FUSE_SYMLINK_MARKER, GATE_FUSE_SYNCFS_MARKER,
    GATE_FUSE_UNLINK_MARKER, GATE_FUSE_WRITE_MARKER, access_self_test, bmap_self_test,
    copy_file_range_self_test, create_self_test, destroy_self_test, fallocate_self_test,
    flush_self_test, forget_self_test, fsync_self_test, fsyncdir_self_test, getattr_self_test,
    getlk_self_test, getxattr_self_test, init_self_test, ioctl_self_test, link_self_test,
    listxattr_self_test, lseek_self_test, mkdir_self_test, mknod_self_test, open_self_test,
    opendir_self_test, poll_self_test, readdir_self_test, readdirplus_self_test, release_self_test,
    releasedir_self_test, removexattr_self_test, rename_self_test, rename2_self_test,
    rmdir_self_test, setattr_self_test, setlk_self_test, setlkw_self_test, setxattr_self_test,
    statfs_self_test, statx_self_test, symlink_self_test, syncfs_self_test, unlink_self_test,
    write_self_test,
};
pub use mount::{
    FuseMount, FuseRequest, list_mounts, loopback_put, mount, read_request, unmount, write_response,
};
pub use ops::{
    fuse_access, fuse_bmap, fuse_copy_file_range, fuse_create, fuse_destroy, fuse_fallocate,
    fuse_flush, fuse_forget, fuse_fsync, fuse_fsyncdir, fuse_getattr, fuse_getlk, fuse_getxattr,
    fuse_init, fuse_ioctl, fuse_link, fuse_listxattr, fuse_lookup, fuse_lseek, fuse_mkdir,
    fuse_mknod, fuse_open, fuse_opendir, fuse_poll, fuse_read, fuse_readdir, fuse_readdirplus,
    fuse_readlink, fuse_release, fuse_releasedir, fuse_removexattr, fuse_rename, fuse_rename2,
    fuse_rmdir, fuse_setattr, fuse_setlk, fuse_setlkw, fuse_setxattr, fuse_statfs, fuse_statx,
    fuse_symlink, fuse_syncfs, fuse_unlink, fuse_write,
};
pub use protocol::{
    F_RDLCK, F_UNLCK, F_WRLCK, FUSE_KERNEL_MINOR_VERSION, FUSE_KERNEL_VERSION, FUSE_R_OK,
    FUSE_W_OK, FUSE_X_OK, FuseAttr, FuseEntry, FuseFileLock, FuseInHeader, FuseOpcode,
    FuseOutHeader, FuseStatfs, POLLIN, POLLOUT, RENAME_NOREPLACE, SEEK_END, SEEK_SET,
    STATX_BASIC_STATS, mode_to_access, parse_fuse_bmap, parse_fuse_copy_file_range,
    parse_fuse_direntplus, parse_fuse_dirents, parse_fuse_entry, parse_fuse_init, parse_fuse_ioctl,
    parse_fuse_lk, parse_fuse_lseek, parse_fuse_open, parse_fuse_poll, parse_fuse_statfs,
    parse_fuse_statx,
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
    let _ = listxattr_self_test();
    let _ = removexattr_self_test();
    let _ = access_self_test();
    let _ = fsync_self_test();
    let _ = open_self_test();
    let _ = release_self_test();
    let _ = flush_self_test();
    let _ = fsyncdir_self_test();
    let _ = opendir_self_test();
    let _ = releasedir_self_test();
    let _ = getlk_self_test();
    let _ = setlk_self_test();
    let _ = setlkw_self_test();
    let _ = bmap_self_test();
    let _ = destroy_self_test();
    let _ = ioctl_self_test();
    let _ = poll_self_test();
    let _ = fallocate_self_test();
    let _ = readdirplus_self_test();
    let _ = rename2_self_test();
    let _ = lseek_self_test();
    let _ = copy_file_range_self_test();
    let _ = syncfs_self_test();
    let _ = statx_self_test();
    let _ = init_self_test();
    let _ = forget_self_test();
}
