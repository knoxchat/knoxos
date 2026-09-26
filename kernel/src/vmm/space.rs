//! Per-process address spaces, VMA operations, and the global process table.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::process::Pid;
use crate::serial_println;

#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::{registers::control::Cr3, structures::paging::PageTableFlags};
#[cfg(target_arch = "x86_64")]
use x86_64::{registers::control::Cr3, structures::paging::PageTableFlags};

use super::frame::{
    FRAME_POOL, allocate_physical_frame, allocate_physical_frame_raw, free_physical_frame,
};
use super::layout::{
    HEAP_MAX, HEAP_START, MMAP_REGION_START, PAGE_SIZE, STACK_GUARD_PAGES, STACK_SIZE, STACK_TOP,
    aslr_heap_start, aslr_mmap_end, page_align_down, page_align_up,
};
use super::page_table::{
    activate_cr3, allocate_page_table_frame, clone_kernel_mappings, copy_physical_frame,
    copy_to_physical_frame, fill_file_backed_page, get_mapped_frame, map_page_in_table,
    unmap_page_in_table, update_page_flags_in_table, zero_physical_frame,
};
use super::region::{MappingType, MmapFlags, ProtFlags, VirtualMemoryArea};
use super::user::read_user_memory;

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

        if crate::hardening::check_wx_violation(crate::hardening::ProtFlags {
            read: prot.read,
            write: prot.write,
            exec: prot.execute,
        }) {
            return None;
        }

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
                    crate::swap::track_anon_page(self.pid, page_addr, frame_phys);
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

    /// Map a file-backed region. Pages are demand-filled from VFS/page cache
    /// unless `flags.populate` is set (Gate H2).
    pub fn mmap_file(
        &mut self,
        addr: u64,
        size: u64,
        prot: ProtFlags,
        flags: MmapFlags,
        path: &str,
        offset: u64,
    ) -> Option<u64> {
        let vaddr = self.mmap_anonymous(addr, size, prot, flags)?;
        if let Some(vma) = self.find_vma_mut(vaddr) {
            vma.mapping_type = MappingType::FileBacked;
            vma.file_path = Some(String::from(path));
            vma.file_offset = offset;
            vma.flags.anonymous = false;
        }
        if flags.populate {
            let pages = page_align_up(size) / PAGE_SIZE;
            for i in 0..pages {
                let page_addr = vaddr + i * PAGE_SIZE;
                if let Some(frame) = unsafe { get_mapped_frame(self.cr3, page_addr) } {
                    fill_file_backed_page(path, offset + i * PAGE_SIZE, frame);
                }
            }
        }
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
        if crate::hardening::check_wx_violation(crate::hardening::ProtFlags {
            read: prot.read,
            write: prot.write,
            exec: prot.execute,
        }) {
            return false;
        }

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
                    // CoW: share the frame, writable removed, execute preserved.
                    // Forcing NO_EXECUTE here made the parent's text page NX, so
                    // `sysretq` from fork #PF'd on the next instruction fetch.
                    let mut cow_flags = vma.prot.to_page_flags();
                    cow_flags.remove(PageTableFlags::WRITABLE);
                    unsafe {
                        map_page_in_table(child.cr3, page_addr, phys_frame, cow_flags);
                        update_page_flags_in_table(self.cr3, page_addr, cow_flags);
                    }

                    // Track CoW pages
                    child.cow_pages.insert(page_addr, phys_frame);
                }
            }

            child.add_vma(child_vma);
        }

        // Flush TLB only if this address space is on the CPU. A self-test
        // fork of a detached space must not `mov cr3` onto tables that Drop
        // will free (that left the kernel running on a recycled L4).
        unsafe {
            #[cfg(target_arch = "x86_64")]
            {
                let (current, _) = Cr3::read();
                if current.start_address().as_u64() == self.cr3 {
                    core::arch::asm!("mov cr3, {}", in(reg) self.cr3, options(nostack, preserves_flags));
                }
            }
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

    /// Restore a swapped-out page on #PF. Returns true if the fault was handled.
    pub fn handle_swap_in(&mut self, fault_addr: u64) -> bool {
        let page_addr = page_align_down(fault_addr);
        if !crate::swap::is_swapped(self.pid, page_addr) {
            return false;
        }
        let Some(frame) = allocate_physical_frame_raw() else {
            return false;
        };
        let Some(data) = crate::swap::page_in(self.pid, page_addr) else {
            FRAME_POOL.lock().free(frame);
            return false;
        };
        let flags = if let Some(vma) = self.find_vma(page_addr) {
            vma.prot.to_page_flags()
        } else {
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE
        };
        self.owned_frames.push(frame);
        unsafe {
            copy_to_physical_frame(frame, &data);
            map_page_in_table(self.cr3, page_addr, frame, flags);
        }
        crate::swap::track_anon_page(self.pid, page_addr, frame);
        true
    }

    /// Handle a demand-paging fault: the page belongs to a valid VMA but has
    /// no physical backing yet (lazy allocation from mmap without MAP_POPULATE).
    /// File-backed VMAs fill one page from the page cache / VFS, not the whole file.
    /// Returns true if a new frame was allocated and mapped.
    pub fn handle_demand_fault(&mut self, fault_addr: u64) -> bool {
        let page_addr = page_align_down(fault_addr);

        // Check if this page lies inside a VMA that could be lazily backed
        let (prot_flags, mapping_type, file_path, file_offset, vma_start) =
            if let Some(vma) = self.find_vma(page_addr) {
                // Guard pages must NOT be demand-faulted (that's a stack overflow)
                if vma.mapping_type == MappingType::Guard {
                    return false;
                }
                (
                    vma.prot.to_page_flags(),
                    vma.mapping_type,
                    vma.file_path.clone(),
                    vma.file_offset,
                    vma.start,
                )
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
            if mapping_type == MappingType::FileBacked {
                if let Some(path) = file_path.as_deref() {
                    let off = file_offset + (page_addr - vma_start);
                    fill_file_backed_page(path, off, frame);
                }
            } else {
                crate::swap::track_anon_page(self.pid, page_addr, frame);
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

    /// Share this mm with another PID (clone(CLONE_VM)): same CR3, copied VMAs.
    /// The child does not own frames; [`destroy_address_space`] transfers them
    /// to a sibling so the last task to exit frees the tables.
    pub fn share(&self, child_pid: Pid) -> AddressSpace {
        AddressSpace {
            pid: child_pid,
            cr3: self.cr3,
            vmas: self.vmas.clone(),
            brk: self.brk,
            mmap_next: self.mmap_next,
            owned_frames: Vec::new(),
            cow_pages: self.cow_pages.clone(),
        }
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
    /// Extra holders of a shared CR3 (clone(CLONE_VM)). 0 means unshared.
    static ref CR3_EXTRA_REFS: Mutex<BTreeMap<u64, u32>> = Mutex::new(BTreeMap::new());
}

fn cr3_share(cr3: u64) {
    if cr3 == 0 {
        return;
    }
    *CR3_EXTRA_REFS.lock().entry(cr3).or_insert(0) += 1;
}

fn cr3_is_shared(cr3: u64) -> bool {
    CR3_EXTRA_REFS.lock().get(&cr3).copied().unwrap_or(0) > 0
}

fn cr3_unshare(cr3: u64) {
    let mut refs = CR3_EXTRA_REFS.lock();
    match refs.get_mut(&cr3) {
        Some(n) if *n > 1 => *n -= 1,
        Some(_) => {
            refs.remove(&cr3);
        }
        None => {}
    }
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
    let mut spaces = ADDRESS_SPACES.lock();
    let Some(mut dying) = spaces.remove(&pid) else {
        serial_println!("[VMM] Destroyed address space for PID {}", pid);
        return;
    };
    let cr3 = dying.cr3;
    if cr3 != 0 && cr3_is_shared(cr3) {
        cr3_unshare(cr3);
        if let Some(sib) = spaces.values_mut().find(|s| s.cr3 == cr3) {
            sib.owned_frames.append(&mut dying.owned_frames);
        }
        dying.cr3 = 0;
        dying.owned_frames.clear();
    }
    drop(spaces);
    serial_println!("[VMM] Destroyed address space for PID {}", pid);
}

/// Share the parent's page tables with a child (clone(CLONE_VM)).
pub fn share_address_space(parent_pid: Pid, child_pid: Pid) -> bool {
    let spaces = ADDRESS_SPACES.lock();
    let Some(parent) = spaces.get(&parent_pid) else {
        return false;
    };
    let child = parent.share(child_pid);
    let cr3 = child.cr3;
    drop(spaces);
    if cr3 == 0 {
        return false;
    }
    cr3_share(cr3);
    ADDRESS_SPACES.lock().insert(child_pid, child);
    serial_println!(
        "[VMM] Shared address space: PID {} -> PID {} cr3={:#x}",
        parent_pid,
        child_pid,
        cr3
    );
    true
}

/// Replace a live process's address space without leaving CR3 on freed tables.
///
/// `execve` used to `destroy` then `create` while the syscall was still
/// executing on the dying L4; `Drop` freed that CR3 and the next heap
/// access triple-faulted. This builds the new tables first, switches onto
/// them, then drops the old ones.
pub fn replace_address_space(pid: Pid) -> bool {
    let Some(new_as) = AddressSpace::new(pid) else {
        serial_println!(
            "[VMM] Failed to allocate replacement address space for PID {}",
            pid
        );
        return false;
    };
    let new_cr3 = new_as.cr3;
    unsafe {
        activate_cr3(new_cr3);
    }
    ADDRESS_SPACES.lock().insert(pid, new_as);
    serial_println!(
        "[VMM] Replaced address space for PID {} (cr3={:#x})",
        pid,
        new_cr3
    );
    true
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

        // 2. Swap-in: a previously paged-out anonymous page.
        if addr_space.handle_swap_in(fault_addr) {
            return true;
        }

        // 3. Check demand paging — page in a valid VMA but not yet backed
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
    if crate::hardening::check_wx_violation(crate::hardening::ProtFlags {
        read: prot_flags.read,
        write: prot_flags.write,
        exec: prot_flags.execute,
    }) {
        return -13; // EACCES
    }
    let mmap_flags = MmapFlags::from_linux(flags);

    let mut spaces = ADDRESS_SPACES.lock();
    if let Some(addr_space) = spaces.get_mut(&pid) {
        if let Some(mapped_addr) = addr_space.mmap_anonymous(addr, size, prot_flags, mmap_flags) {
            return mapped_addr as i64;
        }
    }
    -12 // ENOMEM
}

/// File-backed mmap: VMA is registered immediately; pages fault in from VFS.
pub fn mmap_file(
    pid: Pid,
    addr: u64,
    size: u64,
    prot: u64,
    flags: u64,
    path: &str,
    offset: u64,
) -> i64 {
    let prot_flags = ProtFlags::from_mmap_prot(prot);
    if crate::hardening::check_wx_violation(crate::hardening::ProtFlags {
        read: prot_flags.read,
        write: prot_flags.write,
        exec: prot_flags.execute,
    }) {
        return -13; // EACCES
    }
    let mmap_flags = MmapFlags::from_linux(flags);

    let mut spaces = ADDRESS_SPACES.lock();
    if let Some(addr_space) = spaces.get_mut(&pid) {
        if let Some(mapped_addr) =
            addr_space.mmap_file(addr, size, prot_flags, mmap_flags, path, offset)
        {
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

/// Copy a mapped anonymous page into swap, unmap it, and free the frame.
///
/// Uses `try_lock` so reclaim can run from `allocate_physical_frame` without
/// deadlocking when the caller already holds `ADDRESS_SPACES`.
pub fn swap_out_page(pid: Pid, vaddr: u64) -> bool {
    let page = page_align_down(vaddr);
    let phys = {
        let Some(spaces) = ADDRESS_SPACES.try_lock() else {
            return false;
        };
        let Some(addr_space) = spaces.get(&pid) else {
            return false;
        };
        unsafe { get_mapped_frame(addr_space.cr3, page) }
    };
    let Some(phys) = phys else {
        return false;
    };
    let mut buf = [0u8; PAGE_SIZE as usize];
    read_user_memory(pid, page, &mut buf);
    if crate::swap::page_out(pid, page, &buf).is_none() {
        return false;
    }
    {
        let Some(mut spaces) = ADDRESS_SPACES.try_lock() else {
            return false;
        };
        if let Some(addr_space) = spaces.get_mut(&pid) {
            unsafe {
                unmap_page_in_table(addr_space.cr3, page);
            }
            addr_space.owned_frames.retain(|&f| f != phys);
        }
    }
    free_physical_frame(phys);
    true
}
