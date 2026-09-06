#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::{
    PhysAddr, VirtAddr,
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame,
        Size4KiB,
    },
};
/// Virtual Memory Manager — Per-process page tables, address space management,
/// real mmap page mapping, copy-on-write fork, guard pages, and ASLR
///
/// This module provides true process isolation via separate page tables per process.
/// Each process gets its own address space with:
///   - Program segments mapped from ELF loading
///   - User-mode stack with guard page
///   - User-mode heap (brk/sbrk)
///   - mmap regions for anonymous and file-backed mappings
///   - Copy-on-write (CoW) pages for efficient fork()
///
/// Layout (per-process user virtual address space):
///   0x0000_0000_0040_0000 — Program text/data (ELF segments)
///   0x0000_0000_4000_0000 — Heap start (grows up via brk)
///   0x0000_7000_0000_0000 — mmap region (grows down)
///   0x0000_7FFF_FFFF_0000 — Stack top (grows down, 8MB default)
///   0xFFFF_8000_0000_0000+ — Kernel space (shared across all processes)
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;
#[cfg(target_arch = "x86_64")]
use x86_64::{
    PhysAddr, VirtAddr,
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame,
        Size4KiB,
    },
};

use crate::process::Pid;
use crate::serial_println;

// ─── Constants ──────────────────────────────────────────────────────────

/// User-space virtual address ranges
pub const USER_SPACE_START: u64 = 0x0000_0000_0010_0000;
pub const USER_SPACE_END: u64 = 0x0000_7FFF_FFFF_FFFF;

/// Program load base
pub const PROGRAM_BASE: u64 = 0x0000_0000_0040_0000;

/// Heap region
pub const HEAP_START: u64 = 0x0000_0000_4000_0000;
pub const HEAP_MAX: u64 = 0x0000_0000_C000_0000; // 2GB max heap

/// mmap region (top-down allocation)
pub const MMAP_REGION_START: u64 = 0x0000_2000_0000_0000;
pub const MMAP_REGION_END: u64 = 0x0000_7000_0000_0000;

/// Stack region
pub const STACK_TOP: u64 = 0x0000_7FFF_FFFF_0000;
pub const STACK_SIZE: u64 = 2 * 1024 * 1024; // 2MB default stack
pub const STACK_GUARD_PAGES: u64 = 1; // Guard page below stack

/// Page size
pub const PAGE_SIZE: u64 = 4096;

/// ASLR settings
pub const ASLR_ENABLED: bool = true;
pub const ASLR_PROGRAM_RANGE: u64 = 0x0000_0000_0100_0000; // 16 MiB range for program base
pub const ASLR_STACK_RANGE: u64 = 0x0000_0000_0200_0000; // 32 MiB range for stack
pub const ASLR_MMAP_RANGE: u64 = 0x0000_0100_0000_0000; // 1 TiB range for mmap
pub const ASLR_HEAP_RANGE: u64 = 0x0000_0000_1000_0000; // 256 MiB range for heap

/// Simple PRNG for ASLR (xorshift64)
static ASLR_SEED: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Initialize ASLR seed from TSC
pub fn init_aslr_seed() {
    let tsc = crate::arch_compat::read_tsc();
    ASLR_SEED.store(tsc, core::sync::atomic::Ordering::Relaxed);
}

/// Get a random page-aligned offset within [0, range)
fn aslr_offset(range: u64) -> u64 {
    if !ASLR_ENABLED || range == 0 {
        return 0;
    }
    let mut seed = ASLR_SEED.load(core::sync::atomic::Ordering::Relaxed);
    if seed == 0 {
        seed = 0xDEADBEEF_CAFEBABE;
    }
    // xorshift64
    seed ^= seed << 13;
    seed ^= seed >> 7;
    seed ^= seed << 17;
    ASLR_SEED.store(seed, core::sync::atomic::Ordering::Relaxed);
    // Page-align within range
    let pages = range / PAGE_SIZE;
    if pages == 0 {
        return 0;
    }
    (seed % pages) * PAGE_SIZE
}

/// Get ASLR-randomized program base
pub fn aslr_program_base() -> u64 {
    PROGRAM_BASE + aslr_offset(ASLR_PROGRAM_RANGE)
}

/// Get ASLR-randomized stack top
pub fn aslr_stack_top() -> u64 {
    STACK_TOP - aslr_offset(ASLR_STACK_RANGE)
}

/// Get ASLR-randomized mmap end
pub fn aslr_mmap_end() -> u64 {
    MMAP_REGION_END - aslr_offset(ASLR_MMAP_RANGE)
}

/// Get ASLR-randomized heap start
pub fn aslr_heap_start() -> u64 {
    HEAP_START + aslr_offset(ASLR_HEAP_RANGE)
}

// ─── Memory Region Tracking ────────────────────────────────────────────

/// Type of memory mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingType {
    /// Program text/code from ELF
    Text,
    /// Program data from ELF (.data, .bss)
    Data,
    /// Heap (brk/sbrk)
    Heap,
    /// Stack
    Stack,
    /// Guard page (unmapped, triggers fault on access)
    Guard,
    /// Anonymous mmap
    Anonymous,
    /// File-backed mmap
    FileBacked,
    /// Shared memory
    Shared,
    /// Kernel mapping (not user-accessible)
    Kernel,
}

/// Protection flags for a memory region
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtFlags {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl ProtFlags {
    pub const RX: Self = Self {
        read: true,
        write: false,
        execute: true,
    };
    pub const RW: Self = Self {
        read: true,
        write: true,
        execute: false,
    };
    pub const RWX: Self = Self {
        read: true,
        write: true,
        execute: true,
    };
    pub const R: Self = Self {
        read: true,
        write: false,
        execute: false,
    };
    pub const NONE: Self = Self {
        read: false,
        write: false,
        execute: false,
    };

    /// Convert to x86_64 page table flags
    pub fn to_page_flags(self) -> PageTableFlags {
        let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        if self.write {
            flags |= PageTableFlags::WRITABLE;
        }
        if !self.execute {
            flags |= PageTableFlags::NO_EXECUTE;
        }
        flags
    }

    /// Create from Linux mmap prot constants
    pub fn from_mmap_prot(prot: u64) -> Self {
        const PROT_READ: u64 = 0x1;
        const PROT_WRITE: u64 = 0x2;
        const PROT_EXEC: u64 = 0x4;
        Self {
            read: prot & PROT_READ != 0,
            write: prot & PROT_WRITE != 0,
            execute: prot & PROT_EXEC != 0,
        }
    }
}

/// Flags for mmap
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmapFlags {
    pub shared: bool,
    pub anonymous: bool,
    pub fixed: bool,
    pub populate: bool,
}

impl MmapFlags {
    pub fn from_linux(flags: u64) -> Self {
        const MAP_SHARED: u64 = 0x01;
        const MAP_PRIVATE: u64 = 0x02;
        const MAP_FIXED: u64 = 0x10;
        const MAP_ANONYMOUS: u64 = 0x20;
        const MAP_POPULATE: u64 = 0x08000;
        Self {
            shared: flags & MAP_SHARED != 0,
            anonymous: flags & MAP_ANONYMOUS != 0,
            fixed: flags & MAP_FIXED != 0,
            populate: flags & MAP_POPULATE != 0,
        }
    }
}

/// A virtual memory area (VMA) — contiguous region with uniform protections
#[derive(Debug, Clone)]
pub struct VirtualMemoryArea {
    /// Start virtual address (page-aligned)
    pub start: u64,
    /// End virtual address (exclusive, page-aligned)
    pub end: u64,
    /// Protection flags
    pub prot: ProtFlags,
    /// Mapping type
    pub mapping_type: MappingType,
    /// Mapping flags
    pub flags: MmapFlags,
    /// File path for file-backed mappings
    pub file_path: Option<String>,
    /// File offset for file-backed mappings
    pub file_offset: u64,
    /// Whether pages are copy-on-write
    pub cow: bool,
    /// Reference count for shared mappings
    pub ref_count: u64,
}

impl VirtualMemoryArea {
    /// Number of pages in this VMA
    pub fn page_count(&self) -> u64 {
        (self.end - self.start) / PAGE_SIZE
    }

    /// Check if an address falls within this VMA
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.start && addr < self.end
    }
}

