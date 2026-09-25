#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::VirtAddr;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
#[cfg(target_arch = "x86_64")]
use crate::arch_compat::structures::paging::VirtAddr;
#[cfg(not(target_arch = "x86_64"))]
use crate::arch_compat::structures::tss::TaskStateSegment;
/// GDT - Global Descriptor Table
/// Sets up segmentation and per-CPU TSS for interrupt handling
/// Includes kernel (ring 0) and user (ring 3) segments for syscall/sysret
use core::cell::UnsafeCell;
use lazy_static::lazy_static;
#[cfg(target_arch = "x86_64")]
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
#[cfg(target_arch = "x86_64")]
use x86_64::structures::tss::TaskStateSegment;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
pub const SYSCALL_IST_INDEX: u16 = 1;

/// Must match [`crate::smp::MAX_CPUS`]. Each CPU needs its own TSS (Busy bit
/// and RSP0/IST are per-hardware-thread).
pub const MAX_CPUS: usize = 16;

#[cfg(target_arch = "x86_64")]
type KernelGdt = GlobalDescriptorTable<40>;
#[cfg(not(target_arch = "x86_64"))]
type KernelGdt = GlobalDescriptorTable;

/// Stacks used by exception/IST entry. One set per CPU.
mod stacks {
    use super::MAX_CPUS;

    pub const DOUBLE_FAULT_SIZE: usize = 4096 * 5;
    pub const SYSCALL_SIZE: usize = 4096 * 8;

    #[repr(C, align(16))]
    #[derive(Copy, Clone)]
    pub struct Aligned<const N: usize>(pub [u8; N]);

    pub static mut DOUBLE_FAULT: [Aligned<DOUBLE_FAULT_SIZE>; MAX_CPUS] =
        [Aligned([0; DOUBLE_FAULT_SIZE]); MAX_CPUS];
    pub static mut SYSCALL: [Aligned<SYSCALL_SIZE>; MAX_CPUS] =
        [Aligned([0; SYSCALL_SIZE]); MAX_CPUS];

    pub fn double_fault_top(cpu: usize) -> u64 {
        // SAFETY: address-of only; the array is never read or written as data.
        unsafe { core::ptr::addr_of_mut!(DOUBLE_FAULT[cpu].0) as u64 + DOUBLE_FAULT_SIZE as u64 }
    }

    pub fn syscall_top(cpu: usize) -> u64 {
        // SAFETY: address-of only; the array is never read or written as data.
        unsafe { core::ptr::addr_of_mut!(SYSCALL[cpu].0) as u64 + SYSCALL_SIZE as u64 }
    }
}

/// Per-CPU TSS table. `RSP0` is rewritten on every switch to a user task
/// because it is the stack the CPU selects for Ring 3 → Ring 0 transitions
/// (hardware IRQs and exceptions). `syscall` entry uses `gs:[8]`, kept in
/// step by [`crate::usermode::set_kernel_stack`].
struct TssTable {
    entries: UnsafeCell<[TaskStateSegment; MAX_CPUS]>,
}

// SAFETY: `RSP0` is written from kernel context on the owning CPU before
// that CPU's task runs, and read by that CPU on privilege transitions.
// IST fields are written once in `init_ist_stacks` before `load_tss`.
unsafe impl Sync for TssTable {}

static TSS: TssTable = TssTable {
    entries: UnsafeCell::new([TaskStateSegment::new(); MAX_CPUS]),
};

fn tss(cpu: usize) -> &'static TaskStateSegment {
    let cpu = cpu.min(MAX_CPUS - 1);
    // SAFETY: `entries` is never relocated; callers only read, except through
    // `tss_mut` which is the documented writer.
    unsafe { &(*TSS.entries.get())[cpu] }
}

fn tss_mut(cpu: usize) -> &'static mut TaskStateSegment {
    let cpu = cpu.min(MAX_CPUS - 1);
    // SAFETY: each CPU writes only its own TSS. RSP0 is updated from kernel
    // context on that CPU; IST is initialized once before `ltr`.
    unsafe { &mut (*TSS.entries.get())[cpu] }
}

/// Install the exception/IST stacks. Must run before the TSS is loaded and
/// before any Ring 3 task exists.
fn init_ist_stacks() {
    for cpu in 0..MAX_CPUS {
        let tss = tss_mut(cpu);
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] =
            VirtAddr::new(stacks::double_fault_top(cpu));
        tss.interrupt_stack_table[SYSCALL_IST_INDEX as usize] =
            VirtAddr::new(stacks::syscall_top(cpu));
        tss.privilege_stack_table[0] = VirtAddr::new(stacks::syscall_top(cpu));
    }
}

fn current_cpu() -> usize {
    crate::usermode::current_cpu_index() as usize
}

/// RSP0 used on Ring 3 → Ring 0 privilege change (and the initial syscall stack).
pub fn privilege_stack_top() -> u64 {
    tss(current_cpu()).privilege_stack_table[0].as_u64()
}

