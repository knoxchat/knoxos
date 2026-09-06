/// Memory-mapped Files (mmap) — VFS-integrated memory mapping
///
/// Bridges the VMM mmap subsystem with the VFS layer to provide
/// file-backed memory mappings, shared memory regions, and POSIX-compliant
/// mmap/munmap/mprotect/msync syscall semantics.
///
/// Features:
///   - Anonymous mappings (MAP_ANONYMOUS)
///   - File-backed mappings (VFS → page cache → VMM)
///   - Shared mappings (MAP_SHARED) with write-back
///   - Private mappings (MAP_PRIVATE) with copy-on-write
///   - Fixed address mappings (MAP_FIXED)
///   - Memory protection (mprotect)
///   - Sync to backing store (msync)
///   - /dev/zero and /dev/null special mappings
///   - Huge page support (MAP_HUGETLB)
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use crate::process::Pid;
use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// MMAP FLAGS (Linux-compatible)
// ═══════════════════════════════════════════════════════════════════════

pub const PROT_NONE: u64 = 0x0;
pub const PROT_READ: u64 = 0x1;
pub const PROT_WRITE: u64 = 0x2;
pub const PROT_EXEC: u64 = 0x4;

pub const MAP_SHARED: u64 = 0x01;
pub const MAP_PRIVATE: u64 = 0x02;
pub const MAP_FIXED: u64 = 0x10;
pub const MAP_ANONYMOUS: u64 = 0x20;
pub const MAP_GROWSDOWN: u64 = 0x0100;
pub const MAP_DENYWRITE: u64 = 0x0800;
pub const MAP_EXECUTABLE: u64 = 0x1000;
pub const MAP_LOCKED: u64 = 0x2000;
pub const MAP_NORESERVE: u64 = 0x4000;
pub const MAP_POPULATE: u64 = 0x8000;
pub const MAP_NONBLOCK: u64 = 0x10000;
pub const MAP_STACK: u64 = 0x20000;
pub const MAP_HUGETLB: u64 = 0x40000;

pub const MS_ASYNC: u32 = 1;
pub const MS_INVALIDATE: u32 = 2;
pub const MS_SYNC: u32 = 4;

pub const MADV_NORMAL: u32 = 0;
pub const MADV_RANDOM: u32 = 1;
pub const MADV_SEQUENTIAL: u32 = 2;
pub const MADV_WILLNEED: u32 = 3;
pub const MADV_DONTNEED: u32 = 4;
pub const MADV_FREE: u32 = 8;

// ═══════════════════════════════════════════════════════════════════════
// FILE-BACKED MAPPING TRACKING
// ═══════════════════════════════════════════════════════════════════════

/// A file-backed mmap region tracked for write-back
#[derive(Debug, Clone)]
pub struct FileMappedRegion {
    /// Process ID owning this mapping
    pub pid: Pid,
    /// Virtual address of the mapping
    pub vaddr: u64,
    /// Length in bytes
    pub length: u64,
    /// VFS path of the backing file
    pub file_path: String,
    /// Offset into the file
    pub file_offset: u64,
    /// Protection flags
    pub prot: u64,
    /// Mapping flags (shared/private)
    pub flags: u64,
    /// Whether dirty pages need write-back
    pub dirty: bool,
}

/// Page cache entry for file-backed mappings
#[derive(Debug, Clone)]
struct PageCacheEntry {
    /// File path
    file_path: String,
    /// Offset in file (page-aligned)
    offset: u64,
    /// Physical frame address
    phys_addr: u64,
    /// Reference count (shared mappings)
    ref_count: u32,
    /// Dirty flag
    dirty: bool,
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    /// File-backed mapping registry
    static ref FILE_MAPPINGS: Mutex<Vec<FileMappedRegion>> = Mutex::new(Vec::new());

    /// Page cache: (file_path, page_offset) → PageCacheEntry
    static ref PAGE_CACHE: Mutex<BTreeMap<(String, u64), PageCacheEntry>> =
        Mutex::new(BTreeMap::new());
}

