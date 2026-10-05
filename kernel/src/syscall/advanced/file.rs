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
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    {
        let tables = crate::fd::PROCESS_FD_TABLES.lock();
        let table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
        let _ = table.get(fd).ok_or(SyscallError::BadFileDescriptor)?;
    }
    sys_sync()
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

pub const GATE_AH1_MARKER: &str = "GATE_AH1 syncfs";
const GATE_AH1_PATH: &str = "/tmp/gate_ah1";

/// `syncfs` on a written VFS fd succeeds; a bad fd returns EBADF.
pub fn syncfs_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AH1_PATH);
        if !vfs.write_file(GATE_AH1_PATH, b"x") {
            crate::serial_println!("[file] Gate AH1 FAILED: write {}", GATE_AH1_PATH);
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
                crate::serial_println!("[file] Gate AH1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AH1_PATH,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR),
            crate::fd::FileType::Regular,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[file] Gate AH1 FAILED: open {}", e);
                return false;
            }
        }
    };
    match sys_syncfs(fd) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[file] Gate AH1 FAILED: syncfs {:?}", other);
            return false;
        }
    }
    match sys_syncfs(-1) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[file] Gate AH1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(fd);
        }
    }
    crate::serial_println!("[file] {}", GATE_AH1_MARKER);
    true
}

pub const GATE_AI1_MARKER: &str = "GATE_AI1 fchmod";
const GATE_AI1_PATH: &str = "/tmp/gate_ai1";

/// `fchmod` on a written VFS fd sets mode 0400; a bad fd returns EBADF.
pub fn fchmod_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AI1_PATH);
        if !vfs.write_file(GATE_AI1_PATH, b"x") {
            crate::serial_println!("[file] Gate AI1 FAILED: write {}", GATE_AI1_PATH);
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
                crate::serial_println!("[file] Gate AI1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AI1_PATH,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR),
            crate::fd::FileType::Regular,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[file] Gate AI1 FAILED: open {}", e);
                return false;
            }
        }
    };
    match crate::syscall::fs::sys_fchmod(fd, 0o400) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[file] Gate AI1 FAILED: fchmod {:?}", other);
            return false;
        }
    }
    match crate::syscall::fs::sys_fchmod(-1, 0o400) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[file] Gate AI1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let vfs = crate::vfs::VFS.lock();
        let mode = vfs.stat(GATE_AI1_PATH).map(|s| s.permissions).unwrap_or(0);
        if mode & 0o777 != 0o400 {
            crate::serial_println!("[file] Gate AI1 FAILED: mode {:o}", mode);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(fd);
        }
    }
    crate::serial_println!("[file] {}", GATE_AI1_MARKER);
    true
}

pub const GATE_AJ1_MARKER: &str = "GATE_AJ1 fstatfs";
const GATE_AJ1_PATH: &str = "/tmp/gate_aj1";

/// `fstatfs` on a written VFS fd reports `f_bsize == 4096`; a bad fd returns EBADF.
pub fn fstatfs_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AJ1_PATH);
        if !vfs.write_file(GATE_AJ1_PATH, b"x") {
            crate::serial_println!("[file] Gate AJ1 FAILED: write {}", GATE_AJ1_PATH);
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
                crate::serial_println!("[file] Gate AJ1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AJ1_PATH,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR),
            crate::fd::FileType::Regular,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[file] Gate AJ1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let mut buf = [0u64; 12];
    match crate::syscall::fs::sys_fstatfs(fd, buf.as_mut_ptr() as u64) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[file] Gate AJ1 FAILED: fstatfs {:?}", other);
            return false;
        }
    }
    // f_bsize is the second u64 in the kernel `StatFs` layout.
    if buf[1] != 4096 {
        crate::serial_println!("[file] Gate AJ1 FAILED: f_bsize {}", buf[1]);
        return false;
    }
    match crate::syscall::fs::sys_fstatfs(-1, buf.as_mut_ptr() as u64) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[file] Gate AJ1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(fd);
        }
    }
    crate::serial_println!("[file] {}", GATE_AJ1_MARKER);
    true
}

// ── getdents (old, non-64 version) ──────────────────────────────────

pub fn sys_getdents(fd: i32, dirp: u64, count: u32) -> SyscallResult {
    // Redirect to getdents64 implementation
    crate::syscall::fs::sys_getdents64(fd, dirp, count)
}

// ── newfstatat ──────────────────────────────────────────────────────

pub fn sys_newfstatat(dirfd: i32, path_ptr: u64, stat_buf: u64, flags: i32) -> SyscallResult {
    let _ = flags;
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let path = path_at(dirfd, &path)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let path = crate::overlayfs::apply_overlay(pid, &path, false);
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
    if stat_buf != 0 {
        unsafe {
            core::ptr::write(stat_buf as *mut crate::fd::FileStat, stat);
        }
    }
    Ok(0)
}

pub const GATE_AX1_MARKER: &str = "GATE_AX1 newfstatat";
const GATE_AX1_DIR: &str = "/tmp/gate_ax1";
const GATE_AX1_FILE: &str = "/tmp/gate_ax1/x";

/// `newfstatat` on a relative name via dirfd reports `st_size == 1`; a missing
/// child is ENOENT; a bad dirfd is EBADF.
pub fn newfstatat_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AX1_FILE);
        if vfs.mkdir(GATE_AX1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_AX1_DIR).is_none() {
            crate::serial_println!("[newfstatat] Gate AX1 FAILED: mkdir {}", GATE_AX1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_AX1_FILE, b"x") {
            crate::serial_println!("[newfstatat] Gate AX1 FAILED: write {}", GATE_AX1_FILE);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[newfstatat] Gate AX1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AX1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[newfstatat] Gate AX1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let child = b"x\0";
    let mut stat = core::mem::MaybeUninit::<crate::fd::FileStat>::uninit();
    match sys_newfstatat(dirfd, child.as_ptr() as u64, stat.as_mut_ptr() as u64, 0) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[newfstatat] Gate AX1 FAILED: newfstatat {:?}", other);
            return false;
        }
    }
    let stat = unsafe { stat.assume_init() };
    if stat.st_size != 1 {
        crate::serial_println!("[newfstatat] Gate AX1 FAILED: st_size {}", stat.st_size);
        return false;
    }
    let missing = b"missing\0";
    match sys_newfstatat(dirfd, missing.as_ptr() as u64, 0, 0) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[newfstatat] Gate AX1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_newfstatat(-1, child.as_ptr() as u64, 0, 0) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[newfstatat] Gate AX1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[newfstatat] {}", GATE_AX1_MARKER);
    true
}

