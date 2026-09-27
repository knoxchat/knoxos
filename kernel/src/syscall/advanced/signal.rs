/// tgkill/tkill and rt_sig* leftovers
use crate::syscall::{SyscallError, SyscallResult};

// ── wait / waitpid ──────────────────────────────────────────────────

pub fn sys_tgkill(tgid: u32, tid: u32, sig: u32) -> SyscallResult {
    let signal = crate::signals::Signal::from_number(sig).ok_or(SyscallError::InvalidArgument)?;
    let sender = crate::scheduler::current_pid().unwrap_or(0);
    let _ = tgid;
    crate::signals::kill(tid, signal, sender).map_err(|_| SyscallError::NoSuchProcess)?;
    Ok(0)
}

pub fn sys_tkill(tid: u32, sig: u32) -> SyscallResult {
    let signal = crate::signals::Signal::from_number(sig).ok_or(SyscallError::InvalidArgument)?;
    let sender = crate::scheduler::current_pid().unwrap_or(0);
    crate::signals::kill(tid, signal, sender).map_err(|_| SyscallError::NoSuchProcess)?;
    Ok(0)
}

// ── rt_sigaction / rt_sigprocmask / rt_sigpending / rt_sigsuspend / rt_sigreturn / rt_sigtimedwait / rt_sigqueueinfo

pub fn sys_rt_sigreturn() -> SyscallResult {
    let pid = crate::context::current_pid();
    if crate::signals::sigreturn(pid).is_some() {
        crate::usermode::request_resume_self();
    }
    Ok(0)
}

pub fn sys_rt_sigpending(set_ptr: u64, sigsetsize: usize) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let signals = crate::signals::PROCESS_SIGNALS.lock();
    let mut pending_mask: u64 = 0;
    if let Some(ps) = signals.get(&pid) {
        for sig in &ps.pending {
            pending_mask |= 1u64 << (sig.signal as u32);
        }
    }
    if set_ptr != 0 && sigsetsize >= 8 {
        unsafe {
            *(set_ptr as *mut u64) = pending_mask;
        }
    }
    Ok(0)
}

pub fn sys_rt_sigsuspend(mask_ptr: u64, sigsetsize: usize) -> SyscallResult {
    let _ = (mask_ptr, sigsetsize);
    // Suspend until a signal is delivered
    crate::arch_compat::instructions::interrupts::hlt();
    Err(SyscallError::Interrupted) // Always returns EINTR
}

pub fn sys_rt_sigtimedwait(
    set_ptr: u64,
    info_ptr: u64,
    timeout_ptr: u64,
    sigsetsize: usize,
) -> SyscallResult {
    let _ = (info_ptr, timeout_ptr, sigsetsize);
    // Simplified: immediate check for pending signals
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let signals = crate::signals::PROCESS_SIGNALS.lock();
    if let Some(ps) = signals.get(&pid) {
        let mask = if set_ptr != 0 {
            unsafe { *(set_ptr as *const u64) }
        } else {
            0
        };
        for sig in &ps.pending {
            let signum = sig.signal as u32;
            if mask & (1u64 << signum) != 0 {
                return Ok(signum as u64);
            }
        }
    }
    Err(SyscallError::WouldBlock)
}

pub fn sys_rt_sigqueueinfo(pid: u32, sig: u32, _info_ptr: u64) -> SyscallResult {
    let signal = crate::signals::Signal::from_number(sig).ok_or(SyscallError::InvalidArgument)?;
    let sender = crate::scheduler::current_pid().unwrap_or(0);
    crate::signals::kill(pid, signal, sender).map_err(|_| SyscallError::NoSuchProcess)?;
    Ok(0)
}
