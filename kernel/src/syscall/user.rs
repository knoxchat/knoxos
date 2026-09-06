/// Syscall implementations — User & group management
/// getuid, getgid, setuid, setgid, seteuid, setegid, setreuid, setregid,
/// getgroups, setgroups
use super::{SyscallError, SyscallResult};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

/// Per-process supplementary group list
static PROCESS_GROUPS: Mutex<BTreeMap<u32, Vec<u32>>> = Mutex::new(BTreeMap::new());

pub fn sys_getuid() -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    Ok(crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.uid)
        .unwrap_or(0) as u64)
}

pub fn sys_getgid() -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    Ok(crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.gid)
        .unwrap_or(0) as u64)
}

pub fn sys_setuid(uid: u32) -> SyscallResult {
    crate::users::setuid(uid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

pub fn sys_setgid(gid: u32) -> SyscallResult {
    crate::users::setgid(gid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

pub fn sys_seteuid(uid: u32) -> SyscallResult {
    crate::users::seteuid(uid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

pub fn sys_setegid(gid: u32) -> SyscallResult {
    crate::users::setegid(gid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

pub fn sys_setreuid(ruid: u32, euid: u32) -> SyscallResult {
    crate::users::setreuid(ruid, euid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

pub fn sys_setregid(rgid: u32, egid: u32) -> SyscallResult {
    crate::users::setregid(rgid, egid)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::PermissionDenied)
}

pub fn sys_getgroups(size: i32, list_ptr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let groups = PROCESS_GROUPS.lock();
    let group_list = groups.get(&pid);

    let ngroups = group_list.map(|g| g.len()).unwrap_or(0);

    if size == 0 {
        // Query: return number of supplementary groups
        return Ok(ngroups as u64);
    }

    if (size as usize) < ngroups {
        return Err(SyscallError::InvalidArgument);
    }

    if list_ptr != 0 {
        if let Some(gids) = group_list {
            let out = unsafe { core::slice::from_raw_parts_mut(list_ptr as *mut u32, gids.len()) };
            for (i, &gid) in gids.iter().enumerate() {
                out[i] = gid;
            }
        }
    }
    Ok(ngroups as u64)
}

pub fn sys_setgroups(size: usize, list_ptr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);

    // Only root (uid 0) can call setgroups
    let uid = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.uid)
        .unwrap_or(0);
    if uid != 0 {
        return Err(SyscallError::PermissionDenied);
    }

    if size > 65536 {
        return Err(SyscallError::InvalidArgument);
    }

    let mut groups = PROCESS_GROUPS.lock();
    if size == 0 {
        groups.remove(&pid);
    } else if list_ptr != 0 {
        let gids = unsafe { core::slice::from_raw_parts(list_ptr as *const u32, size) };
        groups.insert(pid, gids.to_vec());
    }
    Ok(0)
}