/// Per-process address space
pub struct AddressSpace {
    /// Process ID
    pub pid: Pid,
    /// Physical address of the L4 page table for this process
    pub cr3: u64,
    /// Virtual memory areas sorted by start address
    pub vmas: BTreeMap<u64, VirtualMemoryArea>,
    /// Current program break (heap end)
    pub brk: u64,
    /// Next mmap address (top-down allocation)
    pub mmap_next: u64,
    /// Physical frames owned by this address space (for cleanup)
    pub owned_frames: Vec<u64>,
    /// Pages marked as copy-on-write (maps vaddr -> original phys frame)
    pub cow_pages: BTreeMap<u64, u64>,
}

impl AddressSpace {
    /// Create a new empty address space for a process
    pub fn new(pid: Pid) -> Option<Self> {
        // Allocate a new L4 page table frame
        let cr3 = allocate_page_table_frame()?;

        // Copy kernel mappings from the current (kernel) page table to the new one
        unsafe { clone_kernel_mappings(cr3) };

        // Use ASLR-randomized layout
        let heap = aslr_heap_start();
        let mmap = aslr_mmap_end();

        Some(Self {
            pid,
            cr3,
            vmas: BTreeMap::new(),
            brk: heap,
            mmap_next: mmap,
            owned_frames: Vec::new(),
            cow_pages: BTreeMap::new(),
        })
    }

    /// Add a VMA to this address space
    pub fn add_vma(&mut self, vma: VirtualMemoryArea) {
        self.vmas.insert(vma.start, vma);
    }

    /// Remove a VMA by start address
    pub fn remove_vma(&mut self, start: u64) -> Option<VirtualMemoryArea> {
        self.vmas.remove(&start)
    }

    /// Find VMA containing the given address
    pub fn find_vma(&self, addr: u64) -> Option<&VirtualMemoryArea> {
        // BTreeMap range search: find the VMA whose start <= addr
        if let Some((_, vma)) = self.vmas.range(..=addr).next_back() {
            if vma.contains(addr) {
                return Some(vma);
            }
        }
        None
    }

    /// Find mutable VMA containing the given address
    pub fn find_vma_mut(&mut self, addr: u64) -> Option<&mut VirtualMemoryArea> {
        let start = if let Some((&s, vma)) = self.vmas.range(..=addr).next_back() {
            if vma.contains(addr) { Some(s) } else { None }
        } else {
            None
        };
        start.and_then(move |s| self.vmas.get_mut(&s))
    }

    /// Map anonymous pages into this address space
    pub fn mmap_anonymous(
        &mut self,
        addr: u64,
        size: u64,
        prot: ProtFlags,
        flags: MmapFlags,
    ) -> Option<u64> {
        let size = page_align_up(size);
        let num_pages = size / PAGE_SIZE;

        // Determine the virtual address
        let vaddr = if flags.fixed && addr != 0 {
            page_align_down(addr)
        } else if addr != 0 {
            // Hint address - try to use it
            let aligned = page_align_down(addr);
            if self.is_region_free(aligned, size) {
                aligned
            } else {
                self.allocate_mmap_region(size)?
            }
        } else {
            self.allocate_mmap_region(size)?
        };

        // Allocate and map physical frames — or defer to page fault (demand paging)
        if flags.populate {
            // MAP_POPULATE: eagerly allocate all pages now
            for i in 0..num_pages {
                let page_addr = vaddr + i * PAGE_SIZE;
                if let Some(frame_phys) = allocate_physical_frame() {
                    self.owned_frames.push(frame_phys);
                    unsafe {
                        map_page_in_table(self.cr3, page_addr, frame_phys, prot.to_page_flags());
                        // Zero the page
                        zero_physical_frame(frame_phys);
                    }
                } else {
                    serial_println!("[VMM] OOM: failed to allocate frame for mmap");
                    return None;
                }
            }
        }
        // else: demand paging — pages are not mapped yet; a page fault on first
        // access will allocate and map each page lazily via handle_demand_fault().

        // Track the VMA
        self.add_vma(VirtualMemoryArea {
            start: vaddr,
            end: vaddr + size,
            prot,
            mapping_type: if flags.shared {
                MappingType::Shared
            } else {
                MappingType::Anonymous
            },
            flags,
            file_path: None,
            file_offset: 0,
            cow: false,
            ref_count: 1,
        });

        Some(vaddr)
    }

    /// Unmap pages from this address space
    pub fn munmap(&mut self, addr: u64, size: u64) -> bool {
        let addr = page_align_down(addr);
        let size = page_align_up(size);
        let end = addr + size;

        // Find and remove overlapping VMAs
        let to_remove: Vec<u64> = self
            .vmas
            .iter()
            .filter(|(_, vma)| vma.start < end && vma.end > addr)
            .map(|(&start, _)| start)
            .collect();

        for start in to_remove {
            if let Some(vma) = self.vmas.remove(&start) {
                // Unmap pages in the overlap
                let unmap_start = vma.start.max(addr);
                let unmap_end = vma.end.min(end);
                let pages = (unmap_end - unmap_start) / PAGE_SIZE;

                for i in 0..pages {
                    let page_addr = unmap_start + i * PAGE_SIZE;
                    unsafe { unmap_page_in_table(self.cr3, page_addr) };
                }

                // If the VMA extends beyond the unmap region, split it
                if vma.start < addr {
                    self.add_vma(VirtualMemoryArea {
                        start: vma.start,
                        end: addr,
                        ..vma.clone()
                    });
                }
                if vma.end > end {
                    self.add_vma(VirtualMemoryArea {
                        start: end,
                        end: vma.end,
                        ..vma.clone()
                    });
                }
            }
        }
        true
    }

    /// Change protection of a memory region
    pub fn mprotect(&mut self, addr: u64, size: u64, prot: ProtFlags) -> bool {
        let addr = page_align_down(addr);
        let size = page_align_up(size);
        let end = addr + size;

        if let Some(vma) = self.find_vma_mut(addr) {
            if vma.start <= addr && vma.end >= end {
                vma.prot = prot;
                // Update page table flags for all pages in range
                let pages = size / PAGE_SIZE;
                for i in 0..pages {
                    let page_addr = addr + i * PAGE_SIZE;
                    unsafe {
                        update_page_flags_in_table(self.cr3, page_addr, prot.to_page_flags());
                    }
                }
                return true;
            }
        }
        false
    }