/// Point future Ring 3 → Ring 0 transitions at `top` on the current CPU.
///
/// # Safety
/// `top` must be the top of a mapped, writable stack owned by the task that is
/// about to run in Ring 3.
pub unsafe fn set_privilege_stack_top(top: u64) {
    tss_mut(current_cpu()).privilege_stack_table[0] = VirtAddr::new(top);
}

/// Point CPU `cpu`'s RSP0 at `top` without reading GS (AP bring-up).
pub unsafe fn set_privilege_stack_top_cpu(cpu: usize, top: u64) {
    tss_mut(cpu).privilege_stack_table[0] = VirtAddr::new(top);
}

lazy_static! {
    static ref GDT: (KernelGdt, Selectors) = {
        #[cfg(target_arch = "x86_64")]
        let mut gdt = KernelGdt::empty();
        #[cfg(not(target_arch = "x86_64"))]
        let mut gdt = KernelGdt::new();
        // Segment order matters for syscall/sysret:
        // Index 1: Kernel Code (0x08) - Ring 0
        let code_selector = gdt.append(Descriptor::kernel_code_segment());
        // Index 2: Kernel Data (0x10) - Ring 0
        let data_selector = gdt.append(Descriptor::kernel_data_segment());
        // Index 3: User Data (0x1B) - Ring 3 (must come before user code for sysret)
        let user_data_selector = gdt.append(Descriptor::user_data_segment());
        // Index 4: User Code (0x23) - Ring 3
        let user_code_selector = gdt.append(Descriptor::user_code_segment());
        // Index 5+: one 64-bit TSS descriptor (2 entries) per CPU
        let mut tss_selectors = [code_selector; MAX_CPUS];
        for cpu in 0..MAX_CPUS {
            tss_selectors[cpu] = gdt.append(Descriptor::tss_segment(tss(cpu)));
        }
        (
            gdt,
            Selectors {
                code_selector,
                data_selector,
                user_code_selector,
                user_data_selector,
                tss_selectors,
            },
        )
    };
}

pub struct Selectors {
    pub code_selector: SegmentSelector,
    pub data_selector: SegmentSelector,
    pub user_code_selector: SegmentSelector,
    pub user_data_selector: SegmentSelector,
    pub tss_selectors: [SegmentSelector; MAX_CPUS],
}

/// Get kernel code segment selector
pub fn kernel_code_selector() -> SegmentSelector {
    GDT.1.code_selector
}

/// Get kernel data segment selector
pub fn kernel_data_selector() -> SegmentSelector {
    GDT.1.data_selector
}

/// Get user code segment selector (ring 3)
pub fn user_code_selector() -> SegmentSelector {
    GDT.1.user_code_selector
}

/// Get user data segment selector (ring 3)
pub fn user_data_selector() -> SegmentSelector {
    GDT.1.user_data_selector
}

/// TSS selector for `cpu` (used by `ltr` / `str`).
pub fn tss_selector(cpu: usize) -> SegmentSelector {
    GDT.1.tss_selectors[cpu.min(MAX_CPUS - 1)]
}

/// The value `str` should return on this CPU after `load_tss`.
pub fn current_tss_selector() -> u16 {
    tss_selector(current_cpu()).0
}

pub fn init() {
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::segmentation::{CS, DS, Segment};
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::tables::load_tss;
    #[cfg(target_arch = "x86_64")]
    use x86_64::instructions::segmentation::{CS, DS, Segment};
    #[cfg(target_arch = "x86_64")]
    use x86_64::instructions::tables::load_tss;

    init_ist_stacks();

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        DS::set_reg(GDT.1.data_selector);
        load_tss(GDT.1.tss_selectors[0]);
    }
    crate::serial_println!(
        "[KnoxOS] GDT: kernel CS={:#x}, DS={:#x}, user CS={:#x}, DS={:#x}",
        GDT.1.code_selector.0,
        GDT.1.data_selector.0,
        GDT.1.user_code_selector.0,
        GDT.1.user_data_selector.0
    );
    crate::serial_println!(
        "[KnoxOS] TSS[0]: selector={:#x} RSP0={:#x} DF-IST={:#x} ({} CPUs)",
        GDT.1.tss_selectors[0].0,
        tss(0).privilege_stack_table[0].as_u64(),
        tss(0).interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize].as_u64(),
        MAX_CPUS
    );
}

/// Initialize GDT + this CPU's TSS on an Application Processor.
pub fn init_ap(cpu_index: u32) {
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::segmentation::{CS, DS, Segment};
    #[cfg(not(target_arch = "x86_64"))]
    use crate::arch_compat::instructions::tables::load_tss;
    #[cfg(target_arch = "x86_64")]
    use x86_64::instructions::segmentation::{CS, DS, Segment};
    #[cfg(target_arch = "x86_64")]
    use x86_64::instructions::tables::load_tss;

    let cpu = (cpu_index as usize).min(MAX_CPUS - 1);
    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        DS::set_reg(GDT.1.data_selector);
        #[cfg(target_arch = "x86_64")]
        {
            use x86_64::instructions::segmentation::{ES, SS};
            ES::set_reg(GDT.1.data_selector);
            SS::set_reg(GDT.1.data_selector);
        }
        load_tss(GDT.1.tss_selectors[cpu]);
    }
}
