/// pread/pwrite, fallocate, sync*, *at path ops, statx, file handles
use crate::syscall::{SyscallError, SyscallResult, read_user_string};

// ── pread64 / pwrite64 ─────────────────────────────────────────────

pub fn sys_pread64(fd: u64, buf_ptr: u64, count: u64, offset: i64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;

    // Save current position, seek to offset, read, restore position
    let saved = fd_table
        .lseek(fd as i32, 0, crate::fd::SeekFrom::Current)
        .unwrap_or(0);
    let _ = fd_table.lseek(fd as i32, offset, crate::fd::SeekFrom::Start);
    let ncap = count as usize;
    let mut tmp = alloc::vec![0u8; ncap];
    let result = fd_table
        .read(fd as i32, &mut tmp)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::IoError);
    let _ = fd_table.lseek(fd as i32, saved as i64, crate::fd::SeekFrom::Start);
    if let Ok(n) = result {
        let n = n as usize;
        if buf_ptr != 0 && n > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(tmp.as_ptr(), buf_ptr as *mut u8, n);
            }
            crate::vmm::write_user_memory(pid, buf_ptr, &tmp[..n]);
        }
        Ok(n as u64)
    } else {
        result
    }
}

pub fn sys_pwrite64(fd: u64, buf_ptr: u64, count: u64, offset: i64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let ncap = count as usize;
    let mut tmp = alloc::vec![0u8; ncap];
    if buf_ptr != 0 && ncap > 0 {
        unsafe {
            core::ptr::copy_nonoverlapping(buf_ptr as *const u8, tmp.as_mut_ptr(), ncap);
        }
        crate::vmm::read_user_memory(pid, buf_ptr, &mut tmp);
    }
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;

    let saved = fd_table
        .lseek(fd as i32, 0, crate::fd::SeekFrom::Current)
        .unwrap_or(0);
    let _ = fd_table.lseek(fd as i32, offset, crate::fd::SeekFrom::Start);
    let result = fd_table
        .write(fd as i32, &tmp)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::IoError);
    let _ = fd_table.lseek(fd as i32, saved as i64, crate::fd::SeekFrom::Start);
    result
}

// ── fallocate ───────────────────────────────────────────────────────

pub fn sys_fallocate(fd: i32, mode: i32, offset: i64, len: i64) -> SyscallResult {
    if len <= 0 || offset < 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
    let file = fd_table.get(fd).ok_or(SyscallError::BadFileDescriptor)?;
    let path = file.path.clone();
    drop(tables);

    let target_size = (offset + len) as usize;
    // FALLOC_FL_KEEP_SIZE (0x01) means don't change file size
    let mut vfs = crate::vfs::VFS.lock();
    let ino = vfs.resolve_path(&path).ok_or(SyscallError::FileNotFound)?;
    let inode = vfs.get_inode_mut(ino).ok_or(SyscallError::FileNotFound)?;
    if mode & 0x01 == 0 && inode.data.len() < target_size {
        inode.data.resize(target_size, 0);
        inode.size = target_size as u64;
        let ts = crate::vfs::now_timestamp();
        inode.mtime = ts;
        inode.ctime = ts;
    }
    Ok(0)
}

// ── sync / fdatasync / syncfs / sync_file_range ─────────────────────

pub fn sys_sync() -> SyscallResult {
    // Persist already writes on VFS mutate; flush dirty pages first so
    // `sync(2)` is a real barrier when VirtIO-blk is present.
    let _ = crate::page_cache::sync_all();
    let _ = crate::virtio_blk::flush();
    Ok(0)
}

pub fn sys_fsync(fd: i32) -> SyscallResult {
    fsync_fd(fd)
}

pub fn sys_fdatasync(fd: i32) -> SyscallResult {
    fsync_fd(fd)
}

fn fsync_fd(fd: i32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let (path, file_type) = {
        let tables = crate::fd::PROCESS_FD_TABLES.lock();
        let table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
        let file = table.get(fd).ok_or(SyscallError::BadFileDescriptor)?;
        (file.path.clone(), file.file_type)
    };
    if file_type == crate::fd::FileType::Regular {
        let _ = crate::page_cache::flush_path(&path);
        if let Some(data) = crate::vfs::read_file_dispatch(&path) {
            let perms = {
                let vfs = crate::vfs::VFS.lock();
                vfs.resolve_path(&path)
                    .and_then(|ino| vfs.get_inode(ino))
                    .map(|i| i.permissions)
                    .unwrap_or(0o644)
            };
            crate::persist::persist_file(&path, &data, perms);
        }
    }
    let _ = crate::virtio_blk::flush();
    Ok(0)
}

pub fn sys_syncfs(fd: i32) -> SyscallResult {
    let _ = fd;
    Ok(0)
}

pub fn sys_sync_file_range(fd: i32, offset: i64, nbytes: i64, flags: u32) -> SyscallResult {
    let _ = (fd, offset, nbytes, flags);
    Ok(0)
}

