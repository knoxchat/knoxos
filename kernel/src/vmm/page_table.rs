//! Page-table walks, CR3 switching, and physical-frame copy helpers.
use core::sync::atomic::{AtomicU64, Ordering};

use crate::serial_println;

#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::{
    PhysAddr, VirtAddr,
    registers::control::Cr3,
    structures::paging::{PageTable, PageTableFlags, PhysFrame},
};
#[cfg(target_arch = "x86_64")]
use x86_64::{
    PhysAddr, VirtAddr,
    registers::control::Cr3,
    structures::paging::{PageTable, PageTableFlags, PhysFrame},
};

use super::frame::allocate_physical_frame;
use super::layout::PAGE_SIZE;

/// Physical memory offset (set during init)
pub(super) static PHYS_MEM_OFFSET: AtomicU64 = AtomicU64::new(0);

/// Boot kernel CR3, captured before any user mappings are added.
pub(super) static KERNEL_CR3: AtomicU64 = AtomicU64::new(0);

/// Frozen copy of the kernel L4 taken after the frame pool is ready and
/// before Gate B2 maps hello into the live kernel tables. New process
/// address spaces clone from this, not from the current CR3.
static KERNEL_L4_TEMPLATE: AtomicU64 = AtomicU64::new(0);

/// Allocate a zeroed frame for a new page table
pub(super) fn allocate_page_table_frame() -> Option<u64> {
    let frame = allocate_physical_frame()?;
    unsafe { zero_physical_frame(frame) };
    Some(frame)
}

/// Kernel virtual address for a physical frame (offset mapping).
pub fn phys_to_virt(phys: u64) -> u64 {
    phys + get_phys_mem_offset()
}

/// Map `bytes` of MMIO at `phys` into the kernel page tables (uncached).
/// Returns the offset-mapped virtual address, or `None` if VMM is not ready.
pub fn map_mmio(phys: u64, bytes: usize) -> Option<u64> {
    if bytes == 0 {
        return None;
    }
    let offset = get_phys_mem_offset();
    if offset == 0 {
        return None;
    }
    let cr3 = KERNEL_CR3.load(Ordering::Relaxed);
    if cr3 == 0 {
        return None;
    }
    let phys_aligned = phys & !0xFFF;
    let end = phys.saturating_add(bytes as u64);
    let mut page = phys_aligned;
    let flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::NO_EXECUTE
        | PageTableFlags::NO_CACHE;
    while page < end {
        let vaddr = page + offset;
        unsafe {
            map_page_in_table(cr3, vaddr, page, flags);
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!("invlpg [{}]", in(reg) vaddr, options(nostack, preserves_flags));
        }
        page += PAGE_SIZE;
    }
    Some(phys + offset)
}

// ─── Page Table Manipulation ────────────────────────────────────────────

/// Map a single page in a process's page table
///
/// # Safety
/// The cr3 value and physical frame must be valid.
pub(super) unsafe fn map_page_in_table(
    cr3: u64,
    vaddr: u64,
    phys_frame: u64,
    flags: PageTableFlags,
) {
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
    } else if l3_table[l3_idx].flags().contains(PageTableFlags::HUGE_PAGE) {
        return;
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
    } else if l2_table[l2_idx].flags().contains(PageTableFlags::HUGE_PAGE) {
        return;
    }
    let l1_phys = l2_table[l2_idx].addr().as_u64();
    let l1_table = &mut *((offset + l1_phys) as *mut PageTable);

    // Map the actual page
    l1_table[l1_idx].set_addr(PhysAddr::new(phys_frame), flags);
}