// ── unlinkat / renameat / renameat2 / linkat / fchmodat / fchownat / futimesat / utimensat

pub fn sys_unlinkat(dirfd: i32, path_ptr: u64, flags: i32) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let path = path_at(dirfd, &path)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let path = crate::overlayfs::apply_overlay(pid, &path, true);
    const AT_REMOVEDIR: i32 = 0x200;
    if flags & AT_REMOVEDIR != 0 {
        crate::vfs::VFS.lock().rmdir(&path).map_err(|e| match e {
            -2 => SyscallError::FileNotFound,
            -39 => SyscallError::NotEmpty,
            _ => SyscallError::IoError,
        })?;
    } else {
        crate::vfs::VFS.lock().unlink(&path).map_err(|e| match e {
            -2 => SyscallError::FileNotFound,
            -21 => SyscallError::IsDirectory,
            _ => SyscallError::IoError,
        })?;
    }
    Ok(0)
}

pub const GATE_AP1_MARKER: &str = "GATE_AP1 unlinkat";
const GATE_AP1_DIR: &str = "/tmp/gate_ap1";
const GATE_AP1_FILE: &str = "/tmp/gate_ap1/x";

/// `unlinkat` on a relative name via dirfd removes the file; a missing
/// child is ENOENT; a bad dirfd is EBADF.
pub fn unlinkat_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AP1_FILE);
        if vfs.mkdir(GATE_AP1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_AP1_DIR).is_none() {
            crate::serial_println!("[unlinkat] Gate AP1 FAILED: mkdir {}", GATE_AP1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_AP1_FILE, b"x") {
            crate::serial_println!("[unlinkat] Gate AP1 FAILED: write {}", GATE_AP1_FILE);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[unlinkat] Gate AP1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AP1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[unlinkat] Gate AP1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let child = b"x\0";
    match sys_unlinkat(dirfd, child.as_ptr() as u64, 0) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[unlinkat] Gate AP1 FAILED: unlinkat {:?}", other);
            return false;
        }
    }
    {
        let vfs = crate::vfs::VFS.lock();
        if vfs.resolve_path(GATE_AP1_FILE).is_some() {
            crate::serial_println!("[unlinkat] Gate AP1 FAILED: file still exists");
            return false;
        }
    }
    let missing = b"missing\0";
    match sys_unlinkat(dirfd, missing.as_ptr() as u64, 0) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[unlinkat] Gate AP1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_unlinkat(-1, child.as_ptr() as u64, 0) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[unlinkat] Gate AP1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[unlinkat] {}", GATE_AP1_MARKER);
    true
}

fn rename_at(olddirfd: i32, oldpath_ptr: u64, newdirfd: i32, newpath_ptr: u64) -> SyscallResult {
    let oldpath = unsafe { read_user_string(oldpath_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let newpath = unsafe { read_user_string(newpath_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let oldpath = path_at(olddirfd, &oldpath)?;
    let newpath = path_at(newdirfd, &newpath)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let oldpath = crate::process::translate_path(pid, &oldpath);
    let oldpath = crate::overlayfs::apply_overlay(pid, &oldpath, false);
    let newpath = crate::process::translate_path(pid, &newpath);
    let newpath = crate::overlayfs::apply_overlay(pid, &newpath, true);
    crate::vfs::VFS
        .lock()
        .rename(&oldpath, &newpath)
        .map_err(|e| match e {
            -2 => SyscallError::FileNotFound,
            -17 => SyscallError::FileExists,
            _ => SyscallError::IoError,
        })?;
    Ok(0)
}

pub fn sys_renameat(
    olddirfd: i32,
    oldpath_ptr: u64,
    newdirfd: i32,
    newpath_ptr: u64,
) -> SyscallResult {
    rename_at(olddirfd, oldpath_ptr, newdirfd, newpath_ptr)
}

pub fn sys_renameat2(
    olddirfd: i32,
    oldpath_ptr: u64,
    newdirfd: i32,
    newpath_ptr: u64,
    flags: u32,
) -> SyscallResult {
    let _ = flags;
    rename_at(olddirfd, oldpath_ptr, newdirfd, newpath_ptr)
}

pub const GATE_AQ1_MARKER: &str = "GATE_AQ1 renameat";
const GATE_AQ1_DIR: &str = "/tmp/gate_aq1";
const GATE_AQ1_OLD: &str = "/tmp/gate_aq1/x";
const GATE_AQ1_NEW: &str = "/tmp/gate_aq1/y";

/// `renameat` on relative names via dirfd moves the file; a missing
/// source is ENOENT; a bad dirfd is EBADF.
pub fn renameat_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AQ1_OLD);
        let _ = vfs.unlink(GATE_AQ1_NEW);
        if vfs.mkdir(GATE_AQ1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_AQ1_DIR).is_none() {
            crate::serial_println!("[renameat] Gate AQ1 FAILED: mkdir {}", GATE_AQ1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_AQ1_OLD, b"x") {
            crate::serial_println!("[renameat] Gate AQ1 FAILED: write {}", GATE_AQ1_OLD);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[renameat] Gate AQ1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AQ1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[renameat] Gate AQ1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let old_child = b"x\0";
    let new_child = b"y\0";
    match sys_renameat(
        dirfd,
        old_child.as_ptr() as u64,
        dirfd,
        new_child.as_ptr() as u64,
    ) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[renameat] Gate AQ1 FAILED: renameat {:?}", other);
            return false;
        }
    }
    {
        let vfs = crate::vfs::VFS.lock();
        if vfs.resolve_path(GATE_AQ1_OLD).is_some() {
            crate::serial_println!("[renameat] Gate AQ1 FAILED: old path still exists");
            return false;
        }
        if vfs.resolve_path(GATE_AQ1_NEW).is_none() {
            crate::serial_println!("[renameat] Gate AQ1 FAILED: new path missing");
            return false;
        }
    }
    let missing = b"missing\0";
    match sys_renameat(
        dirfd,
        missing.as_ptr() as u64,
        dirfd,
        new_child.as_ptr() as u64,
    ) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[renameat] Gate AQ1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_renameat(
        -1,
        old_child.as_ptr() as u64,
        dirfd,
        new_child.as_ptr() as u64,
    ) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[renameat] Gate AQ1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[renameat] {}", GATE_AQ1_MARKER);
    true
}

pub const GATE_BC1_MARKER: &str = "GATE_BC1 renameat2";
const GATE_BC1_DIR: &str = "/tmp/gate_bc1";
const GATE_BC1_OLD: &str = "/tmp/gate_bc1/x";
const GATE_BC1_NEW: &str = "/tmp/gate_bc1/y";

/// `renameat2` on relative names via dirfd moves the file; a missing
/// source is ENOENT; a bad dirfd is EBADF.
pub fn renameat2_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_BC1_OLD);
        let _ = vfs.unlink(GATE_BC1_NEW);
        if vfs.mkdir(GATE_BC1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_BC1_DIR).is_none() {
            crate::serial_println!("[renameat2] Gate BC1 FAILED: mkdir {}", GATE_BC1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_BC1_OLD, b"x") {
            crate::serial_println!("[renameat2] Gate BC1 FAILED: write {}", GATE_BC1_OLD);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[renameat2] Gate BC1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_BC1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[renameat2] Gate BC1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let old_child = b"x\0";
    let new_child = b"y\0";
    match sys_renameat2(
        dirfd,
        old_child.as_ptr() as u64,
        dirfd,
        new_child.as_ptr() as u64,
        0,
    ) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[renameat2] Gate BC1 FAILED: renameat2 {:?}", other);
            return false;
        }
    }
    {
        let vfs = crate::vfs::VFS.lock();
        if vfs.resolve_path(GATE_BC1_OLD).is_some() {
            crate::serial_println!("[renameat2] Gate BC1 FAILED: old path still exists");
            return false;
        }
        if vfs.resolve_path(GATE_BC1_NEW).is_none() {
            crate::serial_println!("[renameat2] Gate BC1 FAILED: new path missing");
            return false;
        }
    }
    let missing = b"missing\0";
    match sys_renameat2(
        dirfd,
        missing.as_ptr() as u64,
        dirfd,
        new_child.as_ptr() as u64,
        0,
    ) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[renameat2] Gate BC1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_renameat2(
        -1,
        old_child.as_ptr() as u64,
        dirfd,
        new_child.as_ptr() as u64,
        0,
    ) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[renameat2] Gate BC1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[renameat2] {}", GATE_BC1_MARKER);
    true
}

