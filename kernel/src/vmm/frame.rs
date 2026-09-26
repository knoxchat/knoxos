//! Global physical buddy allocator and frame-pool helpers.
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::structures::paging::{FrameAllocator, Size4KiB};
#[cfg(target_arch = "x86_64")]
use x86_64::structures::paging::{FrameAllocator, Size4KiB};

use super::layout::PAGE_SIZE;
use super::page_table::snapshot_kernel_l4;
use super::selftest::{buddy_ram_self_test, cow_fault_self_test};

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

    /// Add `nframes` consecutive 4 KiB pages as the largest aligned buddy blocks.
    pub fn add_range(&mut self, mut addr: u64, mut nframes: u64) {
        addr &= !0xFFF;
        while nframes > 0 {
            let mut order = 0;
            while order < BUDDY_MAX_ORDER {
                let next = order + 1;
                if nframes < (1u64 << next) {
                    break;
                }
                if addr & (Self::block_size(next) - 1) != 0 {
                    break;
                }
                order = next;
            }
            let n = 1u64 << order;
            self.free_block(addr, order);
            self.total_frames += n;
            addr += n * PAGE_SIZE;
            nframes -= n;
        }
    }

    /// Allocate a 4 KiB physical frame
    pub fn allocate(&mut self) -> Option<u64> {
        let addr = self.take_block(0)?;
        self.allocated_frames += 1;
        Some(addr)
    }

    /// Allocate `2^order` consecutive 4 KiB frames. Used for virtio DMA regions.
    pub fn allocate_order(&mut self, order: usize) -> Option<u64> {
        if order > BUDDY_MAX_ORDER {
            return None;
        }
        let addr = self.take_block(order)?;
        self.allocated_frames += 1u64 << order;
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

    /// Remove every free buddy block (for the OOM self-test). Does not change
    /// `total_frames`. Returns `(addr, order)` so restore is not O(n) 4 KiB merges.
    pub fn steal_free_frames(&mut self) -> Vec<(u64, usize)> {
        let mut out = Vec::new();
        for order in 0..=BUDDY_MAX_ORDER {
            while let Some(addr) = self.free[order].pop() {
                out.push((addr, order));
            }
        }
        out
    }

    /// Return blocks taken by [`steal_free_frames`] without bumping `total_frames`.
    pub fn restore_free_frames(&mut self, blocks: Vec<(u64, usize)>) {
        for (addr, order) in blocks {
            self.free_block(addr & !0xFFF, order.min(BUDDY_MAX_ORDER));
        }
    }
}

lazy_static::lazy_static! {
    pub static ref FRAME_POOL: Mutex<PhysicalFramePool> = Mutex::new(PhysicalFramePool::new());
}

/// Allocate a physical frame from the global pool.
/// On failure, shrink the page cache, then swap out anonymous pages, then OOM.
pub fn allocate_physical_frame() -> Option<u64> {
    if let Some(addr) = FRAME_POOL.lock().allocate() {
        return Some(addr);
    }
    let _ = crate::page_cache::shrink(64);
    if let Some(addr) = FRAME_POOL.lock().allocate() {
        return Some(addr);
    }
    let _ = crate::swap::reclaim_anonymous(16);
    if let Some(addr) = FRAME_POOL.lock().allocate() {
        return Some(addr);
    }
    crate::oom::trigger_oom();
    FRAME_POOL.lock().allocate()
}

/// Allocate without reclaim. Used by swap-in so a #PF cannot recurse into reclaim.
pub fn allocate_physical_frame_raw() -> Option<u64> {
    FRAME_POOL.lock().allocate()
}

/// Allocate `count` consecutive 4 KiB physical frames (rounded up to a buddy order).
pub fn allocate_contiguous_frames(count: usize) -> Option<u64> {
    if count == 0 {
        return None;
    }
    let mut order = 0;
    while (1usize << order) < count {
        order += 1;
        if order > BUDDY_MAX_ORDER {
            return None;
        }
    }
    FRAME_POOL.lock().allocate_order(order)
}

/// Free a physical frame back to the global pool
pub fn free_physical_frame(phys_addr: u64) {
    FRAME_POOL.lock().free(phys_addr);
}

/// Drain the buddy free lists (Gate H4 OOM self-test). Caller must restore.
pub fn steal_frame_pool() -> Vec<(u64, usize)> {
    FRAME_POOL.lock().steal_free_frames()
}

/// Put blocks from [`steal_frame_pool`] back on the free lists.
pub fn restore_frame_pool(blocks: Vec<(u64, usize)>) {
    FRAME_POOL.lock().restore_free_frames(blocks);
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
    drop(pool);
    snapshot_kernel_l4();
    let _ = cow_fault_self_test();
}

/// Serial marker once leftover bootloader RAM is in the buddy pool.
pub const GATE_J2_MARKER: &str = "GATE_J2 buddy ram";

/// Pull every remaining usable bootloader frame into the buddy allocator.
///
/// Gate J2: physical free is not limited to the 32 MiB VMM prefill.
pub fn ingest_remaining_ram(frame_allocator: &mut crate::memory::BootInfoFrameAllocator) {
    let ranges = frame_allocator.take_remaining_usable();
    let mut extra = 0u64;
    {
        let mut pool = FRAME_POOL.lock();
        for (start, nframes) in ranges {
            pool.add_range(start, nframes);
            extra += nframes;
        }
        serial_println!(
            "[VMM] Buddy ingested leftover RAM: +{} frames ({} MiB), pool total={} available={}",
            extra,
            extra * 4 / 1024,
            pool.total(),
            pool.available()
        );
    }
    let _ = buddy_ram_self_test();
}

/// Get VMM statistics
pub fn get_stats() -> (u64, u64, u64) {
    let pool = FRAME_POOL.lock();
    (pool.total(), pool.allocated(), pool.available())
}
