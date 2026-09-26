use super::{SyscallError, SyscallResult, read_user_string};
/// Syscall implementations — I/O multiplexing & advanced I/O
/// epoll, poll, select, eventfd, inotify, timerfd, mkfifo, openpty,
/// flock, sendfile, splice, readv/writev, copy_file_range,
/// scheduler, message queues, shared memory
use crate::serial_println;

// ── Epoll ───────────────────────────────────────────────────────────

pub fn sys_epoll_create(flags: i32) -> SyscallResult {
    let _ = flags;
    let id = crate::epoll::epoll_create().map_err(|_| SyscallError::TooManyFiles)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get_mut(&pid).ok_or(SyscallError::TooManyFiles)?;
    let fd = fd_table
        .open(
            &alloc::format!("epoll:{}", id),
            crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR),
            crate::fd::FileType::CharDevice,
        )
        .map_err(|_| SyscallError::TooManyFiles)?;
    Ok(fd as u64)
}

fn epoll_id_from_epfd(epfd: i32) -> Result<i32, SyscallError> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::fd::path_for_fd(pid, epfd).ok_or(SyscallError::BadFileDescriptor)?;
    crate::fd::epoll_id_from_path(&path).ok_or(SyscallError::BadFileDescriptor)
}

pub fn sys_epoll_ctl(epfd: i32, op: i32, fd: i32, event_ptr: u64) -> SyscallResult {
    let id = epoll_id_from_epfd(epfd)?;
    let events = if event_ptr != 0 {
        unsafe { *(event_ptr as *const u32) }
    } else {
        0
    };
    let data = if event_ptr != 0 {
        unsafe { core::ptr::read_unaligned((event_ptr as *const u8).add(4) as *const u64) }
    } else {
        0
    };
    let event = crate::epoll::EpollEvent { events, data };
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::fd::path_for_fd(pid, fd).unwrap_or_default();
    crate::epoll::epoll_ctl_with_path(id, op, fd, &event, &path)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_epoll_wait(epfd: i32, events_ptr: u64, max_events: i32, timeout: i32) -> SyscallResult {
    let id = epoll_id_from_epfd(epfd)?;
    let mut events_buf = [crate::epoll::EpollEvent { events: 0, data: 0 }; 64];
    let count = core::cmp::min(max_events as usize, 64);
    let buf = &mut events_buf[..count];
    let ready =
        crate::epoll::epoll_wait(id, buf, timeout).map_err(|_| SyscallError::BadFileDescriptor)?;
    if events_ptr != 0 && ready > 0 {
        for i in 0..ready {
            let p = events_ptr + (i * 12) as u64;
            unsafe {
                *(p as *mut u32) = buf[i].events;
                core::ptr::write_unaligned((p as *mut u8).add(4) as *mut u64, buf[i].data);
            }
        }
    }
    Ok(ready as u64)
}

// ── Poll / Select ───────────────────────────────────────────────────

pub fn sys_poll(fds_ptr: u64, nfds: u32, timeout: i32) -> SyscallResult {
    if nfds == 0 {
        return Ok(0);
    }
    if fds_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let fds = unsafe {
        core::slice::from_raw_parts_mut(fds_ptr as *mut crate::epoll::PollFd, nfds as usize)
    };
    let n = crate::epoll::poll(fds, timeout);
    Ok(n as u64)
}

pub fn sys_select(
    nfds: i32,
    readfds: u64,
    writefds: u64,
    _exceptfds: u64,
    _timeout: u64,
) -> SyscallResult {
    let read_set = if readfds != 0 {
        Some(unsafe { &mut *(readfds as *mut crate::epoll::FdSet) })
    } else {
        None
    };
    let write_set = if writefds != 0 {
        Some(unsafe { &mut *(writefds as *mut crate::epoll::FdSet) })
    } else {
        None
    };
    let n = crate::epoll::select(nfds, read_set, write_set, None, None);
    Ok(n as u64)
}

// ── Eventfd ─────────────────────────────────────────────────────────

pub fn sys_eventfd(initval: u32, flags: i32) -> SyscallResult {
    let id = crate::eventfd::eventfd_create(initval as u64, flags)
        .map_err(|_| SyscallError::TooManyFiles)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get_mut(&pid).ok_or(SyscallError::TooManyFiles)?;
    let mut o_flags = crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR);
    if flags & crate::eventfd::EFD_NONBLOCK != 0 {
        o_flags.0 |= crate::fd::OpenFlags::O_NONBLOCK;
    }
    let fd = fd_table
        .open(
            &alloc::format!("eventfd:{}", id),
            o_flags,
            crate::fd::FileType::CharDevice,
        )
        .map_err(|_| SyscallError::TooManyFiles)?;
    if flags & crate::eventfd::EFD_CLOEXEC != 0 {
        fd_table.set_cloexec(fd, true);
    }
    Ok(fd as u64)
}

