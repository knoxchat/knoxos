//! Copy to/from a process's user virtual addresses (via page-table walk).
use crate::process::Pid;
use core::sync::atomic::Ordering;

use super::page_table::PHYS_MEM_OFFSET;
use super::space::ADDRESS_SPACES;

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
