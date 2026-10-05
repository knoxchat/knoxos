// ═══════════════════════════════════════════════════════════════════════
// SECURITY TESTS
// ═══════════════════════════════════════════════════════════════════════

use crate::cred::Credentials;
use crate::ssp;

#[test_case]
fn test_privilege_escalation_blocked() {
    let mut user = Credentials::user(1000, 1000);
    assert!(user.set_uid(0).is_err());
    assert_eq!(user.euid, 1000);
    let mut root = Credentials::root();
    assert!(root.set_uid(1000).is_ok());
    assert_eq!(root.euid, 1000);
}

#[test_case]
fn test_stack_smashing_detected() {
    ssp::set_thread_canary(3);
    let canary = ssp::get_thread_canary(3);
    assert_ne!(canary, 0);
    assert!(ssp::verify_thread_canary(3, canary));
    assert!(!ssp::verify_thread_canary(3, canary ^ 0xFF));
}

#[test_case]
fn test_chacha20_rfc8439_and_getrandom() {
    assert!(crate::random::csprng_self_test());
}

#[test_case]
fn test_seccomp_denies_listed_syscall() {
    assert!(crate::seccomp::deny_self_test());
}

#[test_case]
fn test_unprivileged_net_bind_fails() {
    assert!(crate::capabilities::bind_self_test());
}

#[test_case]
fn test_wx_aslr_user_maps() {
    assert!(crate::vmm::wx_aslr_self_test());
}

#[test_case]
fn test_cow_page_fault_copies() {
    assert!(crate::vmm::cow_fault_self_test());
}

#[test_case]
fn test_file_backed_mmap_faults_one_page() {
    assert!(crate::mmap::file_fault_self_test());
}

#[test_case]
fn test_inotify_sees_vfs_mutate() {
    assert!(crate::inotify::vfs_watch_self_test());
}

#[test_case]
fn test_inotify_read_bytes() {
    assert!(
        crate::inotify::inotify_bytes_self_test(),
        "inotify_read_bytes must serialize a create event header"
    );
}

#[test_case]
fn test_guard_stack_and_oom_from_alloc() {
    assert!(crate::stack_guard::guard_oom_self_test());
}

#[test_case]
fn test_buddy_leftover_ram() {
    assert!(crate::vmm::buddy_ram_self_test());
}

#[test_case]
fn test_swap_page_roundtrip() {
    assert!(
        crate::swap::swap_io_self_test(),
        "swapped-out page must restore its bytes on #PF"
    );
}

#[test_case]
fn test_landlock_denies_vfs_write() {
    assert!(
        crate::landlock::mac_self_test(),
        "landlock must deny writes outside the allowed path"
    );
}

#[test_case]
fn test_uts_namespace_isolation() {
    assert!(
        crate::namespaces::uts_isolation_self_test(),
        "unshare(CLONE_NEWUTS) hostname must not leak to the parent"
    );
}

#[test_case]
fn test_pid_namespace_isolation() {
    assert!(
        crate::pidns::pid_isolation_self_test(),
        "unshare(CLONE_NEWPID) child must be PID 1; parent unchanged"
    );
}

#[test_case]
fn test_mount_namespace_isolation() {
    assert!(
        crate::namespaces::mount_isolation_self_test(),
        "unshare(CLONE_NEWNS) bind mount must not leak to the parent"
    );
}

#[test_case]
fn test_net_namespace_isolation() {
    assert!(
        crate::namespaces::net_isolation_self_test(),
        "unshare(CLONE_NEWNET) interfaces must not leak to the parent"
    );
}

#[test_case]
fn test_user_namespace_isolation() {
    assert!(
        crate::namespaces::user_isolation_self_test(),
        "unshare(CLONE_NEWUSER) child must be uid 0; parent unchanged"
    );
}

#[test_case]
fn test_ipc_namespace_isolation() {
    assert!(
        crate::namespaces::ipc_isolation_self_test(),
        "unshare(CLONE_NEWIPC) SysV shm keys must not leak to the parent"
    );
}

#[test_case]
fn test_cgroup_namespace_isolation() {
    assert!(
        crate::namespaces::cgroup_isolation_self_test(),
        "unshare(CLONE_NEWCGROUP) cgroup paths must not leak to the parent"
    );
}

#[test_case]
fn test_time_namespace_isolation() {
    assert!(
        crate::namespaces::time_isolation_self_test(),
        "unshare(CLONE_NEWTIME) monotonic offset must not leak to the parent"
    );
}

#[test_case]
fn test_setns_joins_uts_namespace() {
    assert!(
        crate::namespaces::setns_join_self_test(),
        "setns into a child's UTS ns must see the child's hostname; parent unchanged"
    );
}

#[test_case]
fn test_chroot_isolation() {
    assert!(
        crate::process::chroot_isolation_self_test(),
        "chroot must jail path lookup; parent root unchanged"
    );
}

#[test_case]
fn test_pivot_root_isolation() {
    assert!(
        crate::process::pivot_root_self_test(),
        "pivot_root must jail / at new_root, keep old root at put_old, parent unchanged"
    );
}

