#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::{
    VirtAddr,
    structures::paging::{
        FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB, mapper::MapToError,
    },
};
/// Heap Allocator - Dynamic memory allocation for the kernel
/// Uses a two-tier strategy: slab allocator for small objects (≤4096),
/// linked-list fallback for larger allocations.
use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use linked_list_allocator::LockedHeap;
#[cfg(target_arch = "x86_64")]
use x86_64::{
    VirtAddr,
    structures::paging::{
        FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB, mapper::MapToError,
    },
};

pub const HEAP_START: usize = 0x_4444_4444_0000;
pub const HEAP_SIZE: usize = 512 * 1024 * 1024; // 512 MiB heap (supports full-size package downloads + persistence buffers)

/// Two-tier allocator: slab for small objects, linked-list heap for larger ones
struct TieredAllocator {
    fallback: LockedHeap,
}

impl TieredAllocator {
    const fn new() -> Self {
        Self {
            fallback: LockedHeap::empty(),
        }
    }
}

unsafe impl GlobalAlloc for TieredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        // Try slab allocator for small objects (≤ 4096 bytes, alignment ≤ size class)
        if size <= 4096 && layout.align() <= size.next_power_of_two().max(8) {
            if let Some(ptr) = crate::slab::slab_alloc(size) {
                return ptr;
            }
        }
        // Fallback to linked-list heap
        self.fallback.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        // Try returning to slab first
        if size <= 4096 && crate::slab::slab_free(ptr, size) {
            return;
        }
        // Fallback to linked-list heap
        self.fallback.dealloc(ptr, layout)
    }
}

#[global_allocator]
static ALLOCATOR: TieredAllocator = TieredAllocator::new();

pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush();
        }
    }

    unsafe {
        ALLOCATOR
            .fallback
            .lock()
            .init(HEAP_START as *mut u8, HEAP_SIZE);
    }

    Ok(())
}