    /// Extend the program break (brk syscall)
    pub fn brk(&mut self, new_brk: u64) -> u64 {
        if new_brk == 0 {
            return self.brk;
        }

        let new_brk = page_align_up(new_brk);
        if new_brk > HEAP_MAX {
            return self.brk; // Too large
        }

        if new_brk > self.brk {
            // Expanding — allocate new pages
            let old_end = page_align_up(self.brk);
            let new_end = page_align_up(new_brk);
            let pages = (new_end - old_end) / PAGE_SIZE;

            for i in 0..pages {
                let page_addr = old_end + i * PAGE_SIZE;
                if let Some(frame) = allocate_physical_frame() {
                    self.owned_frames.push(frame);
                    unsafe {
                        map_page_in_table(
                            self.cr3,
                            page_addr,
                            frame,
                            PageTableFlags::PRESENT
                                | PageTableFlags::WRITABLE
                                | PageTableFlags::USER_ACCESSIBLE
                                | PageTableFlags::NO_EXECUTE,
                        );
                        zero_physical_frame(frame);
                    }
                } else {
                    return self.brk; // OOM
                }
            }
        } else if new_brk < self.brk {
            // Shrinking — free pages
            let old_end = page_align_up(self.brk);
            let new_end = page_align_up(new_brk);
            let pages = (old_end - new_end) / PAGE_SIZE;

            for i in 0..pages {
                let page_addr = new_end + i * PAGE_SIZE;
                unsafe { unmap_page_in_table(self.cr3, page_addr) };
            }
        }

        self.brk = new_brk;
        self.brk
    }

    /// Map a user-mode stack with guard page
    pub fn map_stack(&mut self, stack_top: u64, stack_size: u64) -> Option<u64> {
        let stack_pages = stack_size / PAGE_SIZE;
        let guard_pages = STACK_GUARD_PAGES;
        let total_pages = stack_pages + guard_pages;
        let stack_bottom = stack_top - total_pages * PAGE_SIZE;

        // Guard page (present but not accessible — will fault)
        // Don't actually map it; just track it
        self.add_vma(VirtualMemoryArea {
            start: stack_bottom,
            end: stack_bottom + guard_pages * PAGE_SIZE,
            prot: ProtFlags::NONE,
            mapping_type: MappingType::Guard,
            flags: MmapFlags {
                shared: false,
                anonymous: true,
                fixed: true,
                populate: false,
            },
            file_path: None,
            file_offset: 0,
            cow: false,
            ref_count: 1,
        });

        // Stack pages
        let stack_start = stack_bottom + guard_pages * PAGE_SIZE;
        for i in 0..stack_pages {
            let page_addr = stack_start + i * PAGE_SIZE;
            let frame = allocate_physical_frame()?;
            self.owned_frames.push(frame);
            unsafe {
                map_page_in_table(
                    self.cr3,
                    page_addr,
                    frame,
                    PageTableFlags::PRESENT
                        | PageTableFlags::WRITABLE
                        | PageTableFlags::USER_ACCESSIBLE
                        | PageTableFlags::NO_EXECUTE,
                );
                zero_physical_frame(frame);
            }
        }

        self.add_vma(VirtualMemoryArea {
            start: stack_start,
            end: stack_top,
            prot: ProtFlags::RW,
            mapping_type: MappingType::Stack,
            flags: MmapFlags {
                shared: false,
                anonymous: true,
                fixed: true,
                populate: true,
            },
            file_path: None,
            file_offset: 0,
            cow: false,
            ref_count: 1,
        });

        Some(stack_top)
    }

    /// Fork this address space (copy-on-write)
    pub fn fork(&self, child_pid: Pid) -> Option<AddressSpace> {
        let mut child = AddressSpace::new(child_pid)?;
        child.brk = self.brk;
        child.mmap_next = self.mmap_next;

        // For each VMA, create CoW mappings
        for vma in self.vmas.values() {
            if vma.mapping_type == MappingType::Guard {
                // Just copy the VMA metadata, don't map anything
                child.add_vma(vma.clone());
                continue;
            }

            let mut child_vma = vma.clone();
            child_vma.cow = true;
            child_vma.ref_count = 2;

            // Mark parent pages as read-only (CoW)
            let pages = vma.page_count();
            for i in 0..pages {
                let page_addr = vma.start + i * PAGE_SIZE;

                // Get the physical frame from parent's page table
                if let Some(phys_frame) = unsafe { get_mapped_frame(self.cr3, page_addr) } {
                    // Map same frame in child as read-only
                    let cow_flags = PageTableFlags::PRESENT
                        | PageTableFlags::USER_ACCESSIBLE
                        | PageTableFlags::NO_EXECUTE;
                    unsafe {
                        map_page_in_table(child.cr3, page_addr, phys_frame, cow_flags);
                    }

                    // Also make parent page read-only
                    unsafe {
                        update_page_flags_in_table(self.cr3, page_addr, cow_flags);
                    }

                    // Track CoW pages
                    child.cow_pages.insert(page_addr, phys_frame);
                }
            }

            child.add_vma(child_vma);
        }

        // Flush TLB after modifying parent page tables
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!("mov cr3, {}", in(reg) self.cr3, options(nostack, preserves_flags));
        }

