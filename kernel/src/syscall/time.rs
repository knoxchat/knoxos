/// Syscall implementations — Time operations
/// gettimeofday, clock_gettime, nanosleep, clock_nanosleep
use super::{SyscallError, SyscallResult};

pub fn sys_gettimeofday(tv_ptr: u64) -> SyscallResult {
    let (tv, _tz) = crate::rtc::gettimeofday();
    if tv_ptr != 0 {
        let pid = crate::scheduler::current_pid().unwrap_or(1);
        let mut buf = [0u8; 16];
        buf[0..8].copy_from_slice(&tv.tv_sec.to_ne_bytes());
        buf[8..16].copy_from_slice(&tv.tv_usec.to_ne_bytes());
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), tv_ptr as *mut u8, buf.len());
        }
        crate::vmm::write_user_memory(pid, tv_ptr, &buf);
    }
    Ok(0)
}

pub fn sys_clock_gettime(clock_id: u32, tp_ptr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let ts = crate::namespaces::namespaced_clock_gettime(pid, clock_id);
    if tp_ptr != 0 {
        let mut buf = [0u8; 16];
        buf[0..8].copy_from_slice(&ts.tv_sec.to_ne_bytes());
        buf[8..16].copy_from_slice(&ts.tv_nsec.to_ne_bytes());
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), tp_ptr as *mut u8, buf.len());
        }
        crate::vmm::write_user_memory(pid, tp_ptr, &buf);
    }
    Ok(0)
}

pub fn sys_nanosleep(req_ptr: u64) -> SyscallResult {
    if req_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let mut raw = [0u8; 16];
    unsafe {
        core::ptr::copy_nonoverlapping(req_ptr as *const u8, raw.as_mut_ptr(), 16);
    }
    crate::vmm::read_user_memory(pid, req_ptr, &mut raw);
    let mut sec_bytes = [0u8; 8];
    let mut nsec_bytes = [0u8; 8];
    sec_bytes.copy_from_slice(&raw[0..8]);
    nsec_bytes.copy_from_slice(&raw[8..16]);
    let tv_sec = i64::from_ne_bytes(sec_bytes);
    let tv_nsec = i64::from_ne_bytes(nsec_bytes);

    // Validate: tv_nsec must be 0..999999999
    if !(0..1_000_000_000).contains(&tv_nsec) || tv_sec < 0 {
        return Err(SyscallError::InvalidArgument);
    }

    let total_ns = (tv_sec as u64) * 1_000_000_000 + (tv_nsec as u64);
    let total_ms = total_ns / 1_000_000;

    if total_ms == 0 {
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

/// `clock_nanosleep(clockid, flags, req, rem)` — relative sleep on a named clock.
pub fn sys_clock_nanosleep(clockid: i32, flags: i32, req_ptr: u64, rem_ptr: u64) -> SyscallResult {
    // CLOCK_REALTIME..CLOCK_TAI
    if !(0..=11).contains(&clockid) {
        return Err(SyscallError::InvalidArgument);
    }
    let _ = (flags, rem_ptr);
    sys_nanosleep(req_ptr)
}