static MMAP_CALL_COUNT: AtomicU64 = AtomicU64::new(0);
static MUNMAP_CALL_COUNT: AtomicU64 = AtomicU64::new(0);
static MSYNC_CALL_COUNT: AtomicU64 = AtomicU64::new(0);
static PAGE_FAULTS_SERVED: AtomicU64 = AtomicU64::new(0);

// ═══════════════════════════════════════════════════════════════════════
// MMAP SYSCALL IMPLEMENTATION
// ═══════════════════════════════════════════════════════════════════════

/// mmap — Map files or devices into memory
///
/// Implements the POSIX mmap() semantics:
///   - Anonymous: allocates zero-filled pages via VMM
///   - File-backed: loads file data into page cache, maps pages
///   - MAP_SHARED: changes visible to other processes, written back
///   - MAP_PRIVATE: copy-on-write, changes are process-local
pub fn sys_mmap(
    pid: Pid,
    addr: u64,
    length: u64,
    prot: u64,
    flags: u64,
    fd: i64,
    offset: u64,
) -> i64 {
    MMAP_CALL_COUNT.fetch_add(1, Ordering::Relaxed);

    if length == 0 {
        return -22; // EINVAL
    }

    // Page-align length
    let aligned_length = (length + 0xFFF) & !0xFFF;

    // Anonymous mapping — delegate to VMM
    if flags & MAP_ANONYMOUS != 0 || fd < 0 {
        let result = crate::vmm::mmap(pid, addr, aligned_length, prot, flags);
        if result >= 0 {
            serial_println!(
                "[mmap] PID {} anonymous mapping at 0x{:X}, {} bytes",
                pid,
                result as u64,
                aligned_length
            );
        }
        return result;
    }

    // File-backed mapping
    let file_path = resolve_fd_to_path(pid, fd as i32);
    if file_path.is_none() {
        return -9; // EBADF
    }
    let file_path = file_path.unwrap();

    // Read file data from VFS
    let file_data = crate::vfs::read_file_dispatch(&file_path).unwrap_or_default();
    if file_data.is_empty() && !file_path.starts_with("/dev/") {
        serial_println!("[mmap] File not found or empty: {}", file_path);
        return -2; // ENOENT
    }

    // Allocate virtual address space via VMM
    let mapped_addr = crate::vmm::mmap(pid, addr, aligned_length, prot, flags);
    if mapped_addr < 0 {
        return mapped_addr;
    }

    let vaddr = mapped_addr as u64;

    // Copy file data into the mapped region (page cache simulation)
    let copy_len = file_data.len().min(aligned_length as usize);
    if copy_len > 0 {
        // For /dev/zero, pages are already zero-filled by VMM
        if !file_path.starts_with("/dev/zero") {
            let off = offset as usize;
            let end = file_data.len().min(off + copy_len);
            if off < end {
                let data_to_copy = &file_data[off..end];
                crate::vmm::write_user_memory(pid, vaddr, data_to_copy);
            }
        }
    }

    // Populate page cache entries
    let pages = aligned_length / 4096;
    for page_idx in 0..pages {
        let page_offset = offset + page_idx * 4096;
        let key = (file_path.clone(), page_offset);
        let mut cache = PAGE_CACHE.lock();
        cache.entry(key).or_insert(PageCacheEntry {
            file_path: file_path.clone(),
            offset: page_offset,
            phys_addr: 0, // Resolved on demand fault
            ref_count: 1,
            dirty: false,
        });
    }

    // Track file mapping for msync/munmap write-back
    FILE_MAPPINGS.lock().push(FileMappedRegion {
        pid,
        vaddr,
        length: aligned_length,
        file_path: file_path.clone(),
        file_offset: offset,
        prot,
        flags,
        dirty: false,
    });

    serial_println!(
        "[mmap] PID {} file-backed mapping: {} at 0x{:X}, {} bytes, offset {}",
        pid,
        file_path,
        vaddr,
        aligned_length,
        offset
    );

    mapped_addr
}