        Some(child)
    }

    /// Handle a copy-on-write page fault
    /// Returns true if the fault was handled (was a CoW page)
    pub fn handle_cow_fault(&mut self, fault_addr: u64) -> bool {
        let page_addr = page_align_down(fault_addr);

        if let Some(&original_frame) = self.cow_pages.get(&page_addr) {
            // Allocate a new frame
            if let Some(new_frame) = allocate_physical_frame() {
                self.owned_frames.push(new_frame);

                // Copy data from original frame to new frame
                unsafe {
                    copy_physical_frame(original_frame, new_frame);
                }

                // Get the correct flags from the VMA
                let flags = if let Some(vma) = self.find_vma(page_addr) {
                    vma.prot.to_page_flags()
                } else {
                    PageTableFlags::PRESENT
                        | PageTableFlags::WRITABLE
                        | PageTableFlags::USER_ACCESSIBLE
                };

                // Remap with write permission
                unsafe {
                    unmap_page_in_table(self.cr3, page_addr);
                    map_page_in_table(self.cr3, page_addr, new_frame, flags);
                }

                // Remove from CoW tracking
                self.cow_pages.remove(&page_addr);
                return true;
            }
        }
        false
    }

    /// Handle a demand-paging fault: the page belongs to a valid VMA but has
    /// no physical backing yet (lazy allocation from mmap without MAP_POPULATE).
    /// Returns true if a new frame was allocated and mapped.
    pub fn handle_demand_fault(&mut self, fault_addr: u64) -> bool {
        let page_addr = page_align_down(fault_addr);

        // Check if this page lies inside a VMA that could be lazily backed
        let (prot_flags, mapping_type) = if let Some(vma) = self.find_vma(page_addr) {
            // Guard pages must NOT be demand-faulted (that's a stack overflow)
            if vma.mapping_type == MappingType::Guard {
                return false;
            }
            (vma.prot.to_page_flags(), vma.mapping_type)
        } else {
            return false; // No VMA covers this address
        };

        // Only demand-fault Anonymous / Data / Heap / Shared pages
        match mapping_type {
            MappingType::Anonymous
            | MappingType::Data
            | MappingType::Heap
            | MappingType::Shared
            | MappingType::FileBacked
            | MappingType::Stack => {}
            _ => return false,
        }

        // Check if there's already a physical frame mapped
        let already_mapped = unsafe { get_mapped_frame(self.cr3, page_addr).is_some() };
        if already_mapped {
            return false; // Page is present — fault is something else
        }

        // Allocate a new zeroed frame and map it
        if let Some(frame) = allocate_physical_frame() {
            self.owned_frames.push(frame);
            unsafe {
                map_page_in_table(self.cr3, page_addr, frame, prot_flags);
                zero_physical_frame(frame);
            }
            serial_println!(
                "[VMM] Demand-paged {:#x} (type={:?})",
                page_addr,
                mapping_type
            );
            return true;
        }

        serial_println!("[VMM] Demand-page OOM for {:#x}", page_addr);
        false
    }

    /// Check if a region is free (no overlapping VMAs)
    fn is_region_free(&self, start: u64, size: u64) -> bool {
        let end = start + size;
        for vma in self.vmas.values() {
            if vma.start < end && vma.end > start {
                return false;
            }
        }
        true
    }

    /// Allocate a region in the mmap area (top-down)
    fn allocate_mmap_region(&mut self, size: u64) -> Option<u64> {
        let size = page_align_up(size);
        if self.mmap_next < MMAP_REGION_START + size {
            return None; // Out of mmap space
        }

        self.mmap_next -= size;
        let addr = self.mmap_next;

        // Verify no overlap
        if self.is_region_free(addr, size) {
            Some(addr)
        } else {
            // Try to find another spot
            let mut candidate = addr;
            while candidate >= MMAP_REGION_START + size {
                candidate -= PAGE_SIZE;
                if self.is_region_free(candidate, size) {
                    self.mmap_next = candidate;
                    return Some(candidate);
                }
            }
            None
        }
    }

    /// Get process memory map (for /proc/[pid]/maps)
    pub fn get_maps_string(&self) -> String {
        let mut s = String::new();
        for vma in self.vmas.values() {
            let perms = alloc::format!(
                "{}{}{}p",
                if vma.prot.read { "r" } else { "-" },
                if vma.prot.write { "w" } else { "-" },
                if vma.prot.execute { "x" } else { "-" },
            );
            let label = match vma.mapping_type {
                MappingType::Text => "[text]",
                MappingType::Data => "[data]",
                MappingType::Heap => "[heap]",
                MappingType::Stack => "[stack]",
                MappingType::Guard => "[guard]",
                MappingType::Anonymous => "[anon]",
                MappingType::FileBacked => "[file]",
                MappingType::Shared => "[shared]",
                MappingType::Kernel => "[kernel]",
            };
            s.push_str(&alloc::format!(
                "{:016x}-{:016x} {} {} {}\n",
                vma.start,
                vma.end,
                perms,
                if vma.cow { "cow" } else { "   " },
                label
            ));
        }
        s
    }

    /// Total virtual memory used by this process (in bytes)
    pub fn total_vm_size(&self) -> u64 {
        self.vmas.values().map(|vma| vma.end - vma.start).sum()
    }

    /// Total physical memory (RSS) used by this process
    pub fn total_rss(&self) -> u64 {
        self.owned_frames.len() as u64 * PAGE_SIZE
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        // Free all owned physical frames
        for &frame_phys in &self.owned_frames {
            free_physical_frame(frame_phys);
        }
        // Free the page table itself
        if self.cr3 != 0 {
            free_physical_frame(self.cr3);
        }
    }
}

// ─── Global Address Space Table ────────────────────────────────────────

lazy_static::lazy_static! {
    /// Global table of per-process address spaces
    pub static ref ADDRESS_SPACES: Mutex<BTreeMap<Pid, AddressSpace>> = Mutex::new(BTreeMap::new());
}

/// Physical memory offset (set during init)
static PHYS_MEM_OFFSET: AtomicU64 = AtomicU64::new(0);

// ─── Physical Frame Allocator (Global) ─────────────────────────────────

/// Physical buddy allocator (order 0 = 4 KiB … order 10 = 4 MiB).
///
/// Frames are merged with their buddy on free so alloc/free of the 32 MiB
/// VMM pool does not leak. Callers still allocate one page at a time.
const BUDDY_MAX_ORDER: usize = 10;

pub struct PhysicalFramePool {
    /// Free blocks of size `PAGE_SIZE << order`
    free: [Vec<u64>; BUDDY_MAX_ORDER + 1],
    total_frames: u64,
    allocated_frames: u64,
}

impl Default for PhysicalFramePool {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicalFramePool {
    pub fn new() -> Self {
        Self {
            free: core::array::from_fn(|_| Vec::new()),
            total_frames: 0,
            allocated_frames: 0,
        }
    }

    fn block_size(order: usize) -> u64 {
        PAGE_SIZE << order
    }

    fn buddy_addr(addr: u64, order: usize) -> u64 {
        addr ^ Self::block_size(order)
    }

    fn take_block(&mut self, order: usize) -> Option<u64> {
        if order > BUDDY_MAX_ORDER {
            return None;
        }
        if let Some(addr) = self.free[order].pop() {
            return Some(addr);
        }
        let larger = self.take_block(order + 1)?;
        let half = Self::block_size(order);
        self.free[order].push(larger + half);
        Some(larger)
    }

    fn free_block(&mut self, addr: u64, order: usize) {
        if order < BUDDY_MAX_ORDER {
            let buddy = Self::buddy_addr(addr, order);
            if let Some(pos) = self.free[order].iter().position(|&a| a == buddy) {
                self.free[order].swap_remove(pos);
                self.free_block(addr.min(buddy), order + 1);
                return;
            }
        }
        self.free[order].push(addr);
    }

    /// Add a 4 KiB frame to the pool (merges with its buddy when present).
    pub fn add_frame(&mut self, phys_addr: u64) {
        self.free_block(phys_addr & !0xFFF, 0);
        self.total_frames += 1;
    }

    /// Allocate a 4 KiB physical frame
    pub fn allocate(&mut self) -> Option<u64> {
        let addr = self.take_block(0)?;
        self.allocated_frames += 1;
        Some(addr)
    }

    /// Free a 4 KiB physical frame back to the pool
    pub fn free(&mut self, phys_addr: u64) {
        self.free_block(phys_addr & !0xFFF, 0);
        if self.allocated_frames > 0 {
            self.allocated_frames -= 1;
        }
    }

    /// Free 4 KiB pages currently sitting on free lists
    pub fn available(&self) -> u64 {
        let mut n = 0u64;
        for (order, list) in self.free.iter().enumerate() {
            n += list.len() as u64 * (1u64 << order);
        }
        n
    }

    pub fn total(&self) -> u64 {
        self.total_frames
    }

    pub fn allocated(&self) -> u64 {
        self.allocated_frames
    }
}

lazy_static::lazy_static! {
    pub static ref FRAME_POOL: Mutex<PhysicalFramePool> = Mutex::new(PhysicalFramePool::new());
}

/// Allocate a physical frame from the global pool
pub fn allocate_physical_frame() -> Option<u64> {
    FRAME_POOL.lock().allocate()
}

/// Free a physical frame back to the global pool
pub fn free_physical_frame(phys_addr: u64) {
    FRAME_POOL.lock().free(phys_addr);
}

/// Allocate a zeroed frame for a new page table
fn allocate_page_table_frame() -> Option<u64> {
    let frame = allocate_physical_frame()?;
    unsafe { zero_physical_frame(frame) };
    Some(frame)
}

// ─── Page Table Manipulation ────────────────────────────────────────────

/// Map a single page in a process's page table
///
/// # Safety
/// The cr3 value and physical frame must be valid.
unsafe fn map_page_in_table(cr3: u64, vaddr: u64, phys_frame: u64, flags: PageTableFlags) {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return; // Not initialized yet
    }