// ── Inotify ─────────────────────────────────────────────────────────

pub fn sys_inotify_init(flags: i32) -> SyscallResult {
    let id = crate::inotify::inotify_init().map_err(|_| SyscallError::TooManyFiles)?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get_mut(&pid).ok_or(SyscallError::TooManyFiles)?;
    let mut o_flags = crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDONLY);
    const IN_NONBLOCK: i32 = 0x800;
    const IN_CLOEXEC: i32 = 0x80000;
    if flags & IN_NONBLOCK != 0 {
        o_flags.0 |= crate::fd::OpenFlags::O_NONBLOCK;
    }
    let fd = fd_table
        .open(
            &alloc::format!("inotify:{}", id),
            o_flags,
            crate::fd::FileType::CharDevice,
        )
        .map_err(|_| SyscallError::TooManyFiles)?;
    if flags & IN_CLOEXEC != 0 {
        fd_table.set_cloexec(fd, true);
    }
    Ok(fd as u64)
}

fn inotify_id_from_fd(fd: i32) -> Result<i32, SyscallError> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::fd::path_for_fd(pid, fd).ok_or(SyscallError::BadFileDescriptor)?;
    crate::fd::inotify_id_from_path(&path).ok_or(SyscallError::BadFileDescriptor)
}

pub fn sys_inotify_add_watch(fd: i32, path_ptr: u64, mask: u32) -> SyscallResult {
    let id = inotify_id_from_fd(fd)?;
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::inotify::inotify_add_watch(id, &path, mask)
        .map(|wd| wd as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_inotify_rm_watch(fd: i32, wd: i32) -> SyscallResult {
    let id = inotify_id_from_fd(fd)?;
    crate::inotify::inotify_rm_watch(id, wd)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── Timerfd ─────────────────────────────────────────────────────────

pub fn sys_timerfd_create(clockid: i32, flags: i32) -> SyscallResult {
    let id = crate::timerfd::timerfd_create(clockid, flags).map_err(|e| {
        if e == -22 {
            SyscallError::InvalidArgument
        } else {
            SyscallError::TooManyFiles
        }
    })?;
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    let fd_table = tables.get_mut(&pid).ok_or(SyscallError::TooManyFiles)?;
    let mut o_flags = crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR);
    if flags & crate::timerfd::TFD_NONBLOCK != 0 {
        o_flags.0 |= crate::fd::OpenFlags::O_NONBLOCK;
    }
    let fd = fd_table
        .open(
            &alloc::format!("timerfd:{}", id),
            o_flags,
            crate::fd::FileType::CharDevice,
        )
        .map_err(|_| SyscallError::TooManyFiles)?;
    if flags & crate::timerfd::TFD_CLOEXEC != 0 {
        fd_table.set_cloexec(fd, true);
    }
    Ok(fd as u64)
}

fn timerfd_id_from_fd(fd: i32) -> Result<i32, SyscallError> {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let path = crate::fd::path_for_fd(pid, fd).ok_or(SyscallError::BadFileDescriptor)?;
    crate::fd::timerfd_id_from_path(&path).ok_or(SyscallError::BadFileDescriptor)
}

pub fn sys_timerfd_settime(fd: i32, flags: i32, new_value: u64, old_value: u64) -> SyscallResult {
    if new_value == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let id = timerfd_id_from_fd(fd)?;
    let spec = unsafe { &*(new_value as *const crate::timerfd::ITimerSpec) };
    let result = crate::timerfd::timerfd_settime(id, flags, spec)
        .map_err(|_| SyscallError::InvalidArgument)?;
    if old_value != 0 {
        let out = unsafe { &mut *(old_value as *mut crate::timerfd::ITimerSpec) };
        *out = result;
    }
    Ok(0)
}

pub fn sys_timerfd_gettime(fd: i32, curr_value: u64) -> SyscallResult {
    let id = timerfd_id_from_fd(fd)?;
    let spec = crate::timerfd::timerfd_gettime(id).map_err(|_| SyscallError::BadFileDescriptor)?;
    if curr_value != 0 {
        let out = unsafe { &mut *(curr_value as *mut crate::timerfd::ITimerSpec) };
        *out = spec;
    }
    Ok(0)
}

// ── FIFO & PTY ──────────────────────────────────────────────────────

pub fn sys_mkfifo(path_ptr: u64, mode: u32) -> SyscallResult {
    let path = unsafe { read_user_string(path_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::fifo::mkfifo(&path, mode as u16)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::IoError)
}

pub fn sys_openpty(master_fd_ptr: u64, slave_name_ptr: u64) -> SyscallResult {
    let (pty_id, slave_name) = crate::pty::openpty().map_err(|_| SyscallError::IoError)?;

    if master_fd_ptr != 0 {
        unsafe {
            *(master_fd_ptr as *mut u32) = pty_id;
        }
    }
    if slave_name_ptr != 0 {
        let bytes = slave_name.as_bytes();
        let len = core::cmp::min(bytes.len(), 255);
        let dst = unsafe { core::slice::from_raw_parts_mut(slave_name_ptr as *mut u8, len + 1) };
        dst[..len].copy_from_slice(&bytes[..len]);
        dst[len] = 0;
    }

    Ok(pty_id as u64)
}

// ── File locking ────────────────────────────────────────────────────

pub fn sys_flock(fd: i32, operation: i32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let key = crate::fd::lock_key_for_fd(pid, fd).ok_or(SyscallError::BadFileDescriptor)?;
    crate::flock::flock(key, operation)
        .map(|_| 0u64)
        .map_err(|e| match e {
            -11 => SyscallError::WouldBlock,
            -22 => SyscallError::InvalidArgument,
            _ => SyscallError::IoError,
        })
}

// ── Zero-copy & vectored I/O ────────────────────────────────────────

pub fn sys_sendfile(out_fd: i32, in_fd: i32, _offset: u64, count: usize) -> SyscallResult {
    crate::splice::sendfile(out_fd, in_fd, None, count)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::IoError)
}

pub fn sys_splice(
    fd_in: i32,
    _off_in: u64,
    fd_out: i32,
    _off_out: u64,
    len: usize,
    flags: u32,
) -> SyscallResult {
    crate::splice::splice(fd_in, None, fd_out, None, len, flags)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::IoError)
}

pub fn sys_tee(fd_in: i32, fd_out: i32, len: usize, flags: u32) -> SyscallResult {
    crate::splice::tee(fd_in, fd_out, len, flags)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::IoError)
}

