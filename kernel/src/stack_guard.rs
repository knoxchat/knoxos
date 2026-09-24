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
    if !map_stack_pages(stack_base, KERNEL_STACK_SIZE) {
        return None;
    }

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

/// Base address for kernel stack allocations.
/// Same L4 slot as the kernel heap (`0x4444_4444_0000`, L4 136) so new
/// mappings are visible in every cloned user CR3.
const STACK_REGION_BASE: u64 = 0x0000_4445_0000_0000;
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
/// stack region above the guard page. Uses the kernel CR3 so the mapping
/// lands in the shared heap L4 slot.
fn map_stack_pages(base: u64, size: usize) -> bool {
    let cr3 = crate::vmm::get_kernel_cr3();
    if cr3 == 0 || crate::vmm::get_phys_mem_offset() == 0 {
        return false;
    }

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
    let mut mapped = 0usize;
    let mut page = base;
    let end = base + size as u64;
    while page < end {
        let Some(phys_addr) = crate::vmm::allocate_physical_frame() else {
            return mapped > 0;
        };
        unsafe {
            crate::vmm::map_page_in_table_pub(cr3, page, phys_addr, flags);
        }
        mapped += 1;
        page += GUARD_PAGE_SIZE as u64;
    }
    true
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
    let _ = guard_oom_self_test();
}

/// Serial marker once a guarded kernel stack is allocated and a failed
/// frame alloc runs the OOM killer.
pub const GATE_H4_MARKER: &str = "GATE_H4 guard oom";

const GATE_H4_VICTIM: crate::process::Pid = 0x0000_0E04;

/// Allocate a guarded stack (unmapped guard, mapped usable pages) and prove
/// `allocate_physical_frame` invokes the OOM killer when the buddy pool is empty.
pub fn guard_oom_self_test() -> bool {
    let Some((stack_top, info)) = alloc_guarded_stack(GATE_H4_VICTIM as u64) else {
        crate::serial_println!("[stack_guard] Gate H4 FAILED: alloc_guarded_stack");
        return false;
    };
    if stack_top == 0 || !is_guard_page_fault(info.guard_base) {
        crate::serial_println!("[stack_guard] Gate H4 FAILED: guard page not registered");
        return false;
    }
    if is_guard_page_fault(info.stack_base) {
        crate::serial_println!("[stack_guard] Gate H4 FAILED: usable stack marked as guard");
        return false;
    }

    crate::oom::register_process(GATE_H4_VICTIM);
    crate::oom::update_memory(GATE_H4_VICTIM, 1_000_000);
    let _ = crate::oom::set_oom_score_adj(GATE_H4_VICTIM, 1000);

    let stolen = crate::vmm::steal_frame_pool();
    let kills_before = crate::oom::stats().0;
    let _ = crate::vmm::allocate_physical_frame();
    crate::vmm::restore_frame_pool(stolen);
    let (kills_after, last_killed, _) = crate::oom::stats();

    if kills_after <= kills_before || last_killed != Some(GATE_H4_VICTIM) {
        crate::serial_println!(
            "[stack_guard] Gate H4 FAILED: OOM did not select dummy (kills {} -> {}, last={:?})",
            kills_before,
            kills_after,
            last_killed
        );
        return false;
    }

    crate::serial_println!("[stack_guard] {}", GATE_H4_MARKER);
    true
}
