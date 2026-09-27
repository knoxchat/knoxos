/// pidfd, memfd, userfaultfd, close_range, signalfd
use crate::syscall::{SyscallError, SyscallResult, read_user_string};

// ── pidfd operations ────────────────────────────────────────────────

pub fn sys_pidfd_open(pid: u64, flags: u32) -> SyscallResult {
    crate::pidfd::sys_pidfd_open(pid, flags).map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_pidfd_send_signal(pidfd: u64, sig: i32, info: u64, flags: u32) -> SyscallResult {
    crate::pidfd::sys_pidfd_send_signal(pidfd, sig, info, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_pidfd_getfd(pidfd: u64, target_fd: i32, flags: u32) -> SyscallResult {
    crate::pidfd::sys_pidfd_getfd(pidfd, target_fd, flags)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::BadFileDescriptor)
}

// ── memfd operations ────────────────────────────────────────────────

pub fn sys_memfd_create(name_ptr: u64, flags: u32) -> SyscallResult {
    let name = unsafe { read_user_string(name_ptr) }.unwrap_or_default();
    let id =
        crate::memfd::sys_memfd_create(&name, flags).map_err(|_| SyscallError::TooManyFiles)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get_mut(&pid).ok_or(SyscallError::TooManyFiles)?;
    let fd = fd_table
        .open(
            &alloc::format!("memfd:{}", id),
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR),
            crate::fd::FileType::CharDevice,
        )
        .map_err(|_| SyscallError::TooManyFiles)?;
    if flags & crate::memfd::MFD_CLOEXEC != 0 {
        fd_table.set_cloexec(fd, true);
    }
    Ok(fd as u64)
}

// ── userfaultfd ─────────────────────────────────────────────────────

pub fn sys_userfaultfd(flags: u32) -> SyscallResult {
    crate::userfaultfd::sys_userfaultfd(flags).map_err(|_| SyscallError::TooManyFiles)
}

// ── close_range ─────────────────────────────────────────────────────

pub fn sys_close_range(first: u32, last: u32, flags: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::close_range::close_range(pid, first, last, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── signalfd ────────────────────────────────────────────────────────

pub fn sys_signalfd(fd: i32, mask_ptr: u64, flags: i32) -> SyscallResult {
    let mask = if mask_ptr != 0 {
        unsafe { *(mask_ptr as *const u64) }
    } else {
        0
    };
    if fd == -1 {
        let id = crate::signalfd::signalfd_create(mask, flags)
            .map_err(|_| SyscallError::TooManyFiles)?;
        let pid = crate::scheduler::current_pid().unwrap_or(1);
        let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
        let fd_table = tables.get_mut(&pid).ok_or(SyscallError::TooManyFiles)?;
        let mut o_flags = crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR);
        if flags & crate::signalfd::SFD_NONBLOCK != 0 {
            o_flags.0 |= crate::fd::OpenFlags::O_NONBLOCK;
        }
        let new_fd = fd_table
            .open(
                &alloc::format!("signalfd:{}", id),
                o_flags,
                crate::fd::FileType::CharDevice,
            )
            .map_err(|_| SyscallError::TooManyFiles)?;
        if flags & crate::signalfd::SFD_CLOEXEC != 0 {
            fd_table.set_cloexec(new_fd, true);
        }
        Ok(new_fd as u64)
    } else {
        let pid = crate::scheduler::current_pid().unwrap_or(1);
        let path = crate::fd::path_for_fd(pid, fd).ok_or(SyscallError::BadFileDescriptor)?;
        let id = crate::fd::signalfd_id_from_path(&path).ok_or(SyscallError::BadFileDescriptor)?;
        crate::signalfd::signalfd_update(id, mask)
            .map(|_| fd as u64)
            .map_err(|_| SyscallError::BadFileDescriptor)
    }
}
