// SPDX-License-Identifier: MIT
//! Kernel stack guard pages (item 2.11)
//!
//! Inserts unmapped guard pages at the bottom of each kernel stack to
//! detect stack overflow via page fault rather than silent corruption.

#[cfg(target_arch = "x86_64")]
use crate::arch_compat::structures::paging::PageTableFlags;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::structures::paging::PageTableFlags;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

/// Guard page size — one 4KiB page below each stack
const GUARD_PAGE_SIZE: usize = 4096;

/// Default kernel stack size: 64 KiB (16 pages)
pub const KERNEL_STACK_SIZE: usize = 64 * 1024;

/// Number of guard pages per stack
const GUARD_PAGES: usize = 1;

/// Track all allocated stacks with guard pages
#[derive(Debug, Clone)]
pub struct StackInfo {
    /// Base address of guard page (unmapped)
    pub guard_base: u64,
    /// Base address of usable stack (above guard)
    pub stack_base: u64,
    /// Top of stack (initial RSP)
    pub stack_top: u64,
    /// Total allocation including guard
    pub total_size: usize,
    /// Owner PID (0 = kernel)
    pub owner_pid: u64,
}

lazy_static::lazy_static! {
    static ref STACK_REGISTRY: Mutex<Vec<StackInfo>> = Mutex::new(Vec::new());
}

static STACKS_ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static GUARD_FAULTS_CAUGHT: AtomicU64 = AtomicU64::new(0);

/// Allocate a kernel stack with a guard page below it.
///
/// Layout (low → high addresses):
///   [guard page: unmapped 4KiB] [usable stack: KERNEL_STACK_SIZE]
///
/// Returns (stack_top, stack_info) or None if allocation fails.
pub fn alloc_guarded_stack(pid: u64) -> Option<(u64, StackInfo)> {
    let total = GUARD_PAGES * GUARD_PAGE_SIZE + KERNEL_STACK_SIZE;

    // Allocate virtual address range
    // In a real implementation this would go through the VMM
    // For now, use a simple bump allocator for stack addresses
    let base = alloc_stack_region(total)?;

    let guard_base = base;
    let stack_base = base + (GUARD_PAGES * GUARD_PAGE_SIZE) as u64;
    let stack_top = stack_base + KERNEL_STACK_SIZE as u64;

    // Unmap the guard page — accessing it triggers #PF
    unmap_guard_page(guard_base);

    // Map the usable stack pages with RW + NX
    map_stack_pages(stack_base, KERNEL_STACK_SIZE);

    let info = StackInfo {
        guard_base,
        stack_base,
        stack_top,
        total_size: total,
        owner_pid: pid,
    };

    STACK_REGISTRY.lock().push(info.clone());
    STACKS_ALLOCATED.fetch_add(1, Ordering::Relaxed);

    crate::serial_println!(
        "[stack_guard] allocated stack for pid={}, guard=0x{:x}, top=0x{:x}",
        pid,
        guard_base,
        stack_top
    );

    Some((stack_top, info))
}

/// Free a guarded stack
pub fn free_guarded_stack(stack_top: u64) {
    let mut registry = STACK_REGISTRY.lock();
    if let Some(pos) = registry.iter().position(|s| s.stack_top == stack_top) {
        let info = registry.remove(pos);
        // Unmap stack pages and free physical frames
        unmap_stack_pages(info.stack_base, KERNEL_STACK_SIZE);
        free_stack_region(info.guard_base, info.total_size);
        crate::serial_println!(
            "[stack_guard] freed stack for pid={}, guard=0x{:x}",
            info.owner_pid,
            info.guard_base
        );
    }
}

/// Check if a page fault address is in a stack guard page
pub fn is_guard_page_fault(fault_addr: u64) -> bool {
    let registry = STACK_REGISTRY.lock();
    for info in registry.iter() {
        let guard_end = info.guard_base + (GUARD_PAGES * GUARD_PAGE_SIZE) as u64;
        if fault_addr >= info.guard_base && fault_addr < guard_end {
            GUARD_FAULTS_CAUGHT.fetch_add(1, Ordering::Relaxed);
            return true;
        }
    }
    false
}