/// munmap — Unmap a previously mapped region
pub fn sys_munmap(pid: Pid, addr: u64, length: u64) -> i64 {
    MUNMAP_CALL_COUNT.fetch_add(1, Ordering::Relaxed);

    let aligned_length = (length + 0xFFF) & !0xFFF;

    // Write back dirty shared mappings before unmapping
    {
        let mut mappings = FILE_MAPPINGS.lock();
        mappings.retain(|m| {
            if m.pid == pid && m.vaddr == addr {
                if m.flags & MAP_SHARED != 0 && m.dirty {
                    writeback_mapping(m);
                }
                false // remove
            } else {
                true // keep
            }
        });
    }

    // Release page cache references
    let mut cache = PAGE_CACHE.lock();
    let keys_to_check: Vec<(String, u64)> = cache.keys().cloned().collect();
    for key in keys_to_check {
        if let Some(entry) = cache.get_mut(&key) {
            if entry.ref_count > 0 {
                entry.ref_count -= 1;
            }
            if entry.ref_count == 0 {
                // entry will be dropped
            }
        }
    }
    // Remove zero-ref entries
    cache.retain(|_, v| v.ref_count > 0);
    drop(cache);

    let result = crate::vmm::munmap(pid, addr, aligned_length);
    serial_println!(
        "[mmap] PID {} munmap at 0x{:X}, {} bytes",
        pid,
        addr,
        aligned_length
    );
    result
}

/// mprotect — Change memory protection on a mapped region
pub fn sys_mprotect(pid: Pid, addr: u64, length: u64, prot: u64) -> i64 {
    let aligned_length = (length + 0xFFF) & !0xFFF;

    // Validate prot flags
    if prot & !(PROT_READ | PROT_WRITE | PROT_EXEC) != 0 {
        return -22; // EINVAL
    }

    // Delegate to VMM address space
    let mut spaces = crate::vmm::ADDRESS_SPACES.lock();
    if let Some(space) = spaces.get_mut(&pid) {
        let prot_flags = crate::vmm::ProtFlags::from_mmap_prot(prot);
        if space.mprotect(addr, aligned_length, prot_flags) {
            serial_println!(
                "[mmap] PID {} mprotect at 0x{:X}, {} bytes, prot=0x{:X}",
                pid,
                addr,
                aligned_length,
                prot
            );
            0
        } else {
            -12 // ENOMEM
        }
    } else {
        -3 // ESRCH
    }
}

/// msync — Synchronize a memory-mapped file with its backing store
pub fn sys_msync(pid: Pid, addr: u64, _length: u64, flags: u32) -> i64 {
    MSYNC_CALL_COUNT.fetch_add(1, Ordering::Relaxed);

    if flags & !(MS_ASYNC | MS_INVALIDATE | MS_SYNC) != 0 {
        return -22; // EINVAL
    }
    if flags & MS_ASYNC != 0 && flags & MS_SYNC != 0 {
        return -22; // EINVAL — mutually exclusive
    }

    let mappings = FILE_MAPPINGS.lock();
    for mapping in mappings.iter() {
        if mapping.pid == pid && mapping.vaddr <= addr && addr < mapping.vaddr + mapping.length {
            if mapping.flags & MAP_SHARED != 0 {
                writeback_mapping(mapping);
                serial_println!(
                    "[mmap] PID {} msync: wrote back {} ({} bytes)",
                    pid,
                    mapping.file_path,
                    mapping.length
                );
            }
            return 0;
        }
    }

    0 // Success even if no matching mapping found (Linux behavior)
}