pub const GATE_AG1_MARKER: &str = "GATE_AG1 fsync";
const GATE_AG1_PATH: &str = "/tmp/gate_ag1";

/// `fsync` on a written VFS fd succeeds; a bad fd returns EBADF.
pub fn fsync_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AG1_PATH);
        if !vfs.write_file(GATE_AG1_PATH, b"x") {
            crate::serial_println!("[file] Gate AG1 FAILED: write {}", GATE_AG1_PATH);
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
                crate::serial_println!("[file] Gate AG1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AG1_PATH,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR),
            crate::fd::FileType::Regular,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[file] Gate AG1 FAILED: open {}", e);
                return false;
            }
        }
    };
    match sys_fsync(fd) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[file] Gate AG1 FAILED: fsync {:?}", other);
            return false;
        }
    }
    match sys_fsync(-1) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[file] Gate AG1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(fd);
        }
    }
    crate::serial_println!("[file] {}", GATE_AG1_MARKER);
    true
}

// ── getdents (old, non-64 version) ──────────────────────────────────

pub fn sys_getdents(fd: i32, dirp: u64, count: u32) -> SyscallResult {
    // Redirect to getdents64 implementation
    crate::syscall::fs::sys_getdents64(fd, dirp, count)
}

// ── newfstatat ──────────────────────────────────────────────────────

pub fn sys_newfstatat(dirfd: i32, path_ptr: u64, stat_buf: u64, flags: i32) -> SyscallResult {
    let _ = (dirfd, flags);
    crate::syscall::fs::sys_stat(path_ptr, stat_buf)
}

// ── unlinkat / renameat / renameat2 / linkat / fchmodat / fchownat / futimesat / utimensat

pub fn sys_unlinkat(dirfd: i32, path_ptr: u64, flags: i32) -> SyscallResult {
    let _ = dirfd;
    if flags & 0x200 != 0 {
        // AT_REMOVEDIR
        crate::syscall::fs::sys_rmdir(path_ptr)
    } else {
        crate::syscall::fs::sys_unlink(path_ptr)
    }
}

pub fn sys_renameat(
    olddirfd: i32,
    oldpath_ptr: u64,
    newdirfd: i32,
    newpath_ptr: u64,
) -> SyscallResult {
    let _ = (olddirfd, newdirfd);
    crate::syscall::fs::sys_rename(oldpath_ptr, newpath_ptr)
}

pub fn sys_renameat2(
    olddirfd: i32,
    oldpath_ptr: u64,
    newdirfd: i32,
    newpath_ptr: u64,
    flags: u32,
) -> SyscallResult {
    let _ = (olddirfd, newdirfd, flags);
    crate::syscall::fs::sys_rename(oldpath_ptr, newpath_ptr)
}

pub fn sys_linkat(
    olddirfd: i32,
    oldpath_ptr: u64,
    newdirfd: i32,
    newpath_ptr: u64,
    flags: i32,
) -> SyscallResult {
    let _ = (olddirfd, newdirfd, flags);
    crate::syscall::fs::sys_link(oldpath_ptr, newpath_ptr)
}

pub fn sys_fchmodat(dirfd: i32, path_ptr: u64, mode: u32, flags: i32) -> SyscallResult {
    let _ = (dirfd, flags);
    crate::syscall::fs::sys_chmod(path_ptr, mode as u16)
}

pub fn sys_fchownat(dirfd: i32, path_ptr: u64, uid: u32, gid: u32, flags: i32) -> SyscallResult {
    let _ = (dirfd, flags);
    crate::syscall::fs::sys_chown(path_ptr, uid, gid)
}

pub fn sys_utimensat(dirfd: i32, path_ptr: u64, times_ptr: u64, flags: i32) -> SyscallResult {
    let _ = (dirfd, flags);
    let path = if path_ptr != 0 {
        unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?
    } else {
        return Err(SyscallError::InvalidArgument);
    };
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);

    const UTIME_NOW: i64 = 0x3fff_ffff;
    const UTIME_OMIT: i64 = 0x3fff_fffe;

    let (atime, mtime) = if times_ptr == 0 {
        let now = crate::vfs::now_timestamp();
        (Some(now), Some(now))
    } else {
        let mut buf = [0u8; 32];
        unsafe {
            core::ptr::copy_nonoverlapping(times_ptr as *const u8, buf.as_mut_ptr(), 32);
        }
        crate::vmm::read_user_memory(pid, times_ptr, &mut buf);
        let i64_at = |off: usize| {
            let mut b = [0u8; 8];
            b.copy_from_slice(&buf[off..off + 8]);
            i64::from_ne_bytes(b)
        };
        let a_sec = i64_at(0);
        let a_nsec = i64_at(8);
        let m_sec = i64_at(16);
        let m_nsec = i64_at(24);
        let now = crate::vfs::now_timestamp();
        let atime = if a_nsec == UTIME_OMIT {
            None
        } else if a_nsec == UTIME_NOW {
            Some(now)
        } else {
            Some(a_sec)
        };
        let mtime = if m_nsec == UTIME_OMIT {
            None
        } else if m_nsec == UTIME_NOW {
            Some(now)
        } else {
            Some(m_sec)
        };
        (atime, mtime)
    };

    crate::vfs::VFS
        .lock()
        .set_times(&path, atime, mtime)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::FileNotFound)
}

