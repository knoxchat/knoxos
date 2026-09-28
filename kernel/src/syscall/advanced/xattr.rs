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
        let mut tmp = alloc::vec![0u8; size.min(4096)];
        match crate::xattr::listxattr(ino, &mut tmp) {
            Ok(n) => {
                let n = n.min(tmp.len());
                if list_ptr != 0 && n > 0 {
                    unsafe {
                        core::ptr::copy_nonoverlapping(tmp.as_ptr(), list_ptr as *mut u8, n);
                    }
                    crate::vmm::write_user_memory(pid, list_ptr, &tmp[..n]);
                }
                Ok(n as u64)
            }
            Err(_) => Err(SyscallError::InvalidArgument),
        }
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

fn inode_for_fd(fd: i32) -> Result<u64, SyscallError> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let (inode, path) = {
        let tables = crate::fd::PROCESS_FD_TABLES.lock();
        let table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
        let file = table.get(fd).ok_or(SyscallError::BadFileDescriptor)?;
        (file.inode, file.path.clone())
    };
    if inode != 0 {
        return Ok(inode);
    }
    crate::vfs::VFS
        .lock()
        .resolve_path(&path)
        .ok_or(SyscallError::FileNotFound)
}

pub fn sys_fsetxattr(
    fd: i32,
    name_ptr: u64,
    value_ptr: u64,
    size: usize,
    flags: i32,
) -> SyscallResult {
    let ino = inode_for_fd(fd)?;
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let value = if size > 0 && value_ptr != 0 {
        unsafe { core::slice::from_raw_parts(value_ptr as *const u8, size) }
    } else {
        &[]
    };
    crate::xattr::setxattr(ino, &name, value, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_fgetxattr(fd: i32, name_ptr: u64, value_ptr: u64, size: usize) -> SyscallResult {
    let ino = inode_for_fd(fd)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    if size == 0 {
        let mut tmp = [0u8; 4096];
        crate::xattr::getxattr(ino, &name, &mut tmp)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    } else {
        let mut tmp = alloc::vec![0u8; size.min(4096)];
        match crate::xattr::getxattr(ino, &name, &mut tmp) {
            Ok(n) => {
                let n = n.min(tmp.len());
                if value_ptr != 0 && n > 0 {
                    unsafe {
                        core::ptr::copy_nonoverlapping(tmp.as_ptr(), value_ptr as *mut u8, n);
                    }
                    crate::vmm::write_user_memory(pid, value_ptr, &tmp[..n]);
                }
                Ok(n as u64)
            }
            Err(_) => Err(SyscallError::InvalidArgument),
        }
    }
}

pub fn sys_flistxattr(fd: i32, list_ptr: u64, size: usize) -> SyscallResult {
    let ino = inode_for_fd(fd)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    if size == 0 {
        let mut tmp = [0u8; 4096];
        crate::xattr::listxattr(ino, &mut tmp)
            .map(|n| n as u64)
            .map_err(|_| SyscallError::InvalidArgument)
    } else {
        let mut tmp = alloc::vec![0u8; size.min(4096)];
        match crate::xattr::listxattr(ino, &mut tmp) {
            Ok(n) => {
                let n = n.min(tmp.len());
                if list_ptr != 0 && n > 0 {
                    unsafe {
                        core::ptr::copy_nonoverlapping(tmp.as_ptr(), list_ptr as *mut u8, n);
                    }
                    crate::vmm::write_user_memory(pid, list_ptr, &tmp[..n]);
                }
                Ok(n as u64)
            }
            Err(_) => Err(SyscallError::InvalidArgument),
        }
    }
}

pub fn sys_fremovexattr(fd: i32, name_ptr: u64) -> SyscallResult {
    let ino = inode_for_fd(fd)?;
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::xattr::removexattr(ino, &name)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub const GATE_AK1_MARKER: &str = "GATE_AK1 fsetxattr";
const GATE_AK1_PATH: &str = "/tmp/gate_ak1";

/// `fsetxattr`/`fgetxattr` on a written VFS fd round-trips `user.knox`; a bad fd is EBADF.
pub fn fsetxattr_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AK1_PATH);
        if !vfs.write_file(GATE_AK1_PATH, b"x") {
            crate::serial_println!("[xattr] Gate AK1 FAILED: write {}", GATE_AK1_PATH);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let fd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[xattr] Gate AK1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AK1_PATH,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR),
            crate::fd::FileType::Regular,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[xattr] Gate AK1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let name = b"user.knox\0";
    let value = b"x";
    match sys_fsetxattr(
        fd,
        name.as_ptr() as u64,
        value.as_ptr() as u64,
        value.len(),
        0,
    ) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[xattr] Gate AK1 FAILED: fsetxattr {:?}", other);
            return false;
        }
    }
    let mut buf = [0u8; 16];
    match sys_fgetxattr(fd, name.as_ptr() as u64, buf.as_mut_ptr() as u64, buf.len()) {
        Ok(1) if buf[0] == b'x' => {}
        other => {
            crate::serial_println!(
                "[xattr] Gate AK1 FAILED: fgetxattr {:?} byte={}",
                other,
                buf[0]
            );
            return false;
        }
    }
    match sys_fsetxattr(
        -1,
        name.as_ptr() as u64,
        value.as_ptr() as u64,
        value.len(),
        0,
    ) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[xattr] Gate AK1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(fd);
        }
    }
    crate::serial_println!("[xattr] {}", GATE_AK1_MARKER);
    true
}

