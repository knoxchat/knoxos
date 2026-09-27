/// rseq, membarrier, madvise/msync/mincore, mlock*, mremap, NUMA policy
use crate::syscall::{SyscallError, SyscallResult};

// ── rseq ────────────────────────────────────────────────────────────

pub fn sys_rseq(rseq_ptr: u64, rseq_len: u32, flags: i32, sig: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::rseq::sys_rseq(pid as u64, rseq_ptr, rseq_len, flags as u32, sig)
        .map(|_| 0u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── membarrier ──────────────────────────────────────────────────────

pub fn sys_membarrier(cmd: u32, flags: u32) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    crate::membarrier::sys_membarrier(cmd, flags, pid)
        .map(|v| v as u64)
        .map_err(|_| SyscallError::InvalidArgument)
}

// ── madvise / msync / mincore ───────────────────────────────────────

pub fn sys_madvise(addr: u64, len: u64, advice: i32) -> SyscallResult {
    let _ = (addr, len, advice);
    // Accept all madvise hints silently
    Ok(0)
}

pub fn sys_msync(addr: u64, len: u64, flags: i32) -> SyscallResult {
    let _ = (addr, len, flags);
    // Sync is a no-op for in-memory filesystems
    Ok(0)
}

pub fn sys_mincore(addr: u64, len: u64, vec_ptr: u64) -> SyscallResult {
    // Report all pages as resident
    let pages = len.div_ceil(4096) as usize;
    if vec_ptr != 0 {
        let vec = unsafe { core::slice::from_raw_parts_mut(vec_ptr as *mut u8, pages) };
        for b in vec.iter_mut() {
            *b = 1; // Page is in core
        }
    }
    let _ = addr;
    Ok(0)
}

// ── mlock / munlock / mlockall / munlockall ─────────────────────────

pub fn sys_mlock(addr: u64, len: u64) -> SyscallResult {
    let _ = (addr, len);
    Ok(0)
}

pub fn sys_munlock(addr: u64, len: u64) -> SyscallResult {
    let _ = (addr, len);
    Ok(0)
}

pub fn sys_mlockall(flags: i32) -> SyscallResult {
    let _ = flags;
    Ok(0)
}

pub fn sys_munlockall() -> SyscallResult {
    Ok(0)
}

pub fn sys_mlock2(addr: u64, len: u64, flags: i32) -> SyscallResult {
    let _ = (addr, len, flags);
    Ok(0)
}

// ── mremap ──────────────────────────────────────────────────────────

pub fn sys_mremap(
    old_addr: u64,
    old_size: u64,
    new_size: u64,
    flags: i32,
    new_addr: u64,
) -> SyscallResult {
    let _ = (flags, new_addr);
    // Simplified: allocate new region and copy
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let has_as = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.has_address_space)
        .unwrap_or(false);
    if has_as {
        let result = crate::vmm::mmap(pid, 0, new_size, 0x7, 0x22); // PROT_READ|WRITE|EXEC, MAP_PRIVATE|ANONYMOUS
        if result >= 0 {
            // Copy old data
            let copy_len = old_size.min(new_size) as usize;
            unsafe {
                core::ptr::copy_nonoverlapping(old_addr as *const u8, result as *mut u8, copy_len);
            }
            let _ = crate::vmm::munmap(pid, old_addr, old_size);
            Ok(result as u64)
        } else {
            Err(SyscallError::OutOfMemory)
        }
    } else {
        // Kernel-mode mmap fallback
        let new = crate::syscall::memory::sys_mmap(0, new_size, 0x3, 0x22, -1, 0)?;
        let copy_len = old_size.min(new_size) as usize;
        unsafe {
            core::ptr::copy_nonoverlapping(old_addr as *const u8, new as *mut u8, copy_len);
        }
        let _ = crate::syscall::memory::sys_munmap(old_addr, old_size);
        Ok(new)
    }
}

pub fn sys_set_mempolicy(mode: i32, nodemask: u64, maxnode: u64) -> SyscallResult {
    let _ = (mode, nodemask, maxnode);
    Ok(0)
}

pub fn sys_get_mempolicy(
    policy: u64,
    nodemask: u64,
    maxnode: u64,
    addr: u64,
    flags: u64,
) -> SyscallResult {
    if policy != 0 {
        unsafe {
            *(policy as *mut i32) = 0;
        }
    } // MPOL_DEFAULT
    let _ = (nodemask, maxnode, addr, flags);
    Ok(0)
}

pub fn sys_mbind(
    addr: u64,
    len: u64,
    mode: i32,
    nodemask: u64,
    maxnode: u64,
    flags: u32,
) -> SyscallResult {
    let _ = (addr, len, mode, nodemask, maxnode, flags);
    Ok(0)
}

pub fn sys_migrate_pages(pid: u32, maxnode: u64, old_nodes: u64, new_nodes: u64) -> SyscallResult {
    let _ = (pid, maxnode, old_nodes, new_nodes);
    Ok(0)
}

pub fn sys_move_pages(
    pid: u32,
    count: u64,
    pages: u64,
    nodes: u64,
    status: u64,
    flags: i32,
) -> SyscallResult {
    let _ = (pid, count, pages, nodes, status, flags);
    Ok(0)
}
