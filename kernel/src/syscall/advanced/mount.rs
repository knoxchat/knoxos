use crate::serial_println;
/// new mount API, mount/umount2, pivot_root, setns, swap
use crate::syscall::{SyscallError, SyscallResult, read_user_string};

// ── New mount API ───────────────────────────────────────────────────

pub fn sys_fsopen(fstype_ptr: u64, flags: u32) -> SyscallResult {
    let fstype = unsafe { read_user_string(fstype_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::mount_api::fsopen(&fstype, flags)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_fsconfig(fd: i32, cmd: u32, key_ptr: u64, value_ptr: u64, aux: i32) -> SyscallResult {
    let key = if key_ptr != 0 {
        unsafe { read_user_string(key_ptr) }
    } else {
        None
    };
    let value = if value_ptr != 0 {
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
    let from = unsafe { read_user_string(from_path_ptr) }.unwrap_or_default();
    let to = unsafe { read_user_string(to_path_ptr) }.unwrap_or_default();
    crate::mount_api::move_mount(from_dfd, &from, to_dfd, &to, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_open_tree(dfd: i32, path_ptr: u64, flags: u32) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.unwrap_or_default();
    crate::mount_api::open_tree(dfd, &path, flags)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_fspick(dfd: i32, path_ptr: u64, flags: u32) -> SyscallResult {
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
        unsafe { read_user_string(new_root_ptr) }.ok_or(SyscallError::InvalidArgument)?;
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
    let source = unsafe { read_user_string(source_ptr) }.unwrap_or_default();
    let target = unsafe { read_user_string(target_ptr) }.unwrap_or_default();
    let fstype = unsafe { read_user_string(fstype_ptr) }.unwrap_or_default();
    serial_println!("[KnoxOS] mount({}, {}, {})", source, target, fstype);
    // Create mount point in VFS
    {
        let mut vfs = crate::vfs::VFS.lock();
        vfs.mkdir(&target, 0o755).ok();
    }
    let _ = data_ptr;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::namespaces::add_mount(pid, &source, &target, &fstype, flags as u32)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::IoError)
}

pub fn sys_umount2(target_ptr: u64, flags: i32) -> SyscallResult {
    let _ = flags;
    let target = unsafe { read_user_string(target_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    serial_println!("[KnoxOS] umount2({})", target);
    Ok(0)
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