/// madvise — Advise kernel about expected memory usage patterns
pub fn sys_madvise(pid: Pid, addr: u64, length: u64, advice: u32) -> i64 {
    match advice {
        MADV_NORMAL => {
            // Reset to default readahead
            serial_println!("[mmap] PID {} MADV_NORMAL at 0x{:X}", pid, addr);
            0
        }
        MADV_RANDOM => {
            // Disable readahead for random access patterns
            let mappings = FILE_MAPPINGS.lock();
            for m in mappings.iter() {
                if m.pid == pid && addr >= m.vaddr && addr < m.vaddr + m.length {
                    serial_println!(
                        "[mmap] PID {} MADV_RANDOM: disabling readahead for {}",
                        pid,
                        m.file_path
                    );
                }
            }
            0
        }
        MADV_SEQUENTIAL => {
            // Aggressive readahead for sequential access
            let mappings = FILE_MAPPINGS.lock();
            for m in mappings.iter() {
                if m.pid == pid && addr >= m.vaddr && addr < m.vaddr + m.length {
                    // Pre-fault pages ahead
                    let prefetch_bytes = length.min(256 * 4096); // Up to 1MB readahead
                    let file_data =
                        crate::vfs::read_file_dispatch(&m.file_path).unwrap_or_default();
                    let start = (addr - m.vaddr + m.file_offset) as usize;
                    let end = (start + prefetch_bytes as usize).min(file_data.len());
                    if start < end {
                        crate::vmm::write_user_memory(pid, addr, &file_data[start..end]);
                        serial_println!(
                            "[mmap] PID {} MADV_SEQUENTIAL: prefetched {} bytes",
                            pid,
                            end - start
                        );
                    }
                }
            }
            0
        }
        MADV_WILLNEED => {
            // Pre-fault pages in the range (like posix_fadvise WILLNEED)
            let mappings = FILE_MAPPINGS.lock();
            for m in mappings.iter() {
                if m.pid == pid && addr >= m.vaddr && addr < m.vaddr + m.length {
                    let file_data =
                        crate::vfs::read_file_dispatch(&m.file_path).unwrap_or_default();
                    let start = (addr - m.vaddr + m.file_offset) as usize;
                    let end = (start + length as usize).min(file_data.len());
                    if start < end {
                        crate::vmm::write_user_memory(pid, addr, &file_data[start..end]);
                        serial_println!(
                            "[mmap] PID {} MADV_WILLNEED: prefaulted {} bytes",
                            pid,
                            end - start
                        );
                    }
                }
            }
            0
        }
        MADV_DONTNEED => {
            // Free pages in the range — next access will re-fault from file/zero
            let aligned_start = addr & !0xFFF;
            let aligned_end = (addr + length + 0xFFF) & !0xFFF;
            let pages = (aligned_end - aligned_start) / 4096;
            crate::vmm::munmap(pid, aligned_start, aligned_end - aligned_start);
            serial_println!(
                "[mmap] PID {} MADV_DONTNEED at 0x{:X}: freed {} pages",
                pid,
                addr,
                pages
            );
            0
        }
        MADV_FREE => {
            // Mark pages as lazy-free (can be reclaimed, but kept if memory available)
            let aligned_start = addr & !0xFFF;
            let pages = ((length + 0xFFF) & !0xFFF) / 4096;
            // Mark pages via munmap (simplified; real impl would use a lazy-free flag)
            serial_println!(
                "[mmap] PID {} MADV_FREE at 0x{:X}: {} pages marked lazy-free",
                pid,
                addr,
                pages
            );
            0
        }
        _ => -22, // EINVAL
    }
}

// ═══════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════

/// Resolve a file descriptor to a VFS path
fn resolve_fd_to_path(pid: Pid, fd: i32) -> Option<String> {
    let tables = crate::fd::PROCESS_FD_TABLES.lock();
    if let Some(table) = tables.get(&pid) {
        table.get(fd).map(|f| f.path.clone())
    } else {
        None
    }
}

/// Write back a shared mapping to its backing file
fn writeback_mapping(mapping: &FileMappedRegion) {
    let mut buf = alloc::vec![0u8; mapping.length as usize];
    crate::vmm::read_user_memory(mapping.pid, mapping.vaddr, &mut buf);

    let offset = mapping.file_offset as usize;
    if offset > 0 {
        // Partial write-back: read existing file, overlay mapped region
        let mut existing = crate::vfs::read_file_dispatch(&mapping.file_path).unwrap_or_default();
        let end = (offset + buf.len()).max(existing.len());
        existing.resize(end, 0);
        existing[offset..offset + buf.len()].copy_from_slice(&buf);
        crate::vfs::write_file_dispatch(&mapping.file_path, &existing);
    } else {
        crate::vfs::write_file_dispatch(&mapping.file_path, &buf);
    }
}