pub fn sys_linkat(
    olddirfd: i32,
    oldpath_ptr: u64,
    newdirfd: i32,
    newpath_ptr: u64,
    flags: i32,
) -> SyscallResult {
    let _ = flags;
    let oldpath = unsafe { read_user_string(oldpath_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let newpath = unsafe { read_user_string(newpath_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let oldpath = path_at(olddirfd, &oldpath)?;
    let newpath = path_at(newdirfd, &newpath)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let oldpath = crate::process::translate_path(pid, &oldpath);
    let oldpath = crate::overlayfs::apply_overlay(pid, &oldpath, false);
    let newpath = crate::process::translate_path(pid, &newpath);
    let newpath = crate::overlayfs::apply_overlay(pid, &newpath, true);
    crate::vfs::VFS
        .lock()
        .link(&oldpath, &newpath)
        .map_err(|e| match e {
            -2 => SyscallError::FileNotFound,
            -17 => SyscallError::FileExists,
            -1 => SyscallError::PermissionDenied,
            -20 => SyscallError::NotDirectory,
            _ => SyscallError::IoError,
        })?;
    Ok(0)
}

pub const GATE_AR1_MARKER: &str = "GATE_AR1 linkat";
const GATE_AR1_DIR: &str = "/tmp/gate_ar1";
const GATE_AR1_OLD: &str = "/tmp/gate_ar1/x";
const GATE_AR1_NEW: &str = "/tmp/gate_ar1/y";

/// `linkat` on relative names via dirfd creates a hard link; a missing
/// source is ENOENT; a bad dirfd is EBADF.
pub fn linkat_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AR1_OLD);
        let _ = vfs.unlink(GATE_AR1_NEW);
        if vfs.mkdir(GATE_AR1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_AR1_DIR).is_none() {
            crate::serial_println!("[linkat] Gate AR1 FAILED: mkdir {}", GATE_AR1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_AR1_OLD, b"x") {
            crate::serial_println!("[linkat] Gate AR1 FAILED: write {}", GATE_AR1_OLD);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[linkat] Gate AR1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AR1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[linkat] Gate AR1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let old_child = b"x\0";
    let new_child = b"y\0";
    match sys_linkat(
        dirfd,
        old_child.as_ptr() as u64,
        dirfd,
        new_child.as_ptr() as u64,
        0,
    ) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[linkat] Gate AR1 FAILED: linkat {:?}", other);
            return false;
        }
    }
    {
        let vfs = crate::vfs::VFS.lock();
        if vfs.resolve_path(GATE_AR1_OLD).is_none() || vfs.resolve_path(GATE_AR1_NEW).is_none() {
            crate::serial_println!("[linkat] Gate AR1 FAILED: link missing");
            return false;
        }
    }
    let missing = b"missing\0";
    match sys_linkat(
        dirfd,
        missing.as_ptr() as u64,
        dirfd,
        new_child.as_ptr() as u64,
        0,
    ) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[linkat] Gate AR1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_linkat(
        -1,
        old_child.as_ptr() as u64,
        dirfd,
        new_child.as_ptr() as u64,
        0,
    ) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[linkat] Gate AR1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[linkat] {}", GATE_AR1_MARKER);
    true
}

pub fn sys_symlinkat(target_ptr: u64, newdirfd: i32, linkpath_ptr: u64) -> SyscallResult {
    let target = unsafe { read_user_string(target_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let linkpath =
        unsafe { read_user_string(linkpath_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let linkpath = path_at(newdirfd, &linkpath)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let linkpath = crate::process::translate_path(pid, &linkpath);
    let linkpath = crate::overlayfs::apply_overlay(pid, &linkpath, true);
    let mut vfs = crate::vfs::VFS.lock();
    if vfs.resolve_path(&linkpath).is_some() {
        return Err(SyscallError::FileExists);
    }
    let parent = match linkpath.rsplit_once('/') {
        Some(("", _)) => alloc::string::String::from("/"),
        Some((p, _)) => {
            if p.is_empty() {
                alloc::string::String::from("/")
            } else {
                alloc::string::String::from(p)
            }
        }
        None => alloc::string::String::from("/"),
    };
    if vfs.resolve_path(&parent).is_none() {
        return Err(SyscallError::FileNotFound);
    }
    vfs.write_file(&linkpath, target.as_bytes());
    if let Some(ino) = vfs.resolve_path(&linkpath) {
        if let Some(inode) = vfs.get_inode_mut(ino) {
            inode.file_type = crate::vfs::FileType::SymLink;
        }
    }
    Ok(0)
}

pub const GATE_AS1_MARKER: &str = "GATE_AS1 symlinkat";
const GATE_AS1_DIR: &str = "/tmp/gate_as1";
const GATE_AS1_LINK: &str = "/tmp/gate_as1/y";

/// `symlinkat` on a relative name via dirfd creates a symlink; a missing
/// parent is ENOENT; a bad dirfd is EBADF.
pub fn symlinkat_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AS1_LINK);
        if vfs.mkdir(GATE_AS1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_AS1_DIR).is_none() {
            crate::serial_println!("[symlinkat] Gate AS1 FAILED: mkdir {}", GATE_AS1_DIR);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[symlinkat] Gate AS1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AS1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[symlinkat] Gate AS1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let target = b"x\0";
    let link_child = b"y\0";
    match sys_symlinkat(target.as_ptr() as u64, dirfd, link_child.as_ptr() as u64) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[symlinkat] Gate AS1 FAILED: symlinkat {:?}", other);
            return false;
        }
    }
    {
        let vfs = crate::vfs::VFS.lock();
        match vfs.stat(GATE_AS1_LINK) {
            Ok(s) if s.file_type == crate::vfs::FileType::SymLink => {}
            other => {
                crate::serial_println!("[symlinkat] Gate AS1 FAILED: stat {:?}", other.err());
                return false;
            }
        }
    }
    let nested = b"missing/z\0";
    match sys_symlinkat(target.as_ptr() as u64, dirfd, nested.as_ptr() as u64) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[symlinkat] Gate AS1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_symlinkat(target.as_ptr() as u64, -1, link_child.as_ptr() as u64) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[symlinkat] Gate AS1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[symlinkat] {}", GATE_AS1_MARKER);
    true
}

pub fn sys_readlinkat(dirfd: i32, path_ptr: u64, buf_ptr: u64, bufsiz: u64) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let path = path_at(dirfd, &path)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let path = crate::overlayfs::apply_overlay(pid, &path, false);
    let target = {
        let vfs = crate::vfs::VFS.lock();
        let ino = vfs.resolve_path(&path).ok_or(SyscallError::FileNotFound)?;
        let inode = vfs.get_inode(ino).ok_or(SyscallError::FileNotFound)?;
        if inode.file_type != crate::vfs::FileType::SymLink {
            return Err(SyscallError::InvalidArgument);
        }
        inode.data.clone()
    };
    let len = target.len().min(bufsiz as usize);
    if buf_ptr != 0 && len > 0 {
        unsafe {
            core::ptr::copy_nonoverlapping(target.as_ptr(), buf_ptr as *mut u8, len);
        }
        crate::vmm::write_user_memory(pid, buf_ptr, &target[..len]);
    }
    Ok(len as u64)
}

pub const GATE_AT1_MARKER: &str = "GATE_AT1 readlinkat";
const GATE_AT1_DIR: &str = "/tmp/gate_at1";
const GATE_AT1_LINK: &str = "/tmp/gate_at1/y";

/// `readlinkat` on a relative name via dirfd returns the target; a missing
/// child is ENOENT; a bad dirfd is EBADF.
pub fn readlinkat_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AT1_LINK);
        if vfs.mkdir(GATE_AT1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_AT1_DIR).is_none() {
            crate::serial_println!("[readlinkat] Gate AT1 FAILED: mkdir {}", GATE_AT1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_AT1_LINK, b"x") {
            crate::serial_println!("[readlinkat] Gate AT1 FAILED: write {}", GATE_AT1_LINK);
            return false;
        }
        if let Some(ino) = vfs.resolve_path(GATE_AT1_LINK) {
            if let Some(inode) = vfs.get_inode_mut(ino) {
                inode.file_type = crate::vfs::FileType::SymLink;
            }
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[readlinkat] Gate AT1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AT1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[readlinkat] Gate AT1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let child = b"y\0";
    let mut buf = [0u8; 8];
    match sys_readlinkat(dirfd, child.as_ptr() as u64, buf.as_mut_ptr() as u64, 8) {
        Ok(1) if buf[0] == b'x' => {}
        other => {
            crate::serial_println!(
                "[readlinkat] Gate AT1 FAILED: readlinkat {:?} buf={:?}",
                other,
                buf
            );
            return false;
        }
    }
    let missing = b"missing\0";
    match sys_readlinkat(dirfd, missing.as_ptr() as u64, buf.as_mut_ptr() as u64, 8) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[readlinkat] Gate AT1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_readlinkat(-1, child.as_ptr() as u64, buf.as_mut_ptr() as u64, 8) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[readlinkat] Gate AT1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[readlinkat] {}", GATE_AT1_MARKER);
    true
}

fn parent_path(path: &str) -> alloc::string::String {
    match path.rsplit_once('/') {
        Some(("", _)) => alloc::string::String::from("/"),
        Some((p, _)) => {
            if p.is_empty() {
                alloc::string::String::from("/")
            } else {
                alloc::string::String::from(p)
            }
        }
        None => alloc::string::String::from("/"),
    }
}

fn mknod_path(path: &str, mode: u32) -> SyscallResult {
    let mut vfs = crate::vfs::VFS.lock();
    if vfs.resolve_path(path).is_some() {
        return Err(SyscallError::FileExists);
    }
    if vfs.resolve_path(&parent_path(path)).is_none() {
        return Err(SyscallError::FileNotFound);
    }
    let file_type = mode & 0o170000;
    let perms = (mode as u16) & 0o7777 & !crate::syscall::fs::current_umask();
    match file_type {
        0o010000 => {
            drop(vfs);
            crate::fifo::mkfifo(path, perms)
                .map(|_| 0u64)
                .map_err(|e| match e {
                    -17 => SyscallError::FileExists,
                    -2 => SyscallError::FileNotFound,
                    _ => SyscallError::IoError,
                })
        }
        0o100000 | 0 => {
            if !vfs.write_file(path, &[]) {
                return Err(SyscallError::IoError);
            }
            if let Some(ino) = vfs.resolve_path(path) {
                if let Some(inode) = vfs.get_inode_mut(ino) {
                    inode.permissions = perms;
                }
            }
            Ok(0)
        }
        0o020000 | 0o060000 => {
            if !vfs.write_file(path, &[]) {
                return Err(SyscallError::IoError);
            }
            if let Some(ino) = vfs.resolve_path(path) {
                if let Some(inode) = vfs.get_inode_mut(ino) {
                    inode.file_type = if file_type == 0o020000 {
                        crate::vfs::FileType::CharDevice
                    } else {
                        crate::vfs::FileType::BlockDevice
                    };
                    inode.permissions = perms;
                }
            }
            Ok(0)
        }
        0o140000 => {
            if !vfs.write_file(path, &[]) {
                return Err(SyscallError::IoError);
            }
            if let Some(ino) = vfs.resolve_path(path) {
                if let Some(inode) = vfs.get_inode_mut(ino) {
                    inode.file_type = crate::vfs::FileType::Socket;
                    inode.permissions = perms;
                }
            }
            Ok(0)
        }
        _ => Err(SyscallError::InvalidArgument),
    }
}

pub fn sys_mknodat(dirfd: i32, path_ptr: u64, mode: u32, _dev: u64) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let path = path_at(dirfd, &path)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let path = crate::overlayfs::apply_overlay(pid, &path, true);
    mknod_path(&path, mode)
}

pub const GATE_AU1_MARKER: &str = "GATE_AU1 mknodat";
const GATE_AU1_DIR: &str = "/tmp/gate_au1";
const GATE_AU1_FILE: &str = "/tmp/gate_au1/x";

/// `mknodat` on a relative name via dirfd creates a regular file; a missing
/// parent is ENOENT; a bad dirfd is EBADF.
pub fn mknodat_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AU1_FILE);
        if vfs.mkdir(GATE_AU1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_AU1_DIR).is_none() {
            crate::serial_println!("[mknodat] Gate AU1 FAILED: mkdir {}", GATE_AU1_DIR);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[mknodat] Gate AU1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AU1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[mknodat] Gate AU1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let child = b"x\0";
    const S_IFREG: u32 = 0o100000;
    match sys_mknodat(dirfd, child.as_ptr() as u64, S_IFREG | 0o644, 0) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[mknodat] Gate AU1 FAILED: mknodat {:?}", other);
            return false;
        }
    }
    {
        let vfs = crate::vfs::VFS.lock();
        match vfs.stat(GATE_AU1_FILE) {
            Ok(s) if s.file_type == crate::vfs::FileType::Regular => {}
            other => {
                crate::serial_println!("[mknodat] Gate AU1 FAILED: stat {:?}", other.err());
                return false;
            }
        }
    }
    let nested = b"missing/z\0";
    match sys_mknodat(dirfd, nested.as_ptr() as u64, S_IFREG | 0o644, 0) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[mknodat] Gate AU1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_mknodat(-1, child.as_ptr() as u64, S_IFREG | 0o644, 0) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[mknodat] Gate AU1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[mknodat] {}", GATE_AU1_MARKER);
    true
}

