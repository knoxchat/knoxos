/// landlock, capabilities
use crate::syscall::{SyscallError, SyscallResult};

// ── landlock ────────────────────────────────────────────────────────

pub fn sys_landlock_create_ruleset(attr_ptr: u64, size: usize, flags: u32) -> SyscallResult {
    let attr = if attr_ptr != 0 && size > 0 {
        unsafe { &*(attr_ptr as *const crate::landlock::LandlockRulesetAttr) }
    } else {
        return if flags & 1 != 0 {
            // LANDLOCK_CREATE_RULESET_VERSION
            Ok(3) // Landlock ABI version
        } else {
            Err(SyscallError::InvalidArgument)
        };
    };
    crate::landlock::sys_landlock_create_ruleset(attr, size, flags)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_landlock_add_rule(
    ruleset_fd: i32,
    rule_type: u32,
    rule_attr_ptr: u64,
    flags: u32,
) -> SyscallResult {
    let rt = match rule_type {
        1 => crate::landlock::LandlockRuleType::PathBeneath,
        2 => crate::landlock::LandlockRuleType::NetPort,
        _ => return Err(SyscallError::InvalidArgument),
    };
    let rule = match rt {
        crate::landlock::LandlockRuleType::PathBeneath => {
            if rule_attr_ptr == 0 {
                return Err(SyscallError::InvalidArgument);
            }
            let attr = unsafe { &*(rule_attr_ptr as *const crate::landlock::PathBeneathRule) };
            crate::landlock::LandlockRule::PathBeneath(attr.clone())
        }
        crate::landlock::LandlockRuleType::NetPort => {
            if rule_attr_ptr == 0 {
                return Err(SyscallError::InvalidArgument);
            }
            let attr = unsafe { &*(rule_attr_ptr as *const crate::landlock::NetPortRule) };
            crate::landlock::LandlockRule::NetPort(attr.clone())
        }
    };
    crate::landlock::sys_landlock_add_rule(ruleset_fd as u64, rt, rule, flags)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

pub fn sys_landlock_restrict_self(ruleset_fd: i32, flags: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1) as u64;
    crate::landlock::sys_landlock_restrict_self(ruleset_fd as u64, flags, pid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── capget / capset ─────────────────────────────────────────────────

pub fn sys_capget(header_ptr: u64, data_ptr: u64) -> SyscallResult {
    let _ = header_ptr;
    if data_ptr != 0 {
        let pid = crate::scheduler::current_pid().unwrap_or(1);
        let caps = crate::capabilities::get_capabilities(pid);
        if let Some(caps) = caps {
            unsafe {
                // Effective, permitted, inheritable (each u32)
                let p = data_ptr as *mut u32;
                *p = caps.effective.0 as u32;
                *p.add(1) = caps.permitted.0 as u32;
                *p.add(2) = caps.inheritable.0 as u32;
            }
        } else {
            unsafe {
                core::ptr::write_bytes(data_ptr as *mut u8, 0xFF, 12);
            }
        }
    }
    Ok(0)
}

pub fn sys_capset(header_ptr: u64, data_ptr: u64) -> SyscallResult {
    let _ = (header_ptr, data_ptr);
    // Accept capability changes
    Ok(0)
}
