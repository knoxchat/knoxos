//! Built-in VMM gate tests (W^X/ASLR, CoW, swap-in, buddy RAM).
use crate::process::Pid;
use crate::serial_println;
use core::sync::atomic::Ordering;

#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::structures::paging::PageTableFlags;
#[cfg(target_arch = "x86_64")]
use x86_64::structures::paging::PageTableFlags;

use super::frame::{
    FRAME_POOL, GATE_J2_MARKER, allocate_physical_frame, free_physical_frame, get_stats,
};
use super::layout::PAGE_SIZE;
use super::page_table::{
    KERNEL_CR3, activate_cr3, copy_to_physical_frame, get_mapped_frame, read_physical_frame_byte,
};
use super::region::{MmapFlags, ProtFlags};
use super::space::{
    ADDRESS_SPACES, AddressSpace, create_address_space, destroy_address_space, handle_page_fault,
    mmap, swap_out_page,
};
use super::user::{read_user_memory, write_user_memory};

/// Serial marker once user maps refuse W+X and ASLR randomizes layout.
pub const GATE_E1_MARKER: &str = "GATE_E1 wx aslr complete";

/// Gate E1: `mprotect`/`mmap` RWX is denied, NX is the default for data, and
/// two address spaces get different ASLR bases.
pub fn wx_aslr_self_test() -> bool {
    let wx = ProtFlags {
        read: true,
        write: true,
        execute: true,
    };
    if !crate::hardening::check_wx_violation(crate::hardening::ProtFlags {
        read: true,
        write: true,
        exec: true,
    }) {
        serial_println!("[VMM] Gate E1 FAILED: W^X policy did not deny RWX");
        return false;
    }

    let Some(mut space) = AddressSpace::new(0x0000_E001) else {
        serial_println!("[VMM] Gate E1 FAILED: could not allocate address space");
        return false;
    };
    let flags = MmapFlags {
        shared: false,
        anonymous: true,
        fixed: false,
        populate: false,
    };
    if space.mmap_anonymous(0, PAGE_SIZE, wx, flags).is_some() {
        serial_println!("[VMM] Gate E1 FAILED: RWX mmap succeeded");
        return false;
    }

    let rw = ProtFlags {
        read: true,
        write: true,
        execute: false,
    };
    let Some(mapped) = space.mmap_anonymous(0, PAGE_SIZE, rw, flags) else {
        serial_println!("[VMM] Gate E1 FAILED: RW mmap denied");
        return false;
    };
    let Some(vma) = space.find_vma(mapped) else {
        serial_println!("[VMM] Gate E1 FAILED: RW VMA missing");
        return false;
    };
    if vma.prot.execute {
        serial_println!("[VMM] Gate E1 FAILED: RW map was executable");
        return false;
    }
    if !vma
        .prot
        .to_page_flags()
        .contains(PageTableFlags::NO_EXECUTE)
    {
        serial_println!("[VMM] Gate E1 FAILED: NX bit not set on data map");
        return false;
    }
    if space.mprotect(mapped, PAGE_SIZE, wx) {
        serial_println!("[VMM] Gate E1 FAILED: mprotect RWX succeeded");
        return false;
    }

    let Some(a) = AddressSpace::new(0x0000_E002) else {
        serial_println!("[VMM] Gate E1 FAILED: second address space");
        return false;
    };
    let Some(b) = AddressSpace::new(0x0000_E003) else {
        serial_println!("[VMM] Gate E1 FAILED: third address space");
        return false;
    };
    if a.mmap_next == b.mmap_next && a.brk == b.brk {
        serial_println!("[VMM] Gate E1 FAILED: ASLR produced identical layouts");
        return false;
    }

    serial_println!("[VMM] {}", GATE_E1_MARKER);
    true
}

/// Serial marker once a forked address space write-faults a CoW page and the
/// parent keeps the original bytes.
pub const GATE_H1_MARKER: &str = "GATE_H1 cow fault";