    // Walk the 4-level page table, creating intermediate tables as needed
    let l4_table = &mut *((offset + cr3) as *mut PageTable);

    let l4_idx = ((vaddr >> 39) & 0x1FF) as usize;
    let l3_idx = ((vaddr >> 30) & 0x1FF) as usize;
    let l2_idx = ((vaddr >> 21) & 0x1FF) as usize;
    let l1_idx = ((vaddr >> 12) & 0x1FF) as usize;

    // L4 -> L3
    if l4_table[l4_idx].is_unused() {
        let new_table = allocate_page_table_frame();
        if let Some(pt_phys) = new_table {
            l4_table[l4_idx].set_addr(
                PhysAddr::new(pt_phys),
                PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::USER_ACCESSIBLE,
            );
        } else {
            return;
        }
    }
    let l3_phys = l4_table[l4_idx].addr().as_u64();
    let l3_table = &mut *((offset + l3_phys) as *mut PageTable);

    // L3 -> L2
    if l3_table[l3_idx].is_unused() {
        let new_table = allocate_page_table_frame();
        if let Some(pt_phys) = new_table {
            l3_table[l3_idx].set_addr(
                PhysAddr::new(pt_phys),
                PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::USER_ACCESSIBLE,
            );
        } else {
            return;
        }
    }
    let l2_phys = l3_table[l3_idx].addr().as_u64();
    let l2_table = &mut *((offset + l2_phys) as *mut PageTable);

    // L2 -> L1
    if l2_table[l2_idx].is_unused() {
        let new_table = allocate_page_table_frame();
        if let Some(pt_phys) = new_table {
            l2_table[l2_idx].set_addr(
                PhysAddr::new(pt_phys),
                PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::USER_ACCESSIBLE,
            );
        } else {
            return;
        }
    }
    let l1_phys = l2_table[l2_idx].addr().as_u64();
    let l1_table = &mut *((offset + l1_phys) as *mut PageTable);

    // Map the actual page
    l1_table[l1_idx].set_addr(PhysAddr::new(phys_frame), flags);
}

/// Unmap a page from a process's page table
unsafe fn unmap_page_in_table(cr3: u64, vaddr: u64) {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return;
    }

    let l4_table = &mut *((offset + cr3) as *mut PageTable);
    let l4_idx = ((vaddr >> 39) & 0x1FF) as usize;
    if l4_table[l4_idx].is_unused() {
        return;
    }

    let l3_phys = l4_table[l4_idx].addr().as_u64();
    let l3_table = &mut *((offset + l3_phys) as *mut PageTable);
    let l3_idx = ((vaddr >> 30) & 0x1FF) as usize;
    if l3_table[l3_idx].is_unused() {
        return;
    }

    let l2_phys = l3_table[l3_idx].addr().as_u64();
    let l2_table = &mut *((offset + l2_phys) as *mut PageTable);
    let l2_idx = ((vaddr >> 21) & 0x1FF) as usize;
    if l2_table[l2_idx].is_unused() {
        return;
    }

    let l1_phys = l2_table[l2_idx].addr().as_u64();
    let l1_table = &mut *((offset + l1_phys) as *mut PageTable);
    let l1_idx = ((vaddr >> 12) & 0x1FF) as usize;

    l1_table[l1_idx].set_unused();

    // Invalidate TLB for this page
    crate::arch_compat::instructions::tlb::flush(VirtAddr::new(vaddr));
}

/// Update page flags without changing the mapping
unsafe fn update_page_flags_in_table(cr3: u64, vaddr: u64, flags: PageTableFlags) {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return;
    }

    let l4_table = &mut *((offset + cr3) as *mut PageTable);
    let l4_idx = ((vaddr >> 39) & 0x1FF) as usize;
    if l4_table[l4_idx].is_unused() {
        return;
    }

    let l3_phys = l4_table[l4_idx].addr().as_u64();
    let l3_table = &mut *((offset + l3_phys) as *mut PageTable);
    let l3_idx = ((vaddr >> 30) & 0x1FF) as usize;
    if l3_table[l3_idx].is_unused() {
        return;
    }

    let l2_phys = l3_table[l3_idx].addr().as_u64();
    let l2_table = &mut *((offset + l2_phys) as *mut PageTable);
    let l2_idx = ((vaddr >> 21) & 0x1FF) as usize;
    if l2_table[l2_idx].is_unused() {
        return;
    }

    let l1_phys = l2_table[l2_idx].addr().as_u64();
    let l1_table = &mut *((offset + l1_phys) as *mut PageTable);
    let l1_idx = ((vaddr >> 12) & 0x1FF) as usize;

    if !l1_table[l1_idx].is_unused() {
        let phys = l1_table[l1_idx].addr();
        l1_table[l1_idx].set_addr(phys, flags);
        crate::arch_compat::instructions::tlb::flush(VirtAddr::new(vaddr));
    }
}

/// Get the physical frame mapped at a virtual address in a given page table
unsafe fn get_mapped_frame(cr3: u64, vaddr: u64) -> Option<u64> {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return None;
    }

    let l4_table = &*((offset + cr3) as *const PageTable);
    let l4_idx = ((vaddr >> 39) & 0x1FF) as usize;
    if l4_table[l4_idx].is_unused() {
        return None;
    }

    let l3_phys = l4_table[l4_idx].addr().as_u64();
    let l3_table = &*((offset + l3_phys) as *const PageTable);
    let l3_idx = ((vaddr >> 30) & 0x1FF) as usize;
    if l3_table[l3_idx].is_unused() {
        return None;
    }

    let l2_phys = l3_table[l3_idx].addr().as_u64();
    let l2_table = &*((offset + l2_phys) as *const PageTable);
    let l2_idx = ((vaddr >> 21) & 0x1FF) as usize;
    if l2_table[l2_idx].is_unused() {
        return None;
    }

    let l1_phys = l2_table[l2_idx].addr().as_u64();
    let l1_table = &*((offset + l1_phys) as *const PageTable);
    let l1_idx = ((vaddr >> 12) & 0x1FF) as usize;
    if l1_table[l1_idx].is_unused() {
        return None;
    }

    Some(l1_table[l1_idx].addr().as_u64())
}

/// Clone the kernel's upper-half page table entries into a new L4 table
unsafe fn clone_kernel_mappings(new_cr3: u64) {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return;
    }

    // Get the current kernel page table
    let (kernel_l4_frame, _) = Cr3::read();
    let kernel_l4_phys = kernel_l4_frame.start_address().as_u64();
    let kernel_l4 = &*((offset + kernel_l4_phys) as *const PageTable);

    let new_l4 = &mut *((offset + new_cr3) as *mut PageTable);

    // Copy entries 256-511 (upper half = kernel space)
    // These are shared across all processes
    for i in 256..512 {
        new_l4[i] = kernel_l4[i].clone();
    }
}

/// Zero a physical frame (accessed via physical memory mapping)
unsafe fn zero_physical_frame(phys_addr: u64) {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return;
    }
    let ptr = (offset + phys_addr) as *mut u8;
    core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize);
}

/// Copy one physical frame to another
unsafe fn copy_physical_frame(src_phys: u64, dst_phys: u64) {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return;
    }
    let src = (offset + src_phys) as *const u8;
    let dst = (offset + dst_phys) as *mut u8;
    core::ptr::copy_nonoverlapping(src, dst, PAGE_SIZE as usize);
}