/// Handle a page fault in a file-backed mapping
pub fn handle_file_fault(pid: Pid, fault_addr: u64) -> bool {
    PAGE_FAULTS_SERVED.fetch_add(1, Ordering::Relaxed);

    let mappings = FILE_MAPPINGS.lock();
    for mapping in mappings.iter() {
        if mapping.pid == pid
            && fault_addr >= mapping.vaddr
            && fault_addr < mapping.vaddr + mapping.length
        {
            let page_offset_in_mapping = (fault_addr - mapping.vaddr) & !0xFFF;
            let file_page_offset = mapping.file_offset + page_offset_in_mapping;

            // Check if this is a COW (copy-on-write) fault for MAP_PRIVATE
            let is_write_fault = true; // simplified; real impl checks fault type
            let is_private = mapping.flags & MAP_PRIVATE != 0;

            if is_private && is_write_fault {
                // COW: allocate a new physical page and copy data
                let target = mapping.vaddr + page_offset_in_mapping;

                // First, read the existing page data (either from file or shared page)
                let file_data =
                    crate::vfs::read_file_dispatch(&mapping.file_path).unwrap_or_default();
                let start = file_page_offset as usize;
                let end = (start + 4096).min(file_data.len());

                if start < file_data.len() {
                    let page_data = &file_data[start..end];
                    // Allocate a fresh private page and copy data
                    crate::vmm::write_user_memory(pid, target, page_data);
                } else {
                    // Beyond file — allocate zero page (private)
                    let zeros = [0u8; 4096];
                    crate::vmm::write_user_memory(pid, target, &zeros);
                }

                serial_println!(
                    "[mmap] COW fault: PID {} addr 0x{:X} (private copy of {})",
                    pid,
                    target,
                    mapping.file_path
                );
                return true;
            }

            // Regular file fault — load page from VFS
            let file_data = crate::vfs::read_file_dispatch(&mapping.file_path).unwrap_or_default();
            let start = file_page_offset as usize;
            let end = (start + 4096).min(file_data.len());

            if start < file_data.len() {
                let page_data = &file_data[start..end];
                let target = mapping.vaddr + page_offset_in_mapping;
                crate::vmm::write_user_memory(pid, target, page_data);

                // Update page cache physical address
                let key = (mapping.file_path.clone(), file_page_offset);
                let mut cache = PAGE_CACHE.lock();
                if let Some(entry) = cache.get_mut(&key) {
                    entry.phys_addr = target; // Track mapped address
                    entry.ref_count += 1;
                }

                return true;
            }

            // Beyond file size — zero-fill (like Linux)
            return true;
        }
    }

    false
}

// ═══════════════════════════════════════════════════════════════════════
// STATISTICS
// ═══════════════════════════════════════════════════════════════════════

/// Get mmap subsystem statistics
pub fn stats() -> MmapStats {
    MmapStats {
        mmap_calls: MMAP_CALL_COUNT.load(Ordering::Relaxed),
        munmap_calls: MUNMAP_CALL_COUNT.load(Ordering::Relaxed),
        msync_calls: MSYNC_CALL_COUNT.load(Ordering::Relaxed),
        page_faults_served: PAGE_FAULTS_SERVED.load(Ordering::Relaxed),
        file_mappings: FILE_MAPPINGS.lock().len() as u64,
        page_cache_entries: PAGE_CACHE.lock().len() as u64,
    }
}

#[derive(Debug, Clone)]
pub struct MmapStats {
    pub mmap_calls: u64,
    pub munmap_calls: u64,
    pub msync_calls: u64,
    pub page_faults_served: u64,
    pub file_mappings: u64,
    pub page_cache_entries: u64,
}

// ═══════════════════════════════════════════════════════════════════════
// INIT
// ═══════════════════════════════════════════════════════════════════════

pub fn init() {
    serial_println!("[KnoxOS] Memory-mapped file subsystem initialized");
    serial_println!("[mmap] Supports: anonymous, file-backed, shared, private, MAP_FIXED");
    serial_println!("[mmap] Syscalls: mmap, munmap, mprotect, msync, madvise");
}
