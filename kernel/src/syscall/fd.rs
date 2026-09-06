/// Syscall implementations — File descriptor operations
/// pipe, pipe2, dup, dup2, dup3, fcntl, ioctl
use super::{SyscallError, SyscallResult};

pub fn sys_pipe(pipefd_ptr: u64) -> SyscallResult {
    let pipe_id = crate::ipc::create_pipe().map_err(|_| SyscallError::IoError)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;
    let read_fd = fd_table
        .open(
            &alloc::format!("pipe:{}", pipe_id),
            crate::fd::OpenFlags(0),
            crate::fd::FileType::Pipe,
        )
        .map_err(|_| SyscallError::TooManyFiles)?;
    let write_fd = fd_table
        .open(
            &alloc::format!("pipe:{}", pipe_id),
            crate::fd::OpenFlags(1),
            crate::fd::FileType::Pipe,
        )
        .map_err(|_| SyscallError::TooManyFiles)?;
    unsafe {
        let fds = pipefd_ptr as *mut [i32; 2];
        (*fds)[0] = read_fd;
        (*fds)[1] = write_fd;
    }
    Ok(0)
}

/// pipe2(pipefd, flags) — like pipe but with O_CLOEXEC / O_NONBLOCK flags
pub fn sys_pipe2(pipefd_ptr: u64, flags: i32) -> SyscallResult {
    let pipe_id = crate::ipc::create_pipe().map_err(|_| SyscallError::IoError)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;

    let o_flags = crate::fd::OpenFlags(0);
    let read_fd = fd_table
        .open(
            &alloc::format!("pipe:{}", pipe_id),
            o_flags,
            crate::fd::FileType::Pipe,
        )
        .map_err(|_| SyscallError::TooManyFiles)?;
    let write_fd = fd_table
        .open(
            &alloc::format!("pipe:{}", pipe_id),
            crate::fd::OpenFlags(1),
            crate::fd::FileType::Pipe,
        )
        .map_err(|_| SyscallError::TooManyFiles)?;

    const O_CLOEXEC: i32 = 0o2000000;
    const O_NONBLOCK: i32 = 0o4000;

    if flags & O_CLOEXEC != 0 {
        fd_table.set_cloexec(read_fd, true);
        fd_table.set_cloexec(write_fd, true);
    }
    if flags & O_NONBLOCK != 0 {
        fd_table.set_nonblock(read_fd, true);
        fd_table.set_nonblock(write_fd, true);
    }

    unsafe {
        let fds = pipefd_ptr as *mut [i32; 2];
        (*fds)[0] = read_fd;
        (*fds)[1] = write_fd;
    }
    Ok(0)
}

pub fn sys_dup(old_fd: i32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;
    fd_table
        .dup(old_fd)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::BadFileDescriptor)
}

pub fn sys_dup2(old_fd: i32, new_fd: i32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;
    fd_table
        .dup2(old_fd, new_fd)
        .map(|fd| fd as u64)
        .map_err(|_| SyscallError::BadFileDescriptor)
}

/// dup3(oldfd, newfd, flags) — like dup2 but with O_CLOEXEC flag
pub fn sys_dup3(old_fd: i32, new_fd: i32, flags: i32) -> SyscallResult {
    if old_fd == new_fd {
        return Err(SyscallError::InvalidArgument);
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;
    let fd = fd_table
        .dup2(old_fd, new_fd)
        .map_err(|_| SyscallError::BadFileDescriptor)?;

    const O_CLOEXEC: i32 = 0o2000000;
    if flags & O_CLOEXEC != 0 {
        fd_table.set_cloexec(fd, true);
    }
    Ok(fd as u64)
}

/// fcntl(fd, cmd, arg)
pub fn sys_fcntl(fd: i32, cmd: i32, arg: u64) -> SyscallResult {
    const F_DUPFD: i32 = 0;
    const F_GETFD: i32 = 1;
    const F_SETFD: i32 = 2;
    const F_GETFL: i32 = 3;
    const F_SETFL: i32 = 4;
    const F_DUPFD_CLOEXEC: i32 = 1030;
    const FD_CLOEXEC: i32 = 1;
    const O_NONBLOCK: i32 = 0o4000;

    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables
        .get_mut(&pid)
        .ok_or(SyscallError::BadFileDescriptor)?;

    match cmd {
        F_DUPFD => fd_table
            .dup(fd)
            .map(|f| f as u64)
            .map_err(|_| SyscallError::BadFileDescriptor),
        F_DUPFD_CLOEXEC => {
            let new_fd = fd_table
                .dup(fd)
                .map_err(|_| SyscallError::BadFileDescriptor)?;
            fd_table.set_cloexec(new_fd, true);
            Ok(new_fd as u64)
        }
        F_GETFD => {
            let cloexec = fd_table.get_cloexec(fd);
            Ok(if cloexec { FD_CLOEXEC as u64 } else { 0 })
        }
        F_SETFD => {
            fd_table.set_cloexec(fd, (arg as i32 & FD_CLOEXEC) != 0);
            Ok(0)
        }
        F_GETFL => {
            let nonblock = fd_table.get_nonblock(fd);
            let flags = fd_table.get_open_flags(fd);
            let mut fl = flags as u64;
            if nonblock {
                fl |= O_NONBLOCK as u64;
            }
            Ok(fl)
        }
        F_SETFL => {
            fd_table.set_nonblock(fd, (arg as i32 & O_NONBLOCK) != 0);
            Ok(0)
        }
        _ => Ok(0),
    }
}

pub fn sys_ioctl(fd: i32, request: u32, arg: u64) -> SyscallResult {
    let _ = fd;
    match request {
        0x5413 => {
            // TIOCGWINSZ
            unsafe {
                *(arg as *mut crate::tty::WinSize) = crate::tty::WinSize::default();
            }
            Ok(0)
        }
        0x5401 => Ok(0), // TCGETS
        0x540E => {
            // TIOCSCTTY — set controlling terminal
            let pid = crate::scheduler::current_pid().unwrap_or(1);
            let sid = crate::pgrp::getsid(pid).unwrap_or(pid);
            // Use fd as tty number (simplified mapping)
            let tty_num = fd as u32;
            match crate::pgrp::set_ctty(sid, tty_num) {
                Ok(_) => Ok(0),
                Err(_) => Err(SyscallError::NoSuchProcess),
            }
        }
        0x5422 => {
            // TIOCSPGRP — set foreground process group (tcsetpgrp)
            let pgid = arg as u32;
            match crate::pgrp::tcsetpgrp(fd, pgid) {
                Ok(_) => Ok(0),
                Err(_) => Err(SyscallError::InvalidArgument),
            }
        }
        0x540F => {
            // TIOCGPGRP — get foreground process group (tcgetpgrp)
            match crate::pgrp::tcgetpgrp(fd) {
                Ok(pgid) => Ok(pgid as u64),
                Err(_) => Err(SyscallError::InvalidArgument),
            }
        }
        0x5410 => {
            // TIOCNOTTY — detach controlling terminal
            Ok(0)
        }
        _ => Ok(0),
    }
}
