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
    Ok(crate::namespaces::ns_uid(pid) as u64)
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
            let mut bytes = Vec::with_capacity(gids.len() * 4);
            for gid in gids {
                bytes.extend_from_slice(&gid.to_ne_bytes());
            }
            unsafe {
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), list_ptr as *mut u8, bytes.len());
            }
            crate::vmm::write_user_memory(pid, list_ptr, &bytes);
        }
    }
    Ok(ngroups as u64)
}

pub fn sys_setgroups(size: usize, list_ptr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);

    // A spawned boot task is namespaced root even when Process.uid is 1000.
    if crate::namespaces::ns_uid(pid) != 0 {
        return Err(SyscallError::PermissionDenied);
    }

    if size > 65536 {
        return Err(SyscallError::InvalidArgument);
    }

    let mut groups = PROCESS_GROUPS.lock();
    if size == 0 {
        groups.remove(&pid);
    } else if list_ptr != 0 {
        let nbytes = size.saturating_mul(4);
        let mut bytes = alloc::vec![0u8; nbytes];
        if nbytes > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(list_ptr as *const u8, bytes.as_mut_ptr(), nbytes);
            }
            crate::vmm::read_user_memory(pid, list_ptr, &mut bytes);
        }
        let mut gids = Vec::with_capacity(size);
        for chunk in bytes.chunks_exact(4) {
            let mut b = [0u8; 4];
            b.copy_from_slice(chunk);
            gids.push(u32::from_ne_bytes(b));
        }
        groups.insert(pid, gids);
    }
    Ok(0)
}
