/// rseq, membarrier, madvise/msync/mincore, mlock*, mremap, NUMA policy
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

use crate::process::Pid;
use crate::syscall::{SyscallError, SyscallResult};

const PAGE_MASK: u64 = !0xFFFu64;
const MCL_CURRENT: i32 = 1;
const MCL_FUTURE: i32 = 2;

#[derive(Clone, Copy)]
struct LockedRange {
    start: u64,
    len: u64,
}

struct LockState {
    ranges: Vec<LockedRange>,
    lock_all: bool,
}

impl LockState {
    fn locked_bytes(&self) -> u64 {
        if self.lock_all {
            return u64::MAX / 2;
        }
        self.ranges.iter().map(|r| r.len).sum()
    }
}

lazy_static::lazy_static! {
    static ref LOCKED: Mutex<BTreeMap<Pid, LockState>> = Mutex::new(BTreeMap::new());
}

fn page_span(addr: u64, len: u64) -> Option<(u64, u64)> {
    if len == 0 {
        return Some((addr & PAGE_MASK, 0));
    }
    let start = addr & PAGE_MASK;
    let end = addr.checked_add(len)?.checked_add(0xFFF)? & PAGE_MASK;
    Some((start, end.saturating_sub(start)))
}

fn lock_range(pid: Pid, addr: u64, len: u64) -> SyscallResult {
    let (start, span) = page_span(addr, len).ok_or(SyscallError::InvalidArgument)?;
    if span == 0 {
        return Ok(0);
    }
    let mut table = LOCKED.lock();
    let state = table.entry(pid).or_insert(LockState {
        ranges: Vec::new(),
        lock_all: false,
    });
    let next = state.locked_bytes().saturating_add(span);
    if !crate::rlimit::check_memlock(pid, next) {
        return Err(SyscallError::OutOfMemory);
    }
    state.ranges.push(LockedRange { start, len: span });
    crate::swap::set_range_pinned(pid, start, span, true);
    Ok(0)
}

fn unlock_range(pid: Pid, addr: u64, len: u64) -> SyscallResult {
    let (start, span) = page_span(addr, len).ok_or(SyscallError::InvalidArgument)?;
    if span == 0 {
        return Ok(0);
    }
    let end = start.saturating_add(span);
    let mut table = LOCKED.lock();
    if let Some(state) = table.get_mut(&pid) {
        state.ranges.retain(|r| {
            let r_end = r.start.saturating_add(r.len);
            r_end <= start || r.start >= end
        });
        if state.ranges.is_empty() && !state.lock_all {
            table.remove(&pid);
        }
    }
    crate::swap::set_range_pinned(pid, start, span, false);
    Ok(0)
}

/// Drop mlock bookkeeping when a process exits.
pub fn cleanup_mlock(pid: Pid) {
    LOCKED.lock().remove(&pid);
}

/// True when `mlock`/`mlockall` covers this page.
pub fn is_page_locked(pid: Pid, vaddr: u64) -> bool {
    let page = vaddr & PAGE_MASK;
    let table = LOCKED.lock();
    let Some(state) = table.get(&pid) else {
        return false;
    };
    if state.lock_all {
        return true;
    }
    state.ranges.iter().any(|r| {
        let end = r.start.saturating_add(r.len);
        page >= r.start && page < end
    })
}

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
    if advice < 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    match crate::mmap::sys_madvise(pid, addr, len, advice as u32) {
        0 => Ok(0),
        -22 => Err(SyscallError::InvalidArgument),
        _ => Err(SyscallError::InvalidArgument),
    }
}

pub fn sys_msync(addr: u64, len: u64, flags: i32) -> SyscallResult {
    let _ = (addr, len, flags);
    // Sync is a no-op for in-memory filesystems
    Ok(0)
}

pub fn sys_mincore(addr: u64, len: u64, vec_ptr: u64) -> SyscallResult {
    let pages = len.div_ceil(4096) as usize;
    if vec_ptr != 0 && pages > 0 {
        let pid = crate::scheduler::current_pid().unwrap_or(1);
        let vec = alloc::vec![1u8; pages];
        crate::vmm::write_user_memory(pid, vec_ptr, &vec);
        if crate::process::PROCESS_TABLE
            .lock()
            .get_process(pid)
            .map(|p| !p.has_address_space)
            .unwrap_or(true)
        {
            // SAFETY: kernel-mode caller supplied a writable `vec` of `pages` bytes.
            let dest = unsafe { core::slice::from_raw_parts_mut(vec_ptr as *mut u8, pages) };
            dest.fill(1);
        }
    }
    let _ = addr;
    Ok(0)
}

// ── mlock / munlock / mlockall / munlockall ─────────────────────────

pub fn sys_mlock(addr: u64, len: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    lock_range(pid, addr, len)
}

pub fn sys_munlock(addr: u64, len: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    unlock_range(pid, addr, len)
}

pub fn sys_mlockall(flags: i32) -> SyscallResult {
    if flags & !(MCL_CURRENT | MCL_FUTURE) != 0 {
        return Err(SyscallError::InvalidArgument);
    }
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    if (flags & MCL_CURRENT) != 0 && !crate::rlimit::check_memlock(pid, u64::MAX / 2) {
        return Err(SyscallError::OutOfMemory);
    }
    let mut table = LOCKED.lock();
    let state = table.entry(pid).or_insert(LockState {
        ranges: Vec::new(),
        lock_all: false,
    });
    state.lock_all = (flags & (MCL_CURRENT | MCL_FUTURE)) != 0;
    Ok(0)
}

pub fn sys_munlockall() -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    LOCKED.lock().remove(&pid);
    Ok(0)
}

pub fn sys_mlock2(addr: u64, len: u64, flags: i32) -> SyscallResult {
    if flags & !1 != 0 {
        return Err(SyscallError::InvalidArgument);
    }
    sys_mlock(addr, len)
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
            // SAFETY: both mappings are this process's address space; lengths were
            // accepted by `mmap`/`munmap` and `copy_len` is the overlap.
            // SAFETY: src and dst are valid for the copied byte count and do not overlap.
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
        // SAFETY: kernel-mode mmap fallback; `new` and `old_addr` are live heap
        // regions of at least `copy_len` bytes.
        // SAFETY: src and dst are valid for the copied byte count and do not overlap.
        unsafe {
            core::ptr::copy_nonoverlapping(old_addr as *const u8, new as *mut u8, copy_len);
        }
        let _ = crate::syscall::memory::sys_munmap(old_addr, old_size);
        Ok(new)
    }
}

pub fn sys_set_mempolicy(mode: i32, nodemask: u64, maxnode: u64) -> SyscallResult {
    let _ = (mode, nodemask, maxnode);
    crate::serial_println!("[KnoxOS] set_mempolicy denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_get_mempolicy(
    policy: u64,
    nodemask: u64,
    maxnode: u64,
    addr: u64,
    flags: u64,
) -> SyscallResult {
    let _ = (policy, nodemask, maxnode, addr, flags);
    crate::serial_println!("[KnoxOS] get_mempolicy denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
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
    crate::serial_println!("[KnoxOS] mbind denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}

pub fn sys_migrate_pages(pid: u32, maxnode: u64, old_nodes: u64, new_nodes: u64) -> SyscallResult {
    let _ = (pid, maxnode, old_nodes, new_nodes);
    crate::serial_println!("[KnoxOS] migrate_pages denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
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
    crate::serial_println!("[KnoxOS] move_pages denied (ENOSYS)");
    Err(SyscallError::NotImplemented)
}