// ─── Helper Functions ───────────────────────────────────────────────────

/// Align address down to page boundary
pub fn page_align_down(addr: u64) -> u64 {
    addr & !(PAGE_SIZE - 1)
}

/// Align address up to page boundary
pub fn page_align_up(addr: u64) -> u64 {
    (addr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

// ─── Public API ─────────────────────────────────────────────────────────

/// Create an address space for a new process
pub fn create_address_space(pid: Pid) -> bool {
    if let Some(addr_space) = AddressSpace::new(pid) {
        ADDRESS_SPACES.lock().insert(pid, addr_space);
        serial_println!("[VMM] Created address space for PID {}", pid);
        true
    } else {
        serial_println!("[VMM] Failed to create address space for PID {}", pid);
        false
    }
}

/// Destroy a process's address space
pub fn destroy_address_space(pid: Pid) {
    ADDRESS_SPACES.lock().remove(&pid);
    serial_println!("[VMM] Destroyed address space for PID {}", pid);
}

/// Fork a process's address space (CoW)
pub fn fork_address_space(parent_pid: Pid, child_pid: Pid) -> bool {
    let spaces = ADDRESS_SPACES.lock();
    if let Some(parent) = spaces.get(&parent_pid) {
        if let Some(child_space) = parent.fork(child_pid) {
            drop(spaces);
            ADDRESS_SPACES.lock().insert(child_pid, child_space);
            serial_println!(
                "[VMM] Forked address space: PID {} -> PID {}",
                parent_pid,
                child_pid
            );
            return true;
        }
    }
    false
}

/// Handle a page fault for a process (check CoW, demand paging, stack growth)
pub fn handle_page_fault(pid: Pid, fault_addr: u64, is_write: bool) -> bool {
    let mut spaces = ADDRESS_SPACES.lock();
    if let Some(addr_space) = spaces.get_mut(&pid) {
        // 1. Check if it's a CoW fault (write to a shared read-only page)
        if is_write && addr_space.handle_cow_fault(fault_addr) {
            return true;
        }

        // 2. Check demand paging — page in a valid VMA but not yet backed
        if addr_space.handle_demand_fault(fault_addr) {
            return true;
        }

        // 3. Check if it's a stack guard page hit (stack overflow)
        if let Some(vma) = addr_space.find_vma(fault_addr) {
            if vma.mapping_type == MappingType::Guard {
                serial_println!(
                    "[VMM] Stack overflow detected for PID {} at {:#x}",
                    pid,
                    fault_addr
                );
                return false; // Stack overflow!
            }
        }
    }
    false
}

/// Get a process's CR3 value
pub fn get_cr3(pid: Pid) -> Option<u64> {
    ADDRESS_SPACES.lock().get(&pid).map(|s| s.cr3)
}

/// Perform mmap for a process
pub fn mmap(pid: Pid, addr: u64, size: u64, prot: u64, flags: u64) -> i64 {
    let prot_flags = ProtFlags::from_mmap_prot(prot);
    let mmap_flags = MmapFlags::from_linux(flags);

    let mut spaces = ADDRESS_SPACES.lock();
    if let Some(addr_space) = spaces.get_mut(&pid) {
        if let Some(mapped_addr) = addr_space.mmap_anonymous(addr, size, prot_flags, mmap_flags) {
            return mapped_addr as i64;
        }
    }
    -12 // ENOMEM
}

/// Perform munmap for a process
pub fn munmap(pid: Pid, addr: u64, size: u64) -> i64 {
    let mut spaces = ADDRESS_SPACES.lock();
    if let Some(addr_space) = spaces.get_mut(&pid) {
        if addr_space.munmap(addr, size) {
            return 0;
        }
    }
    -22 // EINVAL
}

/// Perform brk for a process
pub fn brk(pid: Pid, new_brk: u64) -> i64 {
    let mut spaces = ADDRESS_SPACES.lock();
    if let Some(addr_space) = spaces.get_mut(&pid) {
        addr_space.brk(new_brk) as i64
    } else {
        -12 // ENOMEM
    }
}

// ─── Public API for other modules (vDSO, etc.) ────────────────────────

/// Get the physical memory offset (public access for vDSO etc.)
pub fn get_phys_mem_offset() -> u64 {
    PHYS_MEM_OFFSET.load(Ordering::Relaxed)
}

/// Public wrapper to map a page in a process's page table.
///
/// # Safety
/// cr3, vaddr, and phys_frame must all be valid.
pub unsafe fn map_page_in_table_pub(cr3: u64, vaddr: u64, phys_frame: u64, flags: PageTableFlags) {
    map_page_in_table(cr3, vaddr, phys_frame, flags);
}

/// Memory region info for /proc/[pid]/maps
pub struct MemRegionInfo {
    pub start: u64,
    pub end: u64,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub private: bool,
    pub offset: u64,
    pub name: String,
}

impl MemRegionInfo {
    pub fn perms_str(&self) -> String {
        alloc::format!(
            "{}{}{}{}",
            if self.readable { "r" } else { "-" },
            if self.writable { "w" } else { "-" },
            if self.executable { "x" } else { "-" },
            if self.private { "p" } else { "s" },
        )
    }
}

/// Get memory regions for a process (for /proc/[pid]/maps)
pub fn get_memory_regions(pid: Pid) -> Vec<MemRegionInfo> {
    let spaces = ADDRESS_SPACES.lock();
    let space = match spaces.get(&pid) {
        Some(s) => s,
        None => return Vec::new(),
    };

    let mut regions = Vec::new();

    // Heap region (from HEAP_START to brk)
    if space.brk > HEAP_START {
        regions.push(MemRegionInfo {
            start: HEAP_START,
            end: space.brk,
            readable: true,
            writable: true,
            executable: false,
            private: true,
            offset: 0,
            name: String::from("[heap]"),
        });
    }

    // VMA regions (mmap, program segments, etc.)
    for vma in space.vmas.values() {
        regions.push(MemRegionInfo {
            start: vma.start,
            end: vma.end,
            readable: vma.prot.read,
            writable: vma.prot.write,
            executable: vma.prot.execute,
            private: !vma.flags.shared,
            offset: vma.file_offset,
            name: vma.file_path.clone().unwrap_or_default(),
        });
    }

    // Stack region (default range)
    let stack_bottom = STACK_TOP - STACK_SIZE;
    regions.push(MemRegionInfo {
        start: stack_bottom,
        end: STACK_TOP,
        readable: true,
        writable: true,
        executable: false,
        private: true,
        offset: 0,
        name: String::from("[stack]"),
    });

    regions.sort_by_key(|r| r.start);
    regions
}

/// Initialize the VMM subsystem
pub fn init(phys_mem_offset: u64) {
    PHYS_MEM_OFFSET.store(phys_mem_offset, Ordering::Relaxed);

    // Initialize ASLR seed from CPU timestamp counter
    init_aslr_seed();

    serial_println!("[VMM] Virtual Memory Manager initialized");
    serial_println!("[VMM]   Physical memory offset: {:#x}", phys_mem_offset);
    serial_println!(
        "[VMM]   User space: {:#x}-{:#x}",
        USER_SPACE_START,
        USER_SPACE_END
    );
    serial_println!("[VMM]   Heap range: {:#x}-{:#x}", HEAP_START, HEAP_MAX);
    serial_println!(
        "[VMM]   mmap range: {:#x}-{:#x}",
        MMAP_REGION_START,
        MMAP_REGION_END
    );
    serial_println!(
        "[VMM]   Stack top: {:#x} ({}MB)",
        STACK_TOP,
        STACK_SIZE / 1024 / 1024
    );
    serial_println!(
        "[VMM]   ASLR: {}",
        if ASLR_ENABLED { "enabled" } else { "disabled" }
    );
}

/// Pre-allocate physical frames into the VMM pool
/// Called after the bootloader frame allocator is available
pub fn populate_frame_pool(
    frame_allocator: &mut impl crate::arch_compat::structures::paging::FrameAllocator<Size4KiB>,
    count: usize,
) {
    let mut pool = FRAME_POOL.lock();
    let mut allocated = 0;
    for _ in 0..count {
        if let Some(frame) = frame_allocator.allocate_frame() {
            pool.add_frame(frame.start_address().as_u64());
            allocated += 1;
        } else {
            break;
        }
    }
    serial_println!(
        "[VMM] Buddy frame pool: {} frames pre-allocated ({} KB, {} free)",
        allocated,
        allocated * 4,
        pool.available() * 4
    );
}

/// Get VMM statistics
pub fn get_stats() -> (u64, u64, u64) {
    let pool = FRAME_POOL.lock();
    (pool.total(), pool.allocated(), pool.available())
}

// ─── User Memory Access (for signal frame) ─────────────────────────────

/// Write data to a process's user-mode virtual address
/// Used by signal delivery to push signal frames onto user stack
pub fn write_user_memory(pid: Pid, vaddr: u64, data: &[u8]) {
    let spaces = ADDRESS_SPACES.lock();
    if let Some(addr_space) = spaces.get(&pid) {
        let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        if offset == 0 {
            return;
        }

        let mut written = 0;
        while written < data.len() {
            let page_vaddr = (vaddr + written as u64) & !0xFFF;
            let page_offset = ((vaddr + written as u64) & 0xFFF) as usize;
            let chunk_size = core::cmp::min(4096 - page_offset, data.len() - written);

            // Walk the page table to find the physical address
            if let Some(phys) = translate_vaddr_for_pid(addr_space.cr3, page_vaddr, offset) {
                let dest = (phys + offset + page_offset as u64) as *mut u8;
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        data[written..written + chunk_size].as_ptr(),
                        dest,
                        chunk_size,
                    );
                }
            }
            written += chunk_size;
        }
    }
}