/// Gate H1 / A4: fork marks the page read-only, a write copies it, and the
/// parent's contents are unchanged.
pub fn cow_fault_self_test() -> bool {
    const PARENT: Pid = 0x0000_A401;
    const CHILD: Pid = 0x0000_A402;

    let Some(mut parent) = AddressSpace::new(PARENT) else {
        serial_println!("[VMM] Gate H1 FAILED: parent address space");
        return false;
    };
    let flags = MmapFlags {
        shared: false,
        anonymous: true,
        fixed: false,
        populate: true,
    };
    let Some(addr) = parent.mmap_anonymous(0, PAGE_SIZE, ProtFlags::RW, flags) else {
        serial_println!("[VMM] Gate H1 FAILED: mmap");
        return false;
    };
    let Some(parent_frame) = (unsafe { get_mapped_frame(parent.cr3, addr) }) else {
        serial_println!("[VMM] Gate H1 FAILED: parent page not mapped");
        return false;
    };
    unsafe {
        copy_to_physical_frame(parent_frame, &[0xA4, 0x01]);
    }

    let Some(mut child) = parent.fork(CHILD) else {
        serial_println!("[VMM] Gate H1 FAILED: fork");
        return false;
    };
    if !child.cow_pages.contains_key(&addr) {
        serial_println!("[VMM] Gate H1 FAILED: child has no CoW tracking");
        return false;
    }
    if !child.handle_cow_fault(addr) {
        serial_println!("[VMM] Gate H1 FAILED: CoW fault not handled");
        return false;
    }
    let Some(child_frame) = (unsafe { get_mapped_frame(child.cr3, addr) }) else {
        serial_println!("[VMM] Gate H1 FAILED: child page missing after CoW");
        return false;
    };
    if child_frame == parent_frame {
        serial_println!("[VMM] Gate H1 FAILED: child still shares parent frame");
        return false;
    }
    unsafe {
        copy_to_physical_frame(child_frame, &[0xB4, 0x02]);
    }
    let parent_byte = unsafe { read_physical_frame_byte(parent_frame, 0) };
    let child_byte = unsafe { read_physical_frame_byte(child_frame, 0) };
    if parent_byte != 0xA4 {
        serial_println!(
            "[VMM] Gate H1 FAILED: parent byte {:#x} (want 0xA4)",
            parent_byte
        );
        return false;
    }
    if child_byte != 0xB4 {
        serial_println!(
            "[VMM] Gate H1 FAILED: child byte {:#x} (want 0xB4)",
            child_byte
        );
        return false;
    }

    // Make sure Drop cannot free the live CR3 if fork switched us.
    unsafe {
        activate_cr3(KERNEL_CR3.load(Ordering::Relaxed));
    }

    serial_println!("[VMM] {}", GATE_H1_MARKER);
    true
}

const GATE_K2_PID: Pid = 0x0000_4B02;
const GATE_K2_MAGIC: [u8; 8] = *b"K2SWAPOK";

/// Populate one anonymous page, swap it out, fault it back, and check the bytes.
pub fn swap_fault_self_test() -> bool {
    if !create_address_space(GATE_K2_PID) {
        serial_println!("[VMM] Gate K2 FAILED: address space");
        return false;
    }
    let flags = crate::mmap::MAP_PRIVATE | crate::mmap::MAP_ANONYMOUS | crate::mmap::MAP_POPULATE;
    let mapped = mmap(
        GATE_K2_PID,
        0,
        PAGE_SIZE,
        crate::mmap::PROT_READ | crate::mmap::PROT_WRITE,
        flags,
    );
    if mapped < 0 {
        serial_println!("[VMM] Gate K2 FAILED: mmap {}", mapped);
        destroy_address_space(GATE_K2_PID);
        return false;
    }
    let vaddr = mapped as u64;
    write_user_memory(GATE_K2_PID, vaddr, &GATE_K2_MAGIC);

    if !swap_out_page(GATE_K2_PID, vaddr) {
        serial_println!("[VMM] Gate K2 FAILED: swap_out");
        destroy_address_space(GATE_K2_PID);
        return false;
    }
    if !crate::swap::is_swapped(GATE_K2_PID, vaddr) {
        serial_println!("[VMM] Gate K2 FAILED: not marked swapped");
        destroy_address_space(GATE_K2_PID);
        return false;
    }
    {
        let spaces = ADDRESS_SPACES.lock();
        if let Some(addr_space) = spaces.get(&GATE_K2_PID) {
            if unsafe { get_mapped_frame(addr_space.cr3, vaddr) }.is_some() {
                serial_println!("[VMM] Gate K2 FAILED: page still mapped");
                drop(spaces);
                destroy_address_space(GATE_K2_PID);
                return false;
            }
        }
    }

    if !handle_page_fault(GATE_K2_PID, vaddr, false) {
        serial_println!("[VMM] Gate K2 FAILED: swap-in fault");
        destroy_address_space(GATE_K2_PID);
        return false;
    }
    let mut got = [0u8; 8];
    read_user_memory(GATE_K2_PID, vaddr, &mut got);
    destroy_address_space(GATE_K2_PID);
    if got != GATE_K2_MAGIC {
        serial_println!(
            "[VMM] Gate K2 FAILED: restored {:x?} want {:x?}",
            got,
            GATE_K2_MAGIC
        );
        return false;
    }
    serial_println!("[VMM] {}", crate::swap::GATE_K2_MARKER);
    true
}

/// Alloc/free a frame from the expanded pool and prove free restores it.
pub fn buddy_ram_self_test() -> bool {
    let (total, _allocated, available) = get_stats();
    if total < 8192 + 1024 {
        serial_println!(
            "[VMM] Gate J2 FAILED: pool total {} frames (want leftover RAM beyond 32 MiB)",
            total
        );
        return false;
    }
    if available == 0 {
        serial_println!("[VMM] Gate J2 FAILED: no free frames after ingest");
        return false;
    }
    let Some(frame) = allocate_physical_frame() else {
        serial_println!("[VMM] Gate J2 FAILED: allocate");
        return false;
    };
    let after_alloc = FRAME_POOL.lock().available();
    free_physical_frame(frame);
    let after_free = FRAME_POOL.lock().available();
    if after_free != after_alloc + 1 {
        serial_println!(
            "[VMM] Gate J2 FAILED: free did not restore (available {} -> {} -> {})",
            available,
            after_alloc,
            after_free
        );
        return false;
    }
    serial_println!("[VMM] {}", GATE_J2_MARKER);
    true
}