pub fn sys_readv(fd: i32, iov_ptr: u64, iovcnt: usize) -> SyscallResult {
    if iov_ptr == 0 || iovcnt == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Iovec {
        iov_base: u64,
        iov_len: usize,
    }

    let iovecs = unsafe { core::slice::from_raw_parts(iov_ptr as *const Iovec, iovcnt) };
    let mut iovs: alloc::vec::Vec<(u64, usize)> =
        iovecs.iter().map(|v| (v.iov_base, v.iov_len)).collect();

    crate::splice::readv(fd, &mut iovs)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::IoError)
}

pub fn sys_writev(fd: i32, iov_ptr: u64, iovcnt: usize) -> SyscallResult {
    if iov_ptr == 0 || iovcnt == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Iovec {
        iov_base: u64,
        iov_len: usize,
    }

    let iovecs = unsafe { core::slice::from_raw_parts(iov_ptr as *const Iovec, iovcnt) };
    let iovs: alloc::vec::Vec<(u64, usize)> =
        iovecs.iter().map(|v| (v.iov_base, v.iov_len)).collect();

    crate::splice::writev(fd, &iovs)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::IoError)
}

pub fn sys_copy_file_range(
    fd_in: i32,
    _off_in: u64,
    fd_out: i32,
    _off_out: u64,
    len: usize,
) -> SyscallResult {
    crate::splice::copy_file_range(fd_in, None, fd_out, None, len, 0)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::IoError)
}