/// Read data from a process's user-mode virtual address
pub fn read_user_memory(pid: Pid, vaddr: u64, buf: &mut [u8]) {
    let spaces = ADDRESS_SPACES.lock();
    if let Some(addr_space) = spaces.get(&pid) {
        let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        if offset == 0 {
            return;
        }

        let mut read = 0;
        while read < buf.len() {
            let page_vaddr = (vaddr + read as u64) & !0xFFF;
            let page_offset = ((vaddr + read as u64) & 0xFFF) as usize;
            let chunk_size = core::cmp::min(4096 - page_offset, buf.len() - read);

            if let Some(phys) = translate_vaddr_for_pid(addr_space.cr3, page_vaddr, offset) {
                let src = (phys + offset + page_offset as u64) as *const u8;
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        src,
                        buf[read..read + chunk_size].as_mut_ptr(),
                        chunk_size,
                    );
                }
            } else {
                // Page not mapped — fill with zeros
                buf[read..read + chunk_size].fill(0);
            }
            read += chunk_size;
        }
    }
}

/// Translate a virtual address to physical for a given CR3
fn translate_vaddr_for_pid(cr3: u64, vaddr: u64, phys_offset: u64) -> Option<u64> {
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::structures::paging::PageTable;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::structures::paging::page_table::PageTable;

    let l4_idx = ((vaddr >> 39) & 0x1FF) as usize;
    let l3_idx = ((vaddr >> 30) & 0x1FF) as usize;
    let l2_idx = ((vaddr >> 21) & 0x1FF) as usize;
    let l1_idx = ((vaddr >> 12) & 0x1FF) as usize;

    unsafe {
        let l4_table = &*((cr3 + phys_offset) as *const PageTable);
        let l4_entry = &l4_table[l4_idx];
        if l4_entry.is_unused() {
            return None;
        }

        let l3_phys = l4_entry.addr().as_u64();
        let l3_table = &*((l3_phys + phys_offset) as *const PageTable);
        let l3_entry = &l3_table[l3_idx];
        if l3_entry.is_unused() {
            return None;
        }
        if l3_entry
            .flags()
            .contains(crate::arch_compat::structures::paging::PageTableFlags::HUGE_PAGE)
        {
            // 1 GiB page
            return Some(l3_entry.addr().as_u64() + (vaddr & 0x3FFFFFFF));
        }

        let l2_phys = l3_entry.addr().as_u64();
        let l2_table = &*((l2_phys + phys_offset) as *const PageTable);
        let l2_entry = &l2_table[l2_idx];
        if l2_entry.is_unused() {
            return None;
        }
        if l2_entry
            .flags()
            .contains(crate::arch_compat::structures::paging::PageTableFlags::HUGE_PAGE)
        {
            // 2 MiB page
            return Some(l2_entry.addr().as_u64() + (vaddr & 0x1FFFFF));
        }

        let l1_phys = l2_entry.addr().as_u64();
        let l1_table = &*((l1_phys + phys_offset) as *const PageTable);
        let l1_entry = &l1_table[l1_idx];
        if l1_entry.is_unused() {
            return None;
        }

        Some(l1_entry.addr().as_u64())
    }
}

// ─── ELF Loading into Address Space ─────────────────────────────────────