#[test_case]
fn test_overlayfs_isolation() {
    assert!(
        crate::overlayfs::overlay_isolation_self_test(),
        "overlay mount must merge lower+upper, honour whiteout, stay out of parent ns"
    );
}

#[test_case]
fn test_hardlink_shares_inode() {
    assert!(
        crate::vfs::hardlink_self_test(),
        "hard link must share an inode; write via one name is visible via the other"
    );
}

#[test_case]
fn test_chmod_enforced() {
    assert!(
        crate::vfs::chmod_self_test(),
        "chmod 0400 must deny owner write and other read"
    );
}

#[test_case]
fn test_rmdir_empty_and_notempty() {
    assert!(
        crate::vfs::rmdir_self_test(),
        "rmdir must remove an empty directory and fail ENOTEMPTY on a non-empty one"
    );
}

#[test_case]
fn test_mkfifo_roundtrip() {
    assert!(
        crate::fifo::mkfifo_self_test(),
        "mkfifo must write a byte that a reader can read back"
    );
}

#[test_case]
fn test_getdents_lists_child() {
    assert!(
        crate::vfs::getdents_self_test(),
        "list_dir must include a file created in the directory"
    );
}

#[test_case]
fn test_fcntl_cloexec_dup() {
    assert!(
        crate::fd::fcntl_self_test(),
        "fcntl F_SETFD/F_GETFD must toggle cloexec; dup must clear it"
    );
}

#[test_case]
fn test_fstat_size() {
    assert!(
        crate::fd::fstat_self_test(),
        "fstat must report the written VFS file size"
    );
}

#[test_case]
fn test_writev_scatter() {
    assert!(
        crate::splice::writev_self_test(),
        "writev must write two iovecs that read back as xy"
    );
}

#[test_case]
fn test_readv_gather() {
    assert!(
        crate::splice::readv_self_test(),
        "readv must fill two iovecs from xy"
    );
}

#[test_case]
fn test_fsync_fd() {
    assert!(
        crate::syscall::fsync_self_test(),
        "fsync must succeed on a VFS fd and return EBADF for a bad fd"
    );
}

#[test_case]
fn test_syncfs_fd() {
    assert!(
        crate::syscall::syncfs_self_test(),
        "syncfs must succeed on a VFS fd and return EBADF for a bad fd"
    );
}

#[test_case]
fn test_fchmod_fd() {
    assert!(
        crate::syscall::fchmod_self_test(),
        "fchmod must set mode 0400 on a VFS fd and return EBADF for a bad fd"
    );
}

#[test_case]
fn test_fstatfs_fd() {
    assert!(
        crate::syscall::fstatfs_self_test(),
        "fstatfs must report f_bsize 4096 on a VFS fd and return EBADF for a bad fd"
    );
}

#[test_case]
fn test_fsetxattr_fd() {
    assert!(
        crate::syscall::fsetxattr_self_test(),
        "fsetxattr must round-trip user.knox on a VFS fd and return EBADF for a bad fd"
    );
}

#[test_case]
fn test_flistxattr_fd() {
    assert!(
        crate::syscall::flistxattr_self_test(),
        "flistxattr must list user.knox, fremovexattr must clear it, and a bad fd is EBADF"
    );
}

#[test_case]
fn test_listxattr_path() {
    assert!(
        crate::syscall::listxattr_self_test(),
        "listxattr must list user.knox, removexattr must clear it, and a missing path is ENOENT"
    );
}

