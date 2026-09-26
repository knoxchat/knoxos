//! ELF loading into an address space and user-stack construction.
use crate::process::Pid;
use crate::serial_println;
use core::sync::atomic::Ordering;

#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::{registers::control::Cr3, structures::paging::PageTableFlags};
#[cfg(target_arch = "x86_64")]
use x86_64::{registers::control::Cr3, structures::paging::PageTableFlags};

use super::frame::allocate_physical_frame;
use super::layout::{PAGE_SIZE, STACK_TOP, page_align_down, page_align_up};
use super::page_table::{
    PHYS_MEM_OFFSET, get_mapped_frame, map_page_in_table, update_page_flags_in_table,
    zero_physical_frame,
};
use super::region::{MappingType, MmapFlags, ProtFlags, VirtualMemoryArea};
use super::space::ADDRESS_SPACES;

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

/// Map a static ELF plus a small user stack into the **current** page tables
/// with `USER_ACCESSIBLE`. Used for Gate B2 (first Ring 3 hello) so `iretq`
/// does not need a CR3 switch.
///
/// Returns `(entry_point, user_rsp)`.
pub fn map_static_elf_into_current(elf_data: &[u8]) -> Result<(u64, u64), &'static str> {
    let header = crate::elf::validate_elf(elf_data).map_err(|_| "invalid ELF binary")?;
    let phdrs = crate::elf::parse_program_headers(elf_data, header);

    let offset = PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if offset == 0 {
        return Err("VMM not initialized");
    }

    let (frame, _) = Cr3::read();
    let cr3 = frame.start_address().as_u64();

    for phdr in &phdrs {
        if phdr.p_type != 1 {
            continue;
        }

        let prot = ProtFlags {
            read: phdr.p_flags & 4 != 0,
            write: phdr.p_flags & 2 != 0,
            execute: phdr.p_flags & 1 != 0,
        };

        let seg_start = page_align_down(phdr.p_vaddr);
        let seg_end = page_align_up(phdr.p_vaddr + phdr.p_memsz);
        let num_pages = (seg_end - seg_start) / PAGE_SIZE;
        let load_flags =
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

        serial_println!(
            "[VMM] Current-CR3 ELF segment: {:#x}-{:#x} ({} pages)",
            seg_start,
            seg_end,
            num_pages
        );

        for i in 0..num_pages {
            let page_addr = seg_start + i * PAGE_SIZE;
            if unsafe { get_mapped_frame(cr3, page_addr) }.is_some() {
                return Err("ELF vaddr already mapped in kernel tables");
            }
            let frame_phys = allocate_physical_frame().ok_or("OOM mapping ELF segment")?;
            unsafe {
                map_page_in_table(cr3, page_addr, frame_phys, load_flags);
                zero_physical_frame(frame_phys);
            }

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

        if !prot.write {
            let final_flags = prot.to_page_flags();
            for i in 0..num_pages {
                let page_addr = seg_start + i * PAGE_SIZE;
                unsafe {
                    update_page_flags_in_table(cr3, page_addr, final_flags);
                }
            }
        }
    }

    // Four writable stack pages just below STACK_TOP.
    const HELLO_STACK_PAGES: u64 = 4;
    let stack_end = STACK_TOP;
    let stack_start = stack_end - HELLO_STACK_PAGES * PAGE_SIZE;
    let stack_flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::USER_ACCESSIBLE
        | PageTableFlags::NO_EXECUTE;

    for i in 0..HELLO_STACK_PAGES {
        let page_addr = stack_start + i * PAGE_SIZE;
        if unsafe { get_mapped_frame(cr3, page_addr) }.is_some() {
            return Err("user stack vaddr already mapped in kernel tables");
        }
        let frame_phys = allocate_physical_frame().ok_or("OOM mapping user stack")?;
        unsafe {
            map_page_in_table(cr3, page_addr, frame_phys, stack_flags);
            zero_physical_frame(frame_phys);
        }
    }

    unsafe {
        let (frame, flags) = Cr3::read();
        Cr3::write(frame, flags);
    }

    // SysV: RSP % 16 == 8 at _start.
    let user_rsp = (stack_end - 8) & !0xF | 8;
    serial_println!(
        "[VMM] Current-CR3 hello: entry={:#x} rsp={:#x} stack={:#x}-{:#x}",
        header.e_entry,
        user_rsp,
        stack_start,
        stack_end
    );
    Ok((header.e_entry, user_rsp))
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