// ── Scheduler ───────────────────────────────────────────────────────

pub fn sys_sched_getscheduler(pid: u32) -> SyscallResult {
    crate::sched_ext::sched_getscheduler(pid)
        .map(|p| p as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_sched_setscheduler(pid: u32, policy: i32, param_ptr: u64) -> SyscallResult {
    let param = if param_ptr != 0 {
        unsafe { *(param_ptr as *const crate::sched_ext::SchedParam) }
    } else {
        crate::sched_ext::SchedParam { sched_priority: 0 }
    };
    crate::sched_ext::sched_setscheduler(pid, policy, &param)
        .map(|_| 0u64)
        .map_err(|e| match e {
            -1 => SyscallError::PermissionDenied,
            _ => SyscallError::InvalidArgument,
        })
}

pub fn sys_setpriority(which: i32, who: u32, prio: i32) -> SyscallResult {
    crate::sched_ext::setpriority(which, who, prio)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_getpriority(which: i32, who: u32) -> SyscallResult {
    crate::sched_ext::getpriority(which, who)
        .map(|p| (20 - p) as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── Message queues ──────────────────────────────────────────────────

pub fn sys_mq_open(name_ptr: u64, oflag: i32, mode: u32) -> SyscallResult {
    let name = unsafe { read_user_string(name_ptr) }.ok_or(SyscallError::InvalidArgument)?;
    crate::mqueue::mq_open(&name, oflag, mode, None)
        .map(|fd| fd as u64)
        .map_err(|e| match e {
            -17 => SyscallError::FileExists,
            -2 => SyscallError::FileNotFound,
            _ => SyscallError::IoError,
        })
}

pub fn sys_mq_send(mqd: i32, msg_ptr: u64, msg_len: usize, prio: u32) -> SyscallResult {
    if msg_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let data = unsafe { core::slice::from_raw_parts(msg_ptr as *const u8, msg_len) };
    crate::mqueue::mq_send(mqd, data, prio)
        .map(|_| 0u64)
        .map_err(|e| match e {
            -11 => SyscallError::WouldBlock,
            _ => SyscallError::IoError,
        })
}

pub fn sys_mq_receive(mqd: i32, msg_ptr: u64, msg_len: usize) -> SyscallResult {
    if msg_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let buf = unsafe { core::slice::from_raw_parts_mut(msg_ptr as *mut u8, msg_len) };
    crate::mqueue::mq_receive(mqd, buf)
        .map(|(len, _prio)| len as u64)
        .map_err(|e| match e {
            -11 => SyscallError::WouldBlock,
            _ => SyscallError::IoError,
        })
}

pub fn sys_mq_close(mqd: i32) -> SyscallResult {
    crate::mqueue::mq_close(mqd)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::BadFileDescriptor)
}

// ── Shared memory (SysV IPC) ────────────────────────────────────────

pub fn sys_shmget(key: u32, size: usize, flags: i32) -> SyscallResult {
    let id = crate::shm::shmget(key, size, flags as u32).map_err(|_| SyscallError::OutOfMemory)?;
    serial_println!("[KnoxOS] shmget({}, {}) = {}", key, size, id);
    Ok(id as u64)
}

pub fn sys_shmat(shmid: u32, shmaddr: u64, shmflg: i32) -> SyscallResult {
    let addr = crate::shm::shmat(shmid, shmaddr, shmflg as u32)
        .map_err(|_| SyscallError::InvalidArgument)?;
    Ok(addr)
}

pub fn sys_shmdt(shmaddr: u64) -> SyscallResult {
    crate::shm::shmdt(shmaddr).map_err(|_| SyscallError::InvalidArgument)?;
    Ok(0)
}

pub fn sys_shmctl(shmid: u32, cmd: i32, _buf: u64) -> SyscallResult {
    crate::shm::shmctl(shmid, cmd as u32).map_err(|_| SyscallError::InvalidArgument)?;
    Ok(0)
}
