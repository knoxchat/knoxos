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
    // Linux clone_args: flags@0, pidfd@8, child_tid@16, parent_tid@24,
    // exit_signal@32, stack@40, stack_size@48, tls@56. VER0 size is 64.
    const CLONE_ARGS_SIZE_VER0: u64 = 64;
    if cl_args_ptr == 0 || size < CLONE_ARGS_SIZE_VER0 {
        return Err(SyscallError::InvalidArgument);
    }
    let n = core::cmp::min(size, 88) as usize;
    let mut buf = [0u8; 88];
    // SAFETY: src and dst are valid for the copied byte count and do not overlap.
    unsafe {
        core::ptr::copy_nonoverlapping(cl_args_ptr as *const u8, buf.as_mut_ptr(), n);
    }
    let u64_at = |off: usize| -> u64 {
        let mut b = [0u8; 8];
        if off + 8 <= n {
            b.copy_from_slice(&buf[off..off + 8]);
        }
        u64::from_ne_bytes(b)
    };
    let flags = u64_at(0);
    let child_tid = u64_at(16);
    let parent_tid = u64_at(24);
    let stack = u64_at(40);
    let tls = u64_at(56);

    const CLONE_NEWNS: u64 = 0x00020000;
    const CLONE_NEWPID: u64 = 0x20000000;
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

fn write_user_u32(pid: u32, ptr: u64, val: u32) {
    if ptr == 0 {
        return;
    }
    let bytes = val.to_ne_bytes();
    // SAFETY: src and dst are valid for the copied byte count and do not overlap.
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, 4);
    }
    crate::vmm::write_user_memory(pid, ptr, &bytes);
}

pub fn sys_getresuid(ruid_ptr: u64, euid_ptr: u64, suid_ptr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let uid = crate::namespaces::ns_uid(pid);
    write_user_u32(pid, ruid_ptr, uid);
    write_user_u32(pid, euid_ptr, uid);
    write_user_u32(pid, suid_ptr, uid);
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
    write_user_u32(pid, rgid_ptr, gid);
    write_user_u32(pid, egid_ptr, gid);
    write_user_u32(pid, sgid_ptr, gid);
    Ok(0)
}

// ── sched_rr_get_interval ───────────────────────────────────────────

pub fn sys_sched_rr_get_interval(pid: u32, tp_ptr: u64) -> SyscallResult {
    let _ = pid;
    if tp_ptr != 0 {
        let caller = crate::scheduler::current_pid().unwrap_or(1);
        let mut buf = [0u8; 16];
        buf[8..16].copy_from_slice(&100_000_000i64.to_ne_bytes());
        // SAFETY: src and dst are valid for the copied byte count and do not overlap.
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), tp_ptr as *mut u8, 16);
        }
        crate::vmm::write_user_memory(caller, tp_ptr, &buf);
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
    serial_println!("[KnoxOS] futex_waitv denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

lazy_static::lazy_static! {
    static ref ROBUST_LIST: spin::Mutex<alloc::collections::BTreeMap<u32, (u64, u64)>> =
        spin::Mutex::new(alloc::collections::BTreeMap::new());
}

/// Drop the robust futex list when a process exits.
pub fn cleanup_robust_list(pid: u32) {
    ROBUST_LIST.lock().remove(&pid);
}

pub fn sys_set_robust_list(head: u64, len: usize) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    ROBUST_LIST.lock().insert(pid, (head, len as u64));
    Ok(0)
}

pub fn sys_get_robust_list(pid: i32, head_ptr: u64, len_ptr: u64) -> SyscallResult {
    let current = crate::scheduler::current_pid().unwrap_or(1);
    let target = if pid <= 0 { current } else { pid as u32 };
    let (head, len) = ROBUST_LIST.lock().get(&target).copied().unwrap_or((0, 0));
    if head_ptr != 0 {
        crate::vmm::write_user_memory(current, head_ptr, &head.to_le_bytes());
        if crate::process::PROCESS_TABLE
            .lock()
            .get_process(current)
            .map(|p| !p.has_address_space)
            .unwrap_or(true)
        {
            // SAFETY: kernel-mode caller passed a writable `u64` slot.
            unsafe {
                *(head_ptr as *mut u64) = head;
            }
        }
    }
    if len_ptr != 0 {
        crate::vmm::write_user_memory(current, len_ptr, &len.to_le_bytes());
        if crate::process::PROCESS_TABLE
            .lock()
            .get_process(current)
            .map(|p| !p.has_address_space)
            .unwrap_or(true)
        {
            // SAFETY: kernel-mode caller passed a writable `u64` slot.
            unsafe {
                *(len_ptr as *mut u64) = len;
            }
        }
    }
    Ok(0)
}

pub fn sys_getcpu(cpu: u64, node: u64, _cache: u64) -> SyscallResult {
    let idx = crate::usermode::current_cpu_index();
    crate::smp::note_user_syscall(idx, crate::context::current_pid());
    if cpu != 0 {
        // SAFETY: the pointer is derived from a kernel object we own or a mapped physical address; the type layout matches the hardware or on-disk record.
        unsafe {
            *(cpu as *mut u32) = idx;
        }
    }
    if node != 0 {
        // SAFETY: the pointer is derived from a kernel object we own or a mapped physical address; the type layout matches the hardware or on-disk record.
        unsafe {
            *(node as *mut u32) = 0;
        }
    }
    Ok(0)
}
