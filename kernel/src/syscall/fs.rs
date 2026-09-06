use super::{SyscallError, SyscallResult, read_user_string};
use crate::serial_println;
/// Syscall implementations — Filesystem operations
/// read, write, open, close, stat, fstat, lseek, access, unlink, rename,
/// readlink, getdents64, getcwd, chdir, mkdir, rmdir, symlink, chmod,
/// chown, umask, statfs, truncate, getrusage
use alloc::collections::BTreeMap;
use alloc::string::String;
use spin::Mutex;

/// Per-process umask tracking
lazy_static::lazy_static! {
    static ref PROCESS_UMASKS: Mutex<BTreeMap<u32, u16>> = Mutex::new(BTreeMap::new());
}

pub fn sys_read(fd: u64, buf_ptr: u64, count: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;
    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, count as usize) };
    fd_table
        .read(fd as i32, buf)
        .map(|n| n as u64)
        .map_err(|e| match e {
            -9 => SyscallError::BadFileDescriptor,
            -2 => SyscallError::FileNotFound,
            _ => SyscallError::IoError,
        })
}

pub fn sys_write(fd: u64, buf_ptr: u64, count: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;
    let buf = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, count as usize) };
    fd_table
        .write(fd as i32, buf)
        .map(|n| n as u64)
        .map_err(|e| match e {
            -9 => SyscallError::BadFileDescriptor,
            -32 => SyscallError::BrokenPipe,
            _ => SyscallError::IoError,
        })
}

pub fn sys_open(path_ptr: u64, flags: u32, mode: u16) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    serial_println!("[KnoxOS] open({}, {:#x})", path, flags);

    // Determine file type
    let file_type = {
        let vfs = crate::vfs::VFS.lock();
        if path.starts_with("/proc") {
            crate::fd::FileType::ProcFile
        } else if let Some(ino) = vfs.resolve_path(&path) {
            let inode = vfs.get_inode(ino).ok_or(SyscallError::FileNotFound)?;
            match inode.file_type {
                crate::vfs::FileType::Directory => crate::fd::FileType::Directory,
                crate::vfs::FileType::CharDevice => crate::fd::FileType::CharDevice,
                crate::vfs::FileType::BlockDevice => crate::fd::FileType::BlockDevice,
                crate::vfs::FileType::Pipe => crate::fd::FileType::Pipe,
                _ => crate::fd::FileType::Regular,
            }
        } else if flags & 0x40 != 0 {
            // O_CREAT
            drop(vfs);
            crate::vfs::VFS.lock().write_file(&path, &[]);
            crate::fd::FileType::Regular
        } else {
            return Err(SyscallError::FileNotFound);
        }
    };

    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;
    fd_table
        .open(&path, crate::fd::OpenFlags(flags), file_type)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::TooManyFiles)
}

pub fn sys_close(fd: i32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;
    fd_table
        .close(fd)
        .map_err(|_| SyscallError::BadFileDescriptor)?;
    Ok(0)
}

pub fn sys_lseek(fd: i32, offset: i64, whence: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let seek = match whence {
        0 => crate::fd::SeekFrom::Start,
        1 => crate::fd::SeekFrom::Current,
        2 => crate::fd::SeekFrom::End,
        _ => return Err(SyscallError::InvalidArgument),
    };
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;
    fd_table
        .lseek(fd, offset, seek)
        .map(|p| p as u64)
        .map_err(|_| SyscallError::BadFileDescriptor)
}

pub fn sys_stat(path_ptr: u64, stat_buf: u64) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let vfs = crate::vfs::VFS.lock();
    let s = vfs.stat(&path).map_err(|_| SyscallError::FileNotFound)?;
    let mode = match s.file_type {
        crate::vfs::FileType::Regular => 0o100000,
        crate::vfs::FileType::Directory => 0o040000,
        crate::vfs::FileType::CharDevice => 0o020000,
        crate::vfs::FileType::BlockDevice => 0o060000,
        crate::vfs::FileType::Pipe => 0o010000,
        crate::vfs::FileType::Socket => 0o140000,
        crate::vfs::FileType::SymLink => 0o120000,
    } | s.permissions as u32;
    let stat = crate::fd::FileStat {
        st_dev: 0,
        st_ino: s.ino,
        st_mode: mode,
        st_nlink: s.nlink,
        st_uid: s.uid,
        st_gid: s.gid,
        st_rdev: 0,
        st_size: s.size,
        st_blksize: 4096,
        st_blocks: s.size.div_ceil(512),
        st_atime: 0,
        st_mtime: 0,
        st_ctime: 0,
    };
    unsafe {
        core::ptr::write(stat_buf as *mut crate::fd::FileStat, stat);
    }
    Ok(0)
}

