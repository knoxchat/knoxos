/// clock_*, itimer, alarm, POSIX timers, adjtimex, settimeofday
use crate::syscall::{SyscallError, SyscallResult};

// ── clock_settime / clock_getres / clock_adjtime ────────────────────

pub fn sys_clock_settime(clock_id: u32, tp_ptr: u64) -> SyscallResult {
    let _ = (clock_id, tp_ptr);
    // Setting the clock requires CAP_SYS_TIME; accept as no-op for now
    Ok(0)
}

pub fn sys_clock_getres(clock_id: u32, res_ptr: u64) -> SyscallResult {
    let _ = clock_id;
    if res_ptr != 0 {
        let pid = crate::scheduler::current_pid().unwrap_or(1);
        let mut buf = [0u8; 16];
        buf[8..16].copy_from_slice(&1i64.to_ne_bytes());
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), res_ptr as *mut u8, buf.len());
        }
        crate::vmm::write_user_memory(pid, res_ptr, &buf);
    }
    Ok(0)
}

// ── getitimer / setitimer (Linux standard numbers) ──────────────────

pub fn sys_getitimer_linux(which: i32, curr_value: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let itimer_which = match which {
        0 => crate::posix_timer::ITimerWhich::Real,
        1 => crate::posix_timer::ITimerWhich::Virtual,
        2 => crate::posix_timer::ITimerWhich::Prof,
        _ => return Err(SyscallError::InvalidArgument),
    };

    if curr_value != 0 {
        match crate::posix_timer::getitimer(pid, itimer_which) {
            Ok(val) => {
                // itimerval: {it_interval: {tv_sec, tv_usec}, it_value: {tv_sec, tv_usec}}
                unsafe {
                    let p = curr_value as *mut i64;
                    *p = val.interval.tv_sec;
                    *p.add(1) = val.interval.tv_nsec / 1000; // nsec -> usec
                    *p.add(2) = val.value.tv_sec;
                    *p.add(3) = val.value.tv_nsec / 1000;
                }
            }
            Err(_) => unsafe {
                core::ptr::write_bytes(curr_value as *mut u8, 0, 32);
            },
        }
    }
    Ok(0)
}

pub fn sys_setitimer_linux(which: i32, new_value: u64, old_value: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let itimer_which = match which {
        0 => crate::posix_timer::ITimerWhich::Real,
        1 => crate::posix_timer::ITimerWhich::Virtual,
        2 => crate::posix_timer::ITimerWhich::Prof,
        _ => return Err(SyscallError::InvalidArgument),
    };

    let new_val = if new_value != 0 {
        unsafe {
            let p = new_value as *const i64;
            crate::posix_timer::ITimerVal {
                interval: crate::posix_timer::Timespec {
                    tv_sec: *p,
                    tv_nsec: *p.add(1) * 1000,
                },
                value: crate::posix_timer::Timespec {
                    tv_sec: *p.add(2),
                    tv_nsec: *p.add(3) * 1000,
                },
            }
        }
    } else {
        crate::posix_timer::ITimerVal::default()
    };

    match crate::posix_timer::setitimer(pid, itimer_which, new_val) {
        Ok(old_val) => {
            if old_value != 0 {
                unsafe {
                    let p = old_value as *mut i64;
                    *p = old_val.interval.tv_sec;
                    *p.add(1) = old_val.interval.tv_nsec / 1000;
                    *p.add(2) = old_val.value.tv_sec;
                    *p.add(3) = old_val.value.tv_nsec / 1000;
                }
            }
            Ok(0)
        }
        Err(_) => Err(SyscallError::InvalidArgument),
    }
}

// ── alarm (Linux standard number 37) ────────────────────────────────

pub fn sys_alarm_linux(seconds: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let prev = crate::posix_timer::alarm(pid, seconds);
    Ok(prev as u64)
}

// ── timer_create / timer_settime / timer_gettime / timer_getoverrun / timer_delete

pub fn sys_timer_create_linux(clockid: u32, sevp: u64, timerid: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let notify = if sevp != 0 {
        let sigev_notify = unsafe { *(sevp as *const i32) };
        let sigev_signo = unsafe { *((sevp as usize + 4) as *const i32) };
        match sigev_notify {
            0 => crate::posix_timer::TimerNotify::Signal(sigev_signo as u32),
            1 => crate::posix_timer::TimerNotify::Signal(sigev_signo as u32),
            _ => crate::posix_timer::TimerNotify::Signal(14),
        }
    } else {
        crate::posix_timer::TimerNotify::Signal(14)
    };
    let id = crate::posix_timer::timer_create(pid, clockid, notify)
        .map_err(|_| SyscallError::InvalidArgument)?;
    if timerid != 0 {
        unsafe {
            *(timerid as *mut u32) = id;
        }
    }
    Ok(0)
}

pub fn sys_timer_settime_linux(
    timerid: u32,
    flags: i32,
    new_value: u64,
    old_value: u64,
) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);

    // Parse itimerspec from new_value: {interval: {tv_sec, tv_nsec}, value: {tv_sec, tv_nsec}}
    let (interval, value) = if new_value != 0 {
        let p = new_value as *const i64;
        unsafe {
            let it_interval = crate::posix_timer::Timespec {
                tv_sec: *p,
                tv_nsec: *p.add(1),
            };
            let it_value = crate::posix_timer::Timespec {
                tv_sec: *p.add(2),
                tv_nsec: *p.add(3),
            };
            (it_interval, it_value)
        }
    } else {
        let zero = crate::posix_timer::Timespec::default();
        (zero, zero)
    };

    let absolute = flags & 1 != 0; // TIMER_ABSTIME
    match crate::posix_timer::timer_settime(pid, timerid, interval, value, absolute) {
        Ok(old_val) => {
            if old_value != 0 {
                // Write old itimerspec: {interval(zeroed for simplicity), value}
                unsafe {
                    let p = old_value as *mut i64;
                    *p = 0; // old interval.tv_sec
                    *p.add(1) = 0; // old interval.tv_nsec
                    *p.add(2) = old_val.tv_sec;
                    *p.add(3) = old_val.tv_nsec;
                }
            }
            Ok(0)
        }
        Err(_) => Err(SyscallError::InvalidArgument),
    }
}

pub fn sys_timer_gettime_linux(timerid: u32, curr_value: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let (interval, value) = crate::posix_timer::timer_gettime(pid, timerid)
        .map_err(|_| SyscallError::InvalidArgument)?;
    if curr_value != 0 {
        let mut buf = [0u8; 32];
        buf[0..8].copy_from_slice(&interval.tv_sec.to_ne_bytes());
        buf[8..16].copy_from_slice(&interval.tv_nsec.to_ne_bytes());
        buf[16..24].copy_from_slice(&value.tv_sec.to_ne_bytes());
        buf[24..32].copy_from_slice(&value.tv_nsec.to_ne_bytes());
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), curr_value as *mut u8, buf.len());
        }
        crate::vmm::write_user_memory(pid, curr_value, &buf);
    }
    Ok(0)
}

pub fn sys_timer_getoverrun_linux(timerid: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    match crate::posix_timer::timer_getoverrun(pid, timerid) {
        Ok(count) => Ok(count as u64),
        Err(_) => Ok(0),
    }
}

pub fn sys_timer_delete_linux(timerid: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::posix_timer::timer_delete(pid, timerid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_adjtimex(buf: u64) -> SyscallResult {
    let _ = buf;
    Ok(0) // TIME_OK
}

pub fn sys_settimeofday(tv: u64, tz: u64) -> SyscallResult {
    let _ = (tv, tz);
    Ok(0)
}