pub fn sys_fchmodat(dirfd: i32, path_ptr: u64, mode: u32, flags: i32) -> SyscallResult {
    let _ = flags;
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let path = path_at(dirfd, &path)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let path = crate::overlayfs::apply_overlay(pid, &path, true);
    let mut vfs = crate::vfs::VFS.lock();
    if let Some(ino) = vfs.resolve_path(&path) {
        if let Some(inode) = vfs.get_inode_mut(ino) {
            inode.permissions = (mode as u16) & 0o7777;
            return Ok(0);
        }
    }
    Err(SyscallError::FileNotFound)
}

pub const GATE_AV1_MARKER: &str = "GATE_AV1 fchmodat";
const GATE_AV1_DIR: &str = "/tmp/gate_av1";
const GATE_AV1_FILE: &str = "/tmp/gate_av1/x";

/// `fchmodat` on a relative name via dirfd sets mode 0400; a missing
/// child is ENOENT; a bad dirfd is EBADF.
pub fn fchmodat_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AV1_FILE);
        if vfs.mkdir(GATE_AV1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_AV1_DIR).is_none() {
            crate::serial_println!("[fchmodat] Gate AV1 FAILED: mkdir {}", GATE_AV1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_AV1_FILE, b"x") {
            crate::serial_println!("[fchmodat] Gate AV1 FAILED: write {}", GATE_AV1_FILE);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[fchmodat] Gate AV1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AV1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[fchmodat] Gate AV1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let child = b"x\0";
    match sys_fchmodat(dirfd, child.as_ptr() as u64, 0o400, 0) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[fchmodat] Gate AV1 FAILED: fchmodat {:?}", other);
            return false;
        }
    }
    {
        let vfs = crate::vfs::VFS.lock();
        let mode = vfs.stat(GATE_AV1_FILE).map(|s| s.permissions).unwrap_or(0);
        if mode & 0o777 != 0o400 {
            crate::serial_println!("[fchmodat] Gate AV1 FAILED: mode {:o}", mode);
            return false;
        }
    }
    let missing = b"missing\0";
    match sys_fchmodat(dirfd, missing.as_ptr() as u64, 0o400, 0) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[fchmodat] Gate AV1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_fchmodat(-1, child.as_ptr() as u64, 0o400, 0) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[fchmodat] Gate AV1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[fchmodat] {}", GATE_AV1_MARKER);
    true
}