/// Unmap a page from a process's page table
pub(super) unsafe fn unmap_page_in_table(cr3: u64, vaddr: u64) {
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
pub(super) unsafe fn update_page_flags_in_table(cr3: u64, vaddr: u64, flags: PageTableFlags) {
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
pub(super) unsafe fn get_mapped_frame(cr3: u64, vaddr: u64) -> Option<u64> {
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
    if l3_table[l3_idx].flags().contains(PageTableFlags::HUGE_PAGE) {
        return Some(l3_table[l3_idx].addr().as_u64());
    }

    let l2_phys = l3_table[l3_idx].addr().as_u64();
    let l2_table = &*((offset + l2_phys) as *const PageTable);
    let l2_idx = ((vaddr >> 21) & 0x1FF) as usize;
    if l2_table[l2_idx].is_unused() {
        return None;
    }
    if l2_table[l2_idx].flags().contains(PageTableFlags::HUGE_PAGE) {
        return Some(l2_table[l2_idx].addr().as_u64());
    }

    let l1_phys = l2_table[l2_idx].addr().as_u64();
    let l1_table = &*((offset + l1_phys) as *const PageTable);
    let l1_idx = ((vaddr >> 12) & 0x1FF) as usize;
    if l1_table[l1_idx].is_unused() {
        return None;
    }

    Some(l1_table[l1_idx].addr().as_u64())
}

/// Physical address of the L4 we clone kernel mappings from.
fn kernel_l4_source() -> u64 {
    let tmpl = KERNEL_L4_TEMPLATE.load(Ordering::Relaxed);
    if tmpl != 0 {
        return tmpl;
    }
    let saved = KERNEL_CR3.load(Ordering::Relaxed);
    if saved != 0 {
        return saved;
    }
    let (frame, _) = Cr3::read();
    frame.start_address().as_u64()
}

/// Clone kernel page-table slots into a new L4 table.
///
/// Source is the L4 snapshot taken before any user mappings, not the live
/// CR3. Copying from the current tables after Gate B2 would share the hello
/// L3/L2/L1 with the kernel; `execve` dropping those frames then corrupts
/// the boot page tables.
///
/// Every present snapshot slot is shared, including the lower-half kernel
/// heap (L4 136) and the physical-memory window (L4 20 at 0x28_0000_0000
/// on QEMU). Skipping "user-half" L4 entries unmapped that window and
/// hung the first `switch_to` into Ring 3.
pub(super) unsafe fn clone_kernel_mappings(new_cr3: u64) {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return;
    }

    let kernel_l4_phys = kernel_l4_source();
    let kernel_l4 = &*((offset + kernel_l4_phys) as *const PageTable);
    let new_l4 = &mut *((offset + new_cr3) as *mut PageTable);

    for i in 0..512 {
        if !kernel_l4[i].is_unused() {
            new_l4[i] = kernel_l4[i].clone();
        }
    }
}

/// Load `cr3` into the CPU. `mov cr3` also flushes the TLB.
///
/// # Safety
/// `cr3` must be a valid L4 that maps this kernel (code, heap, phys offset).
pub unsafe fn activate_cr3(cr3: u64) {
    if cr3 == 0 {
        return;
    }
    #[cfg(target_arch = "x86_64")]
    {
        let (_, flags) = Cr3::read();
        let frame = PhysFrame::containing_address(PhysAddr::new(cr3));
        Cr3::write(frame, flags);
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = cr3;
}

/// Switch onto the boot kernel page tables.
pub fn activate_kernel_cr3() {
    let cr3 = KERNEL_CR3.load(Ordering::Relaxed);
    if cr3 != 0 {
        unsafe {
            activate_cr3(cr3);
        }
    }
}

/// Zero a physical frame (accessed via physical memory mapping)
pub(super) unsafe fn zero_physical_frame(phys_addr: u64) {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return;
    }
    let ptr = (offset + phys_addr) as *mut u8;
    core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize);
}

/// Copy one physical frame to another
pub(super) unsafe fn copy_physical_frame(src_phys: u64, dst_phys: u64) {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return;
    }
    let src = (offset + src_phys) as *const u8;
    let dst = (offset + dst_phys) as *mut u8;
    core::ptr::copy_nonoverlapping(src, dst, PAGE_SIZE as usize);
}

pub(super) unsafe fn copy_to_physical_frame(phys_addr: u64, data: &[u8]) {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 || data.is_empty() {
        return;
    }
    let ptr = (offset + phys_addr) as *mut u8;
    let n = data.len().min(PAGE_SIZE as usize);
    core::ptr::copy_nonoverlapping(data.as_ptr(), ptr, n);
}

pub(super) unsafe fn read_physical_frame_byte(phys_addr: u64, offset_in_page: usize) -> u8 {
    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return 0;
    }
    *((offset + phys_addr + offset_in_page as u64) as *const u8)
}

/// Fill one 4 KiB physical frame from the page cache (or VFS) at `file_offset`.
pub(super) fn fill_file_backed_page(path: &str, file_offset: u64, frame_phys: u64) {
    let mut buf = [0u8; PAGE_SIZE as usize];
    let n = if let Some(ino) = crate::page_cache::register_path(path) {
        crate::page_cache::read(ino, file_offset, &mut buf).unwrap_or(0)
    } else {
        crate::vfs::pread_file(path, file_offset, &mut buf).unwrap_or(0)
    };
    if n > 0 {
        unsafe {
            copy_to_physical_frame(frame_phys, &buf[..n]);
        }
    }
}

/// Public wrapper to map a page in a process's page table.
///
/// # Safety
/// cr3, vaddr, and phys_frame must all be valid.
pub unsafe fn map_page_in_table_pub(cr3: u64, vaddr: u64, phys_frame: u64, flags: PageTableFlags) {
    map_page_in_table(cr3, vaddr, phys_frame, flags);
}

/// Get the physical memory offset (public access for vDSO etc.)
pub fn get_phys_mem_offset() -> u64 {
    PHYS_MEM_OFFSET.load(Ordering::Relaxed)
}

/// Whether the VMM has been initialised (physical memory is mapped).
pub fn ready() -> bool {
    PHYS_MEM_OFFSET.load(Ordering::Relaxed) != 0
}

/// Kernel L4 physical address used for kernel-stack mappings.
pub fn get_kernel_cr3() -> u64 {
    KERNEL_CR3.load(Ordering::Relaxed)
}

/// Copy the boot L4 into a private template used by [`clone_kernel_mappings`].
pub(super) fn snapshot_kernel_l4() {
    if KERNEL_CR3.load(Ordering::Relaxed) == 0 {
        let (frame, _) = Cr3::read();
        KERNEL_CR3.store(frame.start_address().as_u64(), Ordering::Relaxed);
    }
    let src = KERNEL_CR3.load(Ordering::Relaxed);
    if let Some(tmpl) = allocate_page_table_frame() {
        unsafe {
            copy_physical_frame(src, tmpl);
        }
        KERNEL_L4_TEMPLATE.store(tmpl, Ordering::Relaxed);
        serial_println!("[VMM] Kernel L4 snapshot at {:#x}", tmpl);
    } else {
        serial_println!("[VMM] Kernel L4 snapshot skipped (no frames)");
    }
}