// ── faccessat / faccessat2 ──────────────────────────────────────────

pub fn sys_faccessat(dirfd: i32, path_ptr: u64, mode: u32, flags: i32) -> SyscallResult {
    let _ = (dirfd, flags);
    crate::syscall::fs::sys_access(path_ptr, mode)
}

// ── copy_file_range already handled, but add preadv2/pwritev2 ──────

pub fn sys_preadv2(fd: i32, iov: u64, iovcnt: i32, offset: i64, flags: i32) -> SyscallResult {
    let _ = (offset, flags);
    crate::syscall::io::sys_readv(fd, iov, iovcnt as usize)
}

pub fn sys_pwritev2(fd: i32, iov: u64, iovcnt: i32, offset: i64, flags: i32) -> SyscallResult {
    let _ = (offset, flags);
    crate::syscall::io::sys_writev(fd, iov, iovcnt as usize)
}

// ── statx ───────────────────────────────────────────────────────────

pub fn sys_statx(dirfd: i32, path_ptr: u64, flags: i32, mask: u32, statxbuf: u64) -> SyscallResult {
    let _ = (dirfd, flags, mask);
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let vfs = crate::vfs::VFS.lock();
    let s = vfs.stat(&path).map_err(|_| SyscallError::FileNotFound)?;
    drop(vfs);
    let size = crate::page_cache::register_path(&path)
        .map(crate::page_cache::logical_size)
        .unwrap_or(s.size)
        .max(s.size);
    if statxbuf != 0 {
        let mut buf = [0u8; 256];
        buf[0..4].copy_from_slice(&0x7FFu32.to_ne_bytes());
        buf[4..8].copy_from_slice(&4096u32.to_ne_bytes());
        buf[16..20].copy_from_slice(&(s.nlink as u32).to_ne_bytes());
        buf[20..24].copy_from_slice(&s.uid.to_ne_bytes());
        buf[24..28].copy_from_slice(&s.gid.to_ne_bytes());
        let mode: u16 = match s.file_type {
            crate::vfs::FileType::Regular => 0o100000,
            crate::vfs::FileType::Directory => 0o040000,
            crate::vfs::FileType::CharDevice => 0o020000,
            crate::vfs::FileType::BlockDevice => 0o060000,
            crate::vfs::FileType::Pipe => 0o010000,
            crate::vfs::FileType::Socket => 0o140000,
            crate::vfs::FileType::SymLink => 0o120000,
        } as u16
            | s.permissions;
        buf[28..30].copy_from_slice(&mode.to_ne_bytes());
        buf[32..40].copy_from_slice(&s.ino.to_ne_bytes());
        buf[40..48].copy_from_slice(&size.to_ne_bytes());
        buf[48..56].copy_from_slice(&size.div_ceil(512).to_ne_bytes());
        buf[64..72].copy_from_slice(&s.atime.to_ne_bytes());
        buf[96..104].copy_from_slice(&s.ctime.to_ne_bytes());
        buf[112..120].copy_from_slice(&s.mtime.to_ne_bytes());
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), statxbuf as *mut u8, buf.len());
        }
        crate::vmm::write_user_memory(pid, statxbuf, &buf);
    }
    Ok(0)
}

// ── name_to_handle_at / open_by_handle_at ───────────────────────────

pub fn sys_name_to_handle_at(
    dirfd: i32,
    path_ptr: u64,
    handle: u64,
    mount_id: u64,
    flags: i32,
) -> SyscallResult {
    let _ = (dirfd, flags);
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let vfs = crate::vfs::VFS.lock();
    let ino = vfs.resolve_path(&path).ok_or(SyscallError::FileNotFound)?;
    if mount_id != 0 {
        unsafe {
            *(mount_id as *mut i32) = 0;
        }
    }
    if handle != 0 {
        // file_handle: u32 handle_bytes, i32 handle_type, then bytes
        unsafe {
            *(handle as *mut u32) = 8;
            *((handle as usize + 4) as *mut i32) = 1;
            *((handle as usize + 8) as *mut u64) = ino;
        }
    }
    Ok(0)
}

pub fn sys_open_by_handle_at(mount_fd: i32, handle: u64, flags: i32) -> SyscallResult {
    let _ = (mount_fd, handle, flags);
    Err(SyscallError::NotImplemented)
}
