/// Syscall implementations — Signal handling
/// sigaction, sigprocmask, sigaltstack, kill, tgkill, tkill,
/// rt_sigreturn, rt_sigpending, rt_sigsuspend, rt_sigtimedwait, rt_sigqueueinfo
use super::{SyscallError, SyscallResult};

/// Linux sigaction structure (x86_64)
#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxSigaction {
    sa_handler: u64, // Function pointer or SIG_DFL(0)/SIG_IGN(1)
    sa_flags: u64,
    sa_restorer: u64,
    sa_mask: u64,
}

pub fn sys_sigaction(sig: u32, act_ptr: u64, oldact_ptr: u64) -> SyscallResult {
    let signal = crate::signals::Signal::from_number(sig).ok_or(SyscallError::InvalidArgument)?;

    // Can't change SIGKILL or SIGSTOP
    if sig == 9 || sig == 19 {
        return Err(SyscallError::InvalidArgument);
    }

    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut signals = crate::signals::PROCESS_SIGNALS.lock();
    let ps = signals.get_mut(&pid).ok_or(SyscallError::NoSuchProcess)?;

    // Write old action if requested
    if oldact_ptr != 0 {
        let old_disp = ps
            .dispositions
            .get(&(signal as u32))
            .copied()
            .unwrap_or(crate::signals::SignalDisposition::Default);
        let old_sa = LinuxSigaction {
            sa_handler: match old_disp {
                crate::signals::SignalDisposition::Default => 0,
                crate::signals::SignalDisposition::Ignore => 1,
                crate::signals::SignalDisposition::Handler(addr) => addr,
            },
            sa_flags: 0,
            sa_restorer: 0,
            sa_mask: 0,
        };
        unsafe {
            core::ptr::write(oldact_ptr as *mut LinuxSigaction, old_sa);
        }
    }

    // Set new action if provided
    if act_ptr != 0 {
        let sa = unsafe { *(act_ptr as *const LinuxSigaction) };
        let disp = match sa.sa_handler {
            0 => crate::signals::SignalDisposition::Default,
            1 => crate::signals::SignalDisposition::Ignore,
            addr => crate::signals::SignalDisposition::Handler(addr),
        };
        ps.set_handler(signal, disp)
            .map_err(|_| SyscallError::InvalidArgument)?;
    }
    Ok(0)
}

pub fn sys_sigprocmask(how: i32, set_ptr: u64, oldset_ptr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut signals = crate::signals::PROCESS_SIGNALS.lock();
    let ps = signals.get_mut(&pid).ok_or(SyscallError::NoSuchProcess)?;
    if oldset_ptr != 0 {
        unsafe {
            *(oldset_ptr as *mut u64) = ps.mask.0;
        }
    }
    if set_ptr != 0 {
        let mask = unsafe { *(set_ptr as *const u64) };
        // Never allow blocking SIGKILL (9) or SIGSTOP (19)
        let sanitized = mask & !((1u64 << 9) | (1u64 << 19));
        match how {
            0 => ps.mask.0 |= sanitized,  // SIG_BLOCK
            1 => ps.mask.0 &= !sanitized, // SIG_UNBLOCK
            2 => ps.mask.0 = sanitized,   // SIG_SETMASK
            _ => return Err(SyscallError::InvalidArgument),
        }
    }
    Ok(0)
}

/// sigaltstack(ss, old_ss) — set alternate signal stack
pub fn sys_sigaltstack(new_ptr: u64, old_ptr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1) as u64;

    // Read old stack if requested
    if old_ptr != 0 {
        let old =
            crate::musl::sys_sigaltstack(pid, None).map_err(|_| SyscallError::InvalidArgument)?;
        unsafe {
            core::ptr::write(old_ptr as *mut crate::musl::LinuxStack, old);
        }
    }

    // Set new stack if provided
    if new_ptr != 0 {
        let new_stack = unsafe { *(new_ptr as *const crate::musl::LinuxStack) };

        // Validate: if not disabling, size must be >= MINSIGSTKSZ (2048)
        if new_stack.ss_flags & crate::musl::SS_DISABLE == 0 && new_stack.ss_size < 2048 {
            return Err(SyscallError::InvalidArgument);
        }

        crate::musl::sys_sigaltstack(pid, Some(new_stack))
            .map_err(|_| SyscallError::InvalidArgument)?;
    }

    Ok(0)
}
