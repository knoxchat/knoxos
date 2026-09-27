use crate::serial_println;
/// io_uring, POSIX AIO stubs, ioprio
use crate::syscall::{SyscallError, SyscallResult};

// ── io_uring ────────────────────────────────────────────────────────

pub fn sys_io_uring_setup(entries: u32, params_ptr: u64) -> SyscallResult {
    let flags = if params_ptr != 0 {
        unsafe { *(params_ptr as *const u32) }
    } else {
        0
    };
    crate::io_uring::sys_io_uring_setup(entries, flags).map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_io_uring_enter(fd: u64, to_submit: u32, min_complete: u32, flags: u32) -> SyscallResult {
    crate::io_uring::sys_io_uring_enter(fd, to_submit, min_complete, flags)
        .map(|n| n as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_io_uring_register(fd: u64, opcode: u32, arg: u64, nr_args: u32) -> SyscallResult {
    crate::io_uring::sys_io_uring_register(fd, opcode, arg, nr_args)
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

pub fn sys_io_submit(_ctx: u64, _nr: i64, _iocbpp: u64) -> SyscallResult {
    serial_println!("[KnoxOS] io_submit denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_io_cancel(_ctx: u64, _iocb: u64, _result: u64) -> SyscallResult {
    // AIO cancel is notoriously unreliable even in Linux
    Err(SyscallError::NotImplemented)
}
