/// Syscall implementations — Time operations
/// gettimeofday, clock_gettime, nanosleep, clock_nanosleep
use super::{SyscallError, SyscallResult};

pub fn sys_gettimeofday(tv_ptr: u64) -> SyscallResult {
    let (tv, _tz) = crate::rtc::gettimeofday();
    if tv_ptr != 0 {
        unsafe {
            let p = tv_ptr as *mut [i64; 2];
            (*p)[0] = tv.tv_sec;
            (*p)[1] = tv.tv_usec;
        }
    }
    Ok(0)
}

pub fn sys_clock_gettime(clock_id: u32, tp_ptr: u64) -> SyscallResult {
    let ts = crate::rtc::clock_gettime(clock_id);
    if tp_ptr != 0 {
        unsafe {
            let p = tp_ptr as *mut [i64; 2];
            (*p)[0] = ts.tv_sec;
            (*p)[1] = ts.tv_nsec;
        }
    }
    Ok(0)
}

pub fn sys_nanosleep(req_ptr: u64) -> SyscallResult {
    if req_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let ts = unsafe { &*(req_ptr as *const crate::rtc::Timespec) };

    // Validate: tv_nsec must be 0..999999999
    if ts.tv_nsec < 0 || ts.tv_nsec >= 1_000_000_000 || ts.tv_sec < 0 {
        return Err(SyscallError::InvalidArgument);
    }

    let total_ns = (ts.tv_sec as u64) * 1_000_000_000 + (ts.tv_nsec as u64);
    let total_ms = total_ns / 1_000_000;

    if total_ms == 0 && total_ns > 0 {
        // Sub-millisecond sleep: just yield once
        crate::arch_compat::instructions::interrupts::hlt();
        return Ok(0);
    }

    let start = crate::interrupts::get_ticks();
    // PIT fires at ~1000 Hz when properly configured, ~18.2 Hz for default
    let ticks_to_wait = if total_ms > 0 { total_ms / 10 } else { 1 };

    let mut elapsed = 0u64;
    while elapsed < ticks_to_wait {
        crate::arch_compat::instructions::interrupts::hlt();
        elapsed = crate::interrupts::get_ticks() - start;

        // Check for pending signals (interruptible sleep)
        let pid = crate::scheduler::current_pid().unwrap_or(1);
        let signals = crate::signals::PROCESS_SIGNALS.lock();
        if let Some(ps) = signals.get(&pid) {
            if !ps.pending.is_empty() {
                return Err(SyscallError::Interrupted);
            }
        }
    }
    Ok(0)
}