pub fn sys_fchownat(dirfd: i32, path_ptr: u64, uid: u32, gid: u32, flags: i32) -> SyscallResult {
    let _ = flags;
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let path = path_at(dirfd, &path)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let path = crate::overlayfs::apply_overlay(pid, &path, true);
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
    Err(SyscallError::FileNotFound)
}

pub const GATE_AW1_MARKER: &str = "GATE_AW1 fchownat";
const GATE_AW1_DIR: &str = "/tmp/gate_aw1";
const GATE_AW1_FILE: &str = "/tmp/gate_aw1/x";

/// `fchownat` on a relative name via dirfd sets uid 1000; a missing
/// child is ENOENT; a bad dirfd is EBADF.
pub fn fchownat_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AW1_FILE);
        if vfs.mkdir(GATE_AW1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_AW1_DIR).is_none() {
            crate::serial_println!("[fchownat] Gate AW1 FAILED: mkdir {}", GATE_AW1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_AW1_FILE, b"x") {
            crate::serial_println!("[fchownat] Gate AW1 FAILED: write {}", GATE_AW1_FILE);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[fchownat] Gate AW1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AW1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[fchownat] Gate AW1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let child = b"x\0";
    match sys_fchownat(dirfd, child.as_ptr() as u64, 1000, 1000, 0) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[fchownat] Gate AW1 FAILED: fchownat {:?}", other);
            return false;
        }
    }
    {
        let vfs = crate::vfs::VFS.lock();
        match vfs.stat(GATE_AW1_FILE) {
            Ok(s) if s.uid == 1000 => {}
            other => {
                crate::serial_println!(
                    "[fchownat] Gate AW1 FAILED: uid {:?}",
                    other.map(|s| s.uid)
                );
                return false;
            }
        }
    }
    let missing = b"missing\0";
    match sys_fchownat(dirfd, missing.as_ptr() as u64, 1000, 1000, 0) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[fchownat] Gate AW1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_fchownat(-1, child.as_ptr() as u64, 1000, 1000, 0) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[fchownat] Gate AW1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[fchownat] {}", GATE_AW1_MARKER);
    true
}

