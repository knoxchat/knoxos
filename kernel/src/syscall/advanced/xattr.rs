/// extended attributes (path and fd variants)
use crate::syscall::{SyscallError, SyscallResult, read_user_string};

// ── xattr operations ────────────────────────────────────────────────

pub fn sys_setxattr(
    path_ptr: u64,
    name_ptr: u64,
    value_ptr: u64,
    size: usize,
    flags: i32,
) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let value = if size > 0 && value_ptr != 0 {
        unsafe { core::slice::from_raw_parts(value_ptr as *const u8, size) }
    } else {
        &[]
    };
    // Resolve inode from path
    let vfs = crate::vfs::VFS.lock();
    let ino = vfs.resolve_path(&path).ok_or(SyscallError::FileNotFound)?;
    drop(vfs);
    crate::xattr::setxattr(ino, &name, value, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_getxattr(path_ptr: u64, name_ptr: u64, value_ptr: u64, size: usize) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let vfs = crate::vfs::VFS.lock();
    let ino = vfs.resolve_path(&path).ok_or(SyscallError::FileNotFound)?;
    drop(vfs);
    if size == 0 {
        // Query size
        let mut tmp = [0u8; 4096];
        crate::xattr::getxattr(ino, &name, &mut tmp)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    } else {
        let mut tmp = alloc::vec![0u8; size.min(4096)];
        match crate::xattr::getxattr(ino, &name, &mut tmp) {
            Ok(n) => {
                let n = n.min(tmp.len());
                unsafe {
                    core::ptr::copy_nonoverlapping(tmp.as_ptr(), value_ptr as *mut u8, n);
                }
                crate::vmm::write_user_memory(pid, value_ptr, &tmp[..n]);
                Ok(n as u64)
            }
            Err(_) => Err(SyscallError::InvalidArgument),
        }
    }
}

pub fn sys_listxattr(path_ptr: u64, list_ptr: u64, size: usize) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let vfs = crate::vfs::VFS.lock();
    let ino = vfs.resolve_path(&path).ok_or(SyscallError::FileNotFound)?;
    drop(vfs);
    if size == 0 {
        let mut tmp = [0u8; 4096];
        crate::xattr::listxattr(ino, &mut tmp)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    } else {
        let buf = unsafe { core::slice::from_raw_parts_mut(list_ptr as *mut u8, size) };
        crate::xattr::listxattr(ino, buf)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    }
}

pub fn sys_removexattr(path_ptr: u64, name_ptr: u64) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let vfs = crate::vfs::VFS.lock();
    let ino = vfs.resolve_path(&path).ok_or(SyscallError::FileNotFound)?;
    drop(vfs);
    crate::xattr::removexattr(ino, &name)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_fsetxattr(
    fd: i32,
    name_ptr: u64,
    value_ptr: u64,
    size: usize,
    flags: i32,
) -> SyscallResult {
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let value = if size > 0 && value_ptr != 0 {
        unsafe { core::slice::from_raw_parts(value_ptr as *const u8, size) }
    } else {
        &[]
    };
    let _ = fd; // Would resolve fd to inode
    crate::xattr::setxattr(fd as u64, &name, value, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_fgetxattr(fd: i32, name_ptr: u64, value_ptr: u64, size: usize) -> SyscallResult {
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    if size == 0 {
        let mut tmp = [0u8; 4096];
        crate::xattr::getxattr(fd as u64, &name, &mut tmp)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    } else {
        let buf = unsafe { core::slice::from_raw_parts_mut(value_ptr as *mut u8, size) };
        crate::xattr::getxattr(fd as u64, &name, buf)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    }
}

pub fn sys_flistxattr(fd: i32, list_ptr: u64, size: usize) -> SyscallResult {
    if size == 0 {
        let mut tmp = [0u8; 4096];
        crate::xattr::listxattr(fd as u64, &mut tmp)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    } else {
        let buf = unsafe { core::slice::from_raw_parts_mut(list_ptr as *mut u8, size) };
        crate::xattr::listxattr(fd as u64, buf)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    }
}

pub fn sys_fremovexattr(fd: i32, name_ptr: u64) -> SyscallResult {
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::xattr::removexattr(fd as u64, &name)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}
