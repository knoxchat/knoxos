use crate::serial_println;
/// gettid, clone3, resuid/resgid, robust list, getcpu, futex_waitv
use crate::syscall::{SyscallError, SyscallResult};

// ── gettid ──────────────────────────────────────────────────────────

pub fn sys_gettid() -> SyscallResult {
    // In many cases tid == pid for the main thread
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    Ok(pid as u64)
}

// ── clone3 ──────────────────────────────────────────────────────────

pub fn sys_clone3(cl_args_ptr: u64, size: u64) -> SyscallResult {
    // clone3 uses a clone_args struct
    // Parse the flags from clone_args (first u64 field)
    let flags = if cl_args_ptr != 0 && size >= 8 {
        unsafe { *(cl_args_ptr as *const u64) }
    } else {
        0
    };

    const CLONE_NEWNS: u64 = 0x00020000;
    const CLONE_NEWPID: u64 = 0x20000000;

    let stack = if cl_args_ptr != 0 && size >= 48 {
        unsafe { *((cl_args_ptr + 40) as *const u64) }
    } else {
        0
    };
    let parent_tid = if cl_args_ptr != 0 && size >= 32 {
        unsafe { *((cl_args_ptr + 16) as *const u64) }
    } else {
        0
    };
    let child_tid = if cl_args_ptr != 0 && size >= 72 {
        unsafe { *((cl_args_ptr + 64) as *const u64) }
    } else {
        0
    };
    let tls = if cl_args_ptr != 0 && size >= 88 {
        unsafe { *((cl_args_ptr + 80) as *const u64) }
    } else {
        0
    };

    if flags & CLONE_NEWNS != 0 || flags & CLONE_NEWPID != 0 {
        serial_println!("[KnoxOS] clone3 with namespace flags {:#x}", flags);
    }

    crate::syscall::process::sys_clone(flags, stack, parent_tid, child_tid, tls)
}

// ── setresuid / getresuid / setresgid / getresgid ───────────────────

pub fn sys_setresuid(ruid: u32, euid: u32, suid: u32) -> SyscallResult {
    let _ = suid;
    crate::users::setreuid(ruid, euid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

pub fn sys_getresuid(ruid_ptr: u64, euid_ptr: u64, suid_ptr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let uid = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.uid)
        .unwrap_or(0);
    if ruid_ptr != 0 {
        unsafe {
            *(ruid_ptr as *mut u32) = uid;
        }
    }
    if euid_ptr != 0 {
        unsafe {
            *(euid_ptr as *mut u32) = uid;
        }
    }
    if suid_ptr != 0 {
        unsafe {
            *(suid_ptr as *mut u32) = uid;
        }
    }
    Ok(0)
}

pub fn sys_setresgid(rgid: u32, egid: u32, sgid: u32) -> SyscallResult {
    let _ = sgid;
    crate::users::setregid(rgid, egid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

pub fn sys_getresgid(rgid_ptr: u64, egid_ptr: u64, sgid_ptr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let gid = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.gid)
        .unwrap_or(0);
    if rgid_ptr != 0 {
        unsafe {
            *(rgid_ptr as *mut u32) = gid;
        }
    }
    if egid_ptr != 0 {
        unsafe {
            *(egid_ptr as *mut u32) = gid;
        }
    }
    if sgid_ptr != 0 {
        unsafe {
            *(sgid_ptr as *mut u32) = gid;
        }
    }
    Ok(0)
}

// ── sched_rr_get_interval ───────────────────────────────────────────

pub fn sys_sched_rr_get_interval(pid: u32, tp_ptr: u64) -> SyscallResult {
    let _ = pid;
    if tp_ptr != 0 {
        unsafe {
            let p = tp_ptr as *mut [i64; 2];
            (*p)[0] = 0;
            (*p)[1] = 100_000_000; // 100ms default quantum
        }
    }
    Ok(0)
}

// ── futex2 ──────────────────────────────────────────────────────────

pub fn sys_futex_waitv(
    waiters: u64,
    nr_futexes: u32,
    flags: u32,
    timeout: u64,
    clockid: u32,
) -> SyscallResult {
    let _ = (waiters, nr_futexes, flags, timeout, clockid);
    // Simplified: return immediately
    Ok(0)
}

pub fn sys_set_robust_list(head: u64, len: usize) -> SyscallResult {
    let _ = (head, len);
    Ok(0)
}

pub fn sys_get_robust_list(pid: i32, head_ptr: u64, len_ptr: u64) -> SyscallResult {
    let _ = pid;
    if head_ptr != 0 {
        unsafe {
            *(head_ptr as *mut u64) = 0;
        }
    }
    if len_ptr != 0 {
        unsafe {
            *(len_ptr as *mut u64) = 0;
        }
    }
    Ok(0)
}

pub fn sys_getcpu(cpu: u64, node: u64, _cache: u64) -> SyscallResult {
    let idx = crate::usermode::current_cpu_index();
    crate::smp::note_user_syscall(idx, crate::context::current_pid());
    if cpu != 0 {
        unsafe {
            *(cpu as *mut u32) = idx;
        }
    }
    if node != 0 {
        unsafe {
            *(node as *mut u32) = 0;
        }
    }
    Ok(0)
}