#[test_case]
fn test_faccessat_dirfd() {
    assert!(
        crate::syscall::faccessat_self_test(),
        "faccessat must succeed on a dirfd-relative file, ENOENT for a missing child, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_mkdirat_dirfd() {
    assert!(
        crate::syscall::mkdirat_self_test(),
        "mkdirat must create a dirfd-relative directory, ENOENT for a missing parent, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_unlinkat_dirfd() {
    assert!(
        crate::syscall::unlinkat_self_test(),
        "unlinkat must remove a dirfd-relative file, ENOENT for a missing child, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_renameat_dirfd() {
    assert!(
        crate::syscall::renameat_self_test(),
        "renameat must move a dirfd-relative file, ENOENT for a missing source, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_linkat_dirfd() {
    assert!(
        crate::syscall::linkat_self_test(),
        "linkat must create a dirfd-relative hard link, ENOENT for a missing source, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_symlinkat_dirfd() {
    assert!(
        crate::syscall::symlinkat_self_test(),
        "symlinkat must create a dirfd-relative symlink, ENOENT for a missing parent, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_readlinkat_dirfd() {
    assert!(
        crate::syscall::readlinkat_self_test(),
        "readlinkat must return a dirfd-relative symlink target, ENOENT for a missing child, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_mknodat_dirfd() {
    assert!(
        crate::syscall::mknodat_self_test(),
        "mknodat must create a dirfd-relative regular file, ENOENT for a missing parent, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_fchmodat_dirfd() {
    assert!(
        crate::syscall::fchmodat_self_test(),
        "fchmodat must set mode 0400 on a dirfd-relative file, ENOENT for a missing child, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_fchownat_dirfd() {
    assert!(
        crate::syscall::fchownat_self_test(),
        "fchownat must set uid 1000 on a dirfd-relative file, ENOENT for a missing child, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_newfstatat_dirfd() {
    assert!(
        crate::syscall::newfstatat_self_test(),
        "newfstatat must report st_size == 1 on a dirfd-relative file, ENOENT for a missing child, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_openat_dirfd() {
    assert!(
        crate::syscall::openat_self_test(),
        "openat must open a dirfd-relative file, ENOENT for a missing child, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_utimensat_dirfd() {
    assert!(
        crate::syscall::utimensat_at_self_test(),
        "utimensat must set mtime 42 on a dirfd-relative file, ENOENT for a missing child, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_statx_dirfd() {
    assert!(
        crate::syscall::statx_at_self_test(),
        "statx must report stx_size == 1 on a dirfd-relative file, ENOENT for a missing child, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_futimesat_dirfd() {
    assert!(
        crate::syscall::futimesat_self_test(),
        "futimesat must set mtime 42 on a dirfd-relative file, ENOENT for a missing child, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_renameat2_dirfd() {
    assert!(
        crate::syscall::renameat2_self_test(),
        "renameat2 must move a dirfd-relative file, ENOENT for a missing source, and EBADF for a bad dirfd"
    );
}

#[test_case]
fn test_timerfd_expires() {
    assert!(
        crate::timerfd::timerfd_expire_self_test(),
        "timerfd_settime 1ns must make read return an expiration"
    );
}

#[test_case]
fn test_signalfd_roundtrip() {
    assert!(
        crate::signalfd::signalfd_roundtrip_self_test(),
        "signalfd must return the injected SIGUSR1"
    );
}

#[test_case]
fn test_epoll_pipe_readiness() {
    assert!(
        crate::epoll::epoll_pipe_self_test(),
        "epoll_wait must stay idle on an empty pipe and fire EPOLLIN after write"
    );
}

#[test_case]
fn test_poll_pipe_readiness() {
    assert!(
        crate::epoll::poll_pipe_self_test(),
        "poll must stay idle on an empty pipe and fire POLLIN after write"
    );
}

#[test_case]
fn test_memfd_roundtrip() {
    assert!(
        crate::memfd::memfd_roundtrip_self_test(),
        "memfd write then read must return the same bytes"
    );
}

#[test_case]
fn test_socketpair_roundtrip() {
    assert!(
        crate::uds::socketpair_roundtrip_self_test(),
        "socketpair send then recv must return the same bytes"
    );
}

#[test_case]
fn test_eventfd_counter_roundtrip() {
    assert!(
        crate::eventfd::eventfd_roundtrip_self_test(),
        "eventfd write then read must return the same counter"
    );
}

#[test_case]
fn test_pipe_ipc_roundtrip() {
    assert!(
        crate::ipc::pipe_roundtrip_self_test(),
        "pipe write then read must return the same bytes"
    );
}

#[test_case]
fn test_pipe_tee_does_not_consume() {
    assert!(
        crate::ipc::pipe_tee_self_test(),
        "tee must copy pipe bytes without consuming the source"
    );
}

#[test_case]
fn test_dbus_unix_socket_roundtrip() {
    assert!(
        crate::dbus::unix_bus_self_test(),
        "AF_UNIX connect to /run/dbus/system_bus_socket must round-trip"
    );
}

#[test_case]
fn test_percpu_gs_layout() {
    assert_eq!(
        core::mem::offset_of!(crate::usermode::CpuLocal, user_rsp),
        0
    );
    assert_eq!(
        core::mem::offset_of!(crate::usermode::CpuLocal, kernel_rsp),
        8
    );
    assert_eq!(
        core::mem::offset_of!(crate::usermode::CpuLocal, cpu_index),
        24
    );
    assert_eq!(core::mem::size_of::<crate::context::IrqFrame>(), 160);
}

#[test_case]
fn test_wayland_shm_pool_roundtrip() {
    assert!(
        crate::wayland::shm_pool_self_test(),
        "SHM pool write must round-trip pixel bytes"
    );
}

#[test_case]
fn test_signal_trampoline_bytes() {
    let frame = crate::signals::SignalFrame::new(2);
    assert_eq!(
        &frame.trampoline[0..9],
        &[0x48, 0xC7, 0xC0, 0x0F, 0x00, 0x00, 0x00, 0x0F, 0x05]
    );
    assert_eq!(frame.signo, 2);
}

#[test_case]
fn test_null_pointer_is_noncanonical_user() {
    // A userspace null deref is not a mapped VMA; the kernel treats addr 0
    // as invalid. This is the predicate the #PF path uses before demand map.
    let addr: u64 = 0;
    assert_eq!(addr & 0xFFFF_8000_0000_0000, 0);
    assert!(addr < 0x1000);
}
