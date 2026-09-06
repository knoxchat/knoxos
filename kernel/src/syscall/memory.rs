use super::{SyscallError, SyscallResult};
/// Syscall implementations — Memory management
/// brk, mmap, munmap
use alloc::alloc::{alloc_zeroed, dealloc};
use alloc::collections::BTreeMap;
use core::alloc::Layout;
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub(super) struct MmapRegion {
    pub len: usize,
    pub align: usize,
}

lazy_static::lazy_static! {
    pub(super) static ref MMAP_REGISTRY: Mutex<BTreeMap<u64, MmapRegion>> = Mutex::new(BTreeMap::new());
}

pub fn sys_brk(addr: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let has_as = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.has_address_space)
        .unwrap_or(false);
    if has_as {
        let result = crate::vmm::brk(pid, addr);
        if result >= 0 {
            Ok(result as u64)
        } else {
            let end = crate::allocator::HEAP_START as u64 + crate::allocator::HEAP_SIZE as u64;
            Ok(end)
        }
    } else {
        let end = crate::allocator::HEAP_START as u64 + crate::allocator::HEAP_SIZE as u64;
        Ok(if addr == 0 { end } else { addr.min(end) })
    }
}

pub fn sys_mmap(addr: u64, len: u64, prot: u32, flags: u32, fd: i32, _off: u64) -> SyscallResult {
    const MAP_ANONYMOUS: u32 = 0x20;
    const MAP_FIXED: u32 = 0x10;
    let _ = fd;

    if len == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let has_as = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.has_address_space)
        .unwrap_or(false);

    if has_as {
        let result = crate::vmm::mmap(pid, addr, len, prot as u64, flags as u64);
        if result >= 0 {
            Ok(result as u64)
        } else {
            Err(SyscallError::OutOfMemory)
        }
    } else {
        if flags & MAP_FIXED != 0 {
            return Err(SyscallError::InvalidArgument);
        }
        if flags & MAP_ANONYMOUS == 0 {
            return Err(SyscallError::NotImplemented);
        }
        let align = 4096usize;
        let size = ((len as usize) + (align - 1)) & !(align - 1);
        let layout =
            Layout::from_size_align(size, align).map_err(|_| SyscallError::InvalidArgument)?;
        let ptr = unsafe { alloc_zeroed(layout) };
        if ptr.is_null() {
            return Err(SyscallError::OutOfMemory);
        }
        let mapped_addr = ptr as u64;
        MMAP_REGISTRY
            .lock()
            .insert(mapped_addr, MmapRegion { len: size, align });
        Ok(mapped_addr)
    }
}

pub fn sys_munmap(addr: u64, len: u64) -> SyscallResult {
    if addr == 0 || len == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    let pid = crate::scheduler::current_pid().unwrap_or(1);
    let has_as = crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .map(|p| p.has_address_space)
        .unwrap_or(false);

    if has_as {
        let result = crate::vmm::munmap(pid, addr, len);
        if result == 0 {
            Ok(0)
        } else {
            Err(SyscallError::InvalidArgument)
        }
    } else {
        let mut registry = MMAP_REGISTRY.lock();
        let region = registry
            .remove(&addr)
            .ok_or(SyscallError::InvalidArgument)?;
        if len as usize != region.len {
            registry.insert(addr, region);
            return Err(SyscallError::InvalidArgument);
        }
        let layout = Layout::from_size_align(region.len, region.align)
            .map_err(|_| SyscallError::InvalidArgument)?;
        unsafe {
            dealloc(addr as *mut u8, layout);
        }
        Ok(0)
    }
}