pub const GATE_AL1_MARKER: &str = "GATE_AL1 flistxattr";
const GATE_AL1_PATH: &str = "/tmp/gate_al1";

/// `flistxattr` lists `user.knox` after `fsetxattr`; `fremovexattr` clears it; a bad fd is EBADF.
pub fn flistxattr_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AL1_PATH);
        if !vfs.write_file(GATE_AL1_PATH, b"x") {
            crate::serial_println!("[xattr] Gate AL1 FAILED: write {}", GATE_AL1_PATH);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let fd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[xattr] Gate AL1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AL1_PATH,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR),
            crate::fd::FileType::Regular,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[xattr] Gate AL1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let name = b"user.knox\0";
    let value = b"x";
    match sys_fsetxattr(
        fd,
        name.as_ptr() as u64,
        value.as_ptr() as u64,
        value.len(),
        0,
    ) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[xattr] Gate AL1 FAILED: fsetxattr {:?}", other);
            return false;
        }
    }
    let mut list = [0u8; 64];
    match sys_flistxattr(fd, list.as_mut_ptr() as u64, list.len()) {
        Ok(n) if n >= 10 && list[..10] == *b"user.knox\0" => {}
        other => {
            crate::serial_println!(
                "[xattr] Gate AL1 FAILED: flistxattr {:?} list={:?}",
                other,
                &list[..10]
            );
            return false;
        }
    }
    match sys_fremovexattr(fd, name.as_ptr() as u64) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[xattr] Gate AL1 FAILED: fremovexattr {:?}", other);
            return false;
        }
    }
    match sys_flistxattr(fd, 0, 0) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[xattr] Gate AL1 FAILED: empty list {:?}", other);
            return false;
        }
    }
    match sys_flistxattr(-1, list.as_mut_ptr() as u64, list.len()) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[xattr] Gate AL1 FAILED: bad fd list {:?}", other);
            return false;
        }
    }
    match sys_fremovexattr(-1, name.as_ptr() as u64) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[xattr] Gate AL1 FAILED: bad fd remove {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(fd);
        }
    }
    crate::serial_println!("[xattr] {}", GATE_AL1_MARKER);
    true
}

pub const GATE_AM1_MARKER: &str = "GATE_AM1 listxattr";
const GATE_AM1_PATH: &str = "/tmp/gate_am1";

/// Path `listxattr` lists `user.knox` after `setxattr`; `removexattr` clears it; a missing path is ENOENT.
pub fn listxattr_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AM1_PATH);
        if !vfs.write_file(GATE_AM1_PATH, b"x") {
            crate::serial_println!("[xattr] Gate AM1 FAILED: write {}", GATE_AM1_PATH);
            return false;
        }
    }
    let path = b"/tmp/gate_am1\0";
    let name = b"user.knox\0";
    let value = b"x";
    match sys_setxattr(
        path.as_ptr() as u64,
        name.as_ptr() as u64,
        value.as_ptr() as u64,
        value.len(),
        0,
    ) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[xattr] Gate AM1 FAILED: setxattr {:?}", other);
            return false;
        }
    }
    let mut list = [0u8; 64];
    match sys_listxattr(path.as_ptr() as u64, list.as_mut_ptr() as u64, list.len()) {
        Ok(n) if n >= 10 && list[..10] == *b"user.knox\0" => {}
        other => {
            crate::serial_println!(
                "[xattr] Gate AM1 FAILED: listxattr {:?} list={:?}",
                other,
                &list[..10]
            );
            return false;
        }
    }
    match sys_removexattr(path.as_ptr() as u64, name.as_ptr() as u64) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[xattr] Gate AM1 FAILED: removexattr {:?}", other);
            return false;
        }
    }
    match sys_listxattr(path.as_ptr() as u64, 0, 0) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[xattr] Gate AM1 FAILED: empty list {:?}", other);
            return false;
        }
    }
    let missing = b"/tmp/gate_am1_missing\0";
    match sys_listxattr(
        missing.as_ptr() as u64,
        list.as_mut_ptr() as u64,
        list.len(),
    ) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[xattr] Gate AM1 FAILED: missing {:?}", other);
            return false;
        }
    }
    crate::serial_println!("[xattr] {}", GATE_AM1_MARKER);
    true
}
