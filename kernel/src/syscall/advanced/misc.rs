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
    Ok(0) // Kernel mode: always has I/O permissions
}

pub fn sys_iopl(level: i32) -> SyscallResult {
    let _ = level;
    Ok(0) // Accept IOPL changes
}

pub fn sys_vhangup() -> SyscallResult {
    Ok(0)
}

pub fn sys_modify_ldt(func: i32, ptr: u64, bytecount: u64) -> SyscallResult {
    let _ = (func, ptr, bytecount);
    Ok(0)
}

pub fn sys_acct(filename_ptr: u64) -> SyscallResult {
    let _ = filename_ptr;
    Ok(0) // Process accounting enable/disable
}

pub fn sys_sysfs(option: i32, arg1: u64, arg2: u64) -> SyscallResult {
    let _ = (option, arg1, arg2);
    Ok(0)
}