/// Load ELF segments into a process's address space
///
/// This maps PT_LOAD segments into the process's per-process page tables,
/// copies file data, zeroes BSS, and sets up the heap break.
///
/// Returns (entry_point, initial_brk) on success.
pub fn load_elf_into_address_space(pid: Pid, elf_data: &[u8]) -> Result<(u64, u64), &'static str> {
    // Parse ELF header
    let header = crate::elf::validate_elf(elf_data).map_err(|_| "invalid ELF binary")?;
    let phdrs = crate::elf::parse_program_headers(elf_data, header);

    let mut spaces = ADDRESS_SPACES.lock();
    let addr_space = spaces.get_mut(&pid).ok_or("no address space for pid")?;

    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return Err("VMM not initialized");
    }

    let mut max_addr: u64 = 0;

    for phdr in &phdrs {
        if phdr.p_type != 1 {
            // Not PT_LOAD
            continue;
        }

        // Determine protection
        let prot = ProtFlags {
            read: phdr.p_flags & 4 != 0,    // PF_R
            write: phdr.p_flags & 2 != 0,   // PF_W
            execute: phdr.p_flags & 1 != 0, // PF_X
        };

        let mapping_type = if phdr.p_flags & 1 != 0 {
            MappingType::Text
        } else {
            MappingType::Data
        };

        let seg_start = page_align_down(phdr.p_vaddr);
        let seg_end = page_align_up(phdr.p_vaddr + phdr.p_memsz);
        let num_pages = (seg_end - seg_start) / PAGE_SIZE;

        serial_println!(
            "[VMM] ELF segment: {:#x}-{:#x} ({} pages) {}{}{}",
            seg_start,
            seg_end,
            num_pages,
            if prot.read { "r" } else { "-" },
            if prot.write { "w" } else { "-" },
            if prot.execute { "x" } else { "-" },
        );

        // Allocate and map pages for this segment
        // Use writable for initial load, we'll fix permissions after copying
        let load_flags =
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

        for i in 0..num_pages {
            let page_addr = seg_start + i * PAGE_SIZE;
            let frame_phys = allocate_physical_frame().ok_or("OOM mapping ELF segment")?;
            addr_space.owned_frames.push(frame_phys);

            unsafe {
                map_page_in_table(addr_space.cr3, page_addr, frame_phys, load_flags);
                // Zero the frame first (handles .bss regions)
                zero_physical_frame(frame_phys);
            }

            // Copy file data that overlaps with this page
            let page_start = page_addr;
            let page_end = page_addr + PAGE_SIZE;
            let file_region_start = phdr.p_vaddr;
            let file_region_end = phdr.p_vaddr + phdr.p_filesz;

            let copy_start = page_start.max(file_region_start);
            let copy_end = page_end.min(file_region_end);

            if copy_start < copy_end {
                let file_offset = phdr.p_offset + (copy_start - phdr.p_vaddr);
                let dest_offset_in_frame = copy_start - page_start;
                let copy_len = (copy_end - copy_start) as usize;

                if (file_offset as usize + copy_len) <= elf_data.len() {
                    let src = &elf_data[file_offset as usize..file_offset as usize + copy_len];
                    unsafe {
                        let dest_ptr = (offset + frame_phys + dest_offset_in_frame) as *mut u8;
                        core::ptr::copy_nonoverlapping(src.as_ptr(), dest_ptr, copy_len);
                    }
                }
            }
        }

        // Now set correct permissions (remove writable from .text)
        if !prot.write {
            let final_flags = prot.to_page_flags();
            for i in 0..num_pages {
                let page_addr = seg_start + i * PAGE_SIZE;
                unsafe {
                    update_page_flags_in_table(addr_space.cr3, page_addr, final_flags);
                }
            }
        }

        // Track VMA
        addr_space.add_vma(VirtualMemoryArea {
            start: seg_start,
            end: seg_end,
            prot,
            mapping_type,
            flags: MmapFlags {
                shared: false,
                anonymous: false,
                fixed: true,
                populate: true,
            },
            file_path: None,
            file_offset: phdr.p_offset,
            cow: false,
            ref_count: 1,
        });

        let end = phdr.p_vaddr + phdr.p_memsz;
        if end > max_addr {
            max_addr = end;
        }
    }

    let brk = page_align_up(max_addr);
    addr_space.brk = brk;

    serial_println!(
        "[VMM] ELF loaded for PID {}: entry={:#x} brk={:#x}",
        pid,
        header.e_entry,
        brk
    );

    Ok((header.e_entry, brk))
}

/// Set up a user-mode stack in a process's address space
/// Returns the stack pointer (top of usable stack)
pub fn setup_user_stack(pid: Pid, stack_top: u64, stack_size: u64) -> Option<u64> {
    let mut spaces = ADDRESS_SPACES.lock();
    let addr_space = spaces.get_mut(&pid)?;
    addr_space.map_stack(stack_top, stack_size)
}

/// Set up initial user stack with argc, argv, envp, and auxiliary vector
///
/// Linux x86_64 initial stack layout (growing down from stack_top):
///   [null terminator for strings]
///   [environment strings]
///   [argument strings]
///   [padding for alignment]
///   [auxv entries]
///   [NULL]
///   [envp[n] ... envp[0]]
///   [NULL]
///   [argv[n] ... argv[0]]
///   [argc]          <-- RSP points here
///
/// Returns the initial RSP value.
pub fn setup_initial_stack(
    pid: Pid,
    stack_top: u64,
    argv: &[&str],
    envp: &[&str],
    entry_point: u64,
    phdr_addr: u64,
    phdr_num: u64,
) -> Option<u64> {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return None;
    }

    let spaces = ADDRESS_SPACES.lock();
    let addr_space = spaces.get(&pid)?;
    let cr3 = addr_space.cr3;
    drop(spaces);

    // We need to write into the stack which is mapped in the process's page tables.
    // We'll write through the physical mapping.
    let mut sp = stack_top;

    // Helper: write a u64 onto the stack
    let push_u64 = |sp: &mut u64, val: u64| -> bool {
        *sp -= 8;
        if let Some(phys) = unsafe { get_mapped_frame(cr3, page_align_down(*sp)) } {
            let off_in_page = *sp & (PAGE_SIZE - 1);
            unsafe {
                let ptr = (offset + phys + off_in_page) as *mut u64;
                core::ptr::write(ptr, val);
            }
            true
        } else {
            false
        }
    };

    // Helper: write a string onto the stack, return the virtual address of the string
    let push_string = |sp: &mut u64, s: &str| -> Option<u64> {
        let bytes = s.as_bytes();
        let len = bytes.len() + 1; // include NUL terminator
        *sp -= len as u64;
        *sp &= !0x7; // align to 8 bytes

        for (i, &b) in bytes.iter().enumerate() {
            let addr = *sp + i as u64;
            let page_phys = unsafe { get_mapped_frame(cr3, page_align_down(addr))? };
            let off = addr & (PAGE_SIZE - 1);
            unsafe {
                *((offset + page_phys + off) as *mut u8) = b;
            }
        }
        // NUL terminator
        let nul_addr = *sp + bytes.len() as u64;
        let page_phys = unsafe { get_mapped_frame(cr3, page_align_down(nul_addr))? };
        let off = nul_addr & (PAGE_SIZE - 1);
        unsafe {
            *((offset + page_phys + off) as *mut u8) = 0;
        }
        Some(*sp)
    };

    // Push environment strings (collect their addresses)
    let mut env_addrs = alloc::vec::Vec::new();
    for &e in envp.iter().rev() {
        if let Some(addr) = push_string(&mut sp, e) {
            env_addrs.push(addr);
        }
    }
    env_addrs.reverse();

    // Push argument strings (collect their addresses)
    let mut arg_addrs = alloc::vec::Vec::new();
    for &a in argv.iter().rev() {
        if let Some(addr) = push_string(&mut sp, a) {
            arg_addrs.push(addr);
        }
    }
    arg_addrs.reverse();

    // Align to 16 bytes
    sp &= !0xF;

    // Auxiliary vector (simplified)
    // AT_NULL = 0
    push_u64(&mut sp, 0);
    push_u64(&mut sp, 0); // AT_NULL
    push_u64(&mut sp, PAGE_SIZE);
    push_u64(&mut sp, 6); // AT_PAGESZ = 6
    push_u64(&mut sp, entry_point);
    push_u64(&mut sp, 9); // AT_ENTRY = 9
    push_u64(&mut sp, phdr_num);
    push_u64(&mut sp, 5); // AT_PHNUM = 5
    push_u64(&mut sp, phdr_addr);
    push_u64(&mut sp, 3); // AT_PHDR = 3

    // NULL terminator for envp
    push_u64(&mut sp, 0);

    // envp pointers
    for &addr in env_addrs.iter().rev() {
        push_u64(&mut sp, addr);
    }

    // NULL terminator for argv
    push_u64(&mut sp, 0);

    // argv pointers
    for &addr in arg_addrs.iter().rev() {
        push_u64(&mut sp, addr);
    }

    // argc
    push_u64(&mut sp, argv.len() as u64);

    // RSP must be 16-byte aligned at process entry
    sp &= !0xF;

    Some(sp)
}