pub fn sys_openat(dirfd: i32, path_ptr: u64, flags: u32, mode: u16) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let path = path_at(dirfd, &path)?;
    crate::syscall::fs::open_path(&path, flags, mode)
}

pub const GATE_AY1_MARKER: &str = "GATE_AY1 openat";
const GATE_AY1_DIR: &str = "/tmp/gate_ay1";
const GATE_AY1_FILE: &str = "/tmp/gate_ay1/x";

/// `openat` on a relative name via dirfd opens the file; a missing child is
/// ENOENT; a bad dirfd is EBADF.
pub fn openat_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AY1_FILE);
        if vfs.mkdir(GATE_AY1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_AY1_DIR).is_none() {
            crate::serial_println!("[openat] Gate AY1 FAILED: mkdir {}", GATE_AY1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_AY1_FILE, b"x") {
            crate::serial_println!("[openat] Gate AY1 FAILED: write {}", GATE_AY1_FILE);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[openat] Gate AY1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AY1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[openat] Gate AY1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let child = b"x\0";
    let fd = match sys_openat(dirfd, child.as_ptr() as u64, 0, 0) {
        Ok(n) if n < 4096 => n as i32,
        other => {
            crate::serial_println!("[openat] Gate AY1 FAILED: openat {:?}", other);
            return false;
        }
    };
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(fd);
        }
    }
    let missing = b"missing\0";
    match sys_openat(dirfd, missing.as_ptr() as u64, 0, 0) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[openat] Gate AY1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_openat(-1, child.as_ptr() as u64, 0, 0) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[openat] Gate AY1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[openat] {}", GATE_AY1_MARKER);
    true
}

pub fn sys_utimensat(dirfd: i32, path_ptr: u64, times_ptr: u64, flags: i32) -> SyscallResult {
    let _ = flags;
    let path = if path_ptr != 0 {
        unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?
    } else {
        return Err(SyscallError::InvalidArgument);
    };
    let path = path_at(dirfd, &path)?;
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

pub const GATE_AZ1_MARKER: &str = "GATE_AZ1 utimensat";
const GATE_AZ1_DIR: &str = "/tmp/gate_az1";
const GATE_AZ1_FILE: &str = "/tmp/gate_az1/x";

/// `utimensat` on a relative name via dirfd sets mtime 42; a missing child is
/// ENOENT; a bad dirfd is EBADF.
pub fn utimensat_at_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AZ1_FILE);
        if vfs.mkdir(GATE_AZ1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_AZ1_DIR).is_none() {
            crate::serial_println!("[utimensat] Gate AZ1 FAILED: mkdir {}", GATE_AZ1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_AZ1_FILE, b"x") {
            crate::serial_println!("[utimensat] Gate AZ1 FAILED: write {}", GATE_AZ1_FILE);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[utimensat] Gate AZ1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AZ1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[utimensat] Gate AZ1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let child = b"x\0";
    let mut times = [0u8; 32];
    times[8..16].copy_from_slice(&0x3fff_fffei64.to_ne_bytes());
    times[16..24].copy_from_slice(&42i64.to_ne_bytes());
    match sys_utimensat(dirfd, child.as_ptr() as u64, times.as_ptr() as u64, 0) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[utimensat] Gate AZ1 FAILED: utimensat {:?}", other);
            return false;
        }
    }
    {
        let vfs = crate::vfs::VFS.lock();
        match vfs.stat(GATE_AZ1_FILE) {
            Ok(s) if s.mtime == 42 => {}
            other => {
                crate::serial_println!(
                    "[utimensat] Gate AZ1 FAILED: mtime {:?}",
                    other.map(|s| s.mtime)
                );
                return false;
            }
        }
    }
    let missing = b"missing\0";
    match sys_utimensat(dirfd, missing.as_ptr() as u64, times.as_ptr() as u64, 0) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[utimensat] Gate AZ1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_utimensat(-1, child.as_ptr() as u64, times.as_ptr() as u64, 0) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[utimensat] Gate AZ1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[utimensat] {}", GATE_AZ1_MARKER);
    true
}

