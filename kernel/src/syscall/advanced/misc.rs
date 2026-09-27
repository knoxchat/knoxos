/// sysctl, ioperm/iopl, vhangup, modify_ldt, acct, sysfs, dcookie
use crate::syscall::{SyscallError, SyscallResult};

// ── sysctl (obsolete but some programs use it) ──────────────────────

pub fn sys_sysctl_old(args_ptr: u64) -> SyscallResult {
    let _ = args_ptr;
    // Deprecated syscall
    Err(SyscallError::NotImplemented)
}

// ── Miscellaneous stubs ─────────────────────────────────────────────

pub fn sys_lookup_dcookie(cookie: u64, buf: u64, len: usize) -> SyscallResult {
    let _ = (cookie, buf, len);
    Err(SyscallError::NotImplemented)
}

pub fn sys_ioperm(from: u64, num: u64, turn_on: i32) -> SyscallResult {
    let _ = (from, num, turn_on);
    crate::serial_println!("[KnoxOS] ioperm denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_iopl(level: i32) -> SyscallResult {
    let _ = level;
    crate::serial_println!("[KnoxOS] iopl denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_vhangup() -> SyscallResult {
    crate::serial_println!("[KnoxOS] vhangup denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_modify_ldt(func: i32, ptr: u64, bytecount: u64) -> SyscallResult {
    let _ = (func, ptr, bytecount);
    crate::serial_println!("[KnoxOS] modify_ldt denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_acct(filename_ptr: u64) -> SyscallResult {
    let _ = filename_ptr;
    crate::serial_println!("[KnoxOS] acct denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_sysfs(option: i32, arg1: u64, arg2: u64) -> SyscallResult {
    let _ = (option, arg1, arg2);
    crate::serial_println!("[KnoxOS] sysfs denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}