pub fn sys_fstat(fd: i32, stat_buf: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
    let stat = fd_table
        .fstat(fd)
        .map_err(|_| SyscallError::BadFileDescriptor)?;
    unsafe {
        core::ptr::write(stat_buf as *mut crate::fd::FileStat, stat);
    }
    Ok(0)
}

pub fn sys_access(path_ptr: u64, _mode: u32) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::vfs::VFS
        .lock()
        .access(&path, _mode)
        .map_err(|_| SyscallError::FileNotFound)?;
    Ok(0)
}

pub fn sys_unlink(path_ptr: u64) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::vfs::VFS.lock().unlink(&path).map_err(|e| match e {
        -2 => SyscallError::FileNotFound,
        -21 => SyscallError::IsDirectory,
        _ => SyscallError::IoError,
    })?;
    Ok(0)
}

pub fn sys_rename(old_ptr: u64, new_ptr: u64) -> SyscallResult {
    let old = unsafe { read_user_string(old_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let new = unsafe { read_user_string(new_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::vfs::VFS
        .lock()
        .rename(&old, &new)
        .map_err(|_| SyscallError::FileNotFound)?;
    Ok(0)
}

pub fn sys_readlink(path_ptr: u64, buf_ptr: u64, bufsiz: u64) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);

    // /proc/self/exe → current executable path
    if path == "/proc/self/exe" || path == alloc::format!("/proc/{}/exe", pid) {
        let table = crate::process::PROCESS_TABLE.lock();
        if let Some(proc) = table.get_process(pid) {
            let target = alloc::format!("/bin/{}", proc.name);
            let len = target.len().min(bufsiz as usize);
            unsafe {
                core::ptr::copy_nonoverlapping(target.as_ptr(), buf_ptr as *mut u8, len);
            }
            return Ok(len as u64);
        }
    }

    // /proc/self/cwd → current working directory
    if path == "/proc/self/cwd" || path == alloc::format!("/proc/{}/cwd", pid) {
        let table = crate::process::PROCESS_TABLE.lock();
        if let Some(proc) = table.get_process(pid) {
            let cwd = proc.cwd.clone();
            let len = cwd.len().min(bufsiz as usize);
            unsafe {
                core::ptr::copy_nonoverlapping(cwd.as_ptr(), buf_ptr as *mut u8, len);
            }
            return Ok(len as u64);
        }
    }

    // /proc/self/fd/N → path of file descriptor N
    if path.starts_with("/proc/self/fd/") || path.starts_with(&alloc::format!("/proc/{}/fd/", pid))
    {
        if let Some(fd_str) = path.rsplit('/').next() {
            if let Ok(fd_num) = fd_str.parse::<i32>() {
                let tables = crate::fd::PROCESS_FD_TABLES.lock();
                if let Some(fd_table) = tables.get(&pid) {
                    if let Some(file) = fd_table.get(fd_num) {
                        let target = file.path.clone();
                        let len = target.len().min(bufsiz as usize);
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                target.as_ptr(),
                                buf_ptr as *mut u8,
                                len,
                            );
                        }
                        return Ok(len as u64);
                    }
                }
            }
        }
    }

    // Regular symlink in VFS
    let vfs = crate::vfs::VFS.lock();
    if let Some(ino) = vfs.resolve_path(&path) {
        if let Some(inode) = vfs.get_inode(ino) {
            if inode.file_type == crate::vfs::FileType::SymLink {
                if let Some(data) = vfs.read_file(&path) {
                    let target = core::str::from_utf8(data).unwrap_or("");
                    let len = target.len().min(bufsiz as usize);
                    unsafe {
                        core::ptr::copy_nonoverlapping(target.as_ptr(), buf_ptr as *mut u8, len);
                    }
                    return Ok(len as u64);
                }
            }
        }
    }

    Err(SyscallError::InvalidArgument)
}