pub fn sys_futimesat(dirfd: i32, path_ptr: u64, times_ptr: u64) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let path = path_at(dirfd, &path)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);

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
        (Some(i64_at(0)), Some(i64_at(16)))
    };

    crate::vfs::VFS
        .lock()
        .set_times(&path, atime, mtime)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::FileNotFound)
}

pub const GATE_BB1_MARKER: &str = "GATE_BB1 futimesat";
const GATE_BB1_DIR: &str = "/tmp/gate_bb1";
const GATE_BB1_FILE: &str = "/tmp/gate_bb1/x";

/// `futimesat` on a relative name via dirfd sets mtime 42; a missing child is
/// ENOENT; a bad dirfd is EBADF.
pub fn futimesat_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_BB1_FILE);
        if vfs.mkdir(GATE_BB1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_BB1_DIR).is_none() {
            crate::serial_println!("[futimesat] Gate BB1 FAILED: mkdir {}", GATE_BB1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_BB1_FILE, b"x") {
            crate::serial_println!("[futimesat] Gate BB1 FAILED: write {}", GATE_BB1_FILE);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[futimesat] Gate BB1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_BB1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[futimesat] Gate BB1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let child = b"x\0";
    let mut times = [0u8; 32];
    times[16..24].copy_from_slice(&42i64.to_ne_bytes());
    match sys_futimesat(dirfd, child.as_ptr() as u64, times.as_ptr() as u64) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[futimesat] Gate BB1 FAILED: futimesat {:?}", other);
            return false;
        }
    }
    {
        let vfs = crate::vfs::VFS.lock();
        match vfs.stat(GATE_BB1_FILE) {
            Ok(s) if s.mtime == 42 => {}
            other => {
                crate::serial_println!(
                    "[futimesat] Gate BB1 FAILED: mtime {:?}",
                    other.map(|s| s.mtime)
                );
                return false;
            }
        }
    }
    let missing = b"missing\0";
    match sys_futimesat(dirfd, missing.as_ptr() as u64, times.as_ptr() as u64) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[futimesat] Gate BB1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_futimesat(-1, child.as_ptr() as u64, times.as_ptr() as u64) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[futimesat] Gate BB1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[futimesat] {}", GATE_BB1_MARKER);
    true
}

// ── faccessat / faccessat2 ──────────────────────────────────────────

const AT_FDCWD: i32 = -100;

fn path_at(dirfd: i32, path: &str) -> Result<alloc::string::String, SyscallError> {
    if path.starts_with('/') {
        return Ok(alloc::string::String::from(path));
    }
    let dir = if dirfd == AT_FDCWD {
        let pid = crate::scheduler::current_pid().unwrap_or(1);
        crate::process::PROCESS_TABLE
            .lock()
            .get_process(pid)
            .map(|p| p.cwd.clone())
            .unwrap_or_else(|| alloc::string::String::from("/"))
    } else {
        let pid = crate::scheduler::current_pid().unwrap_or(1);
        let tables = crate::fd::PROCESS_FD_TABLES.lock();
        let table = tables.get(&pid).ok_or(SyscallError::BadFileDescriptor)?;
        let file = table.get(dirfd).ok_or(SyscallError::BadFileDescriptor)?;
        file.path.clone()
    };
    let mut joined = dir;
    if !joined.ends_with('/') {
        joined.push('/');
    }
    joined.push_str(path);
    Ok(joined)
}

pub fn sys_faccessat(dirfd: i32, path_ptr: u64, mode: u32, flags: i32) -> SyscallResult {
    let _ = flags;
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let path = path_at(dirfd, &path)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let path = crate::overlayfs::apply_overlay(pid, &path, false);
    crate::vfs::VFS
        .lock()
        .access(&path, mode)
        .map_err(|_| SyscallError::FileNotFound)?;
    Ok(0)
}

pub const GATE_AN1_MARKER: &str = "GATE_AN1 faccessat";
const GATE_AN1_DIR: &str = "/tmp/gate_an1";
const GATE_AN1_FILE: &str = "/tmp/gate_an1/x";

/// `faccessat` on a relative name via dirfd succeeds; a missing child is ENOENT; a bad dirfd is EBADF.
pub fn faccessat_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_AN1_FILE);
        if vfs.mkdir(GATE_AN1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_AN1_DIR).is_none() {
            crate::serial_println!("[faccessat] Gate AN1 FAILED: mkdir {}", GATE_AN1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_AN1_FILE, b"x") {
            crate::serial_println!("[faccessat] Gate AN1 FAILED: write {}", GATE_AN1_FILE);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[faccessat] Gate AN1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AN1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[faccessat] Gate AN1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let child = b"x\0";
    match sys_faccessat(dirfd, child.as_ptr() as u64, 0, 0) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[faccessat] Gate AN1 FAILED: faccessat {:?}", other);
            return false;
        }
    }
    let missing = b"missing\0";
    match sys_faccessat(dirfd, missing.as_ptr() as u64, 0, 0) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[faccessat] Gate AN1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_faccessat(-1, child.as_ptr() as u64, 0, 0) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[faccessat] Gate AN1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[faccessat] {}", GATE_AN1_MARKER);
    true
}

pub fn sys_faccessat2(dirfd: i32, path_ptr: u64, mode: u32, flags: i32) -> SyscallResult {
    sys_faccessat(dirfd, path_ptr, mode, flags)
}

pub const GATE_BD1_MARKER: &str = "GATE_BD1 faccessat2";
const GATE_BD1_DIR: &str = "/tmp/gate_bd1";
const GATE_BD1_FILE: &str = "/tmp/gate_bd1/x";

/// `faccessat2` on a relative name via dirfd succeeds; a missing child is ENOENT; a bad dirfd is EBADF.
pub fn faccessat2_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_BD1_FILE);
        if vfs.mkdir(GATE_BD1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_BD1_DIR).is_none() {
            crate::serial_println!("[faccessat2] Gate BD1 FAILED: mkdir {}", GATE_BD1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_BD1_FILE, b"x") {
            crate::serial_println!("[faccessat2] Gate BD1 FAILED: write {}", GATE_BD1_FILE);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[faccessat2] Gate BD1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_BD1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[faccessat2] Gate BD1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let child = b"x\0";
    match sys_faccessat2(dirfd, child.as_ptr() as u64, 0, 0) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[faccessat2] Gate BD1 FAILED: faccessat2 {:?}", other);
            return false;
        }
    }
    let missing = b"missing\0";
    match sys_faccessat2(dirfd, missing.as_ptr() as u64, 0, 0) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[faccessat2] Gate BD1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_faccessat2(-1, child.as_ptr() as u64, 0, 0) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[faccessat2] Gate BD1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[faccessat2] {}", GATE_BD1_MARKER);
    true
}

