use alloc::string::String;

use crate::fs::mount::UmountFlags;
use crate::serial_println;
/// new mount API, mount/umount2, pivot_root, setns, swap
use crate::syscall::{SyscallError, SyscallResult, read_user_string};

// ── New mount API ───────────────────────────────────────────────────

pub fn sys_fsopen(fstype_ptr: u64, flags: u32) -> SyscallResult {
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    let fstype = unsafe { read_user_string(fstype_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::mount_api::fsopen(&fstype, flags)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_fsconfig(fd: i32, cmd: u32, key_ptr: u64, value_ptr: u64, aux: i32) -> SyscallResult {
    let key = if key_ptr != 0 {
        // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
        unsafe { read_user_string(key_ptr) }
    } else {
        None
    };
    let value = if value_ptr != 0 {
        // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
        unsafe { read_user_string(value_ptr) }
    } else {
        None
    };
    let fs_value = value.map(crate::mount_api::FsConfigValue::String);
    crate::mount_api::fsconfig(fd, cmd, key.as_deref(), fs_value)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_fsmount(fs_fd: i32, flags: u32, mount_attrs: u64) -> SyscallResult {
    crate::mount_api::fsmount(fs_fd, flags, mount_attrs)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_move_mount(
    from_dfd: i32,
    from_path_ptr: u64,
    to_dfd: i32,
    to_path_ptr: u64,
    flags: u32,
) -> SyscallResult {
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    let from = unsafe { read_user_string(from_path_ptr) }.unwrap_or_default();
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    let to = unsafe { read_user_string(to_path_ptr) }.unwrap_or_default();
    crate::mount_api::move_mount(from_dfd, &from, to_dfd, &to, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_open_tree(dfd: i32, path_ptr: u64, flags: u32) -> SyscallResult {
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    let path = unsafe { read_user_string(path_ptr) }.unwrap_or_default();
    crate::mount_api::open_tree(dfd, &path, flags)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_fspick(dfd: i32, path_ptr: u64, flags: u32) -> SyscallResult {
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    let path = unsafe { read_user_string(path_ptr) }.unwrap_or_default();
    crate::mount_api::fspick(dfd, &path, flags)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── setns ───────────────────────────────────────────────────────────

pub fn sys_setns(fd: i32, nstype: i32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::fd::path_for_fd(pid, fd).ok_or(SyscallError::BadFileDescriptor)?;
    let (fd_flag, ns_id) =
        crate::namespaces::parse_ns_fd_path(&path).ok_or(SyscallError::InvalidArgument)?;
    let flag = if nstype != 0 { nstype as u32 } else { fd_flag };
    if nstype != 0 && nstype as u32 != fd_flag {
        return Err(SyscallError::InvalidArgument);
    }
    crate::namespaces::setns(pid, ns_id, flag)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── pivot_root ──────────────────────────────────────────────────────

pub fn sys_pivot_root(new_root_ptr: u64, put_old_ptr: u64) -> SyscallResult {
    let new_root =
        // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
        unsafe { read_user_string(new_root_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    let put_old = unsafe { read_user_string(put_old_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    match crate::process::pivot_root(pid, &new_root, &put_old) {
        Ok(()) => {
            serial_println!(
                "[KnoxOS] pivot_root({}, {}) for PID {}",
                new_root,
                put_old,
                pid
            );
            Ok(0)
        }
        Err(-2) => Err(SyscallError::FileNotFound),
        Err(-20) => Err(SyscallError::NotDirectory),
        _ => Err(SyscallError::InvalidArgument),
    }
}

// ── mount / umount2 (Linux standard) ────────────────────────────────

pub fn sys_mount_linux(
    source_ptr: u64,
    target_ptr: u64,
    fstype_ptr: u64,
    flags: u64,
    data_ptr: u64,
) -> SyscallResult {
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    let source = unsafe { read_user_string(source_ptr) }.unwrap_or_default();
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    let target = unsafe { read_user_string(target_ptr) }.unwrap_or_default();
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    let fstype = unsafe { read_user_string(fstype_ptr) }.unwrap_or_default();
    let options = if data_ptr != 0 {
        // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
        unsafe { read_user_string(data_ptr) }.unwrap_or_default()
    } else {
        String::new()
    };
    if target.is_empty() {
        return Err(SyscallError::InvalidArgument);
    }
    serial_println!("[KnoxOS] mount({}, {}, {})", source, target, fstype);
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    // A target already mounted in this namespace is EBUSY, checked *before* any
    // table is touched so a rejected mount leaves no trace.
    if crate::namespaces::has_mount(pid, &target) {
        return Err(SyscallError::DeviceBusy);
    }
    // Real mount table: creates the VFS mount point, drives the fs driver, and
    // records the entry that `umount2` and `/proc/mounts` read.
    crate::mount::mount(&source, &target, &fstype, flags, &options)
        .map_err(crate::syscall::system::mount_errno)?;
    // …and the caller's mount namespace, so the mount is isolated and unmountable.
    if let Err(e) = crate::namespaces::add_mount(pid, &source, &target, &fstype, flags as u32) {
        // Roll the global entry back rather than leaving a mount with no owner.
        let _ = crate::mount::umount2(&target, 0);
        return Err(crate::syscall::system::mount_errno(e));
    }
    Ok(0)
}

pub fn sys_umount2(target_ptr: u64, flags: i32) -> SyscallResult {
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    let target = unsafe { read_user_string(target_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    serial_println!("[KnoxOS] umount2({}, flags={})", target, flags);
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let force = flags >= 0 && (flags as u32) & UmountFlags::MNT_FORCE.bits() != 0;
    // Both tables reject a busy mount before mutating, so this ordering cannot
    // strand a half-removed mount.
    let in_ns = crate::namespaces::has_mount(pid, &target);
    if in_ns {
        crate::namespaces::remove_mount(pid, &target, force)
            .map_err(crate::syscall::system::mount_errno)?;
    }
    match crate::mount::umount2(&target, flags.max(0) as u32) {
        Ok(()) => Ok(0),
        // Bind / overlay mounts only ever lived in the namespace table.
        Err(-22) if in_ns => Ok(0),
        Err(e) => Err(crate::syscall::system::mount_errno(e)),
    }
}

// ── swapon / swapoff ────────────────────────────────────────────────

pub fn sys_swapon(path_ptr: u64, flags: i32) -> SyscallResult {
    let _ = (path_ptr, flags);
    crate::serial_println!("[KnoxOS] swapon denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_swapoff(path_ptr: u64) -> SyscallResult {
    let _ = path_ptr;
    crate::serial_println!("[KnoxOS] swapoff denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}
