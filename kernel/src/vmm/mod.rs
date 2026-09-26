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
use core::sync::atomic::Ordering;

#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::registers::control::Cr3;
use crate::serial_println;
#[cfg(target_arch = "x86_64")]
use x86_64::registers::control::Cr3;

mod frame;
mod layout;
mod loader;
mod page_table;
mod region;
mod selftest;
mod space;
mod user;

pub use frame::{
    FRAME_POOL, GATE_J2_MARKER, PhysicalFramePool, allocate_contiguous_frames,
    allocate_physical_frame, allocate_physical_frame_raw, free_physical_frame, get_stats,
    ingest_remaining_ram, populate_frame_pool, restore_frame_pool, steal_frame_pool,
};
pub use layout::{
    ASLR_ENABLED, ASLR_HEAP_RANGE, ASLR_MMAP_RANGE, ASLR_PROGRAM_RANGE, ASLR_STACK_RANGE, HEAP_MAX,
    HEAP_START, MMAP_REGION_END, MMAP_REGION_START, PAGE_SIZE, PROGRAM_BASE, STACK_GUARD_PAGES,
    STACK_SIZE, STACK_TOP, USER_SPACE_END, USER_SPACE_START, aslr_heap_start, aslr_mmap_end,
    aslr_program_base, aslr_stack_top, init_aslr_seed, page_align_down, page_align_up, reseed_aslr,
};
pub use loader::{
    load_elf_into_address_space, map_static_elf_into_current, setup_initial_stack, setup_user_stack,
};
pub use page_table::{
    activate_cr3, activate_kernel_cr3, get_kernel_cr3, get_phys_mem_offset, map_mmio,
    map_page_in_table_pub, phys_to_virt, ready,
};
pub use region::{MappingType, MmapFlags, ProtFlags, VirtualMemoryArea};
pub use selftest::{
    GATE_E1_MARKER, GATE_H1_MARKER, buddy_ram_self_test, cow_fault_self_test, swap_fault_self_test,
    wx_aslr_self_test,
};
pub use space::{
    ADDRESS_SPACES, AddressSpace, MemRegionInfo, brk, create_address_space, destroy_address_space,
    fork_address_space, get_cr3, get_memory_regions, handle_page_fault, mmap, mmap_file, munmap,
    replace_address_space, share_address_space, swap_out_page,
};
pub use user::{read_user_memory, write_user_memory};

/// Initialize the VMM subsystem
pub fn init(phys_mem_offset: u64) {
    page_table::PHYS_MEM_OFFSET.store(phys_mem_offset, Ordering::Relaxed);

    let (frame, _) = Cr3::read();
    page_table::KERNEL_CR3.store(frame.start_address().as_u64(), Ordering::Relaxed);

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
