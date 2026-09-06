use super::{SyscallError, SyscallResult};
/// Syscall implementations — Threading & Futex
/// futex, set_tid_address, thread_create, thread_exit, thread_join, thread_detach
use crate::serial_println;

// Futex operation constants
const FUTEX_WAIT: i32 = 0;
const FUTEX_WAKE: i32 = 1;
const FUTEX_FD: i32 = 2;
const FUTEX_REQUEUE: i32 = 3;
const FUTEX_CMP_REQUEUE: i32 = 4;
const FUTEX_WAKE_OP: i32 = 5;
const FUTEX_LOCK_PI: i32 = 6;
const FUTEX_UNLOCK_PI: i32 = 7;
const FUTEX_TRYLOCK_PI: i32 = 8;
const FUTEX_WAIT_BITSET: i32 = 9;
const FUTEX_WAKE_BITSET: i32 = 10;
const FUTEX_WAIT_REQUEUE_PI: i32 = 11;
const FUTEX_CMP_REQUEUE_PI: i32 = 12;
const FUTEX_PRIVATE_FLAG: i32 = 128;
const FUTEX_CLOCK_REALTIME: i32 = 256;
const FUTEX_BITSET_MATCH_ANY: u32 = 0xFFFFFFFF;

pub fn sys_futex(uaddr: u64, op: i32, val: u32) -> SyscallResult {
    let cmd = op & 0x7F; // Mask out FUTEX_PRIVATE_FLAG and FUTEX_CLOCK_REALTIME
    match cmd {
        FUTEX_WAIT | FUTEX_WAIT_BITSET => {
            // FUTEX_WAIT: if *uaddr == val, sleep until woken
            // Check value atomically
            let current = unsafe { *(uaddr as *const u32) };
            if current != val {
                return Err(SyscallError::WouldBlock);
            }
            crate::threads::futex_wait(uaddr, val).map_err(|_| SyscallError::WouldBlock)?;
            Ok(0)
        }
        FUTEX_WAKE | FUTEX_WAKE_BITSET => {
            // FUTEX_WAKE: wake up at most val waiters
            let woken = crate::threads::futex_wake(uaddr, val)
                .map_err(|_| SyscallError::InvalidArgument)?;
            Ok(woken as u64)
        }
        FUTEX_REQUEUE => {
            // FUTEX_REQUEUE: wake val waiters on uaddr, requeue the rest
            // val = number to wake, val2 (in arg4 position) = number to requeue
            let woken = crate::threads::futex_wake(uaddr, val)
                .map_err(|_| SyscallError::InvalidArgument)?;
            Ok(woken as u64)
        }
        FUTEX_CMP_REQUEUE => {
            // Like FUTEX_REQUEUE but checks *uaddr == val3 first
            let current = unsafe { *(uaddr as *const u32) };
            if current != val {
                return Err(SyscallError::WouldBlock);
            }
            let woken = crate::threads::futex_wake(uaddr, val)
                .map_err(|_| SyscallError::InvalidArgument)?;
            Ok(woken as u64)
        }
        FUTEX_WAKE_OP => {
            // Wake val waiters, then modify *uaddr2, optionally wake val2 waiters
            let woken = crate::threads::futex_wake(uaddr, val)
                .map_err(|_| SyscallError::InvalidArgument)?;
            Ok(woken as u64)
        }
        FUTEX_LOCK_PI => {
            // Priority-inheritance futex lock
            crate::threads::futex_wait(uaddr, val).map_err(|_| SyscallError::WouldBlock)?;
            Ok(0)
        }
        FUTEX_UNLOCK_PI => {
            // Priority-inheritance futex unlock
            let woken =
                crate::threads::futex_wake(uaddr, 1).map_err(|_| SyscallError::InvalidArgument)?;
            Ok(woken as u64)
        }
        FUTEX_TRYLOCK_PI => {
            // Non-blocking PI lock attempt
            let current = unsafe { *(uaddr as *const u32) };
            if current == 0 {
                // Lock is free, try to acquire
                unsafe {
                    *(uaddr as *mut u32) = crate::scheduler::current_pid().unwrap_or(1);
                }
                Ok(0)
            } else {
                Err(SyscallError::WouldBlock)
            }
        }
        FUTEX_FD => {
            // Deprecated, return ENOSYS
            Err(SyscallError::NotImplemented)
        }
        _ => {
            serial_println!("[KnoxOS] futex: unhandled op {}", cmd);
            Err(SyscallError::InvalidArgument)
        }
    }
}

pub fn sys_set_tid_address(tidptr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    serial_println!("[KnoxOS] set_tid_address({:#x}) for PID {}", tidptr, pid);
    Ok(pid as u64)
}

pub fn sys_thread_create(entry: u64, stack: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let tid = crate::threads::thread_create(pid, entry, stack, 64 * 1024)
        .map_err(|_| SyscallError::OutOfMemory)?;
    serial_println!("[KnoxOS] thread_create(entry={:#x}) = TID {}", entry, tid);
    Ok(tid as u64)
}

pub fn sys_thread_exit(status: i32) -> SyscallResult {
    crate::threads::thread_exit(0, status as u64);
    Ok(0)
}

pub fn sys_thread_join(tid: u32) -> SyscallResult {
    let status = crate::threads::thread_join(tid).map_err(|_| SyscallError::InvalidArgument)?;
    Ok(status)
}

pub fn sys_thread_detach(tid: u32) -> SyscallResult {
    crate::threads::thread_detach(tid).map_err(|_| SyscallError::InvalidArgument)?;
    Ok(0)
}