pub fn sys_getdents64(fd: i32, dirp: u64, count: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
    let file = fd_table.get(fd).ok_or(SyscallError::BadFileDescriptor)?;
    if file.file_type != crate::fd::FileType::Directory {
        return Err(SyscallError::NotDirectory);
    }
    let vfs = crate::vfs::VFS.lock();
    let entries = vfs.list_dir(&file.path).ok_or(SyscallError::FileNotFound)?;
    let mut offset = 0usize;
    for name in &entries {
        let reclen = ((19 + name.len() + 1) + 7) & !7;
        if offset + reclen > count as usize {
            break;
        }
        unsafe {
            let base = (dirp as usize + offset) as *mut u8;
            *(base as *mut u64) = 1;
            *((base as usize + 8) as *mut u64) = (offset + reclen) as u64;
            *((base as usize + 16) as *mut u16) = reclen as u16;
            *((base as usize + 18) as *mut u8) = 8;
            core::ptr::copy_nonoverlapping(
                name.as_ptr(),
                (base as usize + 19) as *mut u8,
                name.len(),
            );
            *((base as usize + 19 + name.len()) as *mut u8) = 0;
        }
        offset += reclen;
    }
    Ok(offset as u64)
}

pub fn sys_getcwd(buf_ptr: u64, size: usize) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let table = crate::process::PROCESS_TABLE.lock();
    let cwd = table
        .get_process(pid)
        .map(|p| p.cwd.clone())
        .unwrap_or_else(|| String::from("/"));
    if size < cwd.len() + 1 {
        return Err(SyscallError::InvalidArgument);
    }
    unsafe {
        core::ptr::copy_nonoverlapping(cwd.as_ptr(), buf_ptr as *mut u8, cwd.len());
        *((buf_ptr + cwd.len() as u64) as *mut u8) = 0;
    }
    Ok(buf_ptr)
}

pub fn sys_chdir(path_ptr: u64) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let vfs = crate::vfs::VFS.lock();
    if let Some(ino) = vfs.resolve_path(&path) {
        if let Some(inode) = vfs.get_inode(ino) {
            if inode.file_type != crate::vfs::FileType::Directory {
                return Err(SyscallError::NotDirectory);
            }
        }
    } else {
        return Err(SyscallError::FileNotFound);
    }
    drop(vfs);
    crate::process::PROCESS_TABLE.lock().chdir(pid, &path);
    Ok(0)
}

pub fn sys_mkdir(path_ptr: u64, mode: u16) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::vfs::VFS
        .lock()
        .mkdir(&path, mode)
        .map_err(|e| match e {
            -17 => SyscallError::FileExists,
            -2 => SyscallError::FileNotFound,
            _ => SyscallError::IoError,
        })?;
    Ok(0)
}

pub fn sys_rmdir(path_ptr: u64) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::vfs::VFS.lock().rmdir(&path).map_err(|e| match e {
        -2 => SyscallError::FileNotFound,
        -39 => SyscallError::NotEmpty,
        _ => SyscallError::IoError,
    })?;
    Ok(0)
}

