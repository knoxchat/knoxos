use crate::serial_println;
/// io_uring, POSIX AIO stubs, ioprio
use crate::syscall::{SyscallError, SyscallResult};

// ── io_uring ────────────────────────────────────────────────────────

/// Linux `io_uring_params` is 120 bytes; we fill sq/cq entries + flags.
const IO_URING_PARAMS_SIZE: usize = 120;

pub fn sys_io_uring_setup(entries: u32, params_ptr: u64) -> SyscallResult {
    // Gate M4: `io_uring_setup(0, 0)` stays `-ENOSYS`. Chromium uses entries ≥ 1.
    if entries == 0 {
        return Err(SyscallError::NotImplemented);
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut flags = 0u32;
    if params_ptr != 0 {
        let mut buf = [0u8; 12];
        crate::vmm::read_user_memory(pid, params_ptr, &mut buf);
        let mut raw = [0u8; 4];
        raw.copy_from_slice(&buf[8..12]);
        flags = u32::from_le_bytes(raw);
    }
    let ring = crate::io_uring::sys_io_uring_setup(entries, flags)
        .map_err(|_| SyscallError::InvalidArgument)?;
    let mut tables = crate::fd::PROCESS_FD_TABLES.lock();
    tables
        .entry(pid)
        .or_insert_with(|| crate::fd::FdTable::new());
    let fd_table = tables.get_mut(&pid).ok_or(SyscallError::TooManyFiles)?;
    let fd = match fd_table.open(
        &alloc::format!("iouring:{ring}"),
        crate::fd::OpenFlags(crate::fd::OpenFlags::O_RDWR),
        crate::fd::FileType::CharDevice,
    ) {
        Ok(fd) => fd,
        Err(_) => {
            drop(tables);
            crate::io_uring::io_uring_destroy(ring);
            return Err(SyscallError::TooManyFiles);
        }
    };
    drop(tables);
    if params_ptr != 0 {
        if let Some(stats) = crate::io_uring::io_uring_stats(ring) {
            let mut params = [0u8; IO_URING_PARAMS_SIZE];
            params[0..4].copy_from_slice(&stats.sq_entries.to_le_bytes());
            params[4..8].copy_from_slice(&stats.cq_entries.to_le_bytes());
            params[8..12].copy_from_slice(&stats.flags.to_le_bytes());
            crate::vmm::write_user_memory(pid, params_ptr, &params);
        }
    }
    Ok(fd as u64)
}

pub fn sys_io_uring_enter(fd: u64, to_submit: u32, min_complete: u32, flags: u32) -> SyscallResult {
    let ring = crate::io_uring::ring_id_for_current_fd(fd as i32)
        .ok_or(SyscallError::BadFileDescriptor)?;
    crate::io_uring::sys_io_uring_enter(ring, to_submit, min_complete, flags)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_io_uring_register(fd: u64, opcode: u32, arg: u64, nr_args: u32) -> SyscallResult {
    let ring = crate::io_uring::ring_id_for_current_fd(fd as i32)
        .ok_or(SyscallError::BadFileDescriptor)?;
    crate::io_uring::sys_io_uring_register(ring, opcode, arg, nr_args)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── ioprio ──────────────────────────────────────────────────────────

pub fn sys_ioprio_set(which: u32, who: u32, ioprio: u32) -> SyscallResult {
    crate::io_prio::ioprio_set(which, who, ioprio as u16)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_ioprio_get(which: u32, who: u32) -> SyscallResult {
    crate::io_prio::ioprio_get(which, who)
        .map(|v| v as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── AIO ─────────────────────────────────────────────────────────────

pub fn sys_io_setup(_max_events: u32, _ctx_ptr: u64) -> SyscallResult {
    serial_println!("[KnoxOS] io_setup denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_io_destroy(_ctx: u64) -> SyscallResult {
    serial_println!("[KnoxOS] io_destroy denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_io_getevents(
    _ctx: u64,
    _min_nr: i64,
    _max_nr: i64,
    _events_ptr: u64,
    _timeout_ptr: u64,
) -> SyscallResult {
    serial_println!("[KnoxOS] io_getevents denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_io_pgetevents(
    _ctx: u64,
    _min_nr: i64,
    _max_nr: i64,
    _events_ptr: u64,
    _timeout_ptr: u64,
    _sigmask: u64,
) -> SyscallResult {
    serial_println!("[KnoxOS] io_pgetevents denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_io_submit(_ctx: u64, _nr: i64, _iocbpp: u64) -> SyscallResult {
    serial_println!("[KnoxOS] io_submit denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_io_cancel(_ctx: u64, _iocb: u64, _result: u64) -> SyscallResult {
    serial_println!("[KnoxOS] io_cancel denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}