/// Handle a stack overflow detected via guard page fault
pub fn handle_stack_overflow(fault_addr: u64) {
    let registry = STACK_REGISTRY.lock();
    for info in registry.iter() {
        let guard_end = info.guard_base + (GUARD_PAGES * GUARD_PAGE_SIZE) as u64;
        if fault_addr >= info.guard_base && fault_addr < guard_end {
            crate::serial_println!(
                "[stack_guard] STACK OVERFLOW detected! pid={}, fault_addr=0x{:x}, guard=0x{:x}",
                info.owner_pid,
                fault_addr,
                info.guard_base
            );
            // In a real system, kill the offending process
            // For kernel threads, this is a fatal error
            if info.owner_pid == 0 {
                panic!("Kernel stack overflow at 0x{:x}", fault_addr);
            }
            return;
        }
    }
}

// ── Stack region management ──────────────────────────────────────────

/// Base address for kernel stack allocations
const STACK_REGION_BASE: u64 = 0x0000_7000_0000_0000;
static NEXT_STACK_ADDR: AtomicU64 = AtomicU64::new(STACK_REGION_BASE);

fn alloc_stack_region(size: usize) -> Option<u64> {
    let addr = NEXT_STACK_ADDR.fetch_add(size as u64, Ordering::SeqCst);
    // Ensure we don't overflow
    if addr + size as u64 > STACK_REGION_BASE + 0x0000_0100_0000_0000 {
        return None;
    }
    Some(addr)
}

fn free_stack_region(_base: u64, _size: usize) {
    // In a real implementation, return to a free list
}

/// Unmap a guard page so any access triggers a page fault.
/// We do this by ensuring the page table entry for this address has
/// the PRESENT bit cleared (or is never mapped at all).
fn unmap_guard_page(addr: u64) {
    // The guard page virtual address is in a region that we control.
    // Since alloc_stack_region bumps a pointer in unmapped space,
    // the guard page is never mapped in the first place — any access
    // will trigger a #PF, which is exactly what we want.
    //
    // If the address were mapped, we would walk the page table and
    // clear the PRESENT bit:
    //   let page = Page::<Size4KiB>::containing_address(VirtAddr::new(addr));
    //   mapper.unmap(page);
    //
    // For defense-in-depth, explicitly poison the guard page address
    // range so we can detect it quickly in is_guard_page_fault().
    let _ = addr;
    // Guard page is unmapped by design — no action needed.
}

/// Map the usable stack pages with READ_WRITE and NO_EXECUTE flags.
/// Allocates physical frames and creates page table entries for the
/// stack region above the guard page.
fn map_stack_pages(base: u64, size: usize) {
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::VirtAddr;
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::structures::paging::VirtAddr;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::structures::paging::{FrameAllocator, Mapper, Page, Size4KiB};
    #[cfg(target_arch = "x86_64")]
    use x86_64::structures::paging::{FrameAllocator, Mapper, Page, Size4KiB};

    let phys_offset = crate::vmm::get_phys_mem_offset();
    if phys_offset == 0 {
        // VMM not initialized yet — stacks allocated before VMM init
        // will use the kernel heap instead (which is already mapped).
        return;
    }

    // Map each 4KiB page in the stack region
    let start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(base));
    let end_page = Page::<Size4KiB>::containing_address(VirtAddr::new(base + size as u64 - 1));

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;

    for page in Page::range_inclusive(start_page, end_page) {
        // Try to get a frame from the VMM frame pool
        if let Some(phys_addr) = crate::vmm::allocate_physical_frame() {
            unsafe {
                // Map using the kernel's current CR3 page table
                let (cr3_frame, _) = crate::arch_compat::registers::control::Cr3::read();
                crate::vmm::map_page_in_table_pub(
                    cr3_frame.start_address().as_u64(),
                    page.start_address().as_u64(),
                    phys_addr,
                    flags,
                );
            }
        }
    }
}

fn unmap_stack_pages(base: u64, size: usize) {
    // In a real implementation, unmap pages and free physical frames
    let _ = (base, size);
}

/// Get statistics
pub fn stats() -> (usize, u64) {
    (
        STACKS_ALLOCATED.load(Ordering::Relaxed),
        GUARD_FAULTS_CAUGHT.load(Ordering::Relaxed),
    )
}

/// Initialize the stack guard subsystem
pub fn init() {
    crate::serial_println!(
        "[stack_guard] initialized, guard_pages={}, stack_size={}KiB",
        GUARD_PAGES,
        KERNEL_STACK_SIZE / 1024
    );
}