// ── mkdirat ─────────────────────────────────────────────────────────

pub fn sys_mkdirat(dirfd: i32, path_ptr: u64, mode: u16) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let path = path_at(dirfd, &path)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let path = crate::overlayfs::apply_overlay(pid, &path, true);
    let mode = mode & !crate::syscall::fs::current_umask();
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

pub const GATE_AO1_MARKER: &str = "GATE_AO1 mkdirat";
const GATE_AO1_DIR: &str = "/tmp/gate_ao1";
const GATE_AO1_CHILD: &str = "/tmp/gate_ao1/x";

/// `mkdirat` on a relative name via dirfd creates a directory; a missing
/// parent is ENOENT; a bad dirfd is EBADF.
pub fn mkdirat_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.rmdir(GATE_AO1_CHILD);
        let _ = vfs.unlink(GATE_AO1_CHILD);
        if vfs.mkdir(GATE_AO1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_AO1_DIR).is_none() {
            crate::serial_println!("[mkdirat] Gate AO1 FAILED: mkdir {}", GATE_AO1_DIR);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[mkdirat] Gate AO1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_AO1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[mkdirat] Gate AO1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let child = b"x\0";
    match sys_mkdirat(dirfd, child.as_ptr() as u64, 0o755) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[mkdirat] Gate AO1 FAILED: mkdirat {:?}", other);
            return false;
        }
    }
    {
        let vfs = crate::vfs::VFS.lock();
        match vfs.stat(GATE_AO1_CHILD) {
            Ok(s) if s.file_type == crate::vfs::FileType::Directory => {}
            other => {
                crate::serial_println!("[mkdirat] Gate AO1 FAILED: stat {:?}", other.err());
                return false;
            }
        }
    }
    let nested = b"missing/y\0";
    match sys_mkdirat(dirfd, nested.as_ptr() as u64, 0o755) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[mkdirat] Gate AO1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_mkdirat(-1, child.as_ptr() as u64, 0o755) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[mkdirat] Gate AO1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[mkdirat] {}", GATE_AO1_MARKER);
    true
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
    let _ = (flags, mask);
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    let path = path_at(dirfd, &path)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::process::translate_path(pid, &path);
    let path = crate::overlayfs::apply_overlay(pid, &path, false);
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

pub const GATE_BA1_MARKER: &str = "GATE_BA1 statx";
const GATE_BA1_DIR: &str = "/tmp/gate_ba1";
const GATE_BA1_FILE: &str = "/tmp/gate_ba1/x";

/// `statx` on a relative name via dirfd reports `stx_size == 1`; a missing
/// child is ENOENT; a bad dirfd is EBADF.
pub fn statx_at_self_test() -> bool {
    {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.unlink(GATE_BA1_FILE);
        if vfs.mkdir(GATE_BA1_DIR, 0o755).is_err() && vfs.resolve_path(GATE_BA1_DIR).is_none() {
            crate::serial_println!("[statx] Gate BA1 FAILED: mkdir {}", GATE_BA1_DIR);
            return false;
        }
        if !vfs.write_file(GATE_BA1_FILE, b"x") {
            crate::serial_println!("[statx] Gate BA1 FAILED: write {}", GATE_BA1_FILE);
            return false;
        }
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let dirfd = {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        tables.entry(pid).or_default();
        let table = match tables.get_mut(&pid) {
            Some(t) => t,
            None => {
                crate::serial_println!("[statx] Gate BA1 FAILED: no fd table");
                return false;
            }
        };
        match table.open(
            GATE_BA1_DIR,
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY),
            crate::fd::FileType::Directory,
        ) {
            Ok(fd) => fd,
            Err(e) => {
                crate::serial_println!("[statx] Gate BA1 FAILED: open {}", e);
                return false;
            }
        }
    };
    let child = b"x\0";
    let mut buf = [0u8; 256];
    match sys_statx(
        dirfd,
        child.as_ptr() as u64,
        0,
        0x7FF,
        buf.as_mut_ptr() as u64,
    ) {
        Ok(0) => {}
        other => {
            crate::serial_println!("[statx] Gate BA1 FAILED: statx {:?}", other);
            return false;
        }
    }
    let mut size_bytes = [0u8; 8];
    size_bytes.copy_from_slice(&buf[40..48]);
    let size = u64::from_ne_bytes(size_bytes);
    if size != 1 {
        crate::serial_println!("[statx] Gate BA1 FAILED: stx_size {}", size);
        return false;
    }
    let missing = b"missing\0";
    match sys_statx(dirfd, missing.as_ptr() as u64, 0, 0, 0) {
        Err(SyscallError::FileNotFound) => {}
        other => {
            crate::serial_println!("[statx] Gate BA1 FAILED: missing {:?}", other);
            return false;
        }
    }
    match sys_statx(-1, child.as_ptr() as u64, 0, 0, 0) {
        Err(SyscallError::BadFileDescriptor) => {}
        other => {
            crate::serial_println!("[statx] Gate BA1 FAILED: bad fd {:?}", other);
            return false;
        }
    }
    {
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        if let Some(table) = tables.get_mut(&pid) {
            let _ = table.close(dirfd);
        }
    }
    crate::serial_println!("[statx] {}", GATE_BA1_MARKER);
    true
}

// ── name_to_handle_at / open_by_handle_at ───────────────────────────

pub fn sys_name_to_handle_at(
    dirfd: i32,
    path_ptr: u64,
    handle: u64,
    mount_id: u64,
    flags: i32,
) -> SyscallResult {
    let _ = (dirfd, path_ptr, handle, mount_id, flags);
    crate::serial_println!("[KnoxOS] name_to_handle_at denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_open_by_handle_at(mount_fd: i32, handle: u64, flags: i32) -> SyscallResult {
    let _ = (mount_fd, handle, flags);
    crate::serial_println!("[KnoxOS] open_by_handle_at denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}
