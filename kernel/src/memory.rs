#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::bootloader_shim::info::{MemoryRegionKind, MemoryRegions};
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::{
    PhysAddr, VirtAddr,
    structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
};
/// Memory Management - Page table mapping and frame allocation
#[cfg(target_arch = "x86_64")]
use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
#[cfg(target_arch = "x86_64")]
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
};

/// Initialize a new OffsetPageTable.
///
/// # Safety
/// The caller must guarantee that the complete physical memory is mapped
/// to virtual memory at the passed `physical_memory_offset`.
pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = active_level_4_table(physical_memory_offset);
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

/// Returns a mutable reference to the active level 4 page table.
unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    #[cfg(target_arch = "x86_64")]
    use crate::arch_compat::registers::control::Cr3;
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();
    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr
}

/// A FrameAllocator that returns usable frames from the bootloader's memory map.
pub struct BootInfoFrameAllocator {
    memory_regions: &'static MemoryRegions,
    next: usize,
    /// Cache the current region index and offset within it to avoid O(n²)
    region_idx: usize,
    frame_offset: u64,
}

impl BootInfoFrameAllocator {
    /// Create a FrameAllocator from the passed memory map.
    ///
    /// # Safety
    /// The caller must guarantee that the passed memory map is valid.
    pub unsafe fn init(memory_regions: &'static MemoryRegions) -> Self {
        // Find first usable region
        let mut region_idx = 0;
        for (i, r) in memory_regions.iter().enumerate() {
            if r.kind == MemoryRegionKind::Usable {
                region_idx = i;
                break;
            }
        }
        let frame_offset = if let Some(r) = memory_regions.get(region_idx) {
            r.start
        } else {
            0
        };
        BootInfoFrameAllocator {
            memory_regions,
            next: 0,
            region_idx,
            frame_offset,
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        // Walk through regions, advancing to the next usable frame
        loop {
            // Find current region by index
            let mut region = None;
            for (i, r) in self.memory_regions.iter().enumerate() {
                if i == self.region_idx {
                    region = Some(r);
                    break;
                }
            }
            let region = region?; // no more regions

            if region.kind != MemoryRegionKind::Usable {
                self.region_idx += 1;
                // Find start of next region
                for (i, r) in self.memory_regions.iter().enumerate() {
                    if i == self.region_idx {
                        self.frame_offset = r.start;
                        break;
                    }
                }
                continue;
            }
            if self.frame_offset >= region.end {
                self.region_idx += 1;
                // Set frame_offset to start of next region
                for (i, r) in self.memory_regions.iter().enumerate() {
                    if i == self.region_idx {
                        self.frame_offset = r.start;
                        break;
                    }
                }
                continue;
            }
            let addr = self.frame_offset;
            self.frame_offset += 4096;
            self.next += 1;
            return Some(PhysFrame::containing_address(PhysAddr::new(addr)));
        }
    }
}

/// Get total available memory in bytes
pub fn get_total_memory(memory_regions: &MemoryRegions) -> u64 {
    memory_regions
        .iter()
        .filter(|r| r.kind == MemoryRegionKind::Usable)
        .map(|r| r.end - r.start)
        .sum()
}