pub fn sys_symlink(target_ptr: u64, linkpath_ptr: u64) -> SyscallResult {
    let target = unsafe { read_user_string(target_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let linkpath =
        unsafe { read_user_string(linkpath_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let mut vfs = crate::vfs::VFS.lock();
    // Check if the linkpath already exists
    if vfs.resolve_path(&linkpath).is_some() {
        return Err(SyscallError::FileExists);
    }
    // Create symlink: store target as file contents, mark as symlink type
    vfs.write_file(&linkpath, target.as_bytes());
    // Mark as symlink in the inode if possible
    if let Some(ino) = vfs.resolve_path(&linkpath) {
        if let Some(inode) = vfs.get_inode_mut(ino) {
            inode.file_type = crate::vfs::FileType::SymLink;
        }
    }
    Ok(0)
}

pub fn sys_chmod(path_or_fd: u64, mode: u16) -> SyscallResult {
    // Try to interpret as path first
    if let Some(path) = unsafe { read_user_string(path_or_fd) } {
        let mut vfs = crate::vfs::VFS.lock();
        if let Some(ino) = vfs.resolve_path(&path) {
            if let Some(inode) = vfs.get_inode_mut(ino) {
                inode.permissions = mode & 0o7777;
                return Ok(0);
            }
        }
        return Err(SyscallError::FileNotFound);
    }
    // Treat as fd (fchmod)
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
    let file = fd_table
        .get(path_or_fd as i32)
        .ok_or(SyscallError::BadFileDescriptor)?;
    let file_path = file.path.clone();
    drop(tables);
    let mut vfs = crate::vfs::VFS.lock();
    if let Some(ino) = vfs.resolve_path(&file_path) {
        if let Some(inode) = vfs.get_inode_mut(ino) {
            inode.permissions = mode & 0o7777;
            return Ok(0);
        }
    }
    Err(SyscallError::FileNotFound)
}

pub fn sys_chown(path_or_fd: u64, uid: u32, gid: u32) -> SyscallResult {
    // Try to interpret as path first
    if let Some(path) = unsafe { read_user_string(path_or_fd) } {
        let mut vfs = crate::vfs::VFS.lock();
        if let Some(ino) = vfs.resolve_path(&path) {
            if let Some(inode) = vfs.get_inode_mut(ino) {
                if uid != 0xFFFFFFFF {
                    inode.uid = uid;
                }
                if gid != 0xFFFFFFFF {
                    inode.gid = gid;
                }
                return Ok(0);
            }
        }
        return Err(SyscallError::FileNotFound);
    }
    Ok(0)
}

pub fn sys_umask(mask: u16) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut umasks = PROCESS_UMASKS.lock();
    let old_mask = *umasks.get(&pid).unwrap_or(&0o022);
    umasks.insert(pid, mask & 0o777);
    Ok(old_mask as u64)
}

pub fn sys_statfs(path_or_fd: u64, buf: u64) -> SyscallResult {
    if buf == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let _ = path_or_fd;
    #[repr(C)]
    struct StatFs {
        f_type: u64,
        f_bsize: u64,
        f_blocks: u64,
        f_bfree: u64,
        f_bavail: u64,
        f_files: u64,
        f_ffree: u64,
        f_fsid: [u32; 2],
        f_namelen: u64,
        f_frsize: u64,
    }
    let statfs = StatFs {
        f_type: 0xEF53, // EXT2_SUPER_MAGIC
        f_bsize: 4096,
        f_blocks: 524288, // 2 GiB total (524288 * 4096)
        f_bfree: 393216,  // ~1.5 GiB free
        f_bavail: 393216,
        f_files: 131072,
        f_ffree: 65536,
        f_fsid: [0, 0],
        f_namelen: 255,
        f_frsize: 4096,
    };
    let out = unsafe { &mut *(buf as *mut StatFs) };
    *out = statfs;
    Ok(0)
}

pub fn sys_truncate(path_or_fd: u64, length: usize) -> SyscallResult {
    // Try as path (truncate) first
    if let Some(path) = unsafe { read_user_string(path_or_fd) } {
        let mut vfs = crate::vfs::VFS.lock();
        if let Some(ino) = vfs.resolve_path(&path) {
            if let Some(data) = vfs.read_file(&path).map(|d| d.to_vec()) {
                if length == 0 {
                    vfs.write_file(&path, &[]);
                } else if length < data.len() {
                    vfs.write_file(&path, &data[..length]);
                } else if length > data.len() {
                    let mut extended = data;
                    extended.resize(length, 0);
                    vfs.write_file(&path, &extended);
                }
                return Ok(0);
            }
        }
        return Err(SyscallError::FileNotFound);
    }
    // Treat as fd (ftruncate)
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
    let file = fd_table
        .get(path_or_fd as i32)
        .ok_or(SyscallError::BadFileDescriptor)?;
    let file_path = file.path.clone();
    drop(tables);
    let mut vfs = crate::vfs::VFS.lock();
    if let Some(data) = vfs.read_file(&file_path).map(|d| d.to_vec()) {
        if length == 0 {
            vfs.write_file(&file_path, &[]);
        } else if length < data.len() {
            vfs.write_file(&file_path, &data[..length]);
        } else if length > data.len() {
            let mut extended = data;
            extended.resize(length, 0);
            vfs.write_file(&file_path, &extended);
        }
        Ok(0)
    } else {
        Err(SyscallError::BadFileDescriptor)
    }
}

pub fn sys_getrusage(who: i32, usage_ptr: u64) -> SyscallResult {
    if usage_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let _ = who;
    let buf = unsafe { core::slice::from_raw_parts_mut(usage_ptr as *mut u8, 144) };
    buf.fill(0);
    Ok(0)
}
